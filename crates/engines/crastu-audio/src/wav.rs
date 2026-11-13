use std::path::Path;

use crate::convert::downmix_interleaved;
use crate::error::AudioError;
use crate::source::{AudioSource, ReadResult};

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

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// Reads the header of a RIFF/WAVE file: PCM 8/16/24/32-bit integer or 32/64-bit float, any
/// channel count. A data chunk that claims more bytes than the file holds is an error
/// (truncated), except the 0 or 0xFFFFFFFF a streaming writer leaves, which means "to the end".
pub fn parse_wav(b: &[u8]) -> Result<WavInfo, AudioError> {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(bad("not a RIFF/WAVE file"));
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // tag, channels, rate, bits
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let size = u32_at(b, pos + 4).ok_or_else(|| bad("truncated chunk header"))? as usize;
        let body = pos + 8;
        if id == b"fmt " {
            if size < 16 || body + size > b.len() {
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
            if channels == 0 || sample_rate == 0 {
                return Err(bad("zero channels or sample rate"));
            }
            let avail = b.len() - body;
            let mut len = size;
            if size == 0 || size == 0xFFFF_FFFF {
                len = avail;
            } else if size > avail {
                return Err(bad("data chunk is truncated"));
            }
            let mut info = WavInfo {
                channels,
                sample_rate,
                bits,
                float,
                data_offset: body,
                data_len: len,
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
            (true, 32) => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
            (true, _) => {
                f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]) as f32
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
    pub fn open(path: &Path) -> Result<Self, AudioError> {
        let bytes = std::fs::read(path)
            .map_err(|e| AudioError::Device(format!("{}: {e}", path.display())))?;
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
        let n = buf.len().min(self.info.frames() - self.next_frame);
        if n == 0 {
            return Ok(ReadResult::End);
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
mod tests {
    use super::*;
    use crate::source::drain;

    /// Builds a WAV file in memory. `samples` are raw little-endian sample bytes.
    fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        v.extend_from_slice(b"WAVEfmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&tag.to_le_bytes());
        v.extend_from_slice(&channels.to_le_bytes());
        v.extend_from_slice(&rate.to_le_bytes());
        let block = channels * bits / 8;
        v.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
        v.extend_from_slice(&block.to_le_bytes());
        v.extend_from_slice(&bits.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&(data.len() as u32).to_le_bytes());
        v.extend_from_slice(data);
        v
    }

    fn pcm16(xs: &[i16]) -> Vec<u8> {
        xs.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn read_all(bytes: Vec<u8>) -> Vec<f32> {
        let mut s = FileSource::from_bytes(bytes).unwrap();
        s.start().unwrap();
        drain(&mut s)
    }

    #[test]
    fn pcm16_mono() {
        let f = wav(1, 1, 16_000, 16, &pcm16(&[0, 16384, -16384, -32768]));
        let info = parse_wav(&f).unwrap();
        assert_eq!(
            (info.channels, info.sample_rate, info.frames()),
            (1, 16_000, 4)
        );
        assert_eq!(read_all(f), vec![0.0, 0.5, -0.5, -1.0]);
    }

    #[test]
    fn pcm16_stereo_is_averaged() {
        let f = wav(1, 2, 44_100, 16, &pcm16(&[16384, 0, 16384, -16384]));
        let s = FileSource::from_bytes(f.clone()).unwrap();
        assert_eq!(s.sample_rate(), 44_100);
        assert_eq!(read_all(f), vec![0.25, 0.0]);
    }

    #[test]
    fn float32_and_24bit_and_8bit() {
        let fl: Vec<u8> = [0.25f32, -0.5]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        assert_eq!(read_all(wav(3, 1, 8000, 32, &fl)), vec![0.25, -0.5]);
        // 24-bit: 0x400000 = 0.5, 0xC00000 (negative) = -0.5
        let d24 = [0x00, 0x00, 0x40, 0x00, 0x00, 0xC0];
        assert_eq!(read_all(wav(1, 1, 8000, 24, &d24)), vec![0.5, -0.5]);
        assert_eq!(
            read_all(wav(1, 1, 8000, 8, &[128, 192, 0])),
            vec![0.0, 0.5, -1.0]
        );
    }

    #[test]
    fn small_reads_cover_the_file_once() {
        let f = wav(
            1,
            1,
            8000,
            16,
            &pcm16(&(0..2000).map(|i| i as i16).collect::<Vec<_>>()),
        );
        let mut s = FileSource::from_bytes(f).unwrap();
        s.start().unwrap();
        let x = drain(&mut s); // drain reads 700 at a time
        assert_eq!(x.len(), 2000);
        assert_eq!(x[1999], 1999.0 / 32768.0);
    }

    #[test]
    fn extra_chunks_and_odd_padding_are_skipped() {
        let mut f = wav(1, 1, 8000, 16, &pcm16(&[8192]));
        let mut g = f[..12].to_vec();
        g.extend_from_slice(b"LIST");
        g.extend_from_slice(&3u32.to_le_bytes());
        g.extend_from_slice(&[1, 2, 3, 0]); // 3 bytes + 1 pad
        g.extend_from_slice(&f.split_off(12));
        assert_eq!(read_all(g), vec![0.25]);
    }

    #[test]
    fn streaming_header_sizes_mean_to_the_end() {
        let mut f = wav(1, 1, 8000, 16, &pcm16(&[8192, 8192]));
        let at = f.len() - 4 - 4;
        f[at..at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(read_all(f).len(), 2);
    }

    #[test]
    fn errors() {
        assert!(matches!(parse_wav(b"nope"), Err(AudioError::Wav(_))));
        assert!(matches!(
            parse_wav(b"RIFF\0\0\0\0AVI LIST"),
            Err(AudioError::Wav(_))
        ));
        let good = wav(1, 1, 8000, 16, &pcm16(&[1, 2, 3, 4]));
        // Cut in the middle of the data: the header still claims 8 bytes.
        let e = parse_wav(&good[..good.len() - 3]).unwrap_err();
        assert_eq!(e, AudioError::Wav("data chunk is truncated".into()));
        // Cut before the data chunk.
        assert!(matches!(parse_wav(&good[..36]), Err(AudioError::Wav(_))));
        assert!(matches!(parse_wav(&good[..20]), Err(AudioError::Wav(_))));
        // 12-bit PCM and ADPCM are not read.
        assert!(matches!(
            parse_wav(&wav(1, 1, 8000, 12, &[0; 4])),
            Err(AudioError::UnsupportedFormat(_))
        ));
        assert!(matches!(
            parse_wav(&wav(2, 1, 8000, 4, &[0; 4])),
            Err(AudioError::UnsupportedFormat(_))
        ));
        // Zero channels.
        assert!(matches!(
            parse_wav(&wav(1, 0, 8000, 16, &[0; 4])),
            Err(AudioError::Wav(_))
        ));
        assert!(FileSource::open(Path::new("/nonexistent/x.wav")).is_err());
    }

    #[test]
    fn a_partial_last_frame_is_ignored() {
        let f = wav(1, 2, 8000, 16, &[0, 0, 0, 0, 0, 0]); // 1.5 stereo frames
        assert_eq!(parse_wav(&f).unwrap().frames(), 1);
    }
}
