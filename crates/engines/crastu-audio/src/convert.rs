use crate::level::{Level, count_clipped, level_of};
use crate::resample::Resampler;

/// The rate speech models read.
pub const TARGET_RATE: u32 = 16_000;
/// Length of one frame in milliseconds.
pub const FRAME_MS: u32 = 20;
/// Samples in one frame at [`TARGET_RATE`].
pub const FRAME_SAMPLES: usize = (TARGET_RATE * FRAME_MS / 1000) as usize;

/// Averages interleaved channels into mono, appending to `out`. A trailing partial frame (fewer
/// samples than `channels`) is ignored. `channels == 0` appends nothing.
pub fn downmix_interleaved(data: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => out.extend_from_slice(data),
        n => out.extend(
            data.chunks_exact(n)
                .map(|f| f.iter().sum::<f32>() / n as f32),
        ),
    }
}

/// One 20 ms frame at 16 kHz.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Always [`FRAME_SAMPLES`] long (the last frame of a stream is zero padded).
    pub samples: Vec<f32>,
    pub level: Level,
    /// The input had clipped samples (see [`crate::CLIP_THRESHOLD`]) since the previous frame.
    pub clipped: bool,
}

/// Mono samples at any rate in, [`Frame`]s at 16 kHz out.
#[derive(Debug)]
pub struct Converter {
    resampler: Resampler,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    clipped: bool,
}

impl Converter {
    pub fn new(input_rate: u32) -> Self {
        Converter {
            resampler: Resampler::new(input_rate, TARGET_RATE),
            pending: Vec::new(),
            scratch: Vec::new(),
            clipped: false,
        }
    }

    /// Feeds mono samples; completed frames are appended to `frames`.
    pub fn push(&mut self, mono: &[f32], frames: &mut Vec<Frame>) {
        if count_clipped(mono) > 0 {
            self.clipped = true;
        }
        self.scratch.clear();
        self.resampler.process(mono, &mut self.scratch);
        self.pending.extend_from_slice(&self.scratch);
        self.emit(frames);
    }

    /// Feeds interleaved multi-channel samples (downmixed first).
    pub fn push_interleaved(&mut self, data: &[f32], channels: usize, frames: &mut Vec<Frame>) {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        downmix_interleaved(data, channels, &mut mono);
        self.push(&mono, frames);
    }

    /// Ends the stream: the resampler's tail and a final zero-padded frame, if any is left.
    pub fn finish(&mut self, frames: &mut Vec<Frame>) {
        self.scratch.clear();
        self.resampler.finish(&mut self.scratch);
        self.pending.extend_from_slice(&self.scratch);
        self.emit(frames);
        if !self.pending.is_empty() {
            self.pending.resize(FRAME_SAMPLES, 0.0);
            self.emit(frames);
        }
    }

    fn emit(&mut self, frames: &mut Vec<Frame>) {
        let whole = self.pending.len() / FRAME_SAMPLES * FRAME_SAMPLES;
        let (chunks, _) = self.pending[..whole].as_chunks::<FRAME_SAMPLES>();
        for chunk in chunks {
            frames.push(Frame {
                samples: chunk.to_vec(),
                level: level_of(&chunk[..]),
                clipped: std::mem::take(&mut self.clipped),
            });
        }
        self.pending.drain(..whole);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resample::tests::sine;

    #[test]
    fn downmix_averages_channels() {
        let mut out = Vec::new();
        downmix_interleaved(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2, &mut out);
        assert_eq!(out, vec![0.5, 0.5, 0.0]);
        out.clear();
        downmix_interleaved(&[1.0, 2.0, 3.0, 4.0, 5.0], 3, &mut out);
        assert_eq!(out, vec![2.0]); // the trailing 4, 5 is a partial frame
        out.clear();
        downmix_interleaved(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, vec![0.1, 0.2]);
        out.clear();
        downmix_interleaved(&[0.1, 0.2], 0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn frames_have_the_fixed_length_and_count() {
        let mut c = Converter::new(16_000);
        let mut frames = Vec::new();
        // 3.5 frames in odd-sized pushes.
        let x = vec![0.1; FRAME_SAMPLES * 7 / 2];
        for chunk in x.chunks(97) {
            c.push(chunk, &mut frames);
        }
        assert_eq!(frames.len(), 3);
        assert!(frames.iter().all(|f| f.samples.len() == FRAME_SAMPLES));
        c.finish(&mut frames);
        assert_eq!(frames.len(), 4);
        let last = &frames[3];
        assert_eq!(last.samples.len(), FRAME_SAMPLES);
        assert!(last.samples[FRAME_SAMPLES / 2..].iter().all(|s| *s == 0.0));
        assert_eq!(FRAME_SAMPLES, 320);
    }

    #[test]
    fn exact_multiple_has_no_padding_frame() {
        let mut c = Converter::new(16_000);
        let mut frames = Vec::new();
        c.push(&vec![0.1; FRAME_SAMPLES * 2], &mut frames);
        c.finish(&mut frames);
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn one_second_at_48k_gives_fifty_frames_with_the_right_level() {
        let mut c = Converter::new(48_000);
        let mut frames = Vec::new();
        c.push(&sine(440.0, 48_000, 1.0, 0.5), &mut frames);
        c.finish(&mut frames);
        assert_eq!(frames.len(), 50);
        let mid = frames[25].level;
        assert!((mid.peak_db + 6.02).abs() < 0.3, "peak {}", mid.peak_db);
        assert!((mid.rms_db + 9.03).abs() < 0.3, "rms {}", mid.rms_db);
    }

    #[test]
    fn clipping_is_flagged_on_the_next_frame_only() {
        let mut c = Converter::new(16_000);
        let mut frames = Vec::new();
        let mut x = vec![0.1; FRAME_SAMPLES * 3];
        x[FRAME_SAMPLES + 5] = 1.0;
        c.push(&x, &mut frames);
        let flags: Vec<bool> = frames.iter().map(|f| f.clipped).collect();
        // The clipped sample is in the input of the second frame, and the flag is reported with
        // the first frame emitted after the push saw it: here all frames come out of one push.
        assert_eq!(flags, vec![true, false, false]);
        let mut frames = Vec::new();
        c.push(&vec![0.1; FRAME_SAMPLES], &mut frames);
        assert!(!frames[0].clipped);
    }

    #[test]
    fn stereo_input_is_downmixed_then_converted() {
        let mut c = Converter::new(16_000);
        let mut frames = Vec::new();
        let mut inter = Vec::new();
        for _ in 0..FRAME_SAMPLES {
            inter.extend_from_slice(&[0.5, -0.5]); // cancels to silence
        }
        c.push_interleaved(&inter, 2, &mut frames);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].level, Level::SILENT);
    }
}
