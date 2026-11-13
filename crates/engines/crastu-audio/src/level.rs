/// The level reported for silence (and the floor of every reading), in dB full scale.
pub const SILENCE_DB: f32 = -100.0;

/// A sample at or above this magnitude counts as clipped.
pub const CLIP_THRESHOLD: f32 = 0.999;

/// Peak and RMS of a block of samples, in dB relative to full scale (1.0), for a UI meter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Level {
    pub peak_db: f32,
    pub rms_db: f32,
}

impl Level {
    pub const SILENT: Level = Level {
        peak_db: SILENCE_DB,
        rms_db: SILENCE_DB,
    };
}

fn to_db(x: f32) -> f32 {
    if x <= 0.0 {
        SILENCE_DB
    } else {
        (20.0 * x.log10()).max(SILENCE_DB)
    }
}

/// The level of `samples`; an empty block is silent.
pub fn level_of(samples: &[f32]) -> Level {
    if samples.is_empty() {
        return Level::SILENT;
    }
    let mut peak = 0.0f32;
    let mut sum = 0.0f64;
    for &s in samples {
        peak = peak.max(s.abs());
        sum += f64::from(s) * f64::from(s);
    }
    let rms = (sum / samples.len() as f64).sqrt() as f32;
    Level {
        peak_db: to_db(peak),
        rms_db: to_db(rms),
    }
}

/// How many samples of the block are at or above [`CLIP_THRESHOLD`] in magnitude.
pub fn count_clipped(samples: &[f32]) -> usize {
    samples.iter().filter(|s| s.abs() >= CLIP_THRESHOLD).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_zero_are_silent() {
        assert_eq!(level_of(&[]), Level::SILENT);
        assert_eq!(level_of(&[0.0; 8]), Level::SILENT);
    }

    #[test]
    fn full_scale_square_wave_is_zero_db() {
        let l = level_of(&[1.0, -1.0, 1.0, -1.0]);
        assert!(l.peak_db.abs() < 1e-4);
        assert!(l.rms_db.abs() < 1e-4);
    }

    #[test]
    fn half_amplitude_is_minus_six_db() {
        let l = level_of(&[0.5, -0.5, 0.5, -0.5]);
        assert!((l.peak_db + 6.0206).abs() < 1e-3, "{}", l.peak_db);
        assert!((l.rms_db + 6.0206).abs() < 1e-3);
    }

    #[test]
    fn peak_and_rms_differ_for_a_single_spike() {
        // One 1.0 in four: peak 0 dB, rms = 0.5 -> -6.02 dB.
        let l = level_of(&[1.0, 0.0, 0.0, 0.0]);
        assert!(l.peak_db.abs() < 1e-4);
        assert!((l.rms_db + 6.0206).abs() < 1e-3);
    }

    #[test]
    fn clipping_counts_both_polarities_and_the_threshold() {
        assert_eq!(count_clipped(&[1.0, -1.0, 0.999, 0.998, -0.5]), 3);
    }
}
