//! Where the session folders go. Pure: tests inject the environment, nothing sets a variable.
//!
//! `CRASTU_LOG_DIR` wins when set and not empty. Otherwise `~/Library/Logs/Crastu` on macOS,
//! `%LOCALAPPDATA%\Crastu\Logs` on Windows, `$XDG_STATE_HOME/crastu/logs` or
//! `~/.local/state/crastu/logs` elsewhere.

use std::ffi::OsString;
use std::path::PathBuf;

use crastu_settings::Platform;

/// The environment variable that moves the log root.
pub const LOG_DIR_ENV: &str = "CRASTU_LOG_DIR";

/// The log root for `platform`, reading variables through `env` (an empty value counts as unset).
pub fn resolve_log_root(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    let get = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(p) = get(LOG_DIR_ENV) {
        return Ok(p);
    }
    let (found, missing) = match platform {
        Platform::Windows => (
            get("LOCALAPPDATA").map(|a| a.join("Crastu").join("Logs")),
            "LOCALAPPDATA",
        ),
        Platform::MacOs => (get("HOME").map(|h| h.join("Library/Logs/Crastu")), "HOME"),
        Platform::Other => (
            get("XDG_STATE_HOME")
                .map(|x| x.join("crastu/logs"))
                .or_else(|| get("HOME").map(|h| h.join(".local/state/crastu/logs"))),
            "HOME",
        ),
    };
    found.ok_or_else(|| format!("{missing} is not set; set {LOG_DIR_ENV}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(platform: Platform, vars: &[(&str, &str)]) -> Result<PathBuf, String> {
        resolve_log_root(platform, &|n| {
            vars.iter().find(|(k, _)| *k == n).map(|(_, v)| (*v).into())
        })
    }

    #[test]
    fn the_override_wins_everywhere() {
        let vars = [
            ("CRASTU_LOG_DIR", "/l"),
            ("HOME", "/h"),
            ("LOCALAPPDATA", "/w"),
        ];
        for p in [Platform::Windows, Platform::MacOs, Platform::Other] {
            assert_eq!(root(p, &vars).unwrap(), PathBuf::from("/l"), "{p:?}");
        }
    }

    #[test]
    fn per_platform_defaults() {
        assert_eq!(
            root(Platform::MacOs, &[("HOME", "/Users/a")]).unwrap(),
            PathBuf::from("/Users/a/Library/Logs/Crastu")
        );
        assert_eq!(
            root(Platform::Windows, &[("LOCALAPPDATA", "C:/L")]).unwrap(),
            PathBuf::from("C:/L").join("Crastu").join("Logs")
        );
        assert_eq!(
            root(Platform::Other, &[("HOME", "/home/a")]).unwrap(),
            PathBuf::from("/home/a/.local/state/crastu/logs")
        );
        assert_eq!(
            root(Platform::Other, &[("XDG_STATE_HOME", "/s"), ("HOME", "/h")]).unwrap(),
            PathBuf::from("/s/crastu/logs")
        );
    }

    #[test]
    fn empty_values_count_as_unset() {
        let e = root(Platform::MacOs, &[("CRASTU_LOG_DIR", ""), ("HOME", "")]).unwrap_err();
        assert!(e.contains("CRASTU_LOG_DIR"), "{e}");
    }
}
