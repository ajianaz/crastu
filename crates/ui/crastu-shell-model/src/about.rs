//! Settings > About as plain data: what the page shows, what its two buttons ask for and how the
//! answers become toasts and clipboard text, and the command that reveals the log folder. The
//! shell draws it and carries the effects out (the toast stack, the clipboard); the desktop app
//! implements [`AboutPlatform`] on top of the flight recorder.

use std::ffi::OsString;
use std::path::Path;

/// What the page shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AboutInfo {
    pub version: String,
    /// For example `macOS (aarch64)`.
    pub os: String,
    /// The folder the flight recorder writes to; `None` when there is none to show.
    pub log_dir: Option<String>,
}

/// What About needs from the operating system.
pub trait AboutPlatform {
    fn about(&self) -> AboutInfo;

    /// Reveals the log folder in the file manager. Returns without waiting for it to open.
    fn open_logs(&self) -> Result<(), String>;

    /// The redacted diagnostics text for the clipboard.
    fn diagnostics(&self) -> Result<String, String>;
}

/// What a button of the page asks for. The owner applies it to the platform as it is then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AboutIntent {
    OpenLogs,
    CopyDiagnostics,
}

/// What the owner has to do after [`apply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AboutEffect {
    /// Put this text on the clipboard.
    Clipboard(String),
    /// Show a toast; `true` is a success.
    Toast(bool, String),
}

/// Carries out an intent on the platform and says what to show.
pub fn apply(intent: AboutIntent, platform: &dyn AboutPlatform) -> Vec<AboutEffect> {
    match intent {
        AboutIntent::OpenLogs => match platform.open_logs() {
            Ok(()) => Vec::new(),
            Err(why) => vec![AboutEffect::Toast(
                false,
                format!("Could not open the log folder: {why}"),
            )],
        },
        AboutIntent::CopyDiagnostics => match platform.diagnostics() {
            Ok(text) => vec![
                AboutEffect::Clipboard(text),
                AboutEffect::Toast(
                    true,
                    "Diagnostics copied. Nothing was sent anywhere.".into(),
                ),
            ],
            Err(why) => vec![AboutEffect::Toast(
                false,
                format!("Could not build the diagnostics: {why}"),
            )],
        },
    }
}

/// A program and its arguments, to be run without a shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenCommand {
    pub program: &'static str,
    pub args: Vec<OsString>,
}

/// The command that shows `folder` in the file manager on `os` (`std::env::consts::OS`): `open`
/// on macOS, `explorer` on Windows, `xdg-open` elsewhere. The path is one argument, never part of
/// a command string, and an empty path gives nothing.
pub fn open_command(os: &str, folder: &Path) -> Option<OpenCommand> {
    if folder.as_os_str().is_empty() {
        return None;
    }
    let program = match os {
        "macos" => "open",
        "windows" => "explorer",
        _ => "xdg-open",
    };
    Some(OpenCommand {
        program,
        args: vec![folder.as_os_str().to_owned()],
    })
}

/// A readable name for `std::env::consts::OS` and `ARCH`: `macOS (aarch64)`.
pub fn os_label(os: &str, arch: &str) -> String {
    let name = match os {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!("{name} ({arch})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        open: Result<(), String>,
        text: Result<String, String>,
        calls: RefCell<Vec<&'static str>>,
    }

    impl AboutPlatform for Fake {
        fn about(&self) -> AboutInfo {
            AboutInfo::default()
        }
        fn open_logs(&self) -> Result<(), String> {
            self.calls.borrow_mut().push("open");
            self.open.clone()
        }
        fn diagnostics(&self) -> Result<String, String> {
            self.calls.borrow_mut().push("diagnostics");
            self.text.clone()
        }
    }

    fn fake(open: Result<(), String>, text: Result<String, String>) -> Fake {
        Fake {
            open,
            text,
            calls: RefCell::default(),
        }
    }

    #[test]
    fn copying_puts_the_text_on_the_clipboard_and_says_so() {
        let p = fake(Ok(()), Ok("report".into()));
        let effects = apply(AboutIntent::CopyDiagnostics, &p);
        assert_eq!(effects[0], AboutEffect::Clipboard("report".into()));
        assert!(matches!(&effects[1], AboutEffect::Toast(true, m) if m.contains("copied")));
        assert_eq!(*p.calls.borrow(), ["diagnostics"]);
    }

    #[test]
    fn a_failure_is_a_toast_and_never_touches_the_clipboard() {
        let p = fake(Err("no file manager".into()), Err("disk".into()));
        for intent in [AboutIntent::OpenLogs, AboutIntent::CopyDiagnostics] {
            let effects = apply(intent, &p);
            assert_eq!(effects.len(), 1);
            assert!(
                matches!(&effects[0], AboutEffect::Toast(false, m) if m.starts_with("Could not")),
                "{effects:?}"
            );
        }
    }

    #[test]
    fn opening_the_folder_succeeds_quietly() {
        let p = fake(Ok(()), Err("unused".into()));
        assert!(apply(AboutIntent::OpenLogs, &p).is_empty());
        assert_eq!(*p.calls.borrow(), ["open"]);
    }

    #[test]
    fn each_os_has_its_own_command_and_the_path_is_one_argument() {
        let dir = Path::new("/Users/a b/Library/Logs/Crastu; rm -rf ~");
        let args = vec![dir.as_os_str().to_owned()];
        for (os, program) in [
            ("macos", "open"),
            ("windows", "explorer"),
            ("linux", "xdg-open"),
            ("freebsd", "xdg-open"),
        ] {
            let c = open_command(os, dir).unwrap();
            assert_eq!((c.program, &c.args), (program, &args), "{os}");
        }
        assert_eq!(open_command("macos", Path::new("")), None);
    }

    #[test]
    fn the_os_has_a_readable_name() {
        assert_eq!(os_label("macos", "aarch64"), "macOS (aarch64)");
        assert_eq!(os_label("haiku", "x86_64"), "haiku (x86_64)");
    }
}
