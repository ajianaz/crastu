//! Redaction of known secret shapes in free text, and validation of plain names, for logs.
//! A safety net, not a promise: see the `crastu-recorder` docs for what is and is not guaranteed.
//! Runs on text that may be inside a panic hook, so it must never panic: every slice starts and
//! ends next to an ASCII byte that was matched, which is always a character boundary.
//!
//! Work is linear in the input: a candidate is decided by looking at no more than [`DECIDE`]
//! bytes, a match is consumed once, and input past [`MAX_INPUT`] is cut off before anything
//! runs (and marked as cut). A secret that straddles the cut can leave its first few characters.

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

/// Names whose value is hidden: `(name, colon_ok, to_end_of_line)`. `name=value` always counts;
/// `name: value` and `"name": "value"` only where `colon_ok` (a bare `key:` or `auth:` is too
/// common in prose). A header value (`to_end_of_line`) can hold a scheme and a token, so it goes
/// to the end of the line.
const KV_NAMES: [(&str, bool, bool); 21] = [
    ("access_token", true, false),
    ("refresh_token", true, false),
    ("id_token", true, false),
    ("token", true, false),
    ("api_key", true, false),
    ("api-key", true, false),
    ("apikey", true, false),
    ("x-api-key", true, true),
    ("key", false, false),
    ("client_secret", true, false),
    ("secret", true, false),
    ("password", true, false),
    ("passwd", true, false),
    ("pwd", true, false),
    ("x-amz-signature", false, false),
    ("x-amz-credential", false, false),
    ("signature", false, false),
    ("sig", false, false),
    ("authorization", true, true),
    ("auth", false, false),
    ("proxy-authorization", true, true),
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
    // JWTs: `eyJ...` with dots between the parts.
    if b.starts_with(b"eyJ") {
        let part = |c: u8| is_token_byte(c) || c == b'.';
        let m = run_cap(b, 0, part);
        if m >= 20 && b.get(..m).is_some_and(|t| t.contains(&b'.')) {
            return Some((run(b, 0, part), MASK.into()));
        }
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
    // `name=value`, `name: value` and `"name": "value"` for names that hold secrets.
    for (name, colon_ok, to_eol) in KV_NAMES {
        let k = name.len();
        if b.len() <= k || !b[..k].eq_ignore_ascii_case(name.as_bytes()) {
            continue;
        }
        let mut j = k;
        // The name may be quoted: `"token"` or `'token'`.
        if matches!(b.get(j), Some(b'"' | b'\'')) {
            j += 1;
        }
        j += run(b, j, |c| c == b' ');
        let sep = b.get(j).copied();
        if sep != Some(b'=') && !(colon_ok && sep == Some(b':')) {
            continue;
        }
        j += 1;
        j += run(b, j, |c| c == b' ');
        let n = match b.get(j).copied() {
            // A quoted value ends at the closing quote of the same kind (escapes honoured), or
            // at the end of the line when it never closes.
            Some(q @ (b'"' | b'\'')) => {
                j += 1;
                quoted_len(b, j, q)
            }
            // A header value can hold a scheme and a token: to the end of the line.
            _ if to_eol => run(b, j, |c| c != b'\n' && c != b'\r'),
            // Anything else runs to a separator. Quotes do not end it, so that
            // a value wrapped in a call, like Some("..."), loses all of it; over-redaction is safe.
            _ => run(b, j, |c| {
                !c.is_ascii_whitespace() && !matches!(c, b'&' | b';' | b',')
            }),
        };
        if n >= 1 {
            return Some((j + n, format!("{}{MASK}", s.get(..j)?)));
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

/// `input` with every known secret shape replaced by `[redacted]`. Input over [`MAX_INPUT`]
/// bytes is cut at a character boundary before anything else, and ` [truncated]` is added.
pub fn redact(input: &str) -> String {
    if input.len() <= MAX_INPUT {
        return redact_all(input);
    }
    let mut cut = MAX_INPUT;
    while !input.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = redact_all(input.get(..cut).unwrap_or_default());
    out.push_str(TRUNCATED);
    out
}

fn redact_all(input: &str) -> String {
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
            ("sk_", "live_abcdefghij1234567890"),
            ("rk_", "test_abcdefghij1234567890"),
            ("AI", "zaSyA1234567890abcdefghijklmnopqrstuv"),
            ("gl", "pat-abcdefghij1234567890"),
            ("ya", "29.a0AfH6SMBabcdefghijklmnopqrstu"),
            ("np", "m_abcdefghijklmnopqrstuvwxyz0123456789"),
        ];
        for (head, tail) in secrets {
            let s = format!("{head}{tail}");
            let r = redact(&format!("see {s} end"));
            assert_eq!(r, "see [redacted] end", "{s}");
        }
        for s in [
            "got Bearer abcdefgh.ijkl-mnop end",
            "bearer ABCDEFGHIJ end",
            "got Basic dXNlcjpwYXNzd29yZA== end",
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
        // JSON, YAML and header forms.
        assert_eq!(
            redact(r#"body {"token": "abc123", "user": "bob", "Password":"p w"}"#),
            r#"body {"token": "[redacted]", "user": "bob", "Password":"[redacted]"}"#
        );
        assert_eq!(
            redact("password: hunter2 next"),
            "password: [redacted] next"
        );
        assert_eq!(
            redact("Authorization: Bearer abcdefgh.ijkl\nHost: x"),
            "Authorization: [redacted]\nHost: x"
        );
        assert_eq!(
            redact("Authorization: rawtoken123\nx-api-key: abc.def"),
            "Authorization: [redacted]\nx-api-key: [redacted]"
        );
        // Prose with a bare `key:` or `auth:` is left alone.
        assert_eq!(redact("key: value, auth: ok"), "key: value, auth: ok");
        // An address with an at sign later in the path is not userinfo.
        assert_eq!(redact("https://example.com/a@b"), "https://example.com/a@b");
    }

    #[test]
    fn quoted_values_are_masked_whole() {
        // Single quotes, the other quote kind inside, escapes, spaces.
        assert_eq!(
            redact("{'password': 'x y', 'user': 'bob'}"),
            "{'password': '[redacted]', 'user': 'bob'}"
        );
        assert_eq!(redact("key='a b' ok"), "key='[redacted]' ok");
        assert_eq!(
            redact(r#"{"token": "a\"b", "n": 1}"#),
            r#"{"token": "[redacted]", "n": 1}"#
        );
        assert_eq!(redact(r#"token="it's" end"#), r#"token="[redacted]" end"#);
        // A quoted value that never closes goes to the end of the line.
        assert_eq!(
            redact("password=\"never closed\nnext line"),
            "password=\"[redacted]\nnext line"
        );
        // An unquoted value does not stop at a quote: nothing of it is left behind.
        for s in [
            r#"password: Some("hunter2")"#,
            r#"secret=Wrapped("hunter2")"#,
            "token=abc\"hunter2\"def",
        ] {
            let r = redact(s);
            assert!(!r.contains("hunter2") && !r.contains("def"), "{s} -> {r}");
            assert!(r.contains(MASK), "{r}");
        }
    }

    #[test]
    fn long_input_is_cut_and_marked() {
        let long = "x".repeat(1 << 20);
        let r = redact(&long);
        assert_eq!(r.len(), MAX_INPUT + TRUNCATED.len());
        assert!(r.ends_with(" [truncated]"));
        // The cut lands on a character boundary.
        let wide = "日".repeat(MAX_INPUT);
        assert!(redact(&wide).ends_with(" [truncated]"));
        assert!(!redact("short").contains("truncated"));
    }

    /// Every shape that scans ahead, repeated to a size where quadratic work would take minutes.
    /// `redact_all` is called directly: the cut in `redact` would hide the problem.
    #[test]
    fn work_is_linear_for_every_shape() {
        let shapes = [
            "eyJ-",
            "eyJ.",
            "sk-",
            "sk-sk-",
            "hf_",
            "AKIA",
            "crastu_",
            "Bearer ",
            "basic ",
            "token=",
            "token=\"",
            "password: ",
            "Authorization: ",
            "://",
            "://a@",
            "key='",
            "a\\",
            "\"",
        ];
        for shape in shapes {
            let input = shape.repeat((512 * 1024) / shape.len());
            let t = std::time::Instant::now();
            let _ = redact_all(&input);
            let took = t.elapsed();
            assert!(
                took < std::time::Duration::from_secs(5),
                "{shape:?} took {took:?} for {} bytes",
                input.len()
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
