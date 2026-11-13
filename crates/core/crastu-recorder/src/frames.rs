//! Frame times for the GUI's `perf.jsonl`, as plain data: the window calls [`FrameStats::frame_at`]
//! once per drawn frame and the once-a-second timer calls [`FrameStats::take`].

use crate::perf::{PerfSample, frame_stats};

/// A gap between two drawn frames longer than this is a pause, not a slow frame: a window only
/// draws when something changed, so the time between a hover and the next click says nothing
/// about how fast it draws.
pub const IDLE_MS: u64 = 500;
/// Frames kept for one window of samples; later ones are dropped until [`FrameStats::take`].
const MAX_FRAMES: usize = 4096;

/// The intervals between consecutive drawn frames since the last [`take`](FrameStats::take).
#[derive(Clone, Debug, Default)]
pub struct FrameStats {
    last_ms: Option<u64>,
    frames: Vec<f64>,
}

impl FrameStats {
    pub fn new() -> FrameStats {
        FrameStats::default()
    }

    /// A frame was drawn at `now_ms` (any monotonic millisecond clock). The interval since the
    /// previous frame counts when it is at most [`IDLE_MS`]; a clock that went backwards is
    /// ignored.
    pub fn frame_at(&mut self, now_ms: u64) {
        if let Some(last) = self.last_ms
            && now_ms >= last
            && now_ms - last <= IDLE_MS
            && self.frames.len() < MAX_FRAMES
        {
            self.frames.push((now_ms - last) as f64);
        }
        self.last_ms = Some(now_ms);
    }

    /// The p50, p99 and max of the frames since the last call (empty fields when there were
    /// none), with `rss_bytes` as given; starts a new window.
    pub fn take(&mut self, rss_bytes: Option<u64>) -> PerfSample {
        let stats = frame_stats(&self.frames);
        self.frames.clear();
        PerfSample {
            frame_p50_ms: stats.map(|s| s.0),
            frame_p99_ms: stats.map(|s| s.1),
            frame_max_ms: stats.map(|s| s.2),
            rss_bytes,
            ..PerfSample::default()
        }
    }
}

/// Frame times shared between a draw callback and the timer that writes them: [`FrameStats`]
/// behind a lock, with the clock started when it was made.
#[derive(Clone, Debug)]
pub struct Frames {
    stats: std::sync::Arc<std::sync::Mutex<FrameStats>>,
    epoch: std::time::Instant,
}

impl Default for Frames {
    fn default() -> Frames {
        Frames {
            stats: std::sync::Arc::default(),
            epoch: std::time::Instant::now(),
        }
    }
}

impl Frames {
    pub fn new() -> Frames {
        Frames::default()
    }

    /// A frame was drawn now.
    pub fn mark(&self) {
        let ms = u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.mark_at(ms);
    }

    fn mark_at(&self, ms: u64) {
        self.stats
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .frame_at(ms);
    }

    /// The sample for the frames since the last call, with the memory in use where it is known.
    pub fn sample(&self) -> PerfSample {
        let rss = crate::resident_bytes();
        self.stats
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take(rss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_reports_p50_p99_and_max_of_the_intervals() {
        let mut s = FrameStats::new();
        let mut t = 1_000;
        s.frame_at(t);
        // 99 frames of 10 ms and one of 100 ms.
        for _ in 0..99 {
            t += 10;
            s.frame_at(t);
        }
        t += 100;
        s.frame_at(t);
        let sample = s.take(Some(7));
        assert_eq!(sample.frame_p50_ms, Some(10.0));
        assert_eq!(sample.frame_p99_ms, Some(10.0));
        assert_eq!(sample.frame_max_ms, Some(100.0));
        assert_eq!(sample.rss_bytes, Some(7));
    }

    #[test]
    fn a_pause_is_not_a_slow_frame() {
        let mut s = FrameStats::new();
        s.frame_at(0);
        s.frame_at(IDLE_MS + 1);
        assert_eq!(s.take(None), PerfSample::default());
        // The frame after the pause measures from the frame that ended it.
        s.frame_at(IDLE_MS + 17);
        assert_eq!(s.take(None).frame_max_ms, Some(16.0));
    }

    #[test]
    fn the_first_frame_and_a_backwards_clock_add_nothing() {
        let mut s = FrameStats::new();
        s.frame_at(100);
        s.frame_at(50);
        assert_eq!(s.take(None).frame_max_ms, None);
    }

    #[test]
    fn take_starts_a_new_window_and_the_window_is_bounded() {
        let mut s = FrameStats::new();
        for i in 0..(MAX_FRAMES as u64 + 100) {
            s.frame_at(i);
        }
        assert!(s.frames.len() <= MAX_FRAMES);
        assert!(s.take(None).frame_p50_ms.is_some());
        assert_eq!(s.take(None).frame_p50_ms, None);
    }

    #[test]
    fn marks_from_a_callback_become_one_sample() {
        let frames = Frames::new();
        let clone = frames.clone();
        for t in [0, 16, 33, 49] {
            clone.mark_at(t);
        }
        let s = frames.sample();
        assert_eq!(s.frame_max_ms, Some(17.0));
        assert_eq!(s.frame_p50_ms, Some(16.0));
        // The next window starts empty.
        assert_eq!(frames.sample().frame_max_ms, None);
    }
}
