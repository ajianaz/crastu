//! Redaction of known secret shapes in free text. A safety net, not a promise: see the crate docs.

const MASK: &str = "[redacted]";

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// The length of a secret that starts at the beginning of `s`, if one does.
fn secret_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    // API keys such as `sk-...` and `sk-ant-...`.
    if b.starts_with(b"sk-") {
        let n = b[3..].iter().take_while(|c| is_token_byte(**c)).count();
        if n >= 10 {
            return Some(3 + n);
        }
    }
    // Crastu agent tokens: `crastu_` plus hex.
    if b.starts_with(b"crastu_") {
        let n = b[7..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
        if n >= 16 {
            return Some(7 + n);
        }
    }
    // `Bearer <token>`, any case.
    if b.len() > 7 && b[..6].eq_ignore_ascii_case(b"bearer") && b[6] == b' ' {
        let n = b[7..]
            .iter()
            .take_while(|c| !c.is_ascii_whitespace() && **c != b'"' && **c != b'\'')
            .count();
        if n >= 8 {
            return Some(7 + n);
        }
    }
    None
}

/// `input` with every known secret shape replaced by `[redacted]`.
pub fn redact(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let boundary = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        if boundary && let Some(n) = secret_len(&input[i..]) {
            out.push_str(MASK);
            i += n;
            continue;
        }
        let Some(ch) = input[i..].chars().next() else {
            break;
        };
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// `input` redacted and cut to at most `max` characters.
pub(crate) fn clean(input: &str, max: usize) -> String {
    let r = redact(input);
    if r.chars().count() <= max {
        return r;
    }
    let mut cut: String = r.chars().take(max).collect();
    cut.push('…');
    cut
}

/// A name that is safe to record as a structured field: ASCII letters, digits and `._:-/`, at
/// most 64 bytes. Anything else (spaces, quotes, newlines, other scripts) is not a name, so it
/// is replaced rather than recorded.
pub(crate) fn ident(input: &str) -> String {
    let ok = !input.is_empty()
        && input.len() <= 64
        && input
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'-' | b'/'));
    if ok && redact(input) == input {
        input.to_string()
    } else {
        "invalid-name".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_shapes_are_masked() {
        for s in [
            "key sk-abcdefghij1234567890 end",
            "key sk-ant-api03-AbCdEf_123456 end",
            "Authorization: Bearer abcdefgh.ijkl-mnop end",
            "bearer ABCDEFGHIJ end",
            "token crastu_0123456789abcdef0123456789abcdef end",
        ] {
            let r = redact(s);
            assert!(r.contains(MASK), "{s} -> {r}");
            assert!(r.ends_with(" end"), "{r}");
            assert!(
                !r.contains("abcdefghij1234") && !r.contains("0123456789abcdef"),
                "{r}"
            );
        }
    }

    #[test]
    fn ordinary_text_is_untouched() {
        for s in [
            "task-list is fine",
            "sk-short",
            "Bearer x",
            "crastu_abc",
            "héllo wörld 日本語",
            "",
        ] {
            assert_eq!(redact(s), s);
        }
    }

    #[test]
    fn names_are_validated() {
        assert_eq!(ident("transcribe.start"), "transcribe.start");
        assert_eq!(ident("has space"), "invalid-name");
        assert_eq!(ident(""), "invalid-name");
        assert_eq!(ident(&"a".repeat(65)), "invalid-name");
        assert_eq!(ident("sk-abcdefghij1234567890"), "invalid-name");
    }

    #[test]
    fn clean_cuts_long_text() {
        assert_eq!(clean("abcdef", 3), "abc…");
        assert_eq!(clean("abc", 3), "abc");
    }
}
