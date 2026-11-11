//! The GPUI views of the base controls. Each one draws the state and look of `crastu-controls`
//! with the tokens and holds no state of its own: the owner keeps the model and handles the
//! change callback. The crate is empty where GPUI is not a dependency (anything but macOS and
//! Windows for now).

#![cfg(any(target_os = "macos", target_os = "windows"))]
#![allow(
    missing_debug_implementations,
    reason = "the views hold callbacks, which cannot be Debug"
)]

mod button;
mod choices;
mod common;
mod field;
mod select;
mod slider;
mod toggles;

pub use button::Button;
pub use choices::{ChoiceKind, Choices};
pub use common::{Spinner, label_text};
pub use field::TextField;
pub use select::SelectView;
pub use slider::{RangeSliderView, SliderView};
pub use toggles::{Checkbox, Switch};
