use super::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn parses_defaults_and_flags() {
    assert_eq!(
        parse_args(&[]).unwrap(),
        Options {
            debug: false,
            identity: None
        }
    );
    let o = parse_args(&s(&["--debug", "--sign", "Dev ID"])).unwrap();
    assert!(o.debug);
    assert_eq!(o.identity.as_deref(), Some("Dev ID"));
}

#[test]
fn rejects_bad_args() {
    assert!(parse_args(&s(&["--sign"])).is_err());
    assert!(parse_args(&s(&["--sign", "--debug"])).is_err());
    assert!(parse_args(&s(&["--nope"])).is_err());
}

#[test]
fn escapes_xml() {
    assert_eq!(
        xml_escape("a&b<c>\"d\"'e'"),
        "a&amp;b&lt;c&gt;&quot;d&quot;&apos;e&apos;"
    );
    assert_eq!(xml_escape("plain"), "plain");
}

#[test]
fn plist_has_required_keys_and_version() {
    let p = info_plist("1.2.3");
    for key in [
        "CFBundleName",
        "CFBundleDisplayName",
        "CFBundleExecutable",
        "CFBundleIdentifier",
        "CFBundleVersion",
        "CFBundleShortVersionString",
        "CFBundlePackageType",
        "LSMinimumSystemVersion",
        "NSHighResolutionCapable",
        "NSPrincipalClass",
        "NSMicrophoneUsageDescription",
    ] {
        assert!(p.contains(&format!("<key>{key}</key>")), "missing {key}");
    }
    assert!(p.contains("<key>CFBundleName</key>\n\t<string>Crastu</string>"));
    assert!(p.contains("<key>CFBundleDisplayName</key>\n\t<string>Crastu</string>"));
    assert!(p.contains("<key>CFBundleVersion</key>\n\t<string>1.2.3</string>"));
    assert!(p.contains("<key>CFBundleShortVersionString</key>\n\t<string>1.2.3</string>"));
    assert!(p.contains("<key>CFBundleExecutable</key>\n\t<string>crastu</string>"));
    assert!(p.contains("<key>CFBundlePackageType</key>\n\t<string>APPL</string>"));
    assert!(p.contains("<key>NSHighResolutionCapable</key>\n\t<true/>"));
    assert!(p.contains("<string>dev.crastu.desktop</string>"));
    assert!(!p.contains("Accessibility"));
}

#[test]
fn plist_escapes_values() {
    assert!(info_plist("1<2&3").contains("<string>1&lt;2&amp;3</string>"));
}

#[test]
fn layout_lists_executable_plist_and_libs() {
    assert_eq!(
        layout(&s(&["libonnxruntime.dylib"])),
        s(&[
            "Contents/Info.plist",
            "Contents/MacOS/crastu",
            "Contents/MacOS/libonnxruntime.dylib"
        ])
    );
}

#[test]
fn finds_only_dylibs() {
    let dir = std::env::temp_dir().join(format!("xtask-bundle-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["b.dylib", "a.dylib", "c.txt", "d.dylib.bak"] {
        std::fs::write(dir.join(n), "x").unwrap();
    }
    let names: Vec<String> = dylibs_in(&dir)
        .unwrap()
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(names, s(&["a.dylib", "b.dylib"]));
}
