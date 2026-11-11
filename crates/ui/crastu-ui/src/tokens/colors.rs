//! Colour tokens: 25 colours in each of the three themes.
//!
//! The values are copied from `docs/design/tokens.json` (written from `DESIGN.md`); a test reads
//! that file and compares every value, so the two cannot drift.

use crastu_app::Theme;

/// A colour as `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(pub u32);

impl Color {
    pub const fn hex(self) -> u32 {
        self.0
    }

    /// `#RRGGBB`, as written in the design files.
    pub fn css(self) -> String {
        format!("#{:06X}", self.0)
    }

    /// For GPUI.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub fn rgb(self) -> gpui::Rgba {
        gpui::rgb(self.0)
    }
}

macro_rules! colors {
    ($($field:ident = $name:literal),* $(,)?) => {
        /// Every colour of one theme. Field names are the token names with `-` as `_`.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct Colors {
            $(pub $field: Color,)*
        }

        impl Colors {
            /// Each colour with its token name, in the order of the design tables.
            pub fn entries(&self) -> Vec<(&'static str, Color)> {
                vec![$(($name, self.$field)),*]
            }
        }
    };
}

colors! {
    bg = "bg",
    surface = "surface",
    raised = "raised",
    border = "border",
    text = "text",
    text_2 = "text-2",
    text_3 = "text-3",
    accent = "accent",
    accent_hover = "accent-hover",
    accent_press = "accent-press",
    on_accent = "on-accent",
    accent_soft = "accent-soft",
    record = "record",
    danger = "danger",
    danger_soft = "danger-soft",
    on_danger = "on-danger",
    warn = "warn",
    warn_soft = "warn-soft",
    media_bg = "media-bg",
    media_icon = "media-icon",
    wave = "wave",
    track_video = "track-video",
    track_audio = "track-audio",
    track_caption = "track-caption",
    on_track = "on-track",
}

const GRAPHITE: Colors = Colors {
    bg: Color(0x111214),
    surface: Color(0x18191C),
    raised: Color(0x202226),
    border: Color(0x2C2E33),
    text: Color(0xECEDEE),
    text_2: Color(0xA4A7AE),
    text_3: Color(0x868A93),
    accent: Color(0x4CC2B0),
    accent_hover: Color(0x63CDBD),
    accent_press: Color(0x3BA897),
    on_accent: Color(0x0B1F1C),
    accent_soft: Color(0x1E3B37),
    record: Color(0xFF5A4E),
    danger: Color(0xF0706A),
    danger_soft: Color(0x3D1C1A),
    on_danger: Color(0x2A0B09),
    warn: Color(0xE6B450),
    warn_soft: Color(0x3A2E14),
    media_bg: Color(0x26282C),
    media_icon: Color(0x5C6068),
    wave: Color(0x3A3D43),
    track_video: Color(0x5B7FD6),
    track_audio: Color(0x4CC2B0),
    track_caption: Color(0xC89B4C),
    on_track: Color(0x0E1013),
};

const PAPER: Colors = Colors {
    bg: Color(0xF6F5F2),
    surface: Color(0xFFFFFF),
    raised: Color(0xEFEDE8),
    border: Color(0xE0DDD6),
    text: Color(0x1B1A17),
    text_2: Color(0x57544D),
    text_3: Color(0x6E6A62),
    accent: Color(0x2F5BD3),
    accent_hover: Color(0x2850BC),
    accent_press: Color(0x2246A6),
    on_accent: Color(0xFFFFFF),
    accent_soft: Color(0xDCE5FA),
    record: Color(0xD93A2B),
    danger: Color(0xB42318),
    danger_soft: Color(0xFCE4E1),
    on_danger: Color(0xFFFFFF),
    warn: Color(0x9A6700),
    warn_soft: Color(0xFBEFD5),
    media_bg: Color(0xE7E4DD),
    media_icon: Color(0xA29E95),
    wave: Color(0xCFCBC2),
    track_video: Color(0x3E63C8),
    track_audio: Color(0x1F8A78),
    track_caption: Color(0x9A6A1A),
    on_track: Color(0xFFFFFF),
};

const SIGNAL: Colors = Colors {
    bg: Color(0x0A0A0B),
    surface: Color(0x121214),
    raised: Color(0x19191C),
    border: Color(0x26262A),
    text: Color(0xF2F2F0),
    text_2: Color(0xA8A8A2),
    text_3: Color(0x8A8A84),
    accent: Color(0xC6F432),
    accent_hover: Color(0xD3F75E),
    accent_press: Color(0xB0DB22),
    on_accent: Color(0x0A0A0B),
    accent_soft: Color(0x2E3A10),
    record: Color(0xFF4D3D),
    danger: Color(0xFF6B5E),
    danger_soft: Color(0x3D1512),
    on_danger: Color(0x1A0505),
    warn: Color(0xFFC94D),
    warn_soft: Color(0x3A2F10),
    media_bg: Color(0x1E1E21),
    media_icon: Color(0x4A4A4E),
    wave: Color(0x34343A),
    track_video: Color(0x7FA2FF),
    track_audio: Color(0x5BE3C9),
    track_caption: Color(0xFFC94D),
    on_track: Color(0x0A0A0B),
};

/// The colours of a theme.
pub fn colors(theme: Theme) -> &'static Colors {
    match theme {
        Theme::Graphite => &GRAPHITE,
        Theme::Paper => &PAPER,
        Theme::Signal => &SIGNAL,
    }
}
