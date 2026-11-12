//! Redaction of known secret shapes in free text, and validation of plain names, for logs.
//! A safety net, not a promise: see the `crastu-recorder` docs for what is and is not guaranteed.
//! Runs on text that may be inside a panic hook, so it must never panic: every slice starts and
//! ends next to an ASCII byte that was matched, which is always a character boundary.
//!
//! Work is linear in the input: a candidate is decided by looking at no more than [`DECIDE`]
//! bytes (a JWT by one scan of its run, cached for the positions inside it; a name by one scan
//! of its run), a match is consumed once, and input past [`MAX_INPUT`] is cut off before
//! anything runs and marked as cut. The cut backs up to the last whitespace, so a token or
//! value that straddles it is dropped whole instead of leaving its first characters.
//!
//! What is covered, in `shapes` (known token formats, JWTs, URL userinfo), `names` (the values
//! of secret names in `name=value`, `name: value`, JSON, escaped JSON and shell flag forms) and
//! `blocks` (PEM private keys, `mysql -p` style flags). Doubt goes to over-redaction.

mod blocks;
mod names;
mod shapes;
mod values;

use blocks::{ShellCtx, pem_at, word_start_prev};
use names::kv_at;
use shapes::{JwtRun, secret_at, userinfo_len};

#[cfg(test)]
use names::split_words;

const MASK: &str = "[redacted]";

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

/// Space, tab, line breaks.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// Space and tab: whitespace that does not leave the line.
fn is_blank(b: u8) -> bool {
    matches!(b, b' ' | b'\t')
}

/// How many bytes a candidate looks at to decide whether it is a secret.
const DECIDE: usize = 64;
/// The most input `redact` processes; the rest is dropped.
pub const MAX_INPUT: usize = 64 * 1024;
const TRUNCATED: &str = " [truncated]";

fn run(b: &[u8], from: usize, ok: impl Fn(u8) -> bool) -> usize {
    b.get(from..)
        .map_or(0, |rest| rest.iter().take_while(|c| ok(**c)).count())
}

/// [`run`], looking at no more than [`DECIDE`] bytes.
fn run_cap(b: &[u8], from: usize, ok: impl Fn(u8) -> bool) -> usize {
    b.get(from..).map_or(0, |rest| {
        rest.iter().take(DECIDE).take_while(|c| ok(**c)).count()
    })
}

/// `input` with every known secret shape replaced by `[redacted]`. Input over [`MAX_INPUT`]
/// bytes is cut before anything else, back to the last whitespace before the limit (so no token
/// or value is left half-cut; input without any whitespace is dropped entirely), and
/// ` [truncated]` is added.
pub fn redact(input: &str) -> String {
    if input.len() <= MAX_INPUT {
        return redact_all(input);
    }
    let mut cut = MAX_INPUT;
    while !input.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = input.get(..cut).unwrap_or_default();
    let cut = head.rfind(char::is_whitespace).unwrap_or(0);
    let mut out = redact_all(head.get(..cut).unwrap_or_default());
    out.push_str(TRUNCATED);
    out
}

fn redact_all(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut jwt = JwtRun::default();
    let mut shell = ShellCtx::default();
    let mut i = 0;
    while i < input.len() {
        let Some(rest) = input.get(i..) else { break };
        if let Some(n) = pem_at(rest) {
            out.push_str("[redacted private key]");
            i += n;
            continue;
        }
        if let Some(after) = rest.strip_prefix("://")
            && let Some(n) = userinfo_len(after)
        {
            out.push_str("://");
            out.push_str(MASK);
            out.push('@');
            i += 3 + n;
            continue;
        }
        let prev = i.checked_sub(1).and_then(|p| b.get(p)).copied();
        shell.note(b, i);
        if prev.is_none_or(word_start_prev)
            && let Some((n, replacement)) = shell.short_flag(rest)
        {
            out.push_str(&replacement);
            i += n;
            continue;
        }
        if prev.is_none_or(|p| !is_ident_byte(p))
            && let Some((n, replacement)) = kv_at(rest, shell.quote_before(b, i))
        {
            out.push_str(&replacement);
            i += n;
            continue;
        }
        if prev.is_none_or(|p| !p.is_ascii_alphanumeric()) {
            if rest.starts_with("eyJ")
                && let Some(n) = jwt.len_at(b, i)
            {
                out.push_str(MASK);
                i += n;
                continue;
            }
            if let Some((n, replacement)) = secret_at(rest) {
                out.push_str(&replacement);
                i += n;
                continue;
            }
        }
        let Some(ch) = rest.chars().next() else { break };
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `input` redacted and cut to at most `max` characters.
pub fn clean(input: &str, max: usize) -> String {
    let r = redact(input);
    if r.chars().count() <= max {
        return r;
    }
    let mut cut: String = r.chars().take(max).collect();
    cut.push('…');
    cut
}

/// Whether `input` is a plain name: ASCII letters, digits and `_:-`, one to 64 bytes, and not a
/// secret shape. No dots, slashes or backslashes, so a name can never be a path, `.` or `..`.
pub fn name_ok(input: &str) -> bool {
    !input.is_empty()
        && input.len() <= 64
        && input
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'-'))
        && redact(input) == input
}

/// `input` when it is a plain name, otherwise `invalid-name`: names are the only free-form
/// strings that job, MCP and perf records take, and nothing else gets through.
pub fn ident(input: &str) -> String {
    if name_ok(input) {
        input.to_string()
    } else {
        "invalid-name".to_string()
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_more;
#[cfg(test)]
mod tests_shell;
