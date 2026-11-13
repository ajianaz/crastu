//! Content-hash frame cache key: what a frame depends on, hashed with SHA-256 (already in the
//! workspace and `cargo deny` clean; BLAKE3 would add a dependency for no gain here).
//!
//! The key covers the frame size, the text layout's identity, `t` and the layers active at `t`
//! (their full data, in paint order). A layer that is not active at `t` cannot change the frame,
//! so editing it does not change the key. The frame rate and the composition duration do not
//! change a frame either and are left out. Bump [`RENDER_VERSION`] when drawing changes so old
//! cached frames are never reused.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use crate::geom::finite;
use crate::model::Composition;
use crate::text::TextLayout;

/// Part of every key; bump it when the drawing code changes what a frame looks like.
pub const RENDER_VERSION: u32 = 1;

/// A 256-bit frame key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameKey(pub [u8; 32]);

impl FrameKey {
    /// Lower-case hex, 64 characters.
    pub fn to_hex(&self) -> String {
        self.0.iter().fold(String::with_capacity(64), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }
}

fn put(h: &mut Sha256, bytes: &[u8]) {
    h.update((bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

/// The key of the frame `render_at(comp, t, layout)` would produce.
pub fn frame_key(comp: &Composition, t: f64, layout: &dyn TextLayout) -> FrameKey {
    let t = finite(t);
    let t = if t == 0.0 { 0.0 } else { t }; // -0.0 and 0.0 are the same frame
    let mut h = Sha256::new();
    put(&mut h, b"crastu-render/frame-key");
    h.update(RENDER_VERSION.to_le_bytes());
    h.update(comp.width.to_le_bytes());
    h.update(comp.height.to_le_bytes());
    put(&mut h, layout.cache_id().as_bytes());
    h.update(t.to_bits().to_le_bytes());
    for layer in comp.layers.iter().filter(|l| l.is_active(t)) {
        // serde_json writes the same text for the same value, so this is stable.
        put(&mut h, &serde_json::to_vec(layer).unwrap_or_default());
    }
    FrameKey(h.finalize().into())
}

#[cfg(test)]
mod tests;
