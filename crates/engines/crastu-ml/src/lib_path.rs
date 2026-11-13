//! Where the ONNX Runtime library is looked for. Plain logic with the environment passed in, so
//! it is tested without a runtime (and compiles without the `ort` feature).
//!
//! Order (decision 28): the folder of the running executable first, where the app bundle puts the
//! one shared `libonnxruntime`, then the `ORT_DYLIB_PATH` variable (the same name `ort` itself
//! reads; tests set it on a child process with `Command::env`, because `set_var` is `unsafe`).
//! A relative `ORT_DYLIB_PATH` is tried against the executable folder first and then as given,
//! which is how `ort` resolves a relative path; both are in the list of paths tried.
//!
//! Security note. Next to the executable first is right for a signed app bundle: the folder is
//! under the bundle's signature and the variable cannot override a bundled library. But a
//! library in a user-writable install folder (a portable install, a development build) is
//! loaded with the app's privileges, and `ORT_DYLIB_PATH` is honoured in release builds too
//! (on macOS the hardened runtime's library validation is what stops an unsigned one; keep it
//! on). Nothing checks a hash or a signature of the file. What this module does: on Unix it
//! refuses a library, or a folder holding it, that group or others can write to (mode `g+w` or
//! `o+w`), with an error that names the path. Not done: a pinned sha256, and any check on
//! Windows.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};

/// The variable that names the runtime library, or a folder that holds it.
pub const ENV_VAR: &str = "ORT_DYLIB_PATH";

/// The file name of the runtime on this platform: `libonnxruntime.dylib`, `libonnxruntime.so` or
/// `onnxruntime.dll`.
pub fn library_file_name() -> String {
    format!(
        "{}onnxruntime{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    )
}

/// The places to try, in order. An empty variable counts as unset. A variable that names a
/// folder (`is_dir`, decided by the file system, never by the name) means the library file
/// inside it.
pub fn search_order(
    exe_dir: Option<&Path>,
    env_value: Option<&OsStr>,
    is_dir: impl Fn(&Path) -> bool,
) -> Vec<PathBuf> {
    let name = library_file_name();
    let mut order = Vec::new();
    if let Some(dir) = exe_dir {
        order.push(dir.join(&name));
    }
    if let Some(value) = env_value.filter(|v| !v.is_empty()) {
        let path = PathBuf::from(value);
        let mut bases = Vec::new();
        if path.is_relative()
            && let Some(dir) = exe_dir
        {
            bases.push(dir.join(&path));
        }
        bases.push(path);
        for base in bases {
            order.push(if is_dir(&base) {
                base.join(&name)
            } else {
                base
            });
        }
    }
    order
}

/// The library was not found in any place that was tried.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryNotFound {
    pub tried: Vec<PathBuf>,
}

impl fmt::Display for LibraryNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the ONNX Runtime library `{}` was not found; tried: ",
            library_file_name()
        )?;
        if self.tried.is_empty() {
            f.write_str("(nowhere: the executable folder is unknown and ")?;
            write!(f, "{ENV_VAR} is not set)")?;
        } else {
            for (i, path) in self.tried.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{}", path.display())?;
            }
        }
        write!(
            f,
            "; put the library next to the executable or set {ENV_VAR} to its path"
        )
    }
}

impl std::error::Error for LibraryNotFound {}

/// Why the library cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryError {
    NotFound(LibraryNotFound),
    /// The library or its folder can be written by group or others.
    Unsafe {
        path: PathBuf,
        writable: PathBuf,
    },
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(e) => e.fmt(f),
            Self::Unsafe { path, writable } => write!(
                f,
                "refusing to load the ONNX Runtime library `{}`: `{}` can be written by group or \
                 others; remove the group and world write permission",
                path.display(),
                writable.display()
            ),
        }
    }
}

impl std::error::Error for LibraryError {}

/// The first candidate for which `is_file` is true.
pub fn find(
    exe_dir: Option<&Path>,
    env_value: Option<&OsStr>,
    is_file: impl Fn(&Path) -> bool,
    is_dir: impl Fn(&Path) -> bool,
) -> Result<PathBuf, LibraryNotFound> {
    let tried = search_order(exe_dir, env_value, is_dir);
    tried
        .iter()
        .find(|p| is_file(p))
        .cloned()
        .ok_or(LibraryNotFound { tried })
}

/// `path` made absolute without touching the file system (unchanged if that fails), so what is
/// probed is what is loaded.
pub fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// On Unix: an error if the file or its folder can be written by group or others. Elsewhere, and
/// when the metadata cannot be read (the load then fails with its own message), nothing is
/// checked.
pub fn check_not_writable_by_others(path: &Path) -> Result<(), LibraryError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let writable = |p: &Path| {
            std::fs::metadata(p)
                .map(|m| m.permissions().mode() & 0o022 != 0)
                .unwrap_or(false)
        };
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        for candidate in std::iter::once(path).chain(parent) {
            if writable(candidate) {
                return Err(LibraryError::Unsafe {
                    path: path.to_path_buf(),
                    writable: candidate.to_path_buf(),
                });
            }
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// [`find`] against the real process (the folder of the current executable, the real variable
/// and the real file system), as an absolute path that passed [`check_not_writable_by_others`].
pub fn find_in_process() -> Result<PathBuf, LibraryError> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let env_value = std::env::var_os(ENV_VAR);
    let found = find(
        exe_dir.as_deref(),
        env_value.as_deref(),
        Path::is_file,
        Path::is_dir,
    )
    .map_err(LibraryError::NotFound)?;
    let found = absolute(&found);
    check_not_writable_by_others(&found)?;
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name() -> String {
        library_file_name()
    }

    fn no_dirs(_: &Path) -> bool {
        false
    }

    #[test]
    fn executable_folder_comes_before_the_variable() {
        let order = search_order(
            Some(Path::new("/app/bin")),
            Some(OsStr::new("/opt/ort/libx.dylib")),
            no_dirs,
        );
        assert_eq!(
            order,
            vec![
                Path::new("/app/bin").join(name()),
                PathBuf::from("/opt/ort/libx.dylib")
            ]
        );
    }

    #[test]
    fn empty_variable_is_unset() {
        assert!(search_order(None, Some(OsStr::new("")), no_dirs).is_empty());
    }

    #[test]
    fn a_folder_is_told_by_the_file_system_not_by_its_name() {
        // A versioned folder name has an "extension" (`.2`); it is still a folder.
        let folder = "/opt/onnxruntime-osx-arm64-1.28.2";
        let order = search_order(None, Some(OsStr::new(folder)), |p| p == Path::new(folder));
        assert_eq!(order, vec![Path::new(folder).join(name())]);
        // A path that is not a folder is taken as the file, whatever its name.
        let order = search_order(None, Some(OsStr::new("/opt/ort/lib")), no_dirs);
        assert_eq!(order, vec![PathBuf::from("/opt/ort/lib")]);
    }

    #[test]
    fn relative_variable_is_tried_against_the_executable_folder_then_as_given() {
        let order = search_order(
            Some(Path::new("/app/bin")),
            Some(OsStr::new("rt/libx.dylib")),
            no_dirs,
        );
        assert_eq!(
            order,
            vec![
                Path::new("/app/bin").join(name()),
                Path::new("/app/bin").join("rt/libx.dylib"),
                PathBuf::from("rt/libx.dylib"),
            ]
        );
    }

    #[test]
    fn first_existing_candidate_wins() {
        let exe = Path::new("/app/bin");
        let env = OsStr::new("/opt/ort/libonnxruntime.1.28.2.dylib");
        let in_exe = exe.join(name());
        let found = find(Some(exe), Some(env), |_| true, no_dirs).unwrap();
        assert_eq!(found, in_exe);
        let found = find(Some(exe), Some(env), |p| p != in_exe, no_dirs).unwrap();
        assert_eq!(found, PathBuf::from(env));
    }

    #[test]
    fn missing_library_names_every_path_tried() {
        let err = find(
            Some(Path::new("/app/bin")),
            Some(OsStr::new("/opt/ort/libx.dylib")),
            |_| false,
            no_dirs,
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains(&Path::new("/app/bin").join(name()).display().to_string()));
        assert!(text.contains("/opt/ort/libx.dylib"));
        assert!(text.contains(ENV_VAR));
        assert_eq!(err.tried.len(), 2);
    }

    #[test]
    fn nothing_to_try_is_still_a_clear_error() {
        let err = find(None, None, |_| true, no_dirs).unwrap_err();
        assert!(err.to_string().contains("nowhere"));
    }

    #[test]
    fn absolute_makes_a_relative_path_absolute() {
        assert!(absolute(Path::new("rt/libx.dylib")).is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn a_library_or_folder_others_can_write_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("crastu-ml-perm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(name());
        std::fs::write(&file, b"x").unwrap();
        let set = |p: &Path, mode| {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap()
        };

        set(&dir, 0o755);
        set(&file, 0o644);
        assert!(check_not_writable_by_others(&file).is_ok());

        set(&file, 0o666);
        let err = check_not_writable_by_others(&file).unwrap_err();
        assert!(err.to_string().contains("refusing"), "{err}");
        set(&file, 0o644);

        set(&dir, 0o777);
        let err = check_not_writable_by_others(&file).unwrap_err();
        match err {
            LibraryError::Unsafe { writable, .. } => assert_eq!(writable, dir),
            other => panic!("{other}"),
        }
        set(&dir, 0o755);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
