//! Deterministic motion and caption composition (issue #35, slice 1).
//!
//! A [`Composition`] is plain data. [`render_at`] turns it and a time in seconds into a [`Frame`]
//! (an ordered display list), and [`render_into`] draws the same thing through any [`Painter`].
//! There is no wall clock and no state between calls: the same inputs give the same frame in any
//! order, and seeking to a time is the same as playing up to it. Motion ([`motion`]) is closed
//! form in `t`, never stepped. GPU drawing (Vello, Parley) is slice 2 and not here.

mod caption;
mod geom;
pub mod key;
pub mod model;
mod painter;
mod render;
pub mod soft;
pub mod style;
pub mod text;

pub use crastu_motion as motion;
pub use geom::{Rect, Rgba};
pub use key::{FrameKey, RENDER_VERSION, frame_key};
pub use model::{CaptionLayer, Composition, Layer, LayerKind, ModelError, TitleLayer};
pub use painter::{AssetKind, DisplayList, Painter, Primitive, TextRun};
pub use render::{Frame, frame_time, render_at, render_into};
pub use soft::SoftFrame;
pub use style::{CaptionStyle, Highlight, Karaoke, PopIn, STYLE_VERSION, StyleError};
pub use text::{FixedAdvance, Line, TextLayout, TextMetrics, WordTiming};
