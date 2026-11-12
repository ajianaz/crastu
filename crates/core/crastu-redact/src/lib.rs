//! Redaction of known secret shapes in free text, and validation of plain names, for logs.
//! A safety net, not a promise: see the `crastu-recorder` docs for what is and is not guaranteed.
//! Runs on text that may be inside a panic hook, so it must never panic: every slice starts and
//! ends next to an ASCII byte that was matched, which is always a character boundary.

const MASK: &str = "[redacted]";

/// Token prefixes and the fewest token characters that must follow for a match.
const PREFIXES: [(&[u8], usize); 15] = [
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
];

/// Names whose `name=value` pair loses its value.
const KV_NAMES: [&str; 19] = [
    "access_token",
    "refresh_token",
    "id_token",
    "token",
    "api_key",
    "api-key",
    "apikey",
    "key",
    "client_secret",
    "secret",
    "password",
    "passwd",
    "pwd",
    "x-amz-signature",
    "x-amz-credential",
    "signature",
    "sig",
    "authorization",
    "auth",
];

fn is_token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

fn run(b: &[u8], from: usize, ok: impl Fn(u8) -> bool) -> usize {
    b.get(from..)
        .map_or(0, |rest| rest.iter().take_while(|c| ok(**c)).count())
}

/// A secret at the beginning of `s`: how many bytes it covers and what replaces them.
fn secret_at(s: &str) -> Option<(usize, String)> {
    let b = s.as_bytes();
    for (prefix, min) in PREFIXES {
        if b.starts_with(prefix) {
            let n = run(b, prefix.len(), is_token_byte);
            if n >= min {
                return Some((prefix.len() + n, MASK.into()));
            }
        }
    }
    // Crastu agent tokens: `crastu_` plus hex.
    if b.starts_with(b"crastu_") {
        let n = run(b, 7, |c| c.is_ascii_hexdigit());
        if n >= 16 {
            return Some((7 + n, MASK.into()));
        }
    }
    // JWTs: `eyJ...` with dots between the parts.
    if b.starts_with(b"eyJ") {
        let n = run(b, 0, |c| is_token_byte(c) || c == b'.');
        if n >= 20 && b.get(..n).is_some_and(|t| t.contains(&b'.')) {
            return Some((n, MASK.into()));
        }
    }
    // `Bearer <token>` and `Basic <base64>`, any case.
    for scheme in [&b"bearer "[..], &b"basic "[..]] {
        if b.len() > scheme.len() && b[..scheme.len()].eq_ignore_ascii_case(scheme) {
            let n = run(b, scheme.len(), |c| {
                !c.is_ascii_whitespace() && c != b'"' && c != b'\''
            });
            if n >= 8 {
                return Some((scheme.len() + n, MASK.into()));
            }
        }
    }
    // `name=value` for names that hold secrets (query strings, key=value text).
    for name in KV_NAMES {
        let k = name.len();
        if b.len() > k + 1 && b[..k].eq_ignore_ascii_case(name.as_bytes()) && b[k] == b'=' {
            let n = run(b, k + 1, |c| {
                !c.is_ascii_whitespace() && !matches!(c, b'&' | b'"' | b'\'' | b';' | b',')
            });
            if n >= 1 {
                return Some((k + 1 + n, format!("{}={MASK}", s.get(..k)?)));
            }
        }
    }
    None
}

/// The length of `user:pass@` at the start of the authority that follows `://`, if there is one.
fn userinfo_len(after_scheme: &str) -> Option<usize> {
    let end = after_scheme
        .find(|c: char| matches!(c, '/' | '?' | '#' | '"' | '\'') || c.is_whitespace())
        .unwrap_or(after_scheme.len());
    after_scheme.get(..end)?.rfind('@').map(|at| at + 1)
}

/// `input` with every known secret shape replaced by `[redacted]`.
pub fn redact(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = String::with_capacity(input.len());
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
        let boundary = i == 0 || b.get(i - 1).is_none_or(|p| !p.is_ascii_alphanumeric());
        if boundary && let Some((n, replacement)) = secret_at(rest) {
            out.push_str(&replacement);
            i += n;
            continue;
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
mod tests {
    use super::*;

    #[test]
    fn known_shapes_are_masked() {
        // Fake values, assembled from parts so that secret scanners do not flag the fixtures.
        let secrets = [
            ("sk-", "abcdefghij1234567890"),
            ("sk-", "ant-api03-AbCdEf_123456"),
            ("hf_", "abcdefghijklmnopqrstuvwxyz012345"),
            ("gh", "p_abcdefghijklmnopqrstuvwxyz0123456789"),
            ("github_", "pat_11ABCDEFG0abcdefghijklmnop"),
            ("xo", "xb-123456789012-abcdefghij"),
            ("AK", "IAIOSFODNN7EXAMPLE"),
            (
                "ey",
                "JhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N",
            ),
            ("crastu_", "0123456789abcdef0123456789abcdef"),
        ];
        for (head, tail) in secrets {
            let s = format!("{head}{tail}");
            let r = redact(&format!("see {s} end"));
            assert_eq!(r, "see [redacted] end", "{s}");
        }
        for s in [
            "Authorization: Bearer abcdefgh.ijkl-mnop end",
            "bearer ABCDEFGHIJ end",
            "Authorization: Basic dXNlcjpwYXNzd29yZA== end",
        ] {
            let r = redact(s);
            assert!(r.contains(MASK) && r.ends_with(" end"), "{s} -> {r}");
            assert!(!r.contains("dXNlcj") && !r.contains("ABCDEFGHIJ"), "{r}");
        }
    }

    #[test]
    fn url_userinfo_and_secret_pairs_are_masked() {
        assert_eq!(
            redact("GET https://user:hunter2@example.com/a/b: refused"),
            "GET https://[redacted]@example.com/a/b: refused"
        );
        assert_eq!(
            redact("GET https://example.com/f?token=abc123&x=1&X-Amz-Signature=deadbeef failed"),
            "GET https://example.com/f?token=[redacted]&x=1&X-Amz-Signature=[redacted] failed"
        );
        assert_eq!(
            redact("api_key=XYZ password=hunter2; ok"),
            "api_key=[redacted] password=[redacted]; ok"
        );
        assert_eq!(redact("monkey=banana"), "monkey=banana");
        // An address with an at sign later in the path is not userinfo.
        assert_eq!(redact("https://example.com/a@b"), "https://example.com/a@b");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        for s in [
            "task-list is fine",
            "sk-short",
            "Bearer x",
            "crastu_abc",
            "eyJ short",
            "hf_short",
            "the key is here",
            "héllo wörld 日本語",
            "",
        ] {
            assert_eq!(redact(s), s);
        }
    }

    #[test]
    fn redacting_odd_input_never_panics() {
        for s in [
            "://",
            "://@",
            "key=",
            "é://é@é",
            "sk-é",
            "Bearer é",
            "token=é日本",
        ] {
            let _ = redact(s);
        }
    }

    #[test]
    fn names_are_validated() {
        assert_eq!(ident("transcribe"), "transcribe");
        assert_eq!(ident("project_list"), "project_list");
        assert_eq!(ident("mcp:tools-list"), "mcp:tools-list");
        for bad in [
            "has space",
            "",
            ".",
            "..",
            "../x",
            ".hidden",
            "a.b",
            "a/b",
            "a\\b",
            "/tmp/x",
            "sk-abcdefghij1234567890",
        ] {
            assert_eq!(ident(bad), "invalid-name", "{bad:?}");
            assert!(!name_ok(bad), "{bad:?}");
        }
        assert_eq!(ident(&"a".repeat(65)), "invalid-name");
    }

    #[test]
    fn clean_cuts_long_text() {
        assert_eq!(clean("abcdef", 3), "abc…");
        assert_eq!(clean("abc", 3), "abc");
    }
}
