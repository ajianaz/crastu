use crate::Time;

/// Interleaved 32-bit float PCM.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuf {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

impl AudioBuf {
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<f32>) -> Self {
        assert!(
            sample_rate > 0 && channels > 0,
            "sample rate and channels must be positive"
        );
        Self {
            sample_rate,
            channels,
            samples,
        }
    }

    pub fn silence(sample_rate: u32, channels: u16, duration: Time) -> Self {
        let frames = (duration.as_secs_f64() * f64::from(sample_rate)).round() as usize;
        Self::new(
            sample_rate,
            channels,
            vec![0.0; frames * usize::from(channels)],
        )
    }

    /// Number of sample frames (one sample per channel).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn duration(&self) -> Time {
        Time::from_secs_f64(self.frames() as f64 / f64::from(self.sample_rate))
    }

    /// Averages all channels into one.
    pub fn to_mono(&self) -> AudioBuf {
        if self.channels == 1 {
            return self.clone();
        }
        let ch = usize::from(self.channels);
        let samples = self
            .samples
            .chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect();
        AudioBuf::new(self.sample_rate, 1, samples)
    }

    /// Peak absolute sample value, 0.0 for silence.
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    /// Root mean square level in dBFS, `-inf` for silence.
    pub fn rms_dbfs(&self) -> f32 {
        if self.samples.is_empty() {
            return f32::NEG_INFINITY;
        }
        let mean_sq = self.samples.iter().map(|s| s * s).sum::<f32>() / self.samples.len() as f32;
        10.0 * mean_sq.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_and_frames() {
        let buf = AudioBuf::silence(16_000, 2, Time::from_millis(1_500));
        assert_eq!(buf.frames(), 24_000);
        assert_eq!(buf.duration(), Time::from_millis(1_500));
        assert_eq!(buf.rms_dbfs(), f32::NEG_INFINITY);
    }

    #[test]
    fn mono_mix_and_levels() {
        let buf = AudioBuf::new(48_000, 2, vec![1.0, 0.0, -0.5, -0.5]);
        let mono = buf.to_mono();
        assert_eq!(mono.samples, vec![0.5, -0.5]);
        assert_eq!(buf.peak(), 1.0);
        assert!((mono.rms_dbfs() - (-6.0206)).abs() < 0.001);
    }
}
