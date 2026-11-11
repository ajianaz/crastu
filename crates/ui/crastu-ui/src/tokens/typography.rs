//! Font pairings and the type scale.

use crastu_app::FontPairing;

/// The two families of a pairing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontFamilies {
    pub label: &'static str,
    pub ui: &'static str,
    pub mono: &'static str,
}

/// What is used when a bundled family is not installed yet (until the fonts ship with the app).
pub const FALLBACK_UI: &str = if cfg!(target_os = "windows") {
    "Segoe UI"
} else {
    "Helvetica Neue"
};
pub const FALLBACK_MONO: &str = if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "Menlo"
};

impl FontFamilies {
    /// The same pairing with each family that is not in `installed` replaced by the system
    /// fallback, so text always draws. `installed` is the font list of the text system.
    pub fn resolve(self, installed: &[String]) -> FontFamilies {
        let has = |name: &str| installed.iter().any(|f| f == name);
        FontFamilies {
            label: self.label,
            ui: if has(self.ui) { self.ui } else { FALLBACK_UI },
            mono: if has(self.mono) {
                self.mono
            } else {
                FALLBACK_MONO
            },
        }
    }
}

/// The families of a pairing. They are bundled with the app, never taken from the system.
pub fn fonts(pairing: FontPairing) -> FontFamilies {
    match pairing {
        FontPairing::Inter => FontFamilies {
            label: "Inter",
            ui: "Inter",
            mono: "JetBrains Mono",
        },
        FontPairing::SpaceGrotesk => FontFamilies {
            label: "Space Grotesk",
            ui: "Space Grotesk",
            mono: "JetBrains Mono",
        },
        FontPairing::IbmPlex => FontFamilies {
            label: "IBM Plex",
            ui: "IBM Plex Sans",
            mono: "IBM Plex Mono",
        },
    }
}

/// Which family a style uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Ui,
    Mono,
}

/// One step of the type scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: u16,
    pub family: Family,
    /// Digits of equal width, so changing numbers do not shift the layout.
    pub tabular: bool,
    /// The colour token this style uses when it is not the default text colour.
    pub color: Option<&'static str>,
}

const fn style(size: f32, weight: u16, family: Family, tabular: bool) -> TextStyle {
    TextStyle {
        size,
        weight,
        family,
        tabular,
        color: None,
    }
}

pub const DISPLAY: TextStyle = style(24.0, 600, Family::Ui, false);
pub const TITLE: TextStyle = style(16.0, 600, Family::Ui, false);
pub const BODY: TextStyle = style(13.0, 400, Family::Ui, false);
pub const LABEL: TextStyle = TextStyle {
    color: Some("text-2"),
    ..style(12.0, 500, Family::Ui, false)
};
pub const TIME: TextStyle = style(13.0, 400, Family::Mono, true);
pub const SCORE: TextStyle = style(20.0, 500, Family::Mono, true);
