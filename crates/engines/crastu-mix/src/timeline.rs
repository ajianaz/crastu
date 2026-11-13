//! The timeline as data, and the rule that turns it into events. Time is an integer sample
//! position at the mixer's sample rate.

use crate::curve::{Automation, smoothstep};

/// Shortest fade applied at either edge of every clip, in samples (about 1.3 ms at 48 kHz). A cut
/// with no fade is a step in the signal, which is heard as a click; this keeps every cut smooth
/// (fades are capped at half the clip, so a clip under twice this length gets shorter ones).
pub const MIN_FADE: u64 = 64;

/// Which bus a clip plays on. The voice bus ducks the music bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Track {
    /// Speech: narration, TTS takes, dialogue.
    Voice,
    /// Music and effects that sit under the voice.
    Music,
}

/// One piece of a source placed on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// The bus it plays on.
    pub track: Track,
    /// Index into the mixer's list of sources.
    pub source: usize,
    /// First timeline sample of the clip.
    pub start: u64,
    /// Number of samples (frames) it plays.
    pub length: u64,
    /// Frame of the source that plays at `start`.
    pub source_offset: u64,
    /// Linear gain of the whole clip.
    pub gain: f32,
    /// Fade-in length in samples (at least [`MIN_FADE`] is applied, at most half the clip).
    pub fade_in: u64,
    /// Fade-out length in samples (same limits).
    pub fade_out: u64,
}

impl Clip {
    /// A clip at unity gain with only the minimum fades.
    #[must_use]
    pub fn new(track: Track, source: usize, start: u64, length: u64) -> Self {
        Self {
            track,
            source,
            start,
            length,
            source_offset: 0,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
        }
    }

    /// First sample after the clip.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.start.saturating_add(self.length)
    }

    /// Gain of the clip at timeline sample `pos`: its own gain times both fades; zero outside it.
    /// A pure function of `pos`.
    #[must_use]
    pub fn gain_at(&self, pos: u64) -> f32 {
        if pos < self.start || pos >= self.end() {
            return 0.0;
        }
        let i = pos - self.start;
        let mut g = self.gain;
        let fi = edge(self.fade_in, self.length);
        if i < fi {
            g *= smoothstep(i as f32 / fi as f32);
        }
        let fo = edge(self.fade_out, self.length);
        let from_end = self.length - 1 - i;
        if from_end < fo {
            g *= smoothstep(from_end as f32 / fo as f32);
        }
        g
    }
}

fn edge(fade: u64, length: u64) -> u64 {
    fade.max(MIN_FADE).min(length / 2)
}

/// What happens at an [`Event`]'s position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    /// The clip stops (listed before a start at the same sample).
    End,
    /// The clip begins.
    Start,
}

/// A clip becoming active or inactive at a sample position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Event {
    /// Timeline sample.
    pub pos: u64,
    /// Start or end.
    pub kind: EventKind,
    /// Index of the clip in [`Timeline::clips`].
    pub clip: usize,
}

/// Clips plus the automation of the music bus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    /// All clips, voice and music, in any order.
    pub clips: Vec<Clip>,
    /// Gain of the music bus over time (before ducking).
    pub music_gain: Automation,
}

impl Timeline {
    /// The timeline-to-events rule, pure: every non-empty clip yields a start and an end, sorted
    /// by position, with ends before starts at the same sample. Preview and export both read
    /// this list, so a cut lands on the same sample in both.
    #[must_use]
    pub fn events(&self) -> Vec<Event> {
        let mut ev = Vec::with_capacity(self.clips.len() * 2);
        for (clip, c) in self.clips.iter().enumerate() {
            if c.length == 0 {
                continue;
            }
            ev.push(Event {
                pos: c.start,
                kind: EventKind::Start,
                clip,
            });
            ev.push(Event {
                pos: c.end(),
                kind: EventKind::End,
                clip,
            });
        }
        ev.sort();
        ev
    }

    /// Where the voice bus is active: the union of the voice clips as sorted, merged, half-open
    /// `(start, end)` spans. This is what the ducker follows.
    #[must_use]
    pub fn voice_spans(&self) -> Vec<(u64, u64)> {
        let mut spans: Vec<(u64, u64)> = self
            .clips
            .iter()
            .filter(|c| c.track == Track::Voice && c.length > 0)
            .map(|c| (c.start, c.end()))
            .collect();
        spans.sort_unstable();
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(spans.len());
        for (s, e) in spans {
            match merged.last_mut() {
                Some(last) if s <= last.1 => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        merged
    }

    /// The first sample after every clip.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.clips.iter().map(Clip::end).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_sorted_with_ends_before_starts() {
        let t = Timeline {
            clips: vec![
                Clip::new(Track::Voice, 0, 100, 50),
                Clip::new(Track::Music, 1, 150, 100),
                Clip::new(Track::Music, 1, 0, 0),
            ],
            ..Timeline::default()
        };
        let ev = t.events();
        let got: Vec<_> = ev.iter().map(|e| (e.pos, e.kind, e.clip)).collect();
        assert_eq!(
            got,
            vec![
                (100, EventKind::Start, 0),
                (150, EventKind::End, 0),
                (150, EventKind::Start, 1),
                (250, EventKind::End, 1),
            ]
        );
    }

    #[test]
    fn voice_spans_merge_touching_and_overlapping_clips() {
        let t = Timeline {
            clips: vec![
                Clip::new(Track::Voice, 0, 100, 50),
                Clip::new(Track::Voice, 0, 150, 50),
                Clip::new(Track::Voice, 0, 120, 100),
                Clip::new(Track::Voice, 0, 1000, 10),
                Clip::new(Track::Music, 1, 0, 5000),
            ],
            ..Timeline::default()
        };
        assert_eq!(t.voice_spans(), vec![(100, 220), (1000, 1010)]);
    }

    #[test]
    fn clip_gain_fades_in_and_out_and_is_zero_outside() {
        let c = Clip::new(Track::Music, 0, 1000, 1000);
        assert_eq!(c.gain_at(999), 0.0);
        assert_eq!(c.gain_at(1000), 0.0);
        assert!(c.gain_at(1000 + MIN_FADE / 2) > 0.0 && c.gain_at(1000 + MIN_FADE / 2) < 1.0);
        assert_eq!(c.gain_at(1500), 1.0);
        assert_eq!(c.gain_at(1999), 0.0);
        assert_eq!(c.gain_at(2000), 0.0);
    }
}
