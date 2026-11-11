//! The base controls of the interface: buttons, text fields, select, segmented control,
//! checkbox, switch, sliders and tabs.
//!
//! The logic is plain data and builds everywhere: how a control looks in each state ([`look`]),
//! keyboard movement ([`nav`]), slider values ([`value`]) and the text of a field ([`text`]).
//! The GPUI views (macOS and Windows) only draw that data. Every colour, size and font comes
//! from `crastu-ui`'s tokens; `cargo xtask ui-literals` checks it.

pub mod choice;
pub mod look;
pub mod nav;
pub mod text;
pub mod value;

pub use choice::{Choice, SelectEvent, SelectModel};
pub use look::{FieldState, State, ToggleState, Variant};
pub use nav::Move;
pub use text::TextModel;
pub use value::{Nudge, RangeSlider, Slider, Thumb};
