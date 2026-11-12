//! Where a secret's value ends. Every function takes the bytes and the index where the value
//! starts and returns the value's length; none looks back, so work stays linear.

use crate::shapes::SCHEMES;
use crate::{is_space, run};

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
pub(crate) fn escaped_len(b: &[u8], from: usize, quote: u8, k: usize) -> usize {
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

/// The value of a header-like name, to the end of the line. Inside a quoted shell argument
/// (`-H 'Authorization: Bearer x'`, `quote` is the quote that was opened earlier on the line)
/// it stops at the matching closing quote instead, and at no other quote.
pub(crate) fn header_len(b: &[u8], from: usize, quote: Option<u8>) -> usize {
    let mut j = from;
    while let Some(&c) = b.get(j) {
        if matches!(c, b'\n' | b'\r') || Some(c) == quote {
            break;
        }
        j += if c == b'\\' && b.get(j + 1).is_some_and(|n| !matches!(n, b'\n' | b'\r')) {
            2
        } else {
            1
        };
    }
    j.min(b.len()) - from
}

/// An unquoted value: to the next space, `&`, `;` or `,`, or (when it held brackets itself) to
/// an unmatched closing bracket.
/// A quote or an opening bracket inside it (`Some("abc def")`, `Key { value: "x" }`) is
/// followed to its balanced close, or to the end of the line when it never closes: in doubt,
/// everything goes.
fn scan_plain(b: &[u8], from: usize) -> usize {
    let (mut depth, mut quote, mut j) = (0usize, None::<u8>, from);
    let mut opened = false;
    while let Some(&c) = b.get(j) {
        if matches!(c, b'\n' | b'\r') {
            break;
        }
        if let Some(q) = quote {
            if c == b'\\' && b.get(j + 1).is_some_and(|n| !matches!(n, b'\n' | b'\r')) {
                j += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            j += 1;
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'(' | b'[' | b'{' => {
                depth += 1;
                opened = true;
            }
            // An unmatched closer ends a value that held brackets of its own (`Some("x")}`),
            // not a plain one (`abc)def`).
            b')' | b']' | b'}' => {
                if depth == 0 {
                    if opened {
                        break;
                    }
                } else {
                    depth -= 1;
                }
            }
            b' ' | b'\t' | b'&' | b';' | b',' if depth == 0 => break,
            _ => {}
        }
        j += 1;
    }
    j.min(b.len()) - from
}

/// [`scan_plain`], and after a scheme word (`Bearer`, `Basic`) the credential that follows it.
pub(crate) fn plain_len(b: &[u8], j: usize) -> usize {
    let mut n = scan_plain(b, j);
    if n == 0
        && b.get(j)
            .is_some_and(|c| !is_space(*c) && !matches!(c, b'&' | b';' | b','))
    {
        n = run(b, j, |c| !is_space(c)); // not a separator: the word, whatever it holds
    }
    let word = b.get(j..j + n).unwrap_or_default();
    let is_scheme = SCHEMES
        .iter()
        .any(|s| word.eq_ignore_ascii_case(s.as_bytes()));
    if is_scheme && b.get(j + n) == Some(&b' ') {
        let more = scan_plain(b, j + n + 1);
        if more > 0 {
            return n + 1 + more;
        }
    }
    n
}

/// The contents of the array that opens at `open` (a `[`): up to the matching `]`, across lines,
/// quotes honoured; to the end of the text when it never closes. The brackets are not included.
pub(crate) fn array_len(b: &[u8], open: usize) -> usize {
    let (mut depth, mut quote) = (1usize, None::<u8>);
    for (j, &c) in b.iter().enumerate().skip(open + 1) {
        if let Some(q) = quote {
            // A doubled escape is rare; a backslash before the quote keeps it open.
            if c == q && b.get(j.wrapping_sub(1)) != Some(&b'\\') {
                quote = None;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = Some(c),
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return j - open - 1;
                }
            }
            _ => {}
        }
    }
    b.len().saturating_sub(open + 1)
}

/// Whether `v` looks like a count, not a secret: digits, and with `words` also `null`, `none`,
/// `unlimited` and the like.
pub(crate) fn count_like(v: &[u8], words: bool) -> bool {
    const WORDS: [&str; 8] = [
        "null",
        "none",
        "nil",
        "unlimited",
        "n/a",
        "na",
        "inf",
        "auto",
    ];
    !v.is_empty()
        && (v
            .iter()
            .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'_' | b','))
            || (words && WORDS.iter().any(|w| v.eq_ignore_ascii_case(w.as_bytes()))))
}

/// The length of the first word of `b` from `from`, up to whitespace.
pub(crate) fn word_len(b: &[u8], from: usize) -> usize {
    run(b, from, |c| !is_space(c))
}
