//! Where Crastu keeps its data: one lookup shared by the desktop app and the CLI, so that a
//! setting saved by one is the setting read by the other.
//!
//! `CRASTU_DATA` wins when set and not empty. Otherwise the per-user folder: `~/Library/
//! Application Support/Crastu` on macOS, `%APPDATA%\Crastu` on Windows, and
//! `$XDG_DATA_HOME/crastu` or `~/.local/share/crastu` elsewhere. There is no fallback to a shared
//! temp folder: without the variables the caller is told to set `CRASTU_DATA`.

use std::ffi::OsString;
use std::path::PathBuf;

/// The platform families that keep their per-user data in different places.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Other,
}

impl Platform {
    /// The platform this build runs on.
    pub fn current() -> Platform {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Other
        }
    }
}

/// The settings file inside the data folder.
pub const SETTINGS_FILE: &str = "settings.json";

/// The data folder for `platform`, reading variables through `env` (an empty value counts as
/// unset). Pure: tests pass their own `env`.
pub fn resolve_data_dir(
    platform: Platform,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    let get = |name: &str| env(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(p) = get("CRASTU_DATA") {
        return Ok(p);
    }
    let (found, missing) = match platform {
        Platform::Windows => (get("APPDATA").map(|a| a.join("Crastu")), "APPDATA"),
        Platform::MacOs => (
            get("HOME").map(|h| h.join("Library/Application Support/Crastu")),
            "HOME",
        ),
        Platform::Other => (
            get("XDG_DATA_HOME")
                .map(|x| x.join("crastu"))
                .or_else(|| get("HOME").map(|h| h.join(".local/share/crastu"))),
            "HOME",
        ),
    };
    found.ok_or_else(|| {
        format!("{missing} is not set, so there is no per-user data directory; set CRASTU_DATA")
    })
}

/// The data folder for this process, from the real environment.
pub fn data_dir() -> Result<PathBuf, String> {
    resolve_data_dir(Platform::current(), &|name| std::env::var_os(name))
}

/// The settings file for this process: `settings.json` in [`data_dir`].
pub fn settings_path() -> Result<PathBuf, String> {
    data_dir().map(|d| d.join(SETTINGS_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(platform: Platform, vars: &[(&str, &str)]) -> Result<PathBuf, String> {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        resolve_data_dir(platform, &|name| {
            vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
        })
    }

    #[test]
    fn crastu_data_wins_everywhere() {
        for p in [Platform::Windows, Platform::MacOs, Platform::Other] {
            let all = [
                ("CRASTU_DATA", "/d"),
                ("HOME", "/h"),
                ("APPDATA", "/a"),
                ("XDG_DATA_HOME", "/x"),
            ];
            assert_eq!(dir(p, &all).unwrap(), PathBuf::from("/d"), "{p:?}");
        }
    }

    #[test]
    fn per_user_folders() {
        assert_eq!(
            dir(Platform::MacOs, &[("HOME", "/Users/a")]).unwrap(),
            PathBuf::from("/Users/a/Library/Application Support/Crastu")
        );
        assert_eq!(
            dir(
                Platform::Windows,
                &[("APPDATA", "C:/Roaming"), ("HOME", "/h")]
            )
            .unwrap(),
            PathBuf::from("C:/Roaming/Crastu")
        );
        assert_eq!(
            dir(Platform::Other, &[("HOME", "/home/a")]).unwrap(),
            PathBuf::from("/home/a/.local/share/crastu")
        );
        assert_eq!(
            dir(
                Platform::Other,
                &[("HOME", "/home/a"), ("XDG_DATA_HOME", "/x")]
            )
            .unwrap(),
            PathBuf::from("/x/crastu")
        );
    }

    #[test]
    fn missing_or_empty_variables_ask_for_crastu_data() {
        for (p, vars) in [
            (Platform::MacOs, vec![]),
            (Platform::MacOs, vec![("HOME", "")]),
            (Platform::Windows, vec![("HOME", "/h")]),
            (Platform::Other, vec![]),
            (Platform::Other, vec![("CRASTU_DATA", ""), ("HOME", "")]),
        ] {
            let e = dir(p, &vars).unwrap_err();
            assert!(e.contains("CRASTU_DATA"), "{p:?}: {e}");
        }
    }

    #[test]
    fn the_settings_file_is_inside_the_data_folder() {
        let p = resolve_data_dir(Platform::Other, &|n| {
            (n == "CRASTU_DATA").then(|| OsString::from("/d"))
        })
        .unwrap()
        .join(SETTINGS_FILE);
        assert_eq!(p, PathBuf::from("/d/settings.json"));
    }
}
