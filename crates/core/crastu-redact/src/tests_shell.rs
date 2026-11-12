use super::*;

/// A PEM block built from parts, so that secret scanners do not flag the fixtures.
fn pem(kind: &str, sep: &str) -> String {
    let label = format!("{kind}PRIVATE KEY");
    format!("-----BEGIN {label}-----{sep}MIIEow{sep}AbCd{sep}-----END {label}-----")
}

fn check(cases: &[(&str, &str)]) {
    for (input, expected) in cases {
        assert_eq!(redact(input), *expected, "{input}");
    }
}

#[test]
fn rust_debug_wrappers_are_masked_through_the_balanced_close() {
    check(&[
        (
            r#"Cfg { token: Some("tokenvalue123"), user: "x" }"#,
            r#"Cfg { token: [redacted], user: "x" }"#,
        ),
        (
            r#"auth_token: Some(Token("tokenvalue123"))"#,
            "auth_token: [redacted]",
        ),
        (
            r#"api_key: Some(ApiKey { value: "k 1" }), n: 3"#,
            "api_key: [redacted], n: 3",
        ),
        (r#"password: Some("abc def")"#, "password: [redacted]"),
        (
            r#"Auth { secret: Secret("abc def") } done"#,
            "Auth { secret: [redacted] } done",
        ),
        // A quote inside a plain value hides the spaces in it.
        (r#"token=abc"d e f"gh next"#, "token=[redacted] next"),
        // Never closes: to the end of the line.
        ("token: Some(\"abc def\nnext", "token: [redacted]\nnext"),
        // A plain word after a colon still goes to the end of the line.
        ("client_secret: abc def", "client_secret: [redacted]"),
    ]);
}

#[test]
fn user_and_password_arguments_of_commands() {
    check(&[
        ("curl -u admin:S3cretPass url", "curl -u [redacted] url"),
        (
            "curl --user admin:S3cretPass url",
            "curl --user [redacted] url",
        ),
        ("curl --user=admin:pw url", "curl --user=[redacted] url"),
        (
            "curl --proxy-user u:p url",
            "curl --proxy-user [redacted] url",
        ),
        ("curl -uadmin:pw url", "curl -u[redacted] url"),
        (r#"curl -u "admin:pw d" url"#, r#"curl -u "[redacted]" url"#),
        (
            "wget --password x --user y",
            "wget --password [redacted] --user y",
        ),
        ("ftp://u:p@h/f", "ftp://[redacted]@h/f"),
        (
            r#"git -c http.extraHeader="Authorization: Bearer abc" clone u"#,
            r#"git -c http.extraHeader="Authorization: [redacted]" clone u"#,
        ),
        // Not credentials: a user without a colon, another flag.
        ("mysql -u root db", "mysql -u root db"),
        ("tool --username a:b", "tool --username a:b"),
    ]);
}

#[test]
fn a_pem_key_as_a_value_goes_whole() {
    let multi = pem("RSA ", "\n");
    let literal = pem("", "\\n");
    check(&[
        (
            &format!("private_key: {multi}\nnext: 1"),
            "private_key: [redacted private key]\nnext: 1",
        ),
        (
            &format!("KEY=\"{literal}\" ok"),
            "KEY=\"[redacted private key]\" ok",
        ),
        (
            &format!("KEY=\"{multi}\" ok"),
            "KEY=\"[redacted private key]\" ok",
        ),
        (
            &format!("{{\"private_key\":\"{literal}\"}}"),
            "{\"private_key\":\"[redacted private key]\"}",
        ),
        (
            &format!("tool -p {multi} go"),
            "tool -p [redacted private key] go",
        ),
        (
            &format!("tool --password {multi} go"),
            "tool --password [redacted private key] go",
        ),
    ]);
}

#[test]
fn a_header_ends_at_its_own_quote_or_at_the_line_end() {
    check(&[
        (
            "curl -H 'Authorization: Bearer x' url",
            "curl -H 'Authorization: [redacted]' url",
        ),
        (
            r#"curl -H "Authorization: Bearer a\"b" url"#,
            r#"curl -H "Authorization: [redacted]" url"#,
        ),
        // A quote of the other kind inside the value does not end it.
        (
            r#"curl -H "Authorization: Bearer a'b" url"#,
            r#"curl -H "Authorization: [redacted]" url"#,
        ),
        // Plain lines: to the end of the line, whatever quotes they hold.
        (
            "Authorization: Bearer abc's \"thing\"",
            "Authorization: [redacted]",
        ),
        (
            "Cookie: a=\"b\"; c='d'\nHost: x",
            "Cookie: [redacted]\nHost: x",
        ),
        (
            "it's Authorization: Bearer abc",
            "it's Authorization: [redacted]",
        ),
    ]);
}

#[test]
fn token_counts_stay_readable_only_while_they_are_counts() {
    for s in [
        "max_tokens: 100",
        "prompt_tokens=12",
        "tokens: n/a",
        "max_tokens: null",
        "max_tokens: Unlimited",
        r#"{"total_tokens": 99, "max_tokens": "100"}"#,
    ] {
        assert_eq!(redact(s), s);
    }
    check(&[
        ("max_tokens: abcdefghij", "max_tokens: [redacted]"),
        ("tokens: abcdefgh", "tokens: [redacted]"),
        ("completion_tokens=sk_x", "completion_tokens=[redacted]"),
    ]);
}

#[test]
fn shell_flags_after_any_word_start() {
    check(&[
        (
            r#"sh -c "mysql -phunter2 db""#,
            r#"sh -c "mysql -p[redacted] db""#,
        ),
        ("$(mysql -phunter2 db)", "$(mysql -p[redacted] db)"),
        ("mariadb -phunter2 db", "mariadb -p[redacted] db"),
        ("mongodump -phunter2", "mongodump -p[redacted]"),
        ("pg_restore -p pw db", "pg_restore -p [redacted] db"),
        ("echo -pfoo", "echo -pfoo"),
    ]);
}

#[test]
fn short_and_session_names() {
    check(&[
        ("DB_PASS=abc", "DB_PASS=[redacted]"),
        ("pass=abc", "pass=[redacted]"),
        ("dbpass=abc", "dbpass=[redacted]"),
        ("tool --pass abc go", "tool --pass [redacted] go"),
        ("session_id=abc123", "session_id=[redacted]"),
        ("JSESSIONID=abc", "JSESSIONID=[redacted]"),
        ("sessionid=abc", "sessionid=[redacted]"),
        ("sessionId: abc", "sessionId: [redacted]"),
        ("bearer=abc", "bearer=[redacted]"),
        ("passcode: 1234", "passcode: [redacted]"),
        ("pin=1234", "pin=[redacted]"),
        ("otp: 123456", "otp: [redacted]"),
        ("sid=abc", "sid=[redacted]"),
    ]);
    // Whole words only: these stay readable.
    for s in [
        "pass_count: 3",
        "bypass=1",
        "compass=north",
        "pinned=true",
        "session_count=2",
        "session: abc",
        "sidebar=left",
        "otpauth=x",
    ] {
        assert_eq!(redact(s), s);
    }
    let secrets = [
        ["do", "p_v1_", &"a".repeat(40)].concat(),
        ["sh", "pat_", &"b".repeat(32)].concat(),
        ["py", "pi-", &"C".repeat(40)].concat(),
    ];
    for secret in secrets {
        assert_eq!(
            redact(&format!("see {secret} end")),
            "see [redacted] end",
            "{secret}"
        );
    }
}

#[test]
fn array_values_are_masked_whole() {
    check(&[
        (
            r#"{"tokens":["abc12345678"]}"#,
            r#"{"tokens":[[redacted]]}"#,
        ),
        (
            r#"{"x-api-key":["abcdef12345"],"n":1}"#,
            r#"{"x-api-key":[[redacted]],"n":1}"#,
        ),
        (
            r#"{"cookie":["sid=abc","b=c"]}"#,
            r#"{"cookie":[[redacted]]}"#,
        ),
        (
            r#"{"secrets":[["a"],["b"]],"n":1}"#,
            r#"{"secrets":[[redacted]],"n":1}"#,
        ),
        (
            "\"token\": [\n  \"abc\",\n  \"def\"\n]\nnext",
            "\"token\": [[redacted]]\nnext",
        ),
        ("token: [abc, def]", "token: [[redacted]]"),
        // A `]` inside a string does not close it; an unclosed array goes to the end.
        (
            r#"{"token":["a]b"],"n":1}"#,
            r#"{"token":[[redacted]],"n":1}"#,
        ),
        ("token: [abc", "token: [[redacted]"),
    ]);
    // Empty arrays and counts in arrays stay.
    assert_eq!(redact("tokens: []"), "tokens: []");
}
