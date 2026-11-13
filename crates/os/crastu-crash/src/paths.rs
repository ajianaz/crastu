//! What the handler needs, prepared at install time: the absolute path of `crash.txt` as a
//! NUL-terminated byte string, and the session id. The handler cannot allocate, so both are
//! built here, once, and kept for the life of the process.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::{CRASH_FILE, InstallError};

/// The longest path accepted, NUL included. macOS's `PATH_MAX` is 1024 (Linux allows 4096); a
/// longer path would fail in `open` anyway.
pub(crate) const MAX_PATH: usize = 1024;
/// The longest session id kept. The recorder's ids are `<UTC stamp>-<pid>`, about 30 bytes.
pub(crate) const MAX_SESSION: usize = 128;

/// `<session_dir>/crash.txt`, absolute and NUL-terminated. Relative folders are made absolute
/// against the current directory now, because the handler must not depend on a later `chdir`.
pub(crate) fn crash_path(session_dir: &Path) -> Result<Box<[u8]>, InstallError> {
    let dir = std::path::absolute(session_dir).map_err(|_| InstallError::BadPath)?;
    let path = dir.join(CRASH_FILE);
    let bytes = path.as_os_str().as_bytes();
    if bytes.contains(&0) {
        return Err(InstallError::BadPath);
    }
    if bytes.len() >= MAX_PATH {
        return Err(InstallError::PathTooLong);
    }
    let mut v = Vec::with_capacity(bytes.len() + 1);
    v.extend_from_slice(bytes);
    v.push(0);
    Ok(v.into_boxed_slice())
}

/// The session id: the session folder's own name, with anything but letters, digits and `_:.-`
/// replaced by `_` and cut to [`MAX_SESSION`] bytes, so `crash.txt` holds a plain name even when
/// the folder was named by someone else.
pub(crate) fn session_id(session_dir: &Path) -> Box<[u8]> {
    let name = session_dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let id: Vec<u8> = name
        .bytes()
        .take(MAX_SESSION)
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"_:.-".contains(&b) {
                b
            } else {
                b'_'
            }
        })
        .collect();
    if id.is_empty() {
        return Box::from(&b"unknown"[..]);
    }
    id.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_absolute_and_nul_terminated() {
        let p = crash_path(Path::new("/tmp/logs/crastu-cli/20261112T101500Z-42")).unwrap();
        assert_eq!(&*p, b"/tmp/logs/crastu-cli/20261112T101500Z-42/crash.txt\0");
        let rel = crash_path(Path::new("rel/dir")).unwrap();
        assert_eq!(rel.first(), Some(&b'/'));
        assert!(rel.ends_with(b"/rel/dir/crash.txt\0"), "{rel:?}");
        assert_eq!(rel.iter().filter(|&&b| b == 0).count(), 1);
    }

    #[test]
    fn a_path_with_nul_or_too_long_is_refused() {
        let nul = std::ffi::OsStr::from_bytes(b"/tmp/a\0b");
        assert_eq!(crash_path(Path::new(nul)), Err(InstallError::BadPath));
        let long = format!("/{}", "a".repeat(MAX_PATH));
        assert_eq!(crash_path(Path::new(&long)), Err(InstallError::PathTooLong));
        // One byte under the limit with the NUL still fits.
        let dir_len = MAX_PATH - 1 - "/crash.txt".len();
        let fits = format!("/{}", "a".repeat(dir_len - 1));
        assert_eq!(crash_path(Path::new(&fits)).unwrap().len(), MAX_PATH);
    }

    #[test]
    fn the_session_id_is_a_plain_name() {
        assert_eq!(
            &*session_id(Path::new("/l/crastu-cli/20261112T101500Z-42")),
            b"20261112T101500Z-42"
        );
        assert_eq!(&*session_id(Path::new("/l/a b/c\nd$e")), b"c_d_e");
        assert_eq!(&*session_id(Path::new("/")), b"unknown");
        let long = format!("/l/{}", "x".repeat(500));
        assert_eq!(session_id(Path::new(&long)).len(), MAX_SESSION);
    }
}
