//! What the onboarding screens show for a `crastu_app::Onboarding`, as plain data: the step list
//! with its marks, the permission rows and their buttons, the model table (from the model catalogue), the shortcut presets.
//! The GPUI view only draws it, and the rules (what can be reached, when Continue is on) stay in
//! `crastu-app`.

use std::sync::OnceLock;

use crastu_app::{Onboarding, Permission, PermissionState, Step};
use crastu_controls::Badge;
use crastu_models::{Catalogue, Commercial, Task};

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
    pub name: String,
    pub note: &'static str,
    pub used_for: &'static str,
    /// Download size as text; "TBD" for a model the catalogue does not know.
    pub size: String,
    pub licence: Badge,
    pub selected: bool,
    pub required: bool,
}

/// The catalogue that ships with Crastu, parsed once.
fn shipped() -> &'static Catalogue {
    static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
    CATALOGUE.get_or_init(crastu_models::catalogue)
}

fn used_for(task: Task) -> &'static str {
    match task {
        Task::SpeechToText => "Dictation and captions",
        Task::VoiceActivity => "Finding speech",
        Task::TextToSpeech => "Voiceover",
        Task::FaceDetection => "Faces for reframing",
        Task::Embedding => "Memory search",
    }
}

/// "610 MB", "2.2 MB", "227 KB".
fn size_text(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    match bytes {
        b if b >= 100 * MB => format!("{} MB", b / MB),
        b if b >= MB => format!("{:.1} MB", b as f64 / MB as f64),
        b => format!("{} KB", b.div_ceil(1024)),
    }
}

/// The licence badge of a catalogue entry: Allowed is Commercial OK, Conditional is Conditional.
/// `Denied` cannot be in a loaded catalogue, and is shown as the careful badge if it ever is.
fn badge(commercial: &Commercial) -> Badge {
    match commercial {
        Commercial::Allowed => Badge::CommercialOk,
        Commercial::Conditional { .. } | Commercial::Denied => Badge::Conditional,
    }
}

/// The model table for `ob`, with names, uses, sizes and licences from the model catalogue. An id
/// the catalogue does not have is shown under its own id with the most careful badge.
pub fn model_rows(ob: &Onboarding) -> Vec<ModelRow> {
    rows_from(ob, shipped())
}

fn rows_from(ob: &Onboarding, catalogue: &Catalogue) -> Vec<ModelRow> {
    ob.models
        .iter()
        .map(|m| {
            let (name, task, size, licence) = match catalogue.get(&m.id) {
                Some(e) => (
                    e.name.clone(),
                    Some(e.task),
                    e.size_bytes.map_or_else(|| "TBD".to_string(), size_text),
                    badge(&e.commercial),
                ),
                None => (m.id.clone(), None, "TBD".to_string(), Badge::Conditional),
            };
            let note = match (m.required, task) {
                (true, _) => "required",
                (false, Some(Task::SpeechToText)) => "recommended",
                _ => "optional",
            };
            ModelRow {
                id: m.id.clone(),
                name,
                note,
                used_for: task.map_or("", used_for),
                size,
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
    fn models_show_the_catalogue_data_and_the_required_one_says_so() {
        let ob = Onboarding::new();
        let rows = model_rows(&ob);
        let by = |id: &str| rows.iter().find(|r| r.id == id).unwrap().clone();
        let whisper = by("whisper-small");
        assert_eq!(whisper.name, "Whisper Small");
        assert_eq!(whisper.used_for, "Dictation and captions");
        assert_eq!(whisper.note, "recommended");
        assert_eq!(whisper.size, "609 MB");
        assert_eq!(whisper.licence, Badge::CommercialOk);
        assert_eq!(by("yunet").used_for, "Faces for reframing");
        assert_eq!(by("yunet").size, "228 KB");
        assert_eq!(by("silero-vad").size, "2.2 MB");
        assert_eq!(by("silero-vad").note, "required");
        assert!(by("silero-vad").required && by("silero-vad").selected);
    }

    #[test]
    fn every_default_onboarding_model_is_in_the_catalogue() {
        let ob = Onboarding::new();
        assert!(!ob.models.is_empty());
        for m in &ob.models {
            assert!(
                shipped().get(&m.id).is_some(),
                "{} is offered in onboarding but is not in the catalogue",
                m.id
            );
        }
        assert!(model_rows(&ob).iter().all(|r| !r.used_for.is_empty()));
    }

    #[test]
    fn a_conditional_entry_gets_the_conditional_badge() {
        let json = r#"[{"id":"whisper-small","name":"Odd","task":"text_to_speech","licence":"OpenRAIL-M",
            "licence_url":"https://e.org/l","commercial":{"status":"conditional","note":"use restrictions"},
            "url":"https://e.org/m","file_name":"m.bin","sha256":"0000000000000000000000000000000000000000000000000000000000000000",
            "size_bytes":null,"attribution":null}]"#;
        let catalogue = Catalogue::from_json(json).unwrap();
        let ob = Onboarding::new();
        let rows = rows_from(&ob, &catalogue);
        let row = rows.iter().find(|r| r.id == "whisper-small").unwrap();
        assert_eq!(row.name, "Odd");
        assert_eq!(row.licence, Badge::Conditional);
        assert_eq!(row.used_for, "Voiceover");
        assert_eq!(row.size, "TBD");
    }

    #[test]
    fn an_unknown_model_is_shown_under_its_id_with_the_careful_badge() {
        let mut ob = Onboarding::new();
        ob.models[0].id = "something-new".into();
        let rows = model_rows(&ob);
        assert_eq!(rows[0].name, "something-new");
        assert_eq!(rows[0].licence, Badge::Conditional);
        assert_eq!(rows[0].note, "optional");
        assert_eq!(rows[0].used_for, "");
    }

    #[test]
    fn sizes_read_as_megabytes_or_kilobytes() {
        assert_eq!(size_text(639_387_718), "609 MB");
        assert_eq!(size_text(2_327_524), "2.2 MB");
        assert_eq!(size_text(232_589), "228 KB");
        assert_eq!(size_text(1), "1 KB");
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
