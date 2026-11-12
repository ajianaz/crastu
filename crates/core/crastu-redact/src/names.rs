//! Secret names and their values: `name=value`, `name: value`, `"name": "value"`, escaped JSON
//! (`{\"name\":\"value\"}`), and shell flags (`--password value`).
//!
//! A name is split into words at `_`, `-`, `.` and camelCase boundaries, and counts when a word
//! is, or ends with, a secret word (`token`, `secret`, `password`...), or is `key` after `api`,
//! `access`, `private` and the like. A name whose last word is `count`, `length`, `size`, `type`,
//! `limit` or `ttl`, a token count (`max_tokens`, `prompt_tokens`), and the environment
//! variables `PWD` and `OLDPWD` are not secrets. Doubt goes to over-redaction: `password_policy:
//! strict` loses its value.
//!
//! Where the value ends: a quoted value at its closing quote (escapes honoured, to the end of
//! the line when it never closes); a header-like value (`Authorization`, `Cookie`, `x-api-key`,
//! and any `token`, `secret` or `credential` name after a colon) at the end of the line; the
//! value of `password=` or `passwd=` with spaces in it, also at the end of the line; anything
//! else at the next space, `&`, `;` or `,`. A value that starts on the line after the separator
//! counts only if that line is the value alone, or if its first word is at least 8 characters.

use crate::shapes::SCHEMES;
use crate::{DECIDE, MASK, is_blank, is_ident_byte, is_space, run, run_cap};

/// Words that make a name a secret, as a whole word or as the end of one (`accesstoken`).
const SECRET_WORDS: [&str; 11] = [
    "token",
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
/// A word before `token(s)` that says it is a count (`max_tokens`, `prompt_tokens`).
const COUNT_WORDS: [&str; 11] = [
    "max",
    "min",
    "n",
    "num",
    "prompt",
    "completion",
    "total",
    "input",
    "output",
    "cached",
    "reasoning",
];
/// How many trailing bytes of a very long name are looked at (its last words decide).
const NAME_TAIL: usize = DECIDE;

/// The lower-case words of a name, split at `_`, `-`, `.` and camelCase boundaries
/// (`accessToken` and `APIKey` give `access`, `token` and `api`, `key`).
pub(crate) fn split_words(id: &[u8]) -> Vec<String> {
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

/// What a secret name says about its value.
#[derive(Default)]
struct Class {
    /// Takes `name: value` as well as `name=value` (weak names, `key`, `sig`, `auth`, only `=`).
    strong: bool,
    /// The value is a header value: to the end of the line, whatever the separator.
    to_eol: bool,
    /// After a colon the value goes to the end of the line (`token: abc def`).
    eol_colon: bool,
    /// A password name: with `=` and spaces in the value, the value goes to the end of the line.
    spaced_pw: bool,
    /// The name is a plural `...tokens` that is a count when the value is a number.
    numeric_ok: bool,
}

/// The class of the name `ident`, or `None` for a name that is not a secret.
fn classify(ident: &[u8]) -> Option<Class> {
    if ident == b"PWD" || ident == b"OLDPWD" {
        return None; // the working directory
    }
    let mut words = split_words(ident.get(ident.len().saturating_sub(NAME_TAIL)..)?);
    if ident.len() > NAME_TAIL && !words.is_empty() {
        words.remove(0); // cut mid-word
    }
    if words
        .last()
        .is_some_and(|w| NOT_SECRET_LAST.contains(&w.as_str()))
    {
        return None;
    }
    let mut c = Class::default();
    let mut weak = false;
    for (i, w) in words.iter().enumerate() {
        let w = w.as_str();
        let before = i
            .checked_sub(1)
            .and_then(|p| words.get(p))
            .map(String::as_str);
        if matches!(w, "token" | "tokens")
            && (before.is_some_and(|p| COUNT_WORDS.contains(&p)) || (w == "tokens" && i == 0))
        {
            continue; // a token count
        }
        if w.ends_with("tokens") {
            c.strong = true;
            c.numeric_ok = true;
            c.eol_colon = true;
        } else if SECRET_WORDS.iter().any(|s| w.ends_with(s)) {
            c.strong = true;
            c.spaced_pw |= w.ends_with("password") || w.ends_with("passwd");
            // A password may be one word; a token, secret or credential after a colon is a
            // header-like value that can hold a scheme and a credential.
            let is_password = ["password", "passwords", "passwd", "pwd", "passphrase"]
                .iter()
                .any(|p| w.ends_with(p));
            c.eol_colon |= !is_password;
        } else if w == "key" {
            let compound = before.is_some_and(|p| KEY_PREFIXES.contains(&p));
            c.strong |= compound;
            c.eol_colon |= before == Some("api");
            weak = true;
        } else if matches!(w, "sig" | "signature" | "auth") {
            weak = true;
        } else if matches!(w, "authorization" | "cookie") {
            c.strong = true;
            c.to_eol = true;
        }
    }
    (c.strong || weak).then_some(c)
}

/// A quote at `j`, possibly behind backslashes (`\"`, `\\\"`): the bytes it covers, the quote,
/// and how many backslashes came first.
fn quote_at(b: &[u8], j: usize) -> Option<(usize, u8, usize)> {
    let k = run_cap(b, j, |c| c == b'\\');
    match b.get(j + k) {
        Some(&q) if q == b'"' || q == b'\'' => Some((k + 1, q, k)),
        _ => None,
    }
}

/// Where a quoted value that starts at `from` (just after the opening `quote`) ends: at the
/// closing quote of the same kind, skipping backslash escapes, or at the end of the line when it
/// never closes. Returns the length of the value, the closing quote not included.
pub(crate) fn quoted_len(b: &[u8], from: usize, quote: u8) -> usize {
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

/// [`quoted_len`] for a value inside escaped JSON: it opened with `k` backslashes and a quote,
/// and ends at the next quote that is behind exactly `k` backslashes (those are not part of the
/// value), or at the end of the line.
fn escaped_len(b: &[u8], from: usize, quote: u8, k: usize) -> usize {
    let mut backslashes = 0;
    for (j, &c) in b.iter().enumerate().skip(from) {
        if c == b'\n' {
            return j - from;
        }
        if c == b'\\' {
            backslashes += 1;
        } else {
            if c == quote && backslashes == k {
                return j - k - from;
            }
            backslashes = 0;
        }
    }
    b.len() - from
}

/// The unquoted value at `j`: to the next space, `&`, `;` or `,`; after a scheme word
/// (`Bearer`, `Basic`) the credential that follows it too.
fn plain_len(b: &[u8], j: usize) -> usize {
    let stop = |c: u8| !is_space(c) && !matches!(c, b'&' | b';' | b',');
    let n = run(b, j, stop);
    let word = b.get(j..j + n).unwrap_or_default();
    let is_scheme = SCHEMES
        .iter()
        .any(|s| word.eq_ignore_ascii_case(s.as_bytes()));
    if is_scheme && b.get(j + n) == Some(&b' ') {
        let more = run(b, j + n + 1, stop);
        if more > 0 {
            return n + 1 + more;
        }
    }
    n
}

/// A secret name, its separator and its value at the beginning of `s`. Returns the bytes
/// covered and the text that replaces them (everything up to the value, then `[redacted]`).
pub(crate) fn kv_at(s: &str) -> Option<(usize, String)> {
    let b = s.as_bytes();
    // The whole name is scanned once (the caller never starts inside one); only its end decides.
    let id_len = run(b, 0, is_ident_byte);
    if id_len == 0 {
        return None;
    }
    let class = classify(b.get(..id_len)?)?;
    let mut j = id_len;
    // The name may be quoted: `"token"`, `'token'`, `\"token\"`; JSON is unambiguous, so `:`
    // is fine then.
    let name_quote = quote_at(b, j);
    if let Some((len, _, _)) = name_quote {
        j += len;
    }
    j += run(b, j, is_blank);
    let sep = b.get(j).copied();
    let flag = b.starts_with(b"--") && name_quote.is_none() && class.strong && j > id_len;
    let mut crossed = false;
    if sep == Some(b'=') || (sep == Some(b':') && (class.strong || name_quote.is_some())) {
        j += 1;
        j += run(b, j, is_blank);
        // A value on the next line: one line break, then the line's indentation.
        if b.get(j) == Some(&b'\r') {
            j += 1;
        }
        if b.get(j) == Some(&b'\n') {
            j += 1;
            j += run(b, j, is_blank);
            crossed = true;
        }
    } else if !(flag && b.get(j).is_some_and(|c| !is_space(*c) && *c != b'-')) {
        return None; // `--password value`: the value follows after blanks, with no separator
    }
    let colon = sep == Some(b':');
    let n = match quote_at(b, j) {
        Some((len, q, k)) => {
            j += len;
            if k == 0 {
                quoted_len(b, j, q)
            } else {
                escaped_len(b, j, q, k)
            }
        }
        None if crossed => {
            // The first word of the next line: the line's value if the line holds nothing else,
            // otherwise only if it is long enough to look like a secret.
            let word = run(b, j, |c| !is_space(c));
            let rest = run(b, j + word, is_blank);
            let alone = matches!(b.get(j + word + rest), None | Some(b'\n' | b'\r'));
            if alone || word >= 8 { word } else { 0 }
        }
        None if class.to_eol || (colon && class.eol_colon) => {
            run(b, j, |c| !matches!(c, b'\n' | b'\r' | b'"' | b'\''))
        }
        None => {
            let n = plain_len(b, j);
            let spaced = class.spaced_pw
                && !b.starts_with(b"--")
                && sep == Some(b'=')
                && b.get(j + n).copied().is_some_and(is_blank);
            if spaced {
                run(b, j, |c| !matches!(c, b'\n' | b'\r'))
            } else {
                n
            }
        }
    };
    let numeric = b
        .get(j..j + n)
        .is_some_and(|v| v.iter().all(u8::is_ascii_digit));
    (n >= 1 && !(class.numeric_ok && numeric))
        .then(|| (j + n, format!("{}{MASK}", s.get(..j).unwrap_or_default())))
}
