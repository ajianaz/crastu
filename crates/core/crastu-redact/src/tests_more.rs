use super::*;

/// A PEM block built from parts, so that secret scanners do not flag the fixtures.
fn pem(kind: &str, body: &str, sep: &str) -> String {
    let (begin, end) = ("-----BEGIN ", "-----END ");
    let label = format!("{kind}PRIVATE KEY");
    format!("{begin}{label}-----{sep}{body}{sep}{end}{label}-----")
}

#[test]
fn shell_flags_lose_their_value() {
    let cases = [
        (
            "curl --password hunter2 http://x",
            "curl --password [redacted] http://x",
        ),
        (
            "cli --token abcdefghijkl --user bob",
            "cli --token [redacted] --user bob",
        ),
        ("tool --api-key sk_x1 run", "tool --api-key [redacted] run"),
        ("tool --secret x", "tool --secret [redacted]"),
        (
            "tool --password=hunter2 go",
            "tool --password=[redacted] go",
        ),
        (
            "tool --password 'a b' go",
            "tool --password '[redacted]' go",
        ),
        (
            "mysql -u root -p hunter2 db",
            "mysql -u root -p [redacted] db",
        ),
        ("mysql -phunter2 db", "mysql -p[redacted] db"),
        (
            "sshpass -p 'hunter 2' ssh host",
            "sshpass -p '[redacted]' ssh host",
        ),
        ("psql -p 5432 db", "psql -p [redacted] db"),
        (
            "curl -H 'Authorization: Bearer abcdefgh1' url",
            "curl -H 'Authorization: [redacted]' url",
        ),
        (
            r#"curl --header "X-Api-Key: y" url"#,
            r#"curl --header "X-Api-Key: [redacted]" url"#,
        ),
        // Not secrets: a flag followed by another flag, a port, `-p` without a value.
        ("tool --password --verbose", "tool --password --verbose"),
        ("docker run -p 8080:80 img", "docker run -p 8080:80 img"),
        ("ls -pfoo", "ls -pfoo"),
        ("tool --max-tokens 100", "tool --max-tokens 100"),
    ];
    for (input, expected) in cases {
        assert_eq!(redact(input), expected, "{input}");
    }
    // A command on an earlier line does not make `-p` on a later one a password.
    assert_eq!(redact("mysql db\nls -pfoo"), "mysql db\nls -pfoo");
}

#[test]
fn escaped_json_is_masked() {
    let cases = [
        (
            r#"{\"password\":\"hunter2\"}"#,
            r#"{\"password\":\"[redacted]\"}"#,
        ),
        (
            r#"\"token\": \"x\", \"n\": 1"#,
            r#"\"token\": \"[redacted]\", \"n\": 1"#,
        ),
        (
            r#"{\\\"password\\\":\\\"hunter2\\\"}"#,
            r#"{\\\"password\\\":\\\"[redacted]\\\"}"#,
        ),
        (
            r#"{\"accessToken\":\"a b\",\"user\":\"bob\"}"#,
            r#"{\"accessToken\":\"[redacted]\",\"user\":\"bob\"}"#,
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(redact(input), expected, "{input}");
    }
}

#[test]
fn pem_private_keys_are_masked_whole() {
    for kind in ["", "RSA ", "EC ", "OPENSSH ", "ENCRYPTED ", "PGP "] {
        let block = pem(kind, "MIIEowIBAAKCAQEA", "\n");
        assert_eq!(
            redact(&format!("key:\n{block}\nafter")),
            "key:\n[redacted private key]\nafter",
            "{kind}"
        );
        // The same with a literal backslash-n between the lines, as in a JSON string.
        let literal = pem(kind, "MIIEowIBAAKCAQEA", "\\n");
        assert_eq!(
            redact(&format!("x {literal} y")),
            "x [redacted private key] y",
            "{kind}"
        );
    }
    // The END marker is missing (cut off): everything after the header goes.
    let cut = pem("RSA ", "MIIEowIBAAKCAQEA", "\n");
    let cut = cut.split("-----END").next().unwrap_or_default();
    assert_eq!(redact(&format!("a {cut}")), "a [redacted private key]");
    // Public material is left alone.
    let cert = "-----BEGIN CERTIFICATE-----\nMIIC\n-----END CERTIFICATE-----";
    assert_eq!(redact(cert), cert);
}

#[test]
fn token_counts_and_the_working_directory_are_not_secrets() {
    for s in [
        "max_tokens: 100",
        "prompt_tokens=12",
        "n_tokens: 5",
        "total_tokens: 99",
        "input_tokens=1",
        "output_tokens: 2",
        "completion_tokens: 3",
        "tokens: 7",
        "max_tokens: unlimited",
        "completion_tokens=null",
        "tokens: n/a",
        "PWD=/home/a",
        "OLDPWD=/tmp",
    ] {
        assert_eq!(redact(s), s);
    }
    assert_eq!(redact("access_tokens: abc"), "access_tokens: [redacted]");
    assert_eq!(redact("auth_tokens=zz"), "auth_tokens=[redacted]");
    // Plural names with a number are counts; a lowercase `pwd` and `PWD_HASH` are still passwords.
    assert_eq!(redact("access_tokens: 12345"), "access_tokens: 12345");
    assert_eq!(redact("pwd = hunter2"), "pwd = [redacted]");
    assert_eq!(redact("PWD_HASH=x"), "PWD_HASH=[redacted]");
}

#[test]
fn a_value_on_the_next_line_is_masked_only_if_it_looks_like_one() {
    // Alone on its line: masked, whatever its length, with or without indentation.
    assert_eq!(redact("password:\nhunter2"), "password:\n[redacted]");
    assert_eq!(
        redact("password:\n  hunter2\nuser: bob"),
        "password:\n  [redacted]\nuser: bob"
    );
    assert_eq!(
        redact("password:\r\nhunter2\r\n"),
        "password:\r\n[redacted]\r\n"
    );
    // Prose on the next line is left alone, unless its first word is 8 or more characters.
    assert_eq!(redact("token:\nnext line text"), "token:\nnext line text");
    assert_eq!(
        redact("token:\nabcdefgh12 more text"),
        "token:\n[redacted] more text"
    );
    // A blank line is not skipped.
    assert_eq!(redact("token:\n\nhunter2abc"), "token:\n\nhunter2abc");
}

#[test]
fn more_token_shapes_and_spaced_passwords() {
    let secrets = [
        ["xa", "pp-1-A012345-12345678-abcdef0123456789"].concat(),
        ["wh", "sec_abcdefghijklmnopqrstuvwxyz"].concat(),
        ["GOCSPX", "-abcdefghijklmnopqrstuvwxyz12"].concat(),
        format!("S{}.{}.{}", "G", "a".repeat(22), "b".repeat(43)),
        format!("{}:{}", "123456789", "A".repeat(35)),
    ];
    for secret in secrets {
        assert_eq!(
            redact(&format!("see {secret} end")),
            "see [redacted] end",
            "{secret}"
        );
    }
    let hook = [
        "https://hooks.",
        "slack.com/services/T00000000/B00000000/abcdefghijklmnopqrstuvwx",
    ];
    assert_eq!(
        redact(&format!("post {} now", hook.concat())),
        "post https://[redacted] now"
    );
    // A password with spaces goes to the end of the line (over-redaction, on purpose).
    assert_eq!(redact("password=my secret phrase"), "password=[redacted]");
    assert_eq!(redact("passwd=a b\nnext"), "passwd=[redacted]\nnext");
    assert_eq!(redact("password=abc&user=x"), "password=[redacted]&user=x");
    assert_eq!(redact("password=hunter2; ok"), "password=[redacted]; ok");
}

#[test]
fn long_names_are_judged_by_their_last_words() {
    let prefix = "very_long_service_account_name_for_the_billing_integration_client_";
    assert!(prefix.len() > DECIDE);
    for tail in [
        "secret_value",
        "secret",
        "password",
        "api_key",
        "access_token",
    ] {
        let name = format!("{prefix}{tail}");
        assert_eq!(
            redact(&format!("{name}: x")),
            format!("{name}: [redacted]"),
            "{tail}"
        );
        assert_eq!(
            redact(&format!("{name}=x")),
            format!("{name}=[redacted]"),
            "{tail}"
        );
    }
    // 100 and 300 byte names, camelCase too, and a long name that is not a secret.
    for len in [100, 300] {
        let name = format!("{}_client_secret", "ab_".repeat(len / 3));
        assert_eq!(redact(&format!("{name}: v")), format!("{name}: [redacted]"));
    }
    let long_camel = format!("{}AccessToken", "service".repeat(20));
    assert_eq!(
        redact(&format!("{long_camel}=v")),
        format!("{long_camel}=[redacted]")
    );
    let plain = format!("{}_name", "ab_".repeat(40));
    assert_eq!(redact(&format!("{plain}: v")), format!("{plain}: v"));
}

#[test]
fn header_values_go_to_the_end_of_the_line() {
    let cases = [
        ("Authorization: Bearer abc", "Authorization: [redacted]"),
        (
            "Authorization: Basic abc\nHost: x",
            "Authorization: [redacted]\nHost: x",
        ),
        (
            "Proxy-Authorization: Digest a b c",
            "Proxy-Authorization: [redacted]",
        ),
        ("x-api-key: abc def", "x-api-key: [redacted]"),
        (
            "X-Auth-Token: abc def\nnext: 1",
            "X-Auth-Token: [redacted]\nnext: 1",
        ),
        ("client_secret: abc def", "client_secret: [redacted]"),
        ("credentials: user pass", "credentials: [redacted]"),
        ("Cookie: a=b; c=d", "Cookie: [redacted]"),
        ("Set-Cookie: sid=abc; Path=/", "Set-Cookie: [redacted]"),
        // A scheme word elsewhere takes the credential after it.
        ("token=Bearer abc next", "token=[redacted] next"),
        ("secret=Basic abc&x=1", "secret=[redacted]&x=1"),
        // A password after a colon is one word.
        ("password: hunter2 next", "password: [redacted] next"),
    ];
    for (input, expected) in cases {
        assert_eq!(redact(input), expected, "{input}");
    }
}
