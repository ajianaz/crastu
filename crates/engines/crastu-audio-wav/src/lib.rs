//! Reads WAV files (PCM 8/16/24/32-bit, float 32/64-bit, any channel count) as a
//! [`crastu_audio::AudioSource`]; used by tests and by `transcribe-file` later.

use std::path::Path;

use crastu_audio::{AudioError, AudioSource, ReadResult, downmix_interleaved, validate_rate};

/// What the header of a WAV file says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
    pub float: bool,
    /// Offset and length in bytes of the sample data (whole frames only).
    pub data_offset: usize,
    pub data_len: usize,
}

impl WavInfo {
    fn frame_bytes(&self) -> usize {
        usize::from(self.channels) * usize::from(self.bits / 8)
    }

    /// Frames (one sample per channel) in the file.
    pub fn frames(&self) -> usize {
        self.data_len / self.frame_bytes()
    }
}

fn bad(s: &str) -> AudioError {
    AudioError::Wav(s.to_string())
}

fn u16_at(b: &[u8], i: u64) -> Option<u16> {
    let i = usize::try_from(i).ok()?;
    Some(u16::from_le_bytes(
        b.get(i..i.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(b: &[u8], i: u64) -> Option<u32> {
    let i = usize::try_from(i).ok()?;
    Some(u32::from_le_bytes(
        b.get(i..i.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// The longest WAV file [`FileSource::open`] reads (256 MiB, about 70 minutes of 16-bit stereo
/// at 48 kHz). The file is held in memory; long recordings will be streamed from disk later.
pub const MAX_FILE_BYTES: u64 = 256 << 20;

/// Reads the header of a RIFF/WAVE file: PCM 8/16/24/32-bit integer or 32/64-bit float, any
/// channel count, sample rate in `1..=384000`. A data chunk that claims more bytes than the
/// file holds is an error (truncated), except the 0xFFFFFFFF a streaming writer leaves, which
/// means "to the end of the file". A data chunk of size 0 is empty. All size arithmetic is in
/// `u64`, so a hostile chunk size cannot overflow on a 32-bit target.
pub fn parse_wav(b: &[u8]) -> Result<WavInfo, AudioError> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(bad("not a RIFF/WAVE file"));
    }
    let total = b.len() as u64;
    let mut pos: u64 = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // tag, channels, rate, bits
    while pos + 8 <= total {
        let at = pos as usize; // pos + 8 <= total, so it fits
        let id = &b[at..at + 4];
        let size = u64::from(u32_at(b, pos + 4).ok_or_else(|| bad("truncated chunk header"))?);
        let body = pos + 8;
        if id == b"fmt " {
            if size < 16 || body + size > total {
                return Err(bad("bad fmt chunk"));
            }
            let mut tag = u16_at(b, body).ok_or_else(|| bad("bad fmt chunk"))?;
            if tag == 0xFFFE {
                // WAVE_FORMAT_EXTENSIBLE: the first two bytes of the sub-format GUID are the tag.
                if size < 26 {
                    return Err(bad("bad extensible fmt chunk"));
                }
                tag = u16_at(b, body + 24).ok_or_else(|| bad("bad fmt chunk"))?;
            }
            fmt = Some((
                tag,
                u16_at(b, body + 2).ok_or_else(|| bad("bad fmt chunk"))?,
                u32_at(b, body + 4).ok_or_else(|| bad("bad fmt chunk"))?,
                u16_at(b, body + 14).ok_or_else(|| bad("bad fmt chunk"))?,
            ));
        } else if id == b"data" {
            let (tag, channels, sample_rate, bits) = fmt.ok_or_else(|| bad("data before fmt"))?;
            let float = match (tag, bits) {
                (1, 8 | 16 | 24 | 32) => false,
                (3, 32 | 64) => true,
                _ => {
                    return Err(AudioError::UnsupportedFormat(format!(
                        "WAV format tag {tag}, {bits} bits"
                    )));
                }
            };
            validate_rate(sample_rate)?;
            if channels == 0 {
                return Err(bad("zero channels"));
            }
            let avail = total - body;
            let len = if size == 0xFFFF_FFFF {
                avail
            } else if size > avail {
                return Err(bad("data chunk is truncated"));
            } else {
                size
            };
            let mut info = WavInfo {
                channels,
                sample_rate,
                bits,
                float,
                data_offset: usize::try_from(body).map_err(|_| bad("file too large"))?,
                data_len: usize::try_from(len).map_err(|_| bad("file too large"))?,
            };
            info.data_len = info.frames() * info.frame_bytes();
            return Ok(info);
        }
        pos = body
            .checked_add(size + (size & 1))
            .ok_or_else(|| bad("chunk size overflow"))?;
    }
    Err(bad(if fmt.is_none() {
        "no fmt chunk"
    } else {
        "no data chunk"
    }))
}

/// Non-finite samples (a damaged float file) become silence.
fn finite(v: f32) -> f32 {
    if v.is_finite() { v } else { 0.0 }
}

fn decode(info: &WavInfo, bytes: &[u8], out: &mut Vec<f32>) {
    let w = usize::from(info.bits / 8);
    for s in bytes.chunks_exact(w) {
        out.push(match (info.float, info.bits) {
            (false, 8) => (f32::from(s[0]) - 128.0) / 128.0,
            (false, 16) => f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0,
            (false, 24) => {
                let v = i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8;
                v as f32 / 8_388_608.0
            }
            (false, _) => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
            (true, 32) => finite(f32::from_le_bytes([s[0], s[1], s[2], s[3]])),
            (true, _) => {
                finite(f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]) as f32)
            }
        });
    }
}

/// A WAV file as an [`AudioSource`]: channels are averaged to mono, samples decoded as asked.
/// The file is held in memory as bytes (about the file's size), decoded a block at a time.
#[derive(Debug)]
pub struct FileSource {
    info: WavInfo,
    bytes: Vec<u8>,
    next_frame: usize,
    started: bool,
}

impl FileSource {
    /// Reads a file of up to [`MAX_FILE_BYTES`]; a longer one is an error.
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        Self::open_with_limit(path, MAX_FILE_BYTES)
    }

    pub fn open_with_limit(path: &Path, limit: u64) -> Result<Self, AudioError> {
        use std::io::Read;
        let io = |e: std::io::Error| AudioError::Device(format!("{}: {e}", path.display()));
        let file = std::fs::File::open(path).map_err(io)?;
        // Sized from the file's length, so the read does not grow by doubling; the `take` below
        // still bounds it if the length is wrong or the file grows meanwhile.
        let len = file.metadata().map_err(io)?.len();
        if len > limit {
            return Err(AudioError::Wav(format!(
                "file is longer than {limit} bytes"
            )));
        }
        let mut bytes = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
        // One byte past the limit tells "exactly the limit" from "longer".
        file.take(limit.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(io)?;
        if bytes.len() as u64 > limit {
            return Err(AudioError::Wav(format!(
                "file is longer than {limit} bytes"
            )));
        }
        Self::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, AudioError> {
        let info = parse_wav(&bytes)?;
        Ok(FileSource {
            info,
            bytes,
            next_frame: 0,
            started: false,
        })
    }

    pub fn info(&self) -> WavInfo {
        self.info
    }
}

impl AudioSource for FileSource {
    fn sample_rate(&self) -> u32 {
        self.info.sample_rate
    }

    fn start(&mut self) -> Result<(), AudioError> {
        if self.started {
            return Err(AudioError::State("already started"));
        }
        self.started = true;
        Ok(())
    }

    fn stop(&mut self) {
        self.next_frame = self.info.frames();
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<ReadResult, AudioError> {
        if !self.started {
            return Err(AudioError::State("read before start"));
        }
        let remaining = self.info.frames() - self.next_frame;
        if remaining == 0 {
            return Ok(ReadResult::End);
        }
        let n = buf.len().min(remaining);
        if n == 0 {
            return Ok(ReadResult::Samples(0)); // an empty buffer: nothing read, not the end
        }
        let fb = self.info.frame_bytes();
        let start = self.info.data_offset + self.next_frame * fb;
        let mut raw = Vec::with_capacity(n * usize::from(self.info.channels));
        decode(&self.info, &self.bytes[start..start + n * fb], &mut raw);
        let mut mono = Vec::with_capacity(n);
        downmix_interleaved(&raw, usize::from(self.info.channels), &mut mono);
        buf[..n].copy_from_slice(&mono);
        self.next_frame += n;
        Ok(ReadResult::Samples(n))
    }
}

#[cfg(test)]
mod tests;
