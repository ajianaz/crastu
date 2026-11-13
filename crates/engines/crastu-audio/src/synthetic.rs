use std::f64::consts::PI;

use crate::error::AudioError;
use crate::source::{AudioSource, ReadResult};

/// What a [`SyntheticSource`] generates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Waveform {
    Silence,
    Sine {
        freq_hz: f64,
        amplitude: f32,
    },
    /// Deterministic white noise (a fixed-seed xorshift), uniform in -amplitude..amplitude.
    Noise {
        amplitude: f32,
    },
}

/// A generator for tests and demos: a fixed number of samples of one waveform.
#[derive(Debug)]
pub struct SyntheticSource {
    rate: u32,
    wave: Waveform,
    remaining: u64,
    produced: u64,
    rng: u64,
    started: bool,
}

impl SyntheticSource {
    pub fn new(rate: u32, wave: Waveform, duration_secs: f64) -> Self {
        SyntheticSource {
            rate,
            wave,
            remaining: (f64::from(rate) * duration_secs.max(0.0)) as u64,
            produced: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
            started: false,
        }
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        ((x >> 40) as f32 / (1u64 << 23) as f32) - 1.0
    }
}

impl AudioSource for SyntheticSource {
    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn start(&mut self) -> Result<(), AudioError> {
        if self.started {
            return Err(AudioError::State("already started"));
        }
        self.started = true;
        Ok(())
    }

    fn stop(&mut self) {
        self.remaining = 0;
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<ReadResult, AudioError> {
        if !self.started {
            return Err(AudioError::State("read before start"));
        }
        if self.remaining == 0 {
            return Ok(ReadResult::End);
        }
        let n = (buf.len() as u64).min(self.remaining) as usize;
        if n == 0 {
            return Ok(ReadResult::Samples(0)); // an empty buffer: nothing read, not the end
        }
        for slot in buf.iter_mut().take(n) {
            *slot = match self.wave {
                Waveform::Silence => 0.0,
                Waveform::Sine { freq_hz, amplitude } => {
                    let t = self.produced as f64 / f64::from(self.rate);
                    amplitude * (2.0 * PI * freq_hz * t).sin() as f32
                }
                Waveform::Noise { amplitude } => amplitude * self.next_noise(),
            };
            self.produced += 1;
        }
        self.remaining -= n as u64;
        Ok(ReadResult::Samples(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::level_of;
    use crate::source::drain;

    #[test]
    fn length_and_rate() {
        let mut s = SyntheticSource::new(8000, Waveform::Silence, 0.5);
        assert_eq!(s.sample_rate(), 8000);
        s.start().unwrap();
        let x = drain(&mut s);
        assert_eq!(x.len(), 4000);
        assert!(x.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn sine_has_the_requested_amplitude() {
        let mut s = SyntheticSource::new(
            16_000,
            Waveform::Sine {
                freq_hz: 440.0,
                amplitude: 0.5,
            },
            1.0,
        );
        s.start().unwrap();
        let l = level_of(&drain(&mut s));
        assert!((l.peak_db + 6.02).abs() < 0.1);
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let run = || {
            let mut s = SyntheticSource::new(16_000, Waveform::Noise { amplitude: 0.1 }, 0.1);
            s.start().unwrap();
            drain(&mut s)
        };
        let (a, b) = (run(), run());
        assert_eq!(a, b);
        assert!(a.iter().all(|v| v.abs() <= 0.1));
        assert!(level_of(&a).rms_db > -30.0); // not silent
    }

    #[test]
    fn an_empty_buffer_is_not_the_end() {
        let mut s = SyntheticSource::new(8000, Waveform::Silence, 0.001);
        s.start().unwrap();
        assert_eq!(s.read(&mut []), Ok(ReadResult::Samples(0)));
        assert_eq!(drain(&mut s).len(), 8);
        assert_eq!(s.read(&mut []), Ok(ReadResult::End));
    }

    #[test]
    fn misuse_and_stop() {
        let mut s = SyntheticSource::new(16_000, Waveform::Silence, 1.0);
        assert_eq!(
            s.read(&mut [0.0; 4]),
            Err(AudioError::State("read before start"))
        );
        s.start().unwrap();
        assert!(s.start().is_err());
        s.stop();
        assert_eq!(s.read(&mut [0.0; 4]), Ok(ReadResult::End));
    }
}
