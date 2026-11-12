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
fn long_input_is_cut_at_whitespace_and_marked() {
    let long = "word ".repeat(1 << 18);
    let r = redact(&long);
    assert!(r.len() <= MAX_INPUT + TRUNCATED.len(), "{}", r.len());
    assert!(r.len() > MAX_INPUT - 10 && r.ends_with(" word [truncated]"));
    // No whitespace to cut at: nothing is kept (and nothing panics on wide characters).
    assert_eq!(redact(&"x".repeat(1 << 20)), TRUNCATED);
    assert_eq!(redact(&"日".repeat(MAX_INPUT)), TRUNCATED);
    assert!(!redact("short").contains("truncated"));
}

#[test]
fn a_secret_across_the_cut_is_dropped_whole() {
    // Fake tokens, assembled from parts. The cut falls inside each one.
    let secrets = [
        ["np", "m_", &"a".repeat(36)].concat(),
        ["Bear", "er ", &"b".repeat(30)].concat(),
        format!("password=\"{}", "c".repeat(30)),
    ];
    for secret in secrets {
        let filler = "x ".repeat((MAX_INPUT - 10) / 2);
        let input = format!("{filler}{secret} tail tail");
        let r = redact(&input);
        assert!(r.ends_with(" [truncated]"), "{secret}");
        assert!(
            !r.contains("aaaaa") && !r.contains("bbbbb") && !r.contains("ccccc"),
            "{secret} leaked"
        );
    }
}

#[test]
fn a_jwt_with_a_long_header_is_masked_whole() {
    // An RS256 header with a long `kid` is longer than the window used for other shapes.
    let jwt = format!(
        "eyJ{}.{}.{}",
        "h".repeat(150),
        "p".repeat(80),
        "s".repeat(60)
    );
    assert_eq!(redact(&format!("auth {jwt} end")), "auth [redacted] end");
    // Two JWTs in a row.
    let two = format!("{jwt} {jwt}");
    assert_eq!(redact(&two), "[redacted] [redacted]");
    // Not a JWT: no dots, or one dot.
    let no_dots = format!("eyJ{}", "a".repeat(100));
    assert_eq!(redact(&no_dots), no_dots);
    let one_dot = format!("eyJ{}.{}", "a".repeat(50), "b".repeat(50));
    assert_eq!(redact(&one_dot), one_dot);
}

#[test]
fn secret_names_in_every_common_form() {
    let cases = [
        (
            r#"{"accessToken":"abc123def","refreshToken":"zzz","clientSecret":"s3"}"#,
            r#"{"accessToken":"[redacted]","refreshToken":"[redacted]","clientSecret":"[redacted]"}"#,
        ),
        (
            r#"{"secret_key":"abc","private_key":"-----BEGIN"}"#,
            r#"{"secret_key":"[redacted]","private_key":"[redacted]"}"#,
        ),
        (
            r#"{"apiKey": "k1", "api_key" : "k2", "access_key":"k3", "session_token":"k4", "id_token":"k5", "authToken":"k6", "bearerToken":"k7", "client-secret":"k8", "credentials":"k9", "signature":"k10"}"#,
            r#"{"apiKey": "[redacted]", "api_key" : "[redacted]", "access_key":"[redacted]", "session_token":"[redacted]", "id_token":"[redacted]", "authToken":"[redacted]", "bearerToken":"[redacted]", "client-secret":"[redacted]", "credentials":"[redacted]", "signature":"[redacted]"}"#,
        ),
        ("\"token\" :\n \"abc\"", "\"token\" :\n \"[redacted]\""),
        (
            "API_KEY=abc123 AWS_SECRET_ACCESS_KEY=xyz",
            "API_KEY=[redacted] AWS_SECRET_ACCESS_KEY=[redacted]",
        ),
        ("export TOKEN=abc", "export TOKEN=[redacted]"),
        (
            "db:\n  password: hunter2\n  user: bob\n  secret_key_base: x",
            "db:\n  password: [redacted]\n  user: bob\n  secret_key_base: [redacted]",
        ),
        ("password:\nhunter2", "password:\n[redacted]"),
        ("password:\thunter2", "password:\t[redacted]"),
        ("pwd = hunter2", "pwd = [redacted]"),
        (
            "Cookie: session=abc123def; theme=dark\nHost: x",
            "Cookie: [redacted]\nHost: x",
        ),
        ("Set-Cookie: sid=abc; Path=/", "Set-Cookie: [redacted]"),
        (r#"{"Cookie": "a=b; c=d"}"#, r#"{"Cookie": "[redacted]"}"#),
        (
            "GET /cb?code=1&access_token=abc&state=x",
            "GET /cb?code=1&access_token=[redacted]&state=x",
        ),
        (
            "postgres://bob:hunter2@db.example.com/app",
            "postgres://[redacted]@db.example.com/app",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(redact(input), expected, "{input}");
    }
}

#[test]
fn ordinary_words_are_not_over_redacted() {
    for s in [
        "monkey: 3",
        "keyboard=us",
        "keys: 5",
        "token_count: 5",
        "token_type: bearer",
        "password_length=12",
        "key_size: 2048",
        "tokenizer=bpe",
        "author: bob",
        "the primary key: id",
        "sig: ok",
        "auth: ok",
        "cookies are tasty",
        "my token was fine",
        r#"{"user": "bob", "count": 3, "keyboard": "us"}"#,
    ] {
        assert_eq!(redact(s), s);
    }
    // In doubt, over-redaction: a name that holds a secret word is masked.
    assert_eq!(
        redact("password_policy: strict"),
        "password_policy: [redacted]"
    );
}

#[test]
fn name_words_split_at_every_boundary() {
    let w = |s: &str| split_words(s.as_bytes());
    assert_eq!(w("accessToken"), ["access", "token"]);
    assert_eq!(w("APIKey"), ["api", "key"]);
    assert_eq!(w("X-Amz-Signature"), ["x", "amz", "signature"]);
    assert_eq!(w("secret_key_base"), ["secret", "key", "base"]);
    assert_eq!(w("a.b-c_d"), ["a", "b", "c", "d"]);
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
        "eyJ.a.",
        "token\n",
        "accessToken",
        "Set-Cookie: ",
        "aB",
        "--password ",
        "mysql -p ",
        "-p ",
        "\\\"",
        concat!("-----BEG", "IN PRIVATE KEY-----"),
        "-----BEGIN CERTIFICATE-----",
        "SG.",
        "hooks.slack.com/services/",
        "12345678:",
        "a_secret_",
        "token: ",
        "tokens: ",
        "password=a ",
        "Basic ",
        "token: Some(\"",
        "token: Some(",
        "-u ",
        "--user=a:",
        "key=\"-----BEG",
        "token: [",
        "token=[[",
        "token=(((",
        "-H 'Authorization: ",
        "pass=",
        "session_id=",
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
