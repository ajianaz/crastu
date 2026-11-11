//! `cargo xtask ui-literals`: no colour or font literal in a UI crate outside the token module.
//!
//! Every colour, font and size of the interface comes from `crastu-ui`'s `tokens` module, which
//! a test compares with `docs/design/tokens.json`. A hex colour or a font name written in a
//! component would bypass that, so this check fails on one in any crate of kind `ui` or
//! `feature-ui`, in all of the crate's Rust files (`src`, `tests`, `examples`, `benches`,
//! `build.rs`). It reads each file as a whole, so multi-line strings and calls, block comments
//! and raw strings are handled; comments are not scanned. A line that really needs a literal can
//! say `ui-literals: allow` in a comment on that line; the allowed lines are counted in the
//! output so they stay visible. Like `size-check` it does not follow symlinks and skips folders
//! named `target` or `vendor` and hidden ones.
//!
//! What counts, in code: a hex integer of 6 or 8 digits (`0x111214`, also with a suffix such as
//! `u32`), `rgb(`, `rgba(`, `hsl(` and `hsla(` with a number or a hex literal of any length, and
//! the named colour functions (`white()`, `red()` and so on). In strings: `#RGB`, `#RGBA`,
//! `#RRGGBB` or `#RRGGBBAA` (a run of only digits is not a short colour), a colour function with
//! numbers, and a font family name from a list of the app's fonts and common system fonts.
//!
//! The token module is exempt only in `crastu-ui` itself (`crates/ui/crastu-ui/src/tokens`).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const ALLOW_MARKER: &str = "ui-literals: allow";
const TOKEN_MODULE: &str = "crates/ui/crastu-ui/src/tokens";
const NAMED_COLOURS: &[&str] = &["white", "black", "red", "green", "blue", "yellow"];
const COLOUR_FUNCTIONS: &[&str] = &["rgba", "rgb", "hsla", "hsl"];
const INT_SUFFIXES: &[&str] = &[
    "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
];
const FONT_NAMES: &[&str] = &[
    "Inter",
    "JetBrains Mono",
    "Space Grotesk",
    "IBM Plex",
    "SF Pro",
    "SF Mono",
    "San Francisco",
    "Helvetica",
    "Arial",
    "Menlo",
    "Monaco",
    "Courier",
    "Roboto",
    "Segoe UI",
    "Consolas",
    "Cascadia",
    "Noto Sans",
    "DejaVu Sans",
    "Fira Code",
    "Source Code Pro",
    "Lucida",
    "Tahoma",
    "Verdana",
    "Georgia",
    "Times New Roman",
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
    /// Lines carrying the allow marker.
    pub allowed: usize,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A source file with comments removed and string contents set aside. `code` keeps every
/// newline, so an index into it maps to the right line; a string becomes `""` in `code` and its
/// content goes to `strings` with the line it starts on.
struct Lexed {
    code: String,
    strings: Vec<(usize, String)>,
}

fn lex(text: &str) -> Lexed {
    let chars: Vec<char> = text.chars().collect();
    let (mut code, mut strings) = (String::new(), Vec::new());
    let (mut i, mut line) = (0, 1);
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 1;
            i += 2;
            while i < chars.len() && depth > 0 {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    if chars[i] == '\n' {
                        code.push('\n');
                        line += 1;
                    }
                    i += 1;
                }
            }
        } else if let Some((start, hashes)) = raw_string_start(&chars, i) {
            // r"..." and r#"..."# (also br): ends at a quote followed by the same hashes.
            let first_line = line;
            let mut content = String::new();
            i = start;
            while i < chars.len() {
                if chars[i] == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
                    i += 1 + hashes;
                    break;
                }
                if chars[i] == '\n' {
                    code.push('\n');
                    line += 1;
                }
                content.push(chars[i]);
                i += 1;
            }
            code.push_str("\"\"");
            strings.push((first_line, content));
        } else if c == '"' {
            let first_line = line;
            let mut content = String::new();
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                }
                if chars[i] == '\n' {
                    code.push('\n');
                    line += 1;
                }
                content.push(chars[i]);
                i += 1;
            }
            i += 1;
            code.push_str("\"\"");
            strings.push((first_line, content));
        } else if c == '\'' && next == Some('\\') {
            // An escaped char literal such as '\n' or '\u{1F600}'.
            i += 2;
            while i < chars.len() && chars[i] != '\'' && chars[i] != '\n' {
                i += 1;
            }
            i += 1;
            code.push_str("' '");
        } else if c == '\'' && chars.get(i + 2) == Some(&'\'') {
            i += 3;
            code.push_str("' '");
        } else {
            if c == '\n' {
                line += 1;
            }
            code.push(c);
            i += 1;
        }
    }
    Lexed { code, strings }
}

/// If a raw string literal (`r"`, `r#"`, `br"`, `br#"`) starts at `i`, the index of its first
/// content character and the number of `#`.
fn raw_string_start(chars: &[char], i: usize) -> Option<(usize, usize)> {
    if i > 0 && is_ident(chars[i - 1]) {
        return None;
    }
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let hashes = chars[j..].iter().take_while(|c| **c == '#').count();
    (chars.get(j + hashes) == Some(&'"')).then_some((j + hashes + 1, hashes))
}

fn line_of(code: &str, byte_index: usize) -> usize {
    code[..byte_index].matches('\n').count() + 1
}

/// Each `0x` literal in `code` with 6 or 8 digits (a type suffix such as `u32` is allowed).
fn hex_colours(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find("0x") {
        let pos = from + at;
        from = pos + 2;
        if code[..pos].chars().next_back().is_some_and(is_ident) {
            continue;
        }
        let digits: String = code[pos + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '_')
            .collect();
        let rest = &code[pos + 2 + digits.len()..];
        let rest = INT_SUFFIXES
            .iter()
            .find_map(|s| rest.strip_prefix(s))
            .unwrap_or(rest);
        let count = digits.chars().filter(|c| *c != '_').count();
        if (count == 6 || count == 8) && !rest.chars().next().is_some_and(is_ident) {
            found.push((pos, format!("0x{digits}")));
        }
    }
    found
}

/// Calls of `rgb(`, `rgba(`, `hsl(` and `hsla(` whose first argument is a number or a hex
/// literal. The argument may start on a later line.
fn colour_calls(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for name in COLOUR_FUNCTIONS {
        let mut from = 0;
        while let Some(at) = code[from..].find(&format!("{name}(")) {
            let pos = from + at;
            from = pos + name.len() + 1;
            if code[..pos].chars().next_back().is_some_and(is_ident) {
                continue;
            }
            let after = code[from..].trim_start();
            let numeric = after
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-');
            // A 6 or 8 digit hex literal is reported on its own by `hex_colours`.
            let reported_as_hex =
                after.starts_with("0x") && hex_colours(after).first().is_some_and(|(p, _)| *p == 0);
            if numeric && !reported_as_hex {
                found.push((pos, format!("{name}(..)")));
            }
        }
    }
    found
}

fn named_colours(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for name in NAMED_COLOURS {
        let call = format!("{name}()");
        let mut from = 0;
        while let Some(at) = code[from..].find(&call) {
            let pos = from + at;
            from = pos + call.len();
            if !code[..pos].chars().next_back().is_some_and(is_ident) {
                found.push((pos, call.clone()));
            }
        }
    }
    found
}

/// `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA` inside a string; a run of only digits is not a
/// colour (it is more likely an issue number).
fn hex_in_string(s: &str) -> Option<String> {
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c != '#' || (i > 0 && is_ident(chars[i - 1])) {
            continue;
        }
        let digits: String = chars[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        let next = chars.get(i + 1 + digits.len());
        let has_letter = digits.chars().any(|c| c.is_ascii_alphabetic());
        let long = digits.len() == 6 || digits.len() == 8;
        let short = (digits.len() == 3 || digits.len() == 4) && has_letter;
        if (long || short) && !next.is_some_and(|c| is_ident(*c)) {
            return Some(format!("#{digits}"));
        }
    }
    None
}

fn colour_call_in_string(s: &str) -> Option<String> {
    colour_calls(s).into_iter().next().map(|(_, what)| what)
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
    let lexed = lex(text);
    let marked: Vec<bool> = text.lines().map(|l| l.contains(ALLOW_MARKER)).collect();
    let mut scan = Scan {
        allowed: marked.iter().filter(|m| **m).count(),
        ..Scan::default()
    };
    let mut found: Vec<(usize, String)> = Vec::new();
    let in_code = [
        hex_colours(&lexed.code),
        colour_calls(&lexed.code),
        named_colours(&lexed.code),
    ];
    for (index, what) in in_code.into_iter().flatten() {
        let line = line_of(&lexed.code, index);
        let label = if what.starts_with("0x") || what.contains('(') && !what.ends_with("()") {
            format!("colour literal `{what}`")
        } else {
            format!("named colour `{what}`")
        };
        found.push((line, label));
    }
    for (line, content) in &lexed.strings {
        if let Some(h) = hex_in_string(content) {
            found.push((*line, format!("colour literal \"{h}\"")));
        }
        if let Some(c) = colour_call_in_string(content) {
            found.push((*line, format!("colour literal \"{c}\"")));
        }
        if let Some(f) = font_in_string(content) {
            found.push((*line, format!("font name \"{f}\"")));
        }
    }
    found.sort();
    for (line, what) in found {
        if marked.get(line - 1).copied().unwrap_or(false) {
            continue;
        }
        scan.findings.push(Finding {
            path: path.to_string(),
            line,
            what,
        });
    }
    scan
}

/// The token module itself is where the literals live, in `crastu-ui` only.
fn is_token_module(relative: &str) -> bool {
    relative == format!("{TOKEN_MODULE}.rs") || relative.starts_with(&format!("{TOKEN_MODULE}/"))
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
        let name = p["name"].as_str().unwrap_or("?");
        let Some(dir) = p["manifest_path"]
            .as_str()
            .and_then(|m| Path::new(m).parent())
        else {
            continue;
        };
        crates += 1;
        let mut paths = Vec::new();
        // Fail closed: a crate that cannot be read must not count as clean.
        crate::size::walk(dir, &mut paths).map_err(|e| format!("{name}: {e}"))?;
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
    if crates == 0 {
        // The check would pass without looking at anything.
        return Err("no crate of kind ui or feature-ui was found: check the metadata".into());
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
