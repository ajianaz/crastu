//! What the onboarding screens show for a `crastu_app::Onboarding`, as plain data: the step list
//! with its marks, the permission rows and their buttons, the model table, the shortcut presets.
//! The GPUI view only draws it, and the rules (what can be reached, when Continue is on) stay in
//! `crastu-app`.

use crastu_app::{Onboarding, Permission, PermissionState, Step};
use crastu_controls::Badge;

/// How a step looks in the list on the left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMark {
    /// Before the current step: a check mark.
    Done,
    Current,
    Upcoming,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepRow {
    pub step: Step,
    /// 1 to 6.
    pub number: usize,
    pub mark: StepMark,
    /// A step can be jumped to when `Onboarding::reachable` says so.
    pub reachable: bool,
}

pub fn step_rows(ob: &Onboarding) -> Vec<StepRow> {
    let current = Step::ALL.iter().position(|s| *s == ob.step).unwrap_or(0);
    Step::ALL
        .iter()
        .enumerate()
        .map(|(i, step)| StepRow {
            step: *step,
            number: i + 1,
            mark: match i.cmp(&current) {
                std::cmp::Ordering::Less => StepMark::Done,
                std::cmp::Ordering::Equal => StepMark::Current,
                std::cmp::Ordering::Greater => StepMark::Upcoming,
            },
            reachable: ob.reachable(*step),
        })
        .collect()
}

pub fn progress(ob: &Onboarding) -> String {
    let n = Step::ALL.iter().position(|s| *s == ob.step).unwrap_or(0) + 1;
    format!("Step {n} of {}", Step::ALL.len())
}

/// The button of a permission row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionButton {
    /// Granted: a check mark and the word Allowed, no button.
    Allowed,
    /// Not asked yet: the system prompt can still appear.
    Allow,
    /// Denied: macOS asks only once, so the way back is System Settings.
    OpenSystemSettings,
}

pub fn permission_button(state: PermissionState) -> PermissionButton {
    match state {
        PermissionState::Granted => PermissionButton::Allowed,
        PermissionState::Denied => PermissionButton::OpenSystemSettings,
        PermissionState::Unknown => PermissionButton::Allow,
    }
}

pub fn permission_name(p: Permission) -> &'static str {
    match p {
        Permission::Microphone => "Microphone",
        Permission::Accessibility => "Accessibility",
        Permission::InputMonitoring => "Input Monitoring",
        Permission::ScreenAndSystemAudio => "Screen and System Audio Recording",
    }
}

/// The permission states the platform reports that differ from the ones in the model: what to
/// tell the model after a poll. `Unknown` from the platform means "not decided", so it is left
/// alone (a request that is still open does not reset a denial) except for a permission the
/// model has as Granted: that one was taken away (for example in System Settings), and the user
/// must not pass the step on a stale Granted.
pub fn permission_changes(
    ob: &Onboarding,
    observed: impl Fn(Permission) -> PermissionState,
) -> Vec<(Permission, PermissionState)> {
    Permission::ALL
        .into_iter()
        .filter_map(|p| {
            let now = observed(p);
            let before = ob.permission(p);
            let changed = now != before
                && (now != PermissionState::Unknown || before == PermissionState::Granted);
            changed.then_some((p, now))
        })
        .collect()
}

/// One row of the model table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRow {
    pub id: String,
    pub name: &'static str,
    pub note: &'static str,
    pub used_for: &'static str,
    pub licence: Badge,
    pub selected: bool,
    pub required: bool,
}

/// The display data of the models offered in onboarding. The licences are those of
/// `docs/ARCHITECTURE.md` (Whisper, Silero VAD and YuNet are MIT; Supertonic 3 is OpenRAIL-M, so
/// Conditional); this table moves into the model catalogue when `crastu-models` has one. An id
/// that is not known is shown under its own id with the most careful badge.
fn model_display(id: &str) -> (&'static str, &'static str, &'static str, Badge) {
    match id {
        "whisper-small" => (
            "Whisper Small",
            "recommended",
            "Dictation and captions",
            Badge::CommercialOk,
        ),
        "silero-vad" => (
            "Silero VAD",
            "required",
            "Finding speech",
            Badge::CommercialOk,
        ),
        "supertonic-3" => ("Supertonic 3", "optional", "Voiceover", Badge::Conditional),
        "yunet" => (
            "YuNet",
            "optional",
            "Faces for reframing",
            Badge::CommercialOk,
        ),
        _ => ("Unknown model", "optional", "", Badge::Conditional),
    }
}

pub fn model_rows(ob: &Onboarding) -> Vec<ModelRow> {
    ob.models
        .iter()
        .map(|m| {
            let (name, note, used_for, licence) = model_display(&m.id);
            ModelRow {
                id: m.id.clone(),
                name,
                note: if m.required { "required" } else { note },
                used_for,
                licence,
                selected: m.selected,
                required: m.required,
            }
        })
        .collect()
}

/// Shortcuts offered in the shortcut step; custom keys come with Settings > Shortcuts.
pub const SHORTCUT_PRESETS: [&str; 3] = ["Option+Space", "Control+Space", "Command+Shift+Space"];

/// The index of the chosen preset; None when the saved shortcut is a custom one.
pub fn shortcut_index(ob: &Onboarding) -> Option<usize> {
    SHORTCUT_PRESETS.iter().position(|s| *s == ob.shortcut)
}

/// The two single-choice controls of the shortcut step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutChoice {
    Preset,
    HoldMode,
}

/// The chosen option of a shortcut-step control for the current onboarding state.
pub fn choice_selected(ob: &Onboarding, which: ShortcutChoice) -> usize {
    match which {
        ShortcutChoice::Preset => shortcut_index(ob).unwrap_or(0),
        ShortcutChoice::HoldMode => usize::from(!ob.hold_to_talk),
    }
}

pub fn choice_count(which: ShortcutChoice) -> usize {
    match which {
        ShortcutChoice::Preset => SHORTCUT_PRESETS.len(),
        ShortcutChoice::HoldMode => HOLD_LABELS.len(),
    }
}

/// The action for choosing option `index`. None when there is no such option.
pub fn choice_action(which: ShortcutChoice, index: usize) -> Option<crastu_app::OnboardingAction> {
    use crastu_app::OnboardingAction as A;
    match which {
        ShortcutChoice::Preset => SHORTCUT_PRESETS
            .get(index)
            .map(|p| A::SetShortcut((*p).to_string())),
        ShortcutChoice::HoldMode => {
            (index < HOLD_LABELS.len()).then_some(A::SetHoldToTalk(index == 0))
        }
    }
}

/// A shortcut as shown on a key chip: `Option+Space` becomes `⌥ Space`.
pub fn shortcut_label(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(|k| match k {
            "Option" => "\u{2325}",
            "Control" => "\u{2303}",
            "Command" => "\u{2318}",
            "Shift" => "\u{21E7}",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The sentence of the Done step around the key chip: hold to talk, or tap to start and stop.
pub fn try_it(ob: &Onboarding) -> (&'static str, &'static str) {
    if ob.hold_to_talk {
        ("Try it now: hold", "in any app and say a sentence.")
    } else {
        (
            "Try it now: tap",
            "in any app, say a sentence, and tap it again to stop.",
        )
    }
}

/// The command that adds Crastu to Claude Code, as in the design.
pub const MCP_COMMAND: &str = "claude mcp add --transport stdio crastu -- crastu mcp";

pub const HOLD_LABELS: [&str; 2] = [
    "Hold to talk, release to paste",
    "Tap to start, tap again to stop",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crastu_app::{Action, App, OnboardingAction, Settings};

    #[test]
    fn the_step_list_marks_done_current_and_upcoming() {
        let mut ob = Onboarding::new();
        ob.step = Step::Models;
        let rows = step_rows(&ob);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].mark, StepMark::Done);
        assert_eq!(rows[2].mark, StepMark::Current);
        assert_eq!(rows[3].mark, StepMark::Upcoming);
        assert_eq!(rows[5].number, 6);
        assert_eq!(progress(&ob), "Step 3 of 6");
    }

    #[test]
    fn only_steps_up_to_the_first_blocked_one_are_reachable() {
        let mut app = App::new(Settings::in_memory());
        // The microphone has not been granted, so nothing past Permissions can be reached.
        let rows = step_rows(app.state().onboarding.as_ref().unwrap());
        assert!(rows[0].reachable && rows[1].reachable);
        assert!(!rows[2].reachable);
        app.dispatch(Action::Onboarding(OnboardingAction::PermissionChanged(
            Permission::Microphone,
            PermissionState::Granted,
        )));
        assert!(step_rows(app.state().onboarding.as_ref().unwrap())[5].reachable);
    }

    #[test]
    fn a_poll_reports_only_what_changed_and_ignores_unknown() {
        let mut ob = Onboarding::new();
        let observed = |p: Permission| match p {
            Permission::Microphone => PermissionState::Granted,
            Permission::Accessibility => PermissionState::Denied,
            _ => PermissionState::Unknown,
        };
        let changes = permission_changes(&ob, observed);
        assert_eq!(
            changes,
            vec![
                (Permission::Microphone, PermissionState::Granted),
                (Permission::Accessibility, PermissionState::Denied)
            ]
        );
        for (p, s) in &changes {
            ob.permissions.iter_mut().find(|(q, _)| q == p).unwrap().1 = *s;
        }
        assert!(permission_changes(&ob, observed).is_empty());
        // A revoked permission is reported as soon as the platform says so, also as Unknown.
        let revoked = |_: Permission| PermissionState::Denied;
        assert_eq!(permission_changes(&ob, revoked).len(), 3);
        let taken_away = |p: Permission| {
            if p == Permission::Accessibility {
                PermissionState::Unknown
            } else {
                observed(p)
            }
        };
        // Accessibility was Denied in the model, so Unknown leaves it alone; the granted
        // microphone going Unknown would be reported.
        assert!(permission_changes(&ob, taken_away).is_empty());
        let mic_gone = |_: Permission| PermissionState::Unknown;
        assert_eq!(
            permission_changes(&ob, mic_gone),
            vec![(Permission::Microphone, PermissionState::Unknown)]
        );
    }

    #[test]
    fn permission_buttons_follow_the_state() {
        assert_eq!(
            permission_button(PermissionState::Granted),
            PermissionButton::Allowed
        );
        assert_eq!(
            permission_button(PermissionState::Unknown),
            PermissionButton::Allow
        );
        assert_eq!(
            permission_button(PermissionState::Denied),
            PermissionButton::OpenSystemSettings
        );
    }

    #[test]
    fn every_permission_has_a_name_and_a_reason() {
        for p in Permission::ALL {
            assert!(!permission_name(p).is_empty());
            assert!(!p.why().is_empty());
        }
    }

    #[test]
    fn models_show_their_licence_and_the_required_one_says_so() {
        let ob = Onboarding::new();
        let rows = model_rows(&ob);
        let by = |id: &str| rows.iter().find(|r| r.id == id).unwrap().clone();
        assert_eq!(by("whisper-small").licence, Badge::CommercialOk);
        assert_eq!(by("supertonic-3").licence, Badge::Conditional);
        assert_eq!(by("silero-vad").note, "required");
        assert!(by("silero-vad").required && by("silero-vad").selected);
        assert!(rows.iter().all(|r| r.name != "Unknown model"));
    }

    #[test]
    fn an_unknown_model_gets_the_careful_badge() {
        let (_, _, _, badge) = model_display("something-new");
        assert_eq!(badge, Badge::Conditional);
    }

    #[test]
    fn the_done_step_follows_the_hold_mode() {
        let mut ob = Onboarding::new();
        assert!(try_it(&ob).0.ends_with("hold"));
        ob.hold_to_talk = false;
        assert!(try_it(&ob).0.ends_with("tap"));
        assert!(try_it(&ob).1.contains("again to stop"));
    }

    #[test]
    fn shortcut_choices_map_to_actions_and_back() {
        use crastu_app::OnboardingAction as A;
        let mut ob = Onboarding::new();
        assert_eq!(
            choice_action(ShortcutChoice::Preset, 1),
            Some(A::SetShortcut("Control+Space".into()))
        );
        assert_eq!(choice_action(ShortcutChoice::Preset, 3), None);
        assert_eq!(
            choice_action(ShortcutChoice::HoldMode, 1),
            Some(A::SetHoldToTalk(false))
        );
        assert_eq!(choice_selected(&ob, ShortcutChoice::HoldMode), 0);
        ob.hold_to_talk = false;
        assert_eq!(choice_selected(&ob, ShortcutChoice::HoldMode), 1);
        assert_eq!(choice_count(ShortcutChoice::Preset), 3);
    }

    #[test]
    fn shortcuts_are_found_among_the_presets_and_shown_with_symbols() {
        let mut ob = Onboarding::new();
        assert_eq!(shortcut_index(&ob), Some(0));
        ob.shortcut = "Control+Shift+K".into();
        assert_eq!(shortcut_index(&ob), None);
        assert_eq!(shortcut_label("Option+Space"), "\u{2325} Space");
        assert_eq!(
            shortcut_label("Command+Shift+Space"),
            "\u{2318} \u{21E7} Space"
        );
        assert_eq!(shortcut_label("F5"), "F5");
    }
}
