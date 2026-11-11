//! How each control looks in each state, as colours from the tokens. Pure data: the views only
//! apply it, so every state of every theme is tested without opening a window.

use crastu_ui::tokens::{Color, Colors};

/// The states a control is documented in. Toggles and fields add their own (selected, error).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Default,
    Hover,
    Pressed,
    /// Keyboard focus: the ring shows.
    Focus,
    Disabled,
    Loading,
}

impl State {
    pub const ALL: [State; 6] = [
        State::Default,
        State::Hover,
        State::Pressed,
        State::Focus,
        State::Disabled,
        State::Loading,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

impl Variant {
    pub const ALL: [Variant; 4] = [
        Variant::Primary,
        Variant::Secondary,
        Variant::Ghost,
        Variant::Danger,
    ];
}

/// What to paint for one control in one state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    pub bg: Option<Color>,
    pub fg: Color,
    pub border: Option<Color>,
    /// The focus ring.
    pub ring: Option<Color>,
    /// Painted at reduced opacity (disabled).
    pub dim: bool,
}

/// The share of the opacity kept by a disabled control, in percent.
pub const DIM_PERCENT: u32 = 45;

/// A colour part of the way from `a` to `b` (`t` in percent): the hover and pressed shades of
/// colours that have no token of their own.
pub fn mix(a: Color, b: Color, t: u32) -> Color {
    let t = t.min(100);
    let channel = |shift: u32| {
        let (x, y) = ((a.0 >> shift) & 0xFF, (b.0 >> shift) & 0xFF);
        (x * (100 - t) + y * t + 50) / 100
    };
    Color((channel(16) << 16) | (channel(8) << 8) | channel(0))
}

pub fn button(variant: Variant, state: State, c: &Colors) -> Look {
    let (bg, fg, hover, pressed) = match variant {
        Variant::Primary => (
            Some(c.accent),
            c.on_accent,
            Some(c.accent_hover),
            Some(c.accent_press),
        ),
        Variant::Secondary => (Some(c.raised), c.text, Some(c.border), Some(c.surface)),
        Variant::Ghost => (None, c.text, Some(c.raised), Some(c.border)),
        Variant::Danger => (
            Some(c.danger),
            c.on_danger,
            Some(mix(c.danger, c.text, 15)),
            Some(mix(c.danger, c.bg, 20)),
        ),
    };
    let bg = match state {
        State::Hover => hover,
        State::Pressed => pressed,
        _ => bg,
    };
    Look {
        bg,
        fg,
        border: (variant == Variant::Secondary).then_some(c.border),
        ring: (state == State::Focus).then_some(c.accent),
        dim: state == State::Disabled,
    }
}

/// Field states. A field shows an error with a red edge and, under it, red help text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldState {
    Default,
    Focus,
    Error,
    Disabled,
}

impl FieldState {
    pub const ALL: [FieldState; 4] = [
        FieldState::Default,
        FieldState::Focus,
        FieldState::Error,
        FieldState::Disabled,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldLook {
    pub bg: Color,
    pub fg: Color,
    pub placeholder: Color,
    pub border: Color,
    pub ring: Option<Color>,
    pub help: Color,
    pub dim: bool,
}

pub fn field(state: FieldState, c: &Colors) -> FieldLook {
    FieldLook {
        bg: if state == FieldState::Disabled {
            c.surface
        } else {
            c.bg
        },
        fg: c.text,
        placeholder: c.text_3,
        border: match state {
            FieldState::Focus => c.accent,
            FieldState::Error => c.danger,
            _ => c.border,
        },
        ring: (state == FieldState::Focus).then_some(c.accent),
        help: if state == FieldState::Error {
            c.danger
        } else {
            c.text_2
        },
        dim: state == FieldState::Disabled,
    }
}

/// Checkbox, switch, segment and tab: on or off, plus focus and disabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleState {
    pub on: bool,
    pub focus: bool,
    pub disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToggleLook {
    pub fill: Option<Color>,
    pub edge: Color,
    /// The check mark or the switch thumb.
    pub mark: Color,
    pub ring: Option<Color>,
    pub dim: bool,
}

pub fn checkbox(s: ToggleState, c: &Colors) -> ToggleLook {
    ToggleLook {
        fill: s.on.then_some(c.accent),
        edge: if s.on { c.accent } else { c.text_3 },
        mark: c.on_accent,
        ring: (s.focus && !s.disabled).then_some(c.accent),
        dim: s.disabled,
    }
}

pub fn switch(s: ToggleState, c: &Colors) -> ToggleLook {
    ToggleLook {
        fill: Some(if s.on { c.accent } else { c.raised }),
        edge: if s.on { c.accent } else { c.border },
        mark: if s.on { c.on_accent } else { c.text_2 },
        ring: (s.focus && !s.disabled).then_some(c.accent),
        dim: s.disabled,
    }
}

/// A segment of the segmented control: the chosen one sits on a raised chip.
pub fn segment(s: ToggleState, c: &Colors) -> Look {
    Look {
        bg: s.on.then_some(c.raised),
        fg: if s.on { c.text } else { c.text_2 },
        border: None,
        ring: (s.focus && !s.disabled).then_some(c.accent),
        dim: s.disabled,
    }
}

/// A tab: the chosen one has an accent line under it.
pub fn tab(s: ToggleState, c: &Colors) -> Look {
    Look {
        bg: None,
        fg: if s.on { c.text } else { c.text_2 },
        border: s.on.then_some(c.accent),
        ring: (s.focus && !s.disabled).then_some(c.accent),
        dim: s.disabled,
    }
}

/// A row of the select list.
pub fn option_row(selected: bool, highlighted: bool, c: &Colors) -> Look {
    Look {
        bg: highlighted.then_some(c.raised),
        fg: if selected { c.accent } else { c.text },
        border: None,
        ring: None,
        dim: false,
    }
}

#[cfg(test)]
mod tests;
