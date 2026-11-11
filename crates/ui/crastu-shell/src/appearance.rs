//! The Appearance settings as choices and actions: which option is chosen for an `Appearance`,
//! and which `Action` an option stands for. Plain data, so the mapping is tested without a
//! window and the view cannot drift from the model.

use crastu_app::{Action, Appearance, Density, FontPairing, Theme};
use crastu_ui::tokens::fonts;

/// The four settings of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Setting {
    Theme,
    Font,
    Density,
    ReduceMotion,
}

pub const THEME_LABELS: [&str; 3] = ["Graphite", "Paper", "Signal"];
pub const DENSITY_LABELS: [&str; 2] = ["Compact", "Comfortable"];
pub const MOTION_LABELS: [&str; 3] = ["Follow system", "Reduce", "Full motion"];

/// The option labels of a setting, in order. The font names come from the tokens.
pub fn labels(setting: Setting) -> Vec<&'static str> {
    match setting {
        Setting::Theme => THEME_LABELS.to_vec(),
        Setting::Font => FontPairing::ALL.iter().map(|f| fonts(*f).label).collect(),
        Setting::Density => DENSITY_LABELS.to_vec(),
        Setting::ReduceMotion => MOTION_LABELS.to_vec(),
    }
}

const DENSITIES: [Density; 2] = [Density::Compact, Density::Comfortable];

/// The chosen option of a setting.
pub fn selected(setting: Setting, a: &Appearance) -> usize {
    match setting {
        Setting::Theme => Theme::ALL.iter().position(|t| *t == a.theme),
        Setting::Font => FontPairing::ALL.iter().position(|f| *f == a.font),
        Setting::Density => DENSITIES.iter().position(|d| *d == a.density),
        Setting::ReduceMotion => Some(match a.reduce_motion {
            None => 0,
            Some(true) => 1,
            Some(false) => 2,
        }),
    }
    .unwrap_or(0)
}

/// The action for choosing option `index` of a setting. None when there is no such option.
pub fn action_for(setting: Setting, index: usize) -> Option<Action> {
    match setting {
        Setting::Theme => Theme::ALL.get(index).map(|t| Action::SetTheme(*t)),
        Setting::Font => FontPairing::ALL.get(index).map(|f| Action::SetFont(*f)),
        Setting::Density => DENSITIES.get(index).map(|d| Action::SetDensity(*d)),
        Setting::ReduceMotion => match index {
            0 => Some(Action::SetReduceMotion(None)),
            1 => Some(Action::SetReduceMotion(Some(true))),
            2 => Some(Action::SetReduceMotion(Some(false))),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_option_round_trips_through_its_action() {
        for (setting, count) in [
            (Setting::Theme, 3),
            (Setting::Font, 3),
            (Setting::Density, 2),
            (Setting::ReduceMotion, 3),
        ] {
            let mut seen = Vec::new();
            for i in 0..count {
                let action = action_for(setting, i).expect("an action for every option");
                let mut a = Appearance::default();
                match action.clone() {
                    Action::SetTheme(t) => a.theme = t,
                    Action::SetFont(f) => a.font = f,
                    Action::SetDensity(d) => a.density = d,
                    Action::SetReduceMotion(r) => a.reduce_motion = r,
                    other => panic!("unexpected {other:?}"),
                }
                assert_eq!(selected(setting, &a), i, "{setting:?} {i}");
                seen.push(action);
            }
            assert!(action_for(setting, count).is_none());
            seen.dedup();
            assert_eq!(seen.len(), count);
        }
    }

    #[test]
    fn labels_follow_the_setting() {
        assert_eq!(labels(Setting::Font)[2], fonts(FontPairing::IbmPlex).label);
        assert_eq!(labels(Setting::ReduceMotion).len(), 3);
    }

    #[test]
    fn the_labels_match_the_models() {
        assert_eq!(THEME_LABELS.len(), Theme::ALL.len());
        assert_eq!(labels(Setting::Font).len(), FontPairing::ALL.len());
        assert_eq!(DENSITY_LABELS.len(), DENSITIES.len());
    }
}
