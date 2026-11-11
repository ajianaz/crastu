//! The token module against `docs/design/tokens.json`, value by value, in both directions.

use std::collections::BTreeMap;

use crastu_app::{Appearance, Density, FontPairing, Theme};
use serde_json::Value;

use super::*;

fn design() -> Value {
    const JSON: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../docs/design/tokens.json"
    ));
    serde_json::from_str(JSON).expect("tokens.json is valid JSON")
}

fn theme_key(theme: Theme) -> &'static str {
    match theme {
        Theme::Graphite => "graphite",
        Theme::Paper => "paper",
        Theme::Signal => "signal",
    }
}

fn font_key(font: FontPairing) -> &'static str {
    match font {
        FontPairing::Inter => "inter",
        FontPairing::SpaceGrotesk => "grotesk",
        FontPairing::IbmPlex => "plex",
    }
}

fn f(v: &Value) -> f32 {
    v.as_f64().expect("a number") as f32
}

#[test]
fn themes_and_defaults_match() {
    let d = design();
    let mut keys: Vec<&str> = d["theme"]["names"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut ours: Vec<&str> = Theme::ALL.iter().map(|t| theme_key(*t)).collect();
    ours.sort_unstable();
    assert_eq!(keys, ours);
    assert_eq!(d["theme"]["default"], theme_key(Theme::default()));
    assert_eq!(d["font"]["default"], font_key(FontPairing::default()));
    assert_eq!(d["density"]["default"], "compact");
    assert_eq!(Density::default(), Density::Compact);
}

#[test]
fn every_colour_of_every_theme_matches() {
    let d = design();
    for theme in Theme::ALL {
        let json: BTreeMap<String, String> = d["color"][theme_key(theme)]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect();
        let ours: BTreeMap<String, String> = colors(theme)
            .entries()
            .into_iter()
            .map(|(name, c)| (name.to_string(), c.css()))
            .collect();
        assert_eq!(ours.len(), 25, "{theme:?}");
        assert_eq!(ours, json, "{theme:?} differs from tokens.json");
    }
}

#[test]
fn themes_really_differ() {
    // Guards against a palette pasted twice.
    let (g, p, s) = (
        colors(Theme::Graphite),
        colors(Theme::Paper),
        colors(Theme::Signal),
    );
    assert!(g.bg != p.bg && p.bg != s.bg && g.bg != s.bg);
    assert!(g.accent != p.accent && p.accent != s.accent);
}

#[test]
fn font_pairings_match() {
    let d = design();
    let pairings = d["font"]["pairings"].as_object().unwrap();
    assert_eq!(pairings.len(), FontPairing::ALL.len());
    for pairing in FontPairing::ALL {
        let json = &pairings[font_key(pairing)];
        let ours = fonts(pairing);
        assert_eq!(json["label"], ours.label, "{pairing:?}");
        assert_eq!(json["ui"], ours.ui, "{pairing:?}");
        assert_eq!(json["mono"], ours.mono, "{pairing:?}");
    }
}

#[test]
fn type_scale_matches() {
    let d = design();
    let scale = d["type"].as_object().unwrap();
    let ours: [(&str, TextStyle); 6] = [
        ("display", typography::DISPLAY),
        ("title", typography::TITLE),
        ("body", typography::BODY),
        ("label", typography::LABEL),
        ("time", typography::TIME),
        ("score", typography::SCORE),
    ];
    assert_eq!(scale.len(), ours.len());
    for (name, style) in ours {
        let json = &scale[name];
        assert_eq!(f(&json["size"]), style.size, "{name} size");
        assert_eq!(
            json["weight"].as_u64().unwrap(),
            u64::from(style.weight),
            "{name} weight"
        );
        let mono = json["family"] == "mono";
        assert_eq!(mono, style.family == Family::Mono, "{name} family");
        assert_eq!(
            json["tabular"].as_bool().unwrap_or(false),
            style.tabular,
            "{name} tabular"
        );
        assert_eq!(json["color"].as_str(), style.color, "{name} colour");
    }
}

#[test]
fn spacing_radius_hit_targets_and_shell_match() {
    let d = design();
    let space: Vec<f32> = d["space"].as_array().unwrap().iter().map(f).collect();
    assert_eq!(space, layout::SPACE);
    let r = &d["radius"];
    assert_eq!(f(&r["control"]), layout::RADIUS_CONTROL);
    assert_eq!(f(&r["card"]), layout::RADIUS_CARD);
    assert_eq!(f(&r["media"]), layout::RADIUS_MEDIA);
    assert_eq!(f(&r["pill"]), layout::RADIUS_PILL);
    assert_eq!(r.as_object().unwrap().len(), 4);
    let h = &d["hitTarget"];
    assert_eq!(f(&h["compact"]), layout::hit_target(Density::Compact));
    assert_eq!(
        f(&h["comfortable"]),
        layout::hit_target(Density::Comfortable)
    );
    assert_eq!(f(&h["dictationPill"]), layout::HIT_TARGET_PILL);
    let s = &d["shell"];
    assert_eq!(f(&s["header"]), layout::SHELL_HEADER);
    assert_eq!(f(&s["rail"]), layout::SHELL_RAIL);
    assert_eq!(f(&s["leftSidebar"]), layout::SHELL_LEFT_SIDEBAR);
    assert_eq!(f(&s["inspector"]), layout::SHELL_INSPECTOR);
}

#[test]
fn motion_matches() {
    let d = design();
    let m = &d["motion"];
    let durations: Vec<u64> = m["durationsMs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    let ours: Vec<u64> = motion::DURATIONS_MS.iter().map(|v| u64::from(*v)).collect();
    assert_eq!(durations, ours);
    let props: Vec<&str> = m["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(props, motion::PROPERTIES);
    assert_eq!(m["respectsReduceMotion"], true);
}

#[test]
fn tokens_follow_the_appearance() {
    let a = Appearance {
        theme: Theme::Paper,
        font: FontPairing::IbmPlex,
        density: Density::Comfortable,
        reduce_motion: None,
    };
    let t = Tokens::new(&a, false);
    assert_eq!(t.colors, colors(Theme::Paper));
    assert_eq!(t.fonts.ui, "IBM Plex Sans");
    assert_eq!(t.hit_target, 36.0);
    assert_eq!(t.duration_ms(motion::SHORT_MS), 120);
    // Changing the appearance changes what a component reads, with no restart.
    let t2 = Tokens::new(
        &Appearance {
            theme: Theme::Signal,
            ..a
        },
        false,
    );
    assert_ne!(t2.colors.bg, t.colors.bg);
}

#[test]
fn reduce_motion_prefers_the_users_choice_over_the_system() {
    let mut a = Appearance::default();
    assert!(
        Tokens::new(&a, true).reduce_motion,
        "follows the system when unset"
    );
    assert_eq!(Tokens::new(&a, true).duration_ms(200), 0);
    a.reduce_motion = Some(false);
    assert!(
        !Tokens::new(&a, true).reduce_motion,
        "the user's choice wins"
    );
    assert_eq!(Tokens::new(&a, true).duration_ms(200), 200);
    a.reduce_motion = Some(true);
    assert!(Tokens::new(&a, false).reduce_motion);
}

#[test]
fn missing_families_fall_back_to_the_system_fonts() {
    let all = fonts(FontPairing::Inter);
    let installed = vec!["Inter".to_string()];
    let resolved = all.resolve(&installed);
    assert_eq!(resolved.ui, "Inter");
    assert_eq!(resolved.mono, typography::FALLBACK_MONO);
    assert_eq!(all.resolve(&[]).ui, typography::FALLBACK_UI);
    assert_eq!(all.resolve(&[]).label, "Inter");
    let both = vec!["Inter".to_string(), "JetBrains Mono".to_string()];
    assert_eq!(all.resolve(&both), all);
}
