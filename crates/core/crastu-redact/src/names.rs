//! Secret names and their values: `name=value`, `name: value`, `"name": "value"`, escaped JSON
//! (`{\"name\":\"value\"}`), and shell flags (`--password value`).
//!
//! A name is split into words at `_`, `-`, `.` and camelCase boundaries, and counts when a word
//! is, or ends with, a secret word (`token`, `secret`, `password`...), or is `key` after `api`,
//! `access`, `private` and the like. The short words `pass`, `pin`, `otp`, `sid`, `bearer` and
//! `passcode` count only as whole words (so `pass_count` and `bypass` are left alone), `dbpass`
//! and `session` followed by `id` (`session_id`, `JSESSIONID`) count too. A name whose last word
//! is `count`, `length`, `size`, `type`, `limit` or `ttl`, and the environment variables `PWD`
//! and `OLDPWD`, are not secrets; a token count name (`max_tokens`, `prompt_tokens`, `tokens`)
//! is one only while its value is a number, `null`, `unlimited` or `n/a`. Doubt goes to
//! over-redaction: `password_policy: strict` loses its value.
//!
//! Where the value ends is in `values`: a PEM private key at the value start (also after a
//! quote) goes whole; an array to its matching bracket; a quoted value at its closing quote; a
//! header-like value (`Authorization`, `Cookie`, `x-api-key`, and any `token`, `secret` or
//! `credential` name after a colon) at the end of the line (inside a quoted shell argument, at
//! its closing quote); `password=` with spaces in the value at the end of the line; anything
//! else at the next space, `&`, `;` or `,`, following quotes and brackets to their balanced
//! close. A value that starts on the line after the separator counts only if that line is the
//! value alone, or if its first word is at least 8 characters.

use crate::blocks::pem_at;
use crate::values::{
    array_len, count_like, escaped_len, header_len, plain_len, quoted_len, word_len,
};
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
/// A last word that says the name is not a secret at all (`token_type: bearer`).
const NOT_SECRET_LAST: [&str; 1] = ["type"];
/// A last word that says the value is a measure (`token_count`, `key_size`): the name is a
/// count name, and its value is left alone only while it is a number.
const COUNT_LAST: [&str; 7] = ["count", "counts", "length", "len", "size", "limit", "ttl"];
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
    /// The name may be a count: a number as the value is left alone.
    numeric_ok: bool,
    /// A pure count name (`max_tokens`, `token_count`): `null`, `unlimited` and `n/a` count too.
    pure_count: bool,
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
    let last = words.len().saturating_sub(1);
    for (i, w) in words.iter().enumerate() {
        let w = w.as_str();
        let before = i
            .checked_sub(1)
            .and_then(|p| words.get(p))
            .map(String::as_str);
        // A token count (`max_tokens`, `tokens`, last word only) is one while its value is a
        // number (or `null`, `unlimited`); `access_tokens` only while it is digits.
        if matches!(w, "token" | "tokens")
            && i == last
            && (before.is_some_and(|p| COUNT_WORDS.contains(&p)) || (w == "tokens" && i == 0))
        {
            c.numeric_ok = true;
            c.pure_count = true;
        }
        if w.ends_with("tokens") {
            c.strong = true;
            c.eol_colon = true;
            if i == last {
                c.numeric_ok = true; // `access_tokens: 12345`: digits only
            }
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
        } else if matches!(w, "pass" | "pin" | "otp" | "sid" | "bearer" | "passcode")
            || w.ends_with("dbpass")
            || w.ends_with("sessionid")
            || (w == "id" && before == Some("session"))
        {
            // Short words only as whole words; the value is one word.
            c.strong = true;
        } else if matches!(w, "sig" | "signature" | "auth") {
            weak = true;
        } else if matches!(w, "authorization" | "cookie") {
            c.strong = true;
            c.to_eol = true;
        }
    }
    if words
        .last()
        .is_some_and(|w| COUNT_LAST.contains(&w.as_str()))
    {
        c.numeric_ok = true;
        c.pure_count = true;
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

/// A secret name, its separator and its value at the beginning of `s`. `line_quote` is the
/// quote opened earlier on the line, if any (a header inside `-H '...'`). Returns the bytes
/// covered and the text that replaces them (everything up to the value, then `[redacted]`).
pub(crate) fn kv_at(s: &str, line_quote: Option<u8>) -> Option<(usize, String)> {
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
    } else {
        // `--password value`: the value follows after blanks, with no separator.
        let starts_value =
            |c: u8| !is_space(c) && (c != b'-' || pem_at(s.get(j..).unwrap_or("")).is_some());
        if !(flag && b.get(j).copied().is_some_and(starts_value)) {
            return None;
        }
    }
    let colon = sep == Some(b':');
    let value = quote_at(b, j);
    let after_quote = j + value.map_or(0, |(len, _, _)| len);
    // A PEM private key as the value, quoted or not, goes whole (it spans lines).
    if let Some(n) = pem_at(s.get(after_quote..).unwrap_or("")) {
        let head = s.get(..after_quote)?;
        return Some((after_quote + n, format!("{head}[redacted private key]")));
    }
    let n = if b.get(j) == Some(&b'[') {
        j += 1;
        array_len(b, j - 1)
    } else if let Some((len, q, k)) = value {
        j += len;
        if k == 0 {
            quoted_len(b, j, q)
        } else {
            escaped_len(b, j, q, k)
        }
    } else if crossed {
        // The first word of the next line: the line's value if the line holds nothing else,
        // otherwise only if it is long enough to look like a secret.
        let word = word_len(b, j);
        let rest = run(b, j + word, is_blank);
        let alone = matches!(b.get(j + word + rest), None | Some(b'\n' | b'\r'));
        if alone || word >= 8 { word } else { 0 }
    } else if class.to_eol || (colon && class.eol_colon) {
        // To the end of the line. A count name (`total_tokens: 99, ...`) is left alone while
        // its value is a count that something ends (a separator, a closing bracket, the end of
        // the line); `token_count: 5 sk-abc...` goes whole.
        let plain = plain_len(b, j);
        let after = j + plain + run(b, j + plain, is_blank);
        let ended = matches!(
            b.get(after),
            None | Some(b'\n' | b'\r' | b',' | b'}' | b']' | b')' | b';')
        );
        let is_count = class.numeric_ok
            && ended
            && b.get(j..j + plain)
                .is_some_and(|v| count_like(v, class.pure_count));
        if is_count {
            plain
        } else {
            header_len(b, j, line_quote)
        }
    } else {
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
    };
    let counted = class.numeric_ok
        && b.get(j..j + n)
            .is_some_and(|v| count_like(v, class.pure_count));
    (n >= 1 && !counted).then(|| (j + n, format!("{}{MASK}", s.get(..j).unwrap_or_default())))
}
