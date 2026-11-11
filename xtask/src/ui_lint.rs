//! `cargo xtask ui-literals`: no colour or font literal in a UI crate outside the token module.
//!
//! Every colour, font and size of the interface comes from `crastu-ui`'s `tokens` module, which
//! a test compares with `docs/design/tokens.json`. A hex colour or a font name written in a
//! component would bypass that, so this check fails on one in any crate of kind `ui` or
//! `feature-ui`. It looks at code and string literals, not comments. A line that really needs a
//! literal can say `ui-literals: allow` in a trailing comment; the allowed lines are counted in
//! the output so they stay visible.
//!
//! What counts: a hex integer of 6 or 8 digits (`0x111214`), `rgb(`, `rgba(`, `hsl(` and `hsla(`
//! with a number, the named colour functions (`white()`, `red()` and so on), a string with
//! `#RRGGBB` or `#RRGGBBAA`, and a string naming one of the app's font families or a common
//! system font.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const ALLOW_MARKER: &str = "ui-literals: allow";
const NAMED_COLOURS: &[&str] = &["white", "black", "red", "green", "blue", "yellow"];
const FONT_NAMES: &[&str] = &[
    "Inter",
    "JetBrains Mono",
    "Space Grotesk",
    "IBM Plex",
    "SF Pro",
    "Helvetica",
    "Arial",
    "Menlo",
    "Monaco",
    "Courier",
    "Roboto",
];

#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub path: String,
    pub line: usize,
    pub what: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Scan {
    pub findings: Vec<Finding>,
    /// Lines skipped because of the allow marker.
    pub allowed: usize,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Splits one line into its code (strings emptied) and the contents of its string literals,
/// dropping a trailing `//` comment. Char literals such as `'"'` are skipped.
fn split_line(line: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = line.chars().collect();
    let (mut code, mut strings) = (String::new(), Vec::new());
    let (mut i, mut current): (usize, Option<String>) = (0, None);
    while i < chars.len() {
        let c = chars[i];
        match current.as_mut() {
            Some(s) => {
                if c == '\\' && i + 1 < chars.len() {
                    s.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == '"' {
                    strings.push(current.take().unwrap_or_default());
                    code.push_str("\"\"");
                } else {
                    s.push(c);
                }
            }
            None => {
                if c == '/' && chars.get(i + 1) == Some(&'/') {
                    break;
                }
                if c == '"' {
                    current = Some(String::new());
                } else if c == '\'' && chars.get(i + 2) == Some(&'\'') {
                    code.push_str("' '");
                    i += 3;
                    continue;
                } else if c == '\'' && chars.get(i + 1) == Some(&'\\') {
                    let end = chars[i + 2..].iter().position(|&x| x == '\'');
                    if let Some(end) = end {
                        code.push_str("' '");
                        i += end + 3;
                        continue;
                    }
                    code.push(c);
                } else {
                    code.push(c);
                }
            }
        }
        i += 1;
    }
    if let Some(s) = current {
        strings.push(s);
    }
    (code, strings)
}

/// A `0x` literal with 6 or 8 hex digits, as written (`0x111214`, `0x11_12_14`).
fn hex_colour(code: &str) -> Option<String> {
    let bytes: Vec<char> = code.chars().collect();
    for start in 0..bytes.len().saturating_sub(1) {
        if bytes[start] != '0' || bytes[start + 1] != 'x' {
            continue;
        }
        if start > 0 && is_ident(bytes[start - 1]) {
            continue;
        }
        let digits: String = bytes[start + 2..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit() || **c == '_')
            .collect();
        let count = digits.chars().filter(|c| *c != '_').count();
        let next = bytes.get(start + 2 + digits.chars().count());
        if (count == 6 || count == 8) && !next.is_some_and(|c| is_ident(*c)) {
            return Some(format!("0x{digits}"));
        }
    }
    None
}

/// A call of `rgb(`, `rgba(`, `hsl(` or `hsla(` whose first argument is a number.
fn colour_call(code: &str) -> Option<String> {
    for name in ["rgba", "rgb", "hsla", "hsl"] {
        let mut from = 0;
        while let Some(at) = code[from..].find(&format!("{name}(")) {
            let pos = from + at;
            let before = code[..pos].chars().next_back();
            let after = code[pos + name.len() + 1..].trim_start();
            // A hex literal is reported on its own by `hex_colour`.
            let numeric = after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-');
            if !before.is_some_and(is_ident) && numeric && !after.starts_with("0x") {
                return Some(format!("{name}(..)"));
            }
            from = pos + name.len() + 1;
        }
    }
    None
}

fn named_colour(code: &str) -> Option<String> {
    for name in NAMED_COLOURS {
        let call = format!("{name}()");
        let mut from = 0;
        while let Some(at) = code[from..].find(&call) {
            let pos = from + at;
            if !code[..pos].chars().next_back().is_some_and(is_ident) {
                return Some(call);
            }
            from = pos + call.len();
        }
    }
    None
}

/// `#RRGGBB` or `#RRGGBBAA` inside a string.
fn hex_in_string(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '#' {
            continue;
        }
        let digits: String = chars[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        let next = chars.get(i + 1 + digits.len());
        if (digits.len() == 6 || digits.len() == 8) && !next.is_some_and(|c| is_ident(*c)) {
            return Some(format!("#{digits}"));
        }
    }
    None
}

/// A font name as a whole word inside a string.
fn font_in_string(s: &str) -> Option<&'static str> {
    for name in FONT_NAMES {
        let mut from = 0;
        while let Some(at) = s[from..].find(name) {
            let pos = from + at;
            let before = s[..pos].chars().next_back();
            let after = s[pos + name.len()..].chars().next();
            if !before.is_some_and(is_ident) && !after.is_some_and(is_ident) {
                return Some(name);
            }
            from = pos + name.len();
        }
    }
    None
}

pub fn check_source(path: &str, text: &str) -> Scan {
    let mut scan = Scan::default();
    for (i, line) in text.lines().enumerate() {
        let no = i + 1;
        if line.contains(ALLOW_MARKER) {
            scan.allowed += 1;
            continue;
        }
        let (code, strings) = split_line(line);
        let mut found = Vec::new();
        if let Some(h) = hex_colour(&code) {
            found.push(format!("colour literal `{h}`"));
        }
        if let Some(c) = colour_call(&code) {
            found.push(format!("colour literal `{c}`"));
        }
        if let Some(n) = named_colour(&code) {
            found.push(format!("named colour `{n}`"));
        }
        for s in &strings {
            if let Some(h) = hex_in_string(s) {
                found.push(format!("colour literal \"{h}\""));
            }
            if let Some(f) = font_in_string(s) {
                found.push(format!("font name \"{f}\""));
            }
        }
        for what in found {
            scan.findings.push(Finding {
                path: path.to_string(),
                line: no,
                what,
            });
        }
    }
    scan
}

/// The token module itself is where the literals live.
fn is_token_module(relative: &str) -> bool {
    relative.contains("/src/tokens/") || relative.ends_with("/src/tokens.rs")
}

pub fn run(meta: &Value) -> Result<(), String> {
    let root = PathBuf::from(meta["workspace_root"].as_str().ok_or("no workspace_root")?);
    let (mut crates, mut files, mut allowed) = (0, 0, 0);
    let mut findings = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let kind = p["metadata"]["crastu"]["kind"].as_str().unwrap_or_default();
        if kind != "ui" && kind != "feature-ui" {
            continue;
        }
        let Some(dir) = p["manifest_path"]
            .as_str()
            .and_then(|m| Path::new(m).parent())
        else {
            continue;
        };
        crates += 1;
        let mut paths = Vec::new();
        // Fail closed: a crate that cannot be read must not count as clean.
        crate::size::walk(&dir.join("src"), &mut paths)?;
        paths.sort();
        for path in paths {
            let relative = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if is_token_module(&relative) {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|e| format!("{relative}: {e}"))?;
            let scan = check_source(&relative, &text);
            files += 1;
            allowed += scan.allowed;
            findings.extend(scan.findings);
        }
    }
    if findings.is_empty() {
        println!(
            "ui-literals: {crates} UI crates, {files} files, no colour or font literal outside the tokens ({allowed} line(s) allowed)"
        );
        return Ok(());
    }
    for f in &findings {
        println!(
            "violation: {}:{}: {}; read it from the tokens instead",
            f.path, f.line, f.what
        );
    }
    Err(format!("{} literal(s) in UI code", findings.len()))
}

#[cfg(test)]
mod tests;
