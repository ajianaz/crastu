//! Which file names a catalogue entry may use: plain names that are valid on every platform
//! Crastu runs on, and that cannot collide with the downloader's own `.part` and `.lock` files.

/// Characters Windows refuses in a file name (and that are path syntax elsewhere).
const INVALID: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Windows device names; they are reserved with or without an extension, in any case.
const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];

fn is_reserved(name: &str) -> bool {
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return true;
    }
    ["com", "lpt"].iter().any(|p| {
        stem.strip_prefix(p)
            .is_some_and(|n| n.len() == 1 && n.chars().all(|c| c.is_ascii_digit()))
    })
}

/// Why `name` cannot be a download file name, or `None` when it is fine.
pub(crate) fn problem(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if name.is_empty() || name == "." || name == ".." {
        Some("file_name must not be empty, . or ..")
    } else if name.chars().any(|c| INVALID.contains(&c) || c.is_control()) {
        Some(
            "file_name must be a plain name: no path separators, ':' or other characters Windows refuses",
        )
    } else if name.ends_with('.') || name.ends_with(' ') {
        Some("file_name must not end with a dot or a space")
    } else if lower.ends_with(".part") || lower.ends_with(".lock") {
        Some("file_name must not end in .part or .lock (the downloader uses them)")
    } else if is_reserved(name) {
        Some("file_name is a reserved device name on Windows")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::problem;

    #[test]
    fn plain_names_pass() {
        for ok in [
            "silero_vad.onnx",
            "sherpa-onnx-whisper-small.tar.bz2",
            "a",
            "console.onnx",
            "com10.bin",
            "lpt.bin",
        ] {
            assert_eq!(problem(ok), None, "{ok}");
        }
    }

    #[test]
    fn unsafe_names_are_refused() {
        for bad in [
            "",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "C:foo",
            "a:b",
            "a*b",
            "a?b",
            "a\"b",
            "a<b",
            "a>b",
            "a|b",
            "a\0b",
            "a\nb",
            "name.",
            "name ",
            "m.part",
            "m.PART",
            "m.Part",
            "m.lock",
            "m.LOCK",
            "CON",
            "con",
            "Nul.txt",
            "aux.onnx",
            "PRN",
            "COM1",
            "com9.bin",
            "LPT3",
            "lpt1.tar.gz",
            "con .txt",
        ] {
            assert!(problem(bad).is_some(), "{bad:?} must be refused");
        }
    }
}
