use super::*;
use std::fs;

fn found(text: &str) -> Vec<String> {
    check_source("x.rs", text)
        .findings
        .into_iter()
        .map(|f| f.what)
        .collect()
}

#[test]
fn clean_code_passes() {
    let code = r##"
        let t = Tokens::new(&appearance, false);
        div().bg(t.colors.bg.rgb()).text_color(t.colors.text.rgb());
        let mask = 0xFF; let n = 0x1234; let big = 0x1234567;
        let name = "Interval"; // not a font
        let id = "#12"; let tag = "#GGGGGG";
    "##;
    assert_eq!(found(code), Vec::<String>::new());
}

#[test]
fn a_hex_colour_integer_is_found() {
    assert_eq!(
        found("div().bg(rgb(0x111214))"),
        ["colour literal `0x111214`"]
    );
    assert_eq!(
        found("let c = 0x11121480;"),
        ["colour literal `0x11121480`"]
    );
    assert_eq!(
        found("let c = 0x11_12_14;"),
        ["colour literal `0x11_12_14`"]
    );
    // Seven digits and four digits are not colours.
    assert!(found("let c = 0x1112143; let d = 0x1112;").is_empty());
}

#[test]
fn colour_calls_with_numbers_are_found() {
    assert_eq!(found("rgb(10, 20, 30)"), ["colour literal `rgb(..)`"]);
    assert_eq!(
        found("hsla(0.1, 0.2, 0.3, 1.0)"),
        ["colour literal `hsla(..)`"]
    );
    assert_eq!(
        found("let c = gpui::rgba(0.5);"),
        ["colour literal `rgba(..)`"]
    );
    // A call with a token is fine, and so is a function that only ends in "rgb".
    assert!(found("rgb(t.bg.hex()); to_rgb(1)").is_empty());
}

#[test]
fn named_colour_functions_are_found() {
    assert_eq!(found("div().bg(gpui::red())"), ["named colour `red()`"]);
    assert_eq!(found(".text_color(white())"), ["named colour `white()`"]);
    assert!(found("let c = colour_white();").is_empty());
}

#[test]
fn colour_and_font_strings_are_found() {
    assert_eq!(
        found(r##"let c = "#111214";"##),
        ["colour literal \"#111214\""]
    );
    assert_eq!(
        found(r##"let c = "#11121480";"##),
        ["colour literal \"#11121480\""]
    );
    assert_eq!(found(r#".font_family("Inter")"#), ["font name \"Inter\""]);
    assert_eq!(
        found(r#"let f = "JetBrains Mono, monospace";"#),
        ["font name \"JetBrains Mono\""]
    );
    assert_eq!(
        found(r#"let f = "IBM Plex Sans";"#),
        ["font name \"IBM Plex\""]
    );
    assert_eq!(
        found(r#"let f = "Helvetica";"#),
        ["font name \"Helvetica\""]
    );
}

#[test]
fn comments_and_char_literals_are_ignored() {
    let code = "// 0x111214 and \"Inter\"\n/// the \"#111214\" colour\nlet quote = '\"'; let x = 1; // #111214 Inter\nlet y = '\\'';";
    assert!(found(code).is_empty(), "{:?}", found(code));
    // A quote character literal must not turn the rest of the line into a string.
    assert_eq!(
        found("let q = '\"'; let c = 0x111214;"),
        ["colour literal `0x111214`"]
    );
}

#[test]
fn the_allow_marker_skips_a_line_and_is_counted() {
    let scan = check_source(
        "x.rs",
        "let c = 0x111214; // ui-literals: allow (test chart)\nlet d = 0x222324;",
    );
    assert_eq!(scan.allowed, 1);
    assert_eq!(scan.findings.len(), 1);
    assert_eq!(scan.findings[0].line, 2);
}

#[test]
fn findings_carry_the_path_and_line() {
    let scan = check_source("crates/ui/a/src/b.rs", "fn a() {}\nlet c = 0x111214;\n");
    assert_eq!(
        scan.findings,
        [Finding {
            path: "crates/ui/a/src/b.rs".into(),
            line: 2,
            what: "colour literal `0x111214`".into()
        }]
    );
}

#[test]
fn the_token_module_is_skipped() {
    assert!(is_token_module("crates/ui/crastu-ui/src/tokens/colors.rs"));
    assert!(is_token_module("crates/ui/crastu-ui/src/tokens.rs"));
    assert!(!is_token_module("crates/ui/crastu-ui/src/button.rs"));
    assert!(!is_token_module("crates/ui/crastu-ui/src/tokenize.rs"));
}

#[test]
fn run_fails_on_a_literal_in_a_ui_crate_and_ignores_the_rest() {
    use serde_json::json;

    let root = std::env::temp_dir().join(format!("xtask-ui-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let write = |path: &str, text: &str| {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    };
    write(
        "crates/ui/a/src/lib.rs",
        "fn draw() { let c = 0x111214; }\n",
    );
    write(
        "crates/ui/a/src/tokens/colors.rs",
        "const BG: u32 = 0x111214;\n",
    );
    write(
        "crates/features/x/x-ui/src/lib.rs",
        "fn f() { font(\"Inter\"); }\n",
    );
    write("crates/core/c/src/lib.rs", "const RED: u32 = 0xFF0000;\n");
    let package = |name: &str, kind: &str, dir: &str| {
        json!({
            "name": name,
            "manifest_path": root.join(dir).join("Cargo.toml").to_string_lossy(),
            "metadata": {"crastu": {"kind": kind}},
        })
    };
    let meta = json!({
        "workspace_root": root.to_string_lossy(),
        "packages": [
            package("a", "ui", "crates/ui/a"),
            package("x-ui", "feature-ui", "crates/features/x/x-ui"),
            package("c", "core", "crates/core/c"),
        ],
    });
    // One violation in `a` (the tokens module is skipped), one in the feature UI crate, none in
    // the core crate.
    assert_eq!(run(&meta).unwrap_err(), "2 literal(s) in UI code");

    // Clean the two violations: the check passes.
    write("crates/ui/a/src/lib.rs", "fn draw() {}\n");
    write("crates/features/x/x-ui/src/lib.rs", "fn f() {}\n");
    run(&meta).unwrap();
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn run_fails_closed_when_a_ui_crate_cannot_be_read() {
    use serde_json::json;

    let root = std::env::temp_dir().join(format!("xtask-ui-missing-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    // The package says it lives in a folder that has no `src`.
    let meta = json!({
        "workspace_root": root.to_string_lossy(),
        "packages": [{
            "name": "ghost-ui",
            "manifest_path": root.join("crates/ui/ghost/Cargo.toml").to_string_lossy(),
            "metadata": {"crastu": {"kind": "ui"}},
        }],
    });
    let err = run(&meta).unwrap_err();
    assert!(err.contains("ghost"), "{err}");
    let _ = fs::remove_dir_all(&root);
}
