//! Where PCM comes from. Generated audio (a TTS take, an effect) is rendered once, kept in a
//! [`PcmCache`] and enters the mixer as an ordinary [`PcmSource`].

use std::collections::HashMap;
use std::sync::Arc;

/// A finite piece of PCM that can be read at any frame, in any order.
///
/// All sources of one mixer are at the mixer's sample rate; resampling is the caller's job.
/// `read` is called from the audio callback: it must not allocate, lock or block, and the result
/// must depend only on `frame`, not on earlier calls (that is what makes seeking and block-size
/// independence work).
pub trait PcmSource: Send {
    /// 1 (mono, played on both sides) or 2 (interleaved left, right).
    fn channels(&self) -> usize;
    /// Length in frames.
    fn len_frames(&self) -> u64;
    /// Fills `out` with interleaved frames starting at `frame`; `out.len()` is a multiple of
    /// `channels()`. Frames past the end are written as zeros.
    fn read(&mut self, frame: u64, out: &mut [f32]);
}

/// PCM held in memory, shared cheaply (a cached take is an `Arc`).
#[derive(Clone, Debug)]
pub struct MemorySource {
    channels: usize,
    data: Arc<[f32]>,
}

impl MemorySource {
    /// `data` is interleaved; `channels` must be 1 or 2 (anything else is treated as 2) and a
    /// trailing partial frame is ignored.
    #[must_use]
    pub fn new(channels: usize, data: Arc<[f32]>) -> Self {
        let channels = if channels == 1 { 1 } else { 2 };
        Self { channels, data }
    }
}

impl PcmSource for MemorySource {
    fn channels(&self) -> usize {
        self.channels
    }

    fn len_frames(&self) -> u64 {
        (self.data.len() / self.channels) as u64
    }

    fn read(&mut self, frame: u64, out: &mut [f32]) {
        let start = usize::try_from(frame)
            .ok()
            .and_then(|f| f.checked_mul(self.channels));
        let avail = start.map_or(&[][..], |s| self.data.get(s..).unwrap_or(&[]));
        let whole = avail.len() - avail.len() % self.channels;
        let n = whole.min(out.len());
        out[..n].copy_from_slice(&avail[..n]);
        out[n..].fill(0.0);
    }
}

/// What a generated take depends on, hashed: the text, the voice, the model revision, the speed.
/// FNV-1a over length-prefixed parts, so `["ab", "c"]` and `["a", "bc"]` differ. Not a security
/// hash: it names a cache entry, nothing more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PcmKey(pub u64);

impl PcmKey {
    /// Hashes the parts in order.
    #[must_use]
    pub fn from_parts(parts: &[&[u8]]) -> Self {
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for b in bytes {
                h = (h ^ u64::from(*b)).wrapping_mul(PRIME);
            }
        };
        for p in parts {
            feed(&(p.len() as u64).to_le_bytes());
            feed(p);
        }
        Self(h)
    }
}

/// Rendered takes by key. Rendering happens once per key; a hit hands out the same buffer.
#[derive(Debug, Default)]
pub struct PcmCache {
    map: HashMap<PcmKey, Arc<[f32]>>,
}

impl PcmCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The take for `key`, rendering it with `render` only on a miss.
    pub fn get_or_render(&mut self, key: PcmKey, render: impl FnOnce() -> Vec<f32>) -> Arc<[f32]> {
        Arc::clone(self.map.entry(key).or_insert_with(|| render().into()))
    }

    /// Number of cached takes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when nothing is cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_source_zero_fills_past_the_end() {
        let mut s = MemorySource::new(2, vec![1.0, 2.0, 3.0, 4.0].into());
        let mut out = [9.0; 6];
        s.read(1, &mut out);
        assert_eq!(out, [3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
        s.read(u64::MAX, &mut out);
        assert_eq!(out, [0.0; 6]);
        assert_eq!(s.len_frames(), 2);
        let mut part = MemorySource::new(2, vec![1.0, 2.0, 3.0].into());
        let mut out = [9.0; 4];
        part.read(0, &mut out);
        assert_eq!(
            out,
            [1.0, 2.0, 0.0, 0.0],
            "a trailing partial frame is not played"
        );
    }

    #[test]
    fn cache_renders_once_per_key() {
        let mut c = PcmCache::new();
        let k = PcmKey::from_parts(&[b"hello", b"voice-1"]);
        let mut renders = 0;
        let a = c.get_or_render(k, || {
            renders += 1;
            vec![0.5; 4]
        });
        let b = c.get_or_render(k, || {
            renders += 1;
            vec![0.0; 4]
        });
        assert_eq!(renders, 1);
        assert!(Arc::ptr_eq(&a, &b));
        assert_ne!(
            PcmKey::from_parts(&[b"ab", b"c"]),
            PcmKey::from_parts(&[b"a", b"bc"])
        );
        assert_eq!(c.len(), 1);
    }
}
