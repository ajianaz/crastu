use crastu_settings::Settings;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Colour theme. Only colour tokens change between themes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Graphite,
    Paper,
    Signal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontPairing {
    /// Inter with JetBrains Mono.
    #[default]
    Inter,
    /// Space Grotesk with JetBrains Mono.
    SpaceGrotesk,
    /// IBM Plex Sans with IBM Plex Mono.
    IbmPlex,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    #[default]
    Compact,
    Comfortable,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Appearance {
    pub theme: Theme,
    pub font: FontPairing,
    pub density: Density,
    /// `None` follows the operating system setting.
    pub reduce_motion: Option<bool>,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Graphite, Theme::Paper, Theme::Signal];
}

impl FontPairing {
    pub const ALL: [FontPairing; 3] = [
        FontPairing::Inter,
        FontPairing::SpaceGrotesk,
        FontPairing::IbmPlex,
    ];
}

impl Density {
    /// Minimum hit target in logical pixels, from the design system.
    pub fn hit_target(self) -> f32 {
        match self {
            Density::Compact => 28.0,
            Density::Comfortable => 36.0,
        }
    }
}

const THEME: &str = "appearance.theme";
const FONT: &str = "appearance.font";
const DENSITY: &str = "appearance.density";
const REDUCE_MOTION: &str = "appearance.reduce_motion";

impl Appearance {
    pub(crate) fn load(s: &Settings) -> Appearance {
        let d = Appearance::default();
        Appearance {
            theme: s.get(THEME).ok().flatten().unwrap_or(d.theme),
            font: s.get(FONT).ok().flatten().unwrap_or(d.font),
            density: s.get(DENSITY).ok().flatten().unwrap_or(d.density),
            reduce_motion: s
                .get::<Option<bool>>(REDUCE_MOTION)
                .ok()
                .flatten()
                .unwrap_or(d.reduce_motion),
        }
    }
}

fn kv<T: Serialize>(key: &str, v: T) -> (String, Value) {
    (
        key.to_string(),
        serde_json::to_value(v).unwrap_or(Value::Null),
    )
}

pub(crate) fn key_value_theme(t: Theme) -> (String, Value) {
    kv(THEME, t)
}
pub(crate) fn key_value_font(f: FontPairing) -> (String, Value) {
    kv(FONT, f)
}
pub(crate) fn key_value_density(d: Density) -> (String, Value) {
    kv(DENSITY, d)
}
pub(crate) fn key_value_reduce_motion(r: Option<bool>) -> (String, Value) {
    kv(REDUCE_MOTION, r)
}
