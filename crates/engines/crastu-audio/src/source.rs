use crate::error::AudioError;

/// What a [`AudioSource::read`] call produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadResult {
    /// This many samples were written to the front of the buffer (0 only when the buffer was empty).
    Samples(usize),
    /// A live source has nothing right now; try again shortly.
    Pending,
    /// A finite source is exhausted (or a live one was stopped and drained).
    End,
}

/// Where mono audio comes from. Samples are `f32` in -1.0..=1.0 at [`AudioSource::sample_rate`],
/// the device's own rate; the [`crate::Converter`] brings them to 16 kHz frames.
pub trait AudioSource: Send {
    /// Samples per second of what `read` returns.
    fn sample_rate(&self) -> u32;
    /// Begins capture or playback of the source.
    fn start(&mut self) -> Result<(), AudioError>;
    /// Ends it; samples already captured can still be read until `End`.
    fn stop(&mut self);
    /// Fills `buf` from the front. Never blocks.
    fn read(&mut self, buf: &mut [f32]) -> Result<ReadResult, AudioError>;
}

/// Reads a source until `End` (a helper for finite sources: file and synthetic).
#[cfg(test)]
pub(crate) fn drain(src: &mut dyn AudioSource) -> Vec<f32> {
    let mut all = Vec::new();
    let mut buf = [0.0f32; 700];
    loop {
        match src.read(&mut buf).unwrap() {
            ReadResult::Samples(n) => all.extend_from_slice(&buf[..n]),
            ReadResult::Pending => {}
            ReadResult::End => return all,
        }
    }
}
