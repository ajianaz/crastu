//! UI-neutral view models: screen state, actions and events shared by every front end.
//!
//! A front end (GPUI today; Flutter through flutter_rust_bridge or a native app later) does three
//! things only: read [`AppState`], send [`Action`]s to [`App::dispatch`], and carry out the
//! platform [`Effect`]s it gets back (asking for a permission, opening System Settings, starting
//! downloads). All rules live here, so they are written and tested once.

mod appearance;
mod nav;
mod onboarding;

use std::sync::mpsc::{Receiver, Sender, channel};

/// Re-exported so a front end can build the `App` without its own dependency on the store.
/// Re-exported so a front end can build the gate it shares with the app.
pub use crastu_core::NetGate;
pub use crastu_settings::Settings;
use crastu_settings::{OFFLINE_KEY, offline_enabled};
use serde::{Deserialize, Serialize};

pub use appearance::{Appearance, Density, FontPairing, Theme};
pub use nav::{ModuleId, Nav, SettingsCategory};
pub use onboarding::{
    ModelChoice, Onboarding, OnboardingAction, Permission, PermissionState, Step,
};

/// Everything a front end needs to draw the app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub appearance: Appearance,
    pub nav: Nav,
    /// Present until the user finishes onboarding.
    pub onboarding: Option<Onboarding>,
    pub network: Network,
}

/// Whether Offline mode is on, as the Settings switch shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Network {
    pub offline: bool,
    /// `CRASTU_OFFLINE` decides, so the switch is shown but cannot be changed here.
    pub locked_by_env: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Action {
    SetTheme(Theme),
    SetFont(FontPairing),
    SetDensity(Density),
    /// `None` follows the operating system.
    SetReduceMotion(Option<bool>),
    /// Turn Offline mode on or off (refused while `CRASTU_OFFLINE` decides).
    SetOffline(bool),
    /// Flip Offline mode from whatever it is when the action is handled.
    ToggleOffline,
    Navigate(ModuleId),
    OpenSettings(SettingsCategory),
    ToggleLeftSidebar,
    ToggleInspector,
    Onboarding(OnboardingAction),
}

/// Work only the platform can do. The front end performs it and reports back with an action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    RequestPermission(Permission),
    OpenSystemSettings(Permission),
    DownloadModels(Vec<String>),
    /// Saving settings failed; the change is kept in memory.
    SettingsNotSaved(String),
    /// An Offline change was refused because `CRASTU_OFFLINE` is set.
    OfflineSetByEnv,
}

const ONBOARDED_KEY: &str = "app.onboarded";

/// Owns the state and the settings it persists to.
#[derive(Debug)]
pub struct App {
    state: AppState,
    settings: Settings,
    gate: NetGate,
    subscribers: Vec<Sender<AppState>>,
}

impl App {
    /// Builds the state from saved settings. Unknown or broken values fall back to defaults. The
    /// network gate follows the `network.offline` setting alone; the desktop app uses
    /// [`App::with_network`] to also honour `CRASTU_OFFLINE`.
    pub fn new(settings: Settings) -> App {
        let gate = NetGate::new(offline_enabled(&settings, None));
        App::with_network(settings, gate, false)
    }

    /// Like [`App::new`] with the gate the process shares. `locked_by_env` says that
    /// `CRASTU_OFFLINE` decided it, so the switch cannot change it.
    pub fn with_network(settings: Settings, gate: NetGate, locked_by_env: bool) -> App {
        let appearance = Appearance::load(&settings);
        let onboarded = settings
            .get::<bool>(ONBOARDED_KEY)
            .ok()
            .flatten()
            .unwrap_or(false);
        let state = AppState {
            appearance,
            nav: Nav::default(),
            onboarding: (!onboarded).then(Onboarding::new),
            network: Network {
                offline: gate.is_offline(),
                locked_by_env,
            },
        };
        App {
            state,
            settings,
            gate,
            subscribers: Vec::new(),
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// The gate every outbound request asks; it shares its switch with every clone.
    pub fn gate(&self) -> &NetGate {
        &self.gate
    }

    fn set_offline(
        &mut self,
        on: bool,
        persist: &mut Vec<(String, serde_json::Value)>,
    ) -> Option<Effect> {
        if self.state.network.locked_by_env {
            return Some(Effect::OfflineSetByEnv);
        }
        self.state.network.offline = on;
        self.gate.set_offline(on);
        persist.push((OFFLINE_KEY.into(), serde_json::Value::Bool(on)));
        None
    }

    /// A stream of full state snapshots, one per dispatched action that changed something.
    pub fn subscribe(&mut self) -> Receiver<AppState> {
        let (tx, rx) = channel();
        self.subscribers.push(tx);
        rx
    }

    pub fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        let before = self.state.clone();
        let mut effects = Vec::new();
        let mut persist: Vec<(String, serde_json::Value)> = Vec::new();

        match action {
            Action::SetTheme(t) => {
                self.state.appearance.theme = t;
                persist.push(appearance::key_value_theme(t));
            }
            Action::SetFont(f) => {
                self.state.appearance.font = f;
                persist.push(appearance::key_value_font(f));
            }
            Action::SetDensity(d) => {
                self.state.appearance.density = d;
                persist.push(appearance::key_value_density(d));
            }
            Action::SetReduceMotion(r) => {
                self.state.appearance.reduce_motion = r;
                persist.push(appearance::key_value_reduce_motion(r));
            }
            Action::SetOffline(on) => effects.extend(self.set_offline(on, &mut persist)),
            Action::ToggleOffline => {
                let on = !self.state.network.offline;
                effects.extend(self.set_offline(on, &mut persist));
            }
            Action::Navigate(m) => self.state.nav.navigate(m),
            Action::OpenSettings(c) => self.state.nav.open_settings(c),
            Action::ToggleLeftSidebar => self.state.nav.left_open = !self.state.nav.left_open,
            Action::ToggleInspector => {
                self.state.nav.inspector_open = !self.state.nav.inspector_open
            }
            Action::Onboarding(a) => {
                if let Some(ob) = self.state.onboarding.as_mut() {
                    let (done, fx) = ob.apply(a);
                    effects.extend(fx);
                    if done {
                        self.state.onboarding = None;
                        persist.push((ONBOARDED_KEY.into(), serde_json::Value::Bool(true)));
                    }
                }
            }
        }

        if !persist.is_empty() {
            let mut result = Ok(());
            for (k, v) in persist {
                result = result.and(self.settings.set(&k, v));
            }
            if let Err(e) = result.and_then(|_| self.settings.save()) {
                effects.push(Effect::SettingsNotSaved(e.to_string()));
            }
        }
        if self.state != before {
            let snapshot = self.state.clone();
            self.subscribers
                .retain(|s| s.send(snapshot.clone()).is_ok());
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_changes_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut app = App::new(Settings::load(&path).unwrap());
        assert_eq!(app.state().appearance, Appearance::default());

        assert!(app.dispatch(Action::SetTheme(Theme::Paper)).is_empty());
        app.dispatch(Action::SetFont(FontPairing::SpaceGrotesk));
        app.dispatch(Action::SetDensity(Density::Comfortable));
        app.dispatch(Action::SetReduceMotion(Some(true)));

        let again = App::new(Settings::load(&path).unwrap());
        assert_eq!(
            again.state().appearance,
            Appearance {
                theme: Theme::Paper,
                font: FontPairing::SpaceGrotesk,
                density: Density::Comfortable,
                reduce_motion: Some(true)
            }
        );
    }

    #[test]
    fn broken_saved_values_fall_back_to_defaults() {
        let mut s = Settings::in_memory();
        s.set("appearance.theme", "neon").unwrap();
        s.set("appearance.density", 42).unwrap();
        let app = App::new(s);
        assert_eq!(app.state().appearance.theme, Theme::Graphite);
        assert_eq!(app.state().appearance.density, Density::Compact);
    }

    #[test]
    fn subscribers_get_snapshots_only_on_change() {
        let mut app = App::new(Settings::in_memory());
        let rx = app.subscribe();
        app.dispatch(Action::ToggleInspector);
        app.dispatch(Action::SetTheme(Theme::Graphite)); // already the default: no change
        let got: Vec<AppState> = rx.try_iter().collect();
        assert_eq!(got.len(), 1);
        assert!(!got[0].nav.inspector_open);
    }

    #[test]
    fn finishing_onboarding_is_remembered() {
        let mut settings = Settings::in_memory();
        settings.set(ONBOARDED_KEY, false).unwrap();
        let mut app = App::new(settings);
        assert!(app.state().onboarding.is_some());
        app.dispatch(Action::Onboarding(OnboardingAction::PermissionChanged(
            Permission::Microphone,
            PermissionState::Granted,
        )));
        for _ in 0..5 {
            app.dispatch(Action::Onboarding(OnboardingAction::Next));
        }
        app.dispatch(Action::Onboarding(OnboardingAction::Finish));
        assert!(app.state().onboarding.is_none());
        assert_eq!(app.settings.get::<bool>(ONBOARDED_KEY).unwrap(), Some(true));
    }

    #[test]
    fn offline_switch_flips_the_shared_gate_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let gate = NetGate::new(false);
        let mut app = App::with_network(Settings::load(&path).unwrap(), gate.clone(), false);
        assert!(!app.state().network.offline);
        assert!(app.dispatch(Action::SetOffline(true)).is_empty());
        assert!(gate.is_offline(), "the clone the rest of the process holds");
        assert!(app.state().network.offline);
        assert!(
            gate.check("model download x", "https://h.example/p")
                .is_err()
        );
        let again = Settings::load(&path).unwrap();
        assert_eq!(again.get::<bool>(OFFLINE_KEY).unwrap(), Some(true));
        assert!(offline_enabled(&again, None));
        app.dispatch(Action::SetOffline(false));
        assert!(!gate.is_offline());
        let again = Settings::load(&path).unwrap();
        assert_eq!(again.get::<bool>(OFFLINE_KEY).unwrap(), Some(false));
    }

    #[test]
    fn toggle_flips_the_current_value_each_time() {
        let mut app = App::new(Settings::in_memory());
        for expected in [true, false, true] {
            app.dispatch(Action::ToggleOffline);
            assert_eq!(app.state().network.offline, expected);
            assert_eq!(app.gate().is_offline(), expected);
        }
    }

    #[test]
    fn the_environment_locks_the_switch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let gate = NetGate::new(true);
        let mut app = App::with_network(Settings::load(&path).unwrap(), gate.clone(), true);
        assert!(app.state().network.offline && app.state().network.locked_by_env);
        for action in [Action::SetOffline(false), Action::ToggleOffline] {
            assert_eq!(app.dispatch(action), vec![Effect::OfflineSetByEnv]);
            assert!(gate.is_offline());
            assert!(app.state().network.offline);
        }
        assert!(!path.exists(), "nothing was written");
    }

    #[test]
    fn a_wrong_typed_setting_starts_offline_and_a_switch_fixes_it() {
        let mut s = Settings::in_memory();
        s.set(OFFLINE_KEY, "yes please").unwrap();
        let mut app = App::new(s);
        assert!(app.state().network.offline);
        app.dispatch(Action::SetOffline(false));
        assert_eq!(app.settings.get::<bool>(OFFLINE_KEY).unwrap(), Some(false));
    }

    #[test]
    fn state_serialises_for_other_front_ends() {
        let app = App::new(Settings::in_memory());
        let json = serde_json::to_string(app.state()).unwrap();
        let back: AppState = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, app.state());
    }
}
