//! The network gate the desktop app builds at start, as plain data so it can be tested without a
//! window.

use crastu_app::{NetGate, Settings};
use crastu_settings::{offline_enabled, parse_offline_env};

/// Reads `settings.json` from the per-user folder.
pub fn load_settings() -> Result<Settings, String> {
    crastu_settings::settings_path().and_then(|p| Settings::load(p).map_err(|e| e.to_string()))
}

/// The one gate for this process, whether `CRASTU_OFFLINE` decided it (the Settings switch then
/// cannot change it), and a notice when the settings could not be read.
///
/// When the settings fail to load the gate is built Offline ON: a switch that cannot be read must
/// not let a request through. A non-empty `env` still wins, so a user who sets
/// `CRASTU_OFFLINE=0` stays in charge.
pub fn desktop_gate(
    loaded: &Result<Settings, String>,
    env: Option<&str>,
) -> (NetGate, bool, Option<String>) {
    let locked_by_env = parse_offline_env(env).is_some();
    match loaded {
        Ok(settings) => (
            NetGate::new(offline_enabled(settings, env)),
            locked_by_env,
            None,
        ),
        Err(e) => {
            // The defaults carry no `network.offline`, so ask with an empty settings value but
            // force it on unless the environment decides.
            let on = parse_offline_env(env).unwrap_or(true);
            let tail = if on {
                "Offline is on."
            } else {
                "Offline follows CRASTU_OFFLINE."
            };
            let notice = format!(
                "Settings could not be read ({e}): {tail} Using the defaults for this session; the file was left as it is."
            );
            (NetGate::new(on), locked_by_env, Some(notice))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crastu_settings::OFFLINE_KEY;

    fn with(value: serde_json::Value) -> Result<Settings, String> {
        let mut s = Settings::in_memory();
        s.set(OFFLINE_KEY, value).unwrap();
        Ok(s)
    }

    #[test]
    fn readable_settings_decide() {
        let (gate, locked, notice) = desktop_gate(&Ok(Settings::in_memory()), None);
        assert!(!gate.is_offline() && !locked && notice.is_none());
        let (gate, locked, _) = desktop_gate(&with(true.into()), None);
        assert!(gate.is_offline() && !locked);
    }

    #[test]
    fn unreadable_settings_start_offline_and_say_so() {
        let (gate, locked, notice) = desktop_gate(&Err("bad json at line 3".into()), None);
        assert!(gate.is_offline());
        assert!(!locked);
        let notice = notice.unwrap();
        assert!(notice.contains("Settings could not be read"), "{notice}");
        assert!(notice.contains("Offline is on"), "{notice}");
        assert!(notice.contains("bad json at line 3"), "{notice}");
    }

    #[test]
    fn an_empty_environment_value_does_not_decide() {
        let (gate, locked, _) = desktop_gate(&Err("x".into()), Some("  "));
        assert!(gate.is_offline() && !locked);
    }

    #[test]
    fn the_environment_wins_over_the_setting_and_locks_the_switch() {
        let (gate, locked, _) = desktop_gate(&with(true.into()), Some("0"));
        assert!(!gate.is_offline() && locked);
        let (gate, locked, _) = desktop_gate(&with(false.into()), Some("1"));
        assert!(gate.is_offline() && locked);
        let (gate, _, _) = desktop_gate(&Ok(Settings::in_memory()), Some("ture"));
        assert!(gate.is_offline(), "a typo fails closed");
    }

    #[test]
    fn the_environment_wins_when_the_settings_cannot_be_read() {
        let (gate, locked, notice) = desktop_gate(&Err("x".into()), Some("off"));
        assert!(!gate.is_offline() && locked);
        assert!(notice.unwrap().contains("CRASTU_OFFLINE"));
        let (gate, locked, _) = desktop_gate(&Err("x".into()), Some("1"));
        assert!(gate.is_offline() && locked);
    }

    #[test]
    fn a_wrong_typed_setting_is_on() {
        for v in [
            serde_json::Value::Null,
            "false".into(),
            0.into(),
            serde_json::json!([]),
        ] {
            let (gate, _, _) = desktop_gate(&with(v.clone()), None);
            assert!(gate.is_offline(), "{v}");
        }
    }
}
