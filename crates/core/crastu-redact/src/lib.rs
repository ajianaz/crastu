//! Redaction of known secret shapes in free text, and validation of plain names, for logs.
//! A safety net, not a promise: see the `crastu-recorder` docs for what is and is not guaranteed.
//! Runs on text that may be inside a panic hook, so it must never panic: every slice starts and
//! ends next to an ASCII byte that was matched, which is always a character boundary.
//!
//! Work is linear in the input: a candidate is decided by looking at no more than [`DECIDE`]
//! bytes (a JWT by one scan of its run, cached for the positions inside it), a match is consumed
//! once, and input past [`MAX_INPUT`] is cut off before anything runs and marked as cut. The cut
//! backs up to the last whitespace, so a token or value that straddles it is dropped whole
//! instead of leaving its first characters.
//!
//! Secret names (`password`, `accessToken`, `client-secret`, `API_KEY`, `Set-Cookie`...) are
//! recognised by their words: the name is split at `_`, `-`, `.` and camelCase boundaries, and
//! it counts when a word is, or ends with, a secret word (`token`, `secret`, `password`...), or
//! is `key` after `api`, `access`, `private` and the like. A name whose last word is `count`,
//! `length`, `size`, `type`, `limit`, `ttl` or `expires` is not a secret (`token_count: 5` stays
//! readable). Otherwise doubt goes to over-redaction: `password_policy: strict` loses its value.

const MASK: &str = "[redacted]";

/// Token prefixes and the fewest token characters that must follow for a match.
const PREFIXES: [(&[u8], usize); 23] = [
    (b"sk-", 10),
    (b"hf_", 16),
    (b"ghp_", 20),
    (b"gho_", 20),
    (b"ghu_", 20),
    (b"ghs_", 20),
    (b"ghr_", 20),
    (b"github_pat_", 20),
    (b"xoxb-", 10),
    (b"xoxa-", 10),
    (b"xoxp-", 10),
    (b"xoxr-", 10),
    (b"xoxs-", 10),
    (b"AKIA", 16),
    (b"ASIA", 16),
    (b"sk_live_", 10),
    (b"sk_test_", 10),
    (b"rk_live_", 10),
    (b"rk_test_", 10),
    (b"AIza", 30),
    (b"glpat-", 16),
    (b"ya29.", 20),
    (b"npm_", 30),
];

/// Words that make a name a secret, as a whole word or as the end of one (`accesstoken`).
const SECRET_WORDS: [&str; 12] = [
    "token",
    "tokens",
    "secret",
    "secrets",
    "password",
    "passwords",
    "passwd",
    "pwd",
    "passphrase",
    "credential",
    "credentials",
    "apikey",
];
/// Words that turn a following `key` into a secret name (`api_key`, `private_key`).
const KEY_PREFIXES: [&str; 12] = [
    "api",
    "access",
    "private",
    "secret",
    "signing",
    "encryption",
    "master",
    "ssh",
    "client",
    "auth",
    "license",
    "session",
];
/// A last word that says the value is a measure, not a secret (`token_count`).
const NOT_SECRET_LAST: [&str; 8] = [
    "count", "counts", "length", "len", "size", "type", "limit", "ttl",
];

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
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

/// Where a quoted value that starts at `from` (just after the opening `quote`) ends: at the
/// closing quote of the same kind, skipping backslash escapes, or at the end of the line when it
/// never closes. Returns the length of the value, the closing quote not included.
fn quoted_len(b: &[u8], from: usize, quote: u8) -> usize {
    let mut j = from;
    while let Some(&c) = b.get(j) {
        if c == quote || c == b'\n' {
            break;
        }
        // A backslash hides the next byte (but not a line break).
        j += if c == b'\\' && b.get(j + 1).is_some_and(|n| *n != b'\n') {
            2
        } else {
            1
        };
    }
    j.min(b.len()) - from
}

/// A secret at the beginning of `s`: how many bytes it covers and what replaces them.
fn secret_at(s: &str) -> Option<(usize, String)> {
    let b = s.as_bytes();
    for (prefix, min) in PREFIXES {
        if b.starts_with(prefix) && run_cap(b, prefix.len(), is_token_byte) >= min {
            let n = run(b, prefix.len(), is_token_byte);
            return Some((prefix.len() + n, MASK.into()));
        }
    }
    // Crastu agent tokens: `crastu_` plus hex.
    if b.starts_with(b"crastu_") && run_cap(b, 7, |c| c.is_ascii_hexdigit()) >= 16 {
        return Some((7 + run(b, 7, |c| c.is_ascii_hexdigit()), MASK.into()));
    }
    // `Bearer <token>` and `Basic <base64>`, any case.
    for scheme in [&b"bearer "[..], &b"basic "[..]] {
        if b.len() > scheme.len() && b[..scheme.len()].eq_ignore_ascii_case(scheme) {
            let value = |c: u8| !c.is_ascii_whitespace() && c != b'"' && c != b'\'';
            if run_cap(b, scheme.len(), value) >= 8 {
                return Some((scheme.len() + run(b, scheme.len(), value), MASK.into()));
            }
        }
    }
    None
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// The lower-case words of a name, split at `_`, `-`, `.` and camelCase boundaries
/// (`accessToken` and `APIKey` give `access`, `token` and `api`, `key`).
fn split_words(id: &[u8]) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for (i, &c) in id.iter().enumerate() {
        if matches!(c, b'_' | b'-' | b'.') {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        let prev = i
            .checked_sub(1)
            .and_then(|p| id.get(p))
            .copied()
            .unwrap_or(0);
        let next = id.get(i + 1).copied().unwrap_or(0);
        let starts_word = c.is_ascii_uppercase()
            && (prev.is_ascii_lowercase()
                || prev.is_ascii_digit()
                || (prev.is_ascii_uppercase() && next.is_ascii_lowercase()));
        if starts_word && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        cur.push(c.to_ascii_lowercase() as char);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// What a name says about its value: `(strong, to_end_of_line)`, or `None` for a name that is
/// not a secret. Strong names take `name: value` as well as `name=value`; weak ones (`key`,
/// `sig`, `signature`, `auth`) only `=`, because `key: ...` is common prose.
fn classify(words: &[String]) -> Option<(bool, bool)> {
    if words
        .last()
        .is_some_and(|w| NOT_SECRET_LAST.contains(&w.as_str()))
    {
        return None;
    }
    let (mut strong, mut weak, mut to_eol) = (false, false, false);
    for (i, w) in words.iter().enumerate() {
        let w = w.as_str();
        if SECRET_WORDS.iter().any(|s| w.ends_with(s)) {
            strong = true;
        } else if w == "key" {
            let compound = i > 0
                && words
                    .get(i - 1)
                    .is_some_and(|p| KEY_PREFIXES.contains(&p.as_str()));
            strong |= compound;
            weak = true;
        } else if matches!(w, "sig" | "signature" | "auth") {
            weak = true;
        } else if matches!(w, "authorization" | "cookie") {
            strong = true;
            to_eol = true;
        }
    }
    (strong || weak).then_some((strong, to_eol))
}

/// A secret name, its separator and its value at the beginning of `s`: `name=value`,
/// `name: value`, `"name": "value"`, any whitespace around the separator. Returns the bytes
/// covered and the text that replaces them (everything up to the value, then `[redacted]`).
fn kv_at(s: &str) -> Option<(usize, String)> {
    let b = s.as_bytes();
    let id_len = run_cap(b, 0, is_ident_byte);
    if id_len == 0 || b.get(id_len).is_some_and(|c| is_ident_byte(*c)) {
        return None; // not a name, or too long to be one
    }
    let (strong, to_eol) = classify(&split_words(b.get(..id_len)?))?;
    let mut j = id_len;
    // The name may be quoted: `"token"` or `'token'`; JSON is unambiguous, so `:` is fine then.
    let quoted_name = matches!(b.get(j), Some(b'"' | b'\''));
    if quoted_name {
        j += 1;
    }
    j += run(b, j, is_space);
    let sep = b.get(j).copied();
    if sep != Some(b'=') && !((strong || quoted_name) && sep == Some(b':')) {
        return None;
    }
    j += 1;
    j += run(b, j, is_space);
    let n = match b.get(j).copied() {
        // A quoted value ends at the closing quote of the same kind (escapes honoured), or at
        // the end of the line when it never closes.
        Some(q @ (b'"' | b'\'')) => {
            j += 1;
            quoted_len(b, j, q)
        }
        // A header value can hold a scheme and a token: to the end of the line.
        _ if to_eol => run(b, j, |c| c != b'\n' && c != b'\r'),
        // Anything else runs to a separator. Quotes do not end it, so that a value wrapped in a
        // call, like Some("..."), loses all of it; over-redaction is the safe side.
        _ => run(b, j, |c| !is_space(c) && !matches!(c, b'&' | b';' | b',')),
    };
    (n >= 1).then(|| (j + n, format!("{}{MASK}", s.get(..j).unwrap_or_default())))
}

/// The run of token and dot bytes that a JWT candidate sits in, scanned once. Every `eyJ` inside
/// the same run asks the same question (does a JWT starting here, ending at the run's end, have
/// two dots?), which the last two dots answer without scanning again.
#[derive(Default)]
struct JwtRun {
    end: usize,
    last_dot: Option<usize>,
    second_last_dot: Option<usize>,
}

impl JwtRun {
    /// The length of the JWT starting at `i` (where `b` has `eyJ`), if there is one: the whole
    /// run of token and dot bytes from `i`, at least 20 bytes with two dots in it.
    fn len_at(&mut self, b: &[u8], i: usize) -> Option<usize> {
        if i >= self.end {
            let part = |c: u8| is_token_byte(c) || c == b'.';
            self.end = i + run(b, i, part);
            self.last_dot = None;
            self.second_last_dot = None;
            for (k, c) in b.iter().enumerate().take(self.end).skip(i) {
                if *c == b'.' {
                    self.second_last_dot = self.last_dot;
                    self.last_dot = Some(k);
                }
            }
        }
        let n = self.end - i;
        (n >= 20 && self.second_last_dot.is_some_and(|d| d >= i)).then_some(n)
    }
}

/// The length of `user:pass@` at the start of the authority that follows `://`, if there is one.
fn userinfo_len(after_scheme: &str) -> Option<usize> {
    let end = after_scheme
        .find(|c: char| matches!(c, '/' | '?' | '#' | '"' | '\'') || c.is_whitespace())
        .unwrap_or(after_scheme.len());
    after_scheme.get(..end)?.rfind('@').map(|at| at + 1)
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
    let mut i = 0;
    while i < input.len() {
        let Some(rest) = input.get(i..) else { break };
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
        if prev.is_none_or(|p| !is_ident_byte(p))
            && let Some((n, replacement)) = kv_at(rest)
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
