//! The base controls of the interface: buttons, text fields, select, segmented control,
//! checkbox, switch, sliders and tabs.
//!
//! The logic is plain data and builds everywhere: how a control looks in each state ([`look`]),
//! keyboard movement ([`nav`]), slider values ([`value`]) and the text of a field ([`text`]).
//! The GPUI views (macOS and Windows) only draw that data. Every colour, size and font comes
//! from `crastu-ui`'s tokens; `cargo xtask ui-literals` checks it.

pub mod badge;
pub mod choice;
pub mod dialog;
pub mod look;
pub mod nav;
pub mod text;
pub mod toast;
pub mod value;

pub use badge::Badge;
pub use choice::{Choice, ChoiceInput, SelectEvent, SelectInput, SelectModel};
pub use dialog::{DialogEffect, DialogInput, DialogModel};
pub use look::{FieldState, State, ToggleState, Variant};
pub use nav::Move;
pub use text::{Edit, TextModel};
pub use toast::{Toast, ToastKind, Toasts};
pub use value::{Nudge, RangeInput, RangeSlider, Slider, SliderInput, Thumb};
