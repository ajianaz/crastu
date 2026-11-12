use serde::{Deserialize, Serialize};

use crate::Effect;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    Welcome,
    Permissions,
    Models,
    Shortcut,
    Agents,
    Done,
}

impl Step {
    pub const ALL: [Step; 6] = [
        Step::Welcome,
        Step::Permissions,
        Step::Models,
        Step::Shortcut,
        Step::Agents,
        Step::Done,
    ];

    fn index(self) -> usize {
        Step::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    pub fn title(self) -> &'static str {
        match self {
            Step::Welcome => "Welcome",
            Step::Permissions => "Permissions",
            Step::Models => "Models",
            Step::Shortcut => "Shortcut",
            Step::Agents => "AI agents",
            Step::Done => "Done",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Microphone,
    Accessibility,
    InputMonitoring,
    ScreenAndSystemAudio,
}

impl Permission {
    pub const ALL: [Permission; 4] = [
        Permission::Microphone,
        Permission::Accessibility,
        Permission::InputMonitoring,
        Permission::ScreenAndSystemAudio,
    ];

    /// Without the microphone nothing works; the rest can be granted later.
    pub fn blocks_setup(self) -> bool {
        self == Permission::Microphone
    }

    pub fn optional(self) -> bool {
        self == Permission::ScreenAndSystemAudio
    }

    pub fn why(self) -> &'static str {
        match self {
            Permission::Microphone => "To hear you while dictating and recording.",
            Permission::Accessibility => "To paste text into the app you are using.",
            Permission::InputMonitoring => "To notice your push-to-talk key in any app.",
            Permission::ScreenAndSystemAudio => {
                "To capture sound from other apps for clips and meeting notes."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    #[default]
    Unknown,
    Granted,
    Denied,
}

/// A model offered during onboarding. Required models cannot be unticked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoice {
    pub id: String,
    pub selected: bool,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnboardingAction {
    Next,
    Back,
    GoTo(Step),
    /// The front end asks the OS and reports the answer back.
    AskPermission(Permission),
    PermissionChanged(Permission, PermissionState),
    ToggleModel(String),
    SetShortcut(String),
    SetHoldToTalk(bool),
    Finish,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Onboarding {
    pub step: Step,
    pub permissions: Vec<(Permission, PermissionState)>,
    pub models: Vec<ModelChoice>,
    pub shortcut: String,
    pub hold_to_talk: bool,
}

impl Onboarding {
    pub fn new() -> Onboarding {
        Onboarding {
            step: Step::Welcome,
            permissions: Permission::ALL
                .iter()
                .map(|p| (*p, PermissionState::Unknown))
                .collect(),
            models: vec![
                ModelChoice {
                    id: "whisper-small".into(),
                    selected: true,
                    required: false,
                },
                ModelChoice {
                    id: "silero-vad".into(),
                    selected: true,
                    required: true,
                },
                // Supertonic 3 is not offered until the redistribution question
                // (docs/ARCHITECTURE.md, open question 3) is answered and the catalogue has it.
                ModelChoice {
                    id: "yunet".into(),
                    selected: false,
                    required: false,
                },
            ],
            shortcut: "Option+Space".into(),
            hold_to_talk: true,
        }
    }

    pub fn permission(&self, p: Permission) -> PermissionState {
        self.permissions
            .iter()
            .find(|(q, _)| *q == p)
            .map(|(_, s)| *s)
            .unwrap_or_default()
    }

    /// Whether Continue is enabled on the current step.
    pub fn can_continue(&self) -> bool {
        match self.step {
            Step::Permissions => Permission::ALL
                .iter()
                .filter(|p| p.blocks_setup())
                .all(|p| self.permission(*p) == PermissionState::Granted),
            Step::Done => false,
            _ => true,
        }
    }

    /// Steps the user may jump to from the sidebar: anything up to the first blocked step.
    pub fn reachable(&self, target: Step) -> bool {
        let mut probe = self.clone();
        for s in Step::ALL {
            if s == target {
                return true;
            }
            probe.step = s;
            if !probe.can_continue() {
                return false;
            }
        }
        false
    }

    pub fn selected_models(&self) -> Vec<String> {
        self.models
            .iter()
            .filter(|m| m.selected)
            .map(|m| m.id.clone())
            .collect()
    }

    /// Returns whether onboarding is finished, plus effects for the platform.
    pub(crate) fn apply(&mut self, a: OnboardingAction) -> (bool, Vec<Effect>) {
        let mut fx = Vec::new();
        match a {
            OnboardingAction::Next => {
                if self.can_continue() {
                    if self.step == Step::Models {
                        fx.push(Effect::DownloadModels(self.selected_models()));
                    }
                    self.step = Step::ALL[(self.step.index() + 1).min(Step::ALL.len() - 1)];
                }
            }
            OnboardingAction::Back => self.step = Step::ALL[self.step.index().saturating_sub(1)],
            OnboardingAction::GoTo(s) => {
                if self.reachable(s) {
                    // Jumping past the Models step must start the downloads like Continue does.
                    if self.step <= Step::Models && s > Step::Models {
                        fx.push(Effect::DownloadModels(self.selected_models()));
                    }
                    self.step = s;
                }
            }
            OnboardingAction::AskPermission(p) => {
                fx.push(match self.permission(p) {
                    // macOS only shows the prompt once; after a denial the user has to go to System Settings.
                    PermissionState::Denied => Effect::OpenSystemSettings(p),
                    _ => Effect::RequestPermission(p),
                });
            }
            OnboardingAction::PermissionChanged(p, s) => {
                if let Some(entry) = self.permissions.iter_mut().find(|(q, _)| *q == p) {
                    entry.1 = s;
                }
            }
            OnboardingAction::ToggleModel(id) => {
                if let Some(m) = self.models.iter_mut().find(|m| m.id == id && !m.required) {
                    m.selected = !m.selected;
                }
            }
            OnboardingAction::SetShortcut(s) => {
                if !s.trim().is_empty() {
                    self.shortcut = s;
                }
            }
            OnboardingAction::SetHoldToTalk(b) => self.hold_to_talk = b,
            OnboardingAction::Finish => return (self.step == Step::Done, fx),
        }
        (false, fx)
    }
}

impl Default for Onboarding {
    fn default() -> Self {
        Onboarding::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_gates_the_permissions_step() {
        let mut ob = Onboarding::new();
        ob.apply(OnboardingAction::Next);
        assert_eq!(ob.step, Step::Permissions);
        ob.apply(OnboardingAction::Next);
        assert_eq!(
            ob.step,
            Step::Permissions,
            "blocked until the microphone is granted"
        );
        assert!(!ob.reachable(Step::Models));
        ob.apply(OnboardingAction::PermissionChanged(
            Permission::Microphone,
            PermissionState::Granted,
        ));
        assert!(ob.reachable(Step::Agents));
        ob.apply(OnboardingAction::Next);
        assert_eq!(ob.step, Step::Models);
    }

    #[test]
    fn asking_after_a_denial_opens_system_settings() {
        let mut ob = Onboarding::new();
        let (_, fx) = ob.apply(OnboardingAction::AskPermission(Permission::Accessibility));
        assert_eq!(
            fx,
            vec![Effect::RequestPermission(Permission::Accessibility)]
        );
        ob.apply(OnboardingAction::PermissionChanged(
            Permission::Accessibility,
            PermissionState::Denied,
        ));
        let (_, fx) = ob.apply(OnboardingAction::AskPermission(Permission::Accessibility));
        assert_eq!(
            fx,
            vec![Effect::OpenSystemSettings(Permission::Accessibility)]
        );
    }

    #[test]
    fn leaving_the_models_step_starts_downloads_and_required_models_stay() {
        let mut ob = Onboarding::new();
        ob.apply(OnboardingAction::PermissionChanged(
            Permission::Microphone,
            PermissionState::Granted,
        ));
        ob.apply(OnboardingAction::GoTo(Step::Models));
        ob.apply(OnboardingAction::ToggleModel("silero-vad".into()));
        ob.apply(OnboardingAction::ToggleModel("yunet".into()));
        let (_, fx) = ob.apply(OnboardingAction::Next);
        assert_eq!(
            fx,
            vec![Effect::DownloadModels(vec![
                "whisper-small".into(),
                "silero-vad".into(),
                "yunet".into()
            ])]
        );
    }

    #[test]
    fn finish_only_counts_on_the_last_step() {
        let mut ob = Onboarding::new();
        assert!(!ob.apply(OnboardingAction::Finish).0);
        ob.apply(OnboardingAction::SetShortcut("  ".into()));
        assert_eq!(ob.shortcut, "Option+Space");
        ob.apply(OnboardingAction::PermissionChanged(
            Permission::Microphone,
            PermissionState::Granted,
        ));
        ob.apply(OnboardingAction::GoTo(Step::Done));
        assert!(ob.apply(OnboardingAction::Finish).0);
    }

    #[test]
    fn jumping_past_the_models_step_starts_the_downloads() {
        let mut ob = Onboarding::new();
        ob.apply(OnboardingAction::PermissionChanged(
            Permission::Microphone,
            PermissionState::Granted,
        ));
        let (_, fx) = ob.apply(OnboardingAction::GoTo(Step::Shortcut));
        assert_eq!(
            fx,
            vec![Effect::DownloadModels(ob.selected_models())],
            "from Welcome to Shortcut crosses Models"
        );
        // Moving within the steps after Models, or back, starts nothing.
        let (_, fx) = ob.apply(OnboardingAction::GoTo(Step::Done));
        assert!(fx.is_empty());
        let (_, fx) = ob.apply(OnboardingAction::GoTo(Step::Models));
        assert!(fx.is_empty());
    }
}
