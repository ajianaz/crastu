use super::*;
use crastu_module::{Module, ModuleManifest, Registry, Risk, ToolError, ToolSpec};
use crastu_store::Store;
use serde_json::json;

fn settings_file(json: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    std::fs::write(&path, json).unwrap();
    (dir, path)
}

#[test]
fn the_gate_follows_the_settings_file_and_the_environment() {
    let (_d, on) = settings_file(r#"{"network.offline": true}"#);
    let p = on.clone();
    assert!(gate_from(None, || Ok(p)).unwrap().is_offline());
    let p = on.clone();
    assert!(!gate_from(Some("0"), || Ok(p)).unwrap().is_offline());
    let (_d, off) = settings_file("{}");
    let p = off.clone();
    assert!(!gate_from(None, || Ok(p)).unwrap().is_offline());
    assert!(gate_from(Some("yes"), || Ok(off)).unwrap().is_offline());
}

#[test]
fn a_missing_file_means_online_and_a_broken_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("settings.json");
    assert!(!gate_from(None, || Ok(missing)).unwrap().is_offline());
    let (_d, bad) = settings_file("{ not json");
    let e = gate_from(None, || Ok(bad.clone())).unwrap_err();
    assert!(e.contains("CRASTU_OFFLINE"), "{e}");
    // The environment still decides without reading the broken file.
    assert!(gate_from(Some("1"), || Ok(bad)).unwrap().is_offline());
}

#[test]
fn without_a_data_folder_the_gate_asks_instead_of_guessing_online() {
    let e = gate_from(None, || Err("no home".into())).unwrap_err();
    assert!(
        e.contains("CRASTU_DATA") && e.contains("CRASTU_OFFLINE"),
        "{e}"
    );
    // An empty or blank variable defers to the setting, so it fails the same way.
    assert!(gate_from(Some(" "), || Err("no home".into())).is_err());
    // The environment variable alone is enough, and a typo fails closed.
    assert!(
        !gate_from(Some("0"), || Err("no home".into()))
            .unwrap()
            .is_offline()
    );
    assert!(
        gate_from(Some("1"), || Err("no home".into()))
            .unwrap()
            .is_offline()
    );
    assert!(
        gate_from(Some("ture"), || Err("no home".into()))
            .unwrap()
            .is_offline()
    );
}

#[test]
fn the_offline_variable_is_read_without_losing_odd_values() {
    use std::ffi::OsString;
    assert_eq!(offline_env(None), None);
    assert_eq!(offline_env(Some(OsString::from("1"))).as_deref(), Some("1"));
    assert_eq!(offline_env(Some(OsString::new())).as_deref(), Some(""));
}

#[cfg(unix)]
#[test]
fn a_non_unicode_offline_variable_means_on_even_if_the_setting_is_off() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let env = offline_env(Some(OsString::from_vec(vec![0x66, 0xff, 0xfe])));
    assert!(env.is_some(), "not read as unset");
    let (_d, off) = settings_file("{}");
    assert!(gate_from(env.as_deref(), || Ok(off)).unwrap().is_offline());
}

#[test]
fn the_host_gate_starts_from_the_same_sources_as_models() {
    let (_d, on) = settings_file(r#"{"network.offline": true}"#);
    let p = on.clone();
    assert!(HostGate::new(None, || Ok(p)).gate().is_offline());
    assert!(!HostGate::new(Some("0"), || Ok(on)).gate().is_offline());
    let (_d, off) = settings_file(r#"{"network.offline": false}"#);
    let p = off.clone();
    assert!(!HostGate::new(None, || Ok(p)).gate().is_offline());
    assert!(HostGate::new(Some("1"), || Ok(off)).gate().is_offline());
}

#[test]
fn the_host_gate_fails_closed_and_says_so_once() {
    // No data folder and no variable: models stops, the host runs with Offline on.
    let hg = HostGate::new(None, || Err("no home".into()));
    assert!(hg.gate().is_offline());
    let said = hg.reported.lock().unwrap().clone().unwrap();
    assert!(said.contains("CRASTU_DATA"), "{said}");
    assert_eq!(hg.refresh(), None, "reported once, not on every call");
    assert!(hg.gate().is_offline());
    // A settings file that cannot be read, even when it would say off.
    let (_d, bad) = settings_file("{ not json");
    let hg = HostGate::new(None, || Ok(bad));
    assert!(hg.gate().is_offline());
    assert!(hg.reported.lock().unwrap().is_some());
}

#[test]
fn the_host_gate_follows_the_setting_while_it_runs() {
    let (_d, path) = settings_file(r#"{"network.offline": false}"#);
    let p = path.clone();
    let hg = HostGate::new(None, || Ok(p));
    assert!(!hg.gate().is_offline());
    std::fs::write(&path, r#"{"network.offline": true}"#).unwrap();
    assert_eq!(hg.refresh(), None);
    assert!(hg.gate().is_offline(), "turned on by the desktop");
    std::fs::write(&path, r#"{"network.offline": false}"#).unwrap();
    hg.refresh();
    assert!(!hg.gate().is_offline(), "turned off again");
    // Half-written or broken: on, reported once.
    std::fs::write(&path, "{ not json").unwrap();
    let line = hg.refresh().expect("a broken file is reported");
    assert!(line.starts_with("Offline is on: cannot read"), "{line}");
    assert!(hg.gate().is_offline());
    assert_eq!(hg.refresh(), None);
    // Fixed: the setting decides again, and the same break later is reported again.
    std::fs::write(&path, "{}").unwrap();
    assert_eq!(hg.refresh(), None);
    assert!(!hg.gate().is_offline());
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(hg.refresh(), Some(line));
    // Removed: the default, off.
    std::fs::remove_file(&path).unwrap();
    hg.refresh();
    assert!(!hg.gate().is_offline());
}

#[test]
fn the_setting_never_overrides_the_environment() {
    let (_d, path) = settings_file(r#"{"network.offline": false}"#);
    let p = path.clone();
    let hg = HostGate::new(Some("1"), || Ok(p));
    assert!(hg.gate().is_offline());
    // Someone flips the shared gate (or the file says off): the next call puts the variable back.
    hg.gate().set_offline(false);
    hg.refresh();
    assert!(hg.gate().is_offline(), "never looser than CRASTU_OFFLINE");
    std::fs::write(&path, "{ not json").unwrap();
    assert_eq!(hg.refresh(), None, "the file is not read at all");
    let p = path.clone();
    let hg = HostGate::new(Some("0"), || Ok(p));
    std::fs::write(&path, r#"{"network.offline": true}"#).unwrap();
    hg.refresh();
    assert!(!hg.gate().is_offline(), "the variable decides both ways");
}

/// A tool that wants the network: it asks the gate before it would open a request.
struct Fetcher(NetGate);
impl Module for Fetcher {
    fn manifest(&self) -> ModuleManifest {
        ModuleManifest {
            id: "fetcher",
            name: "Fetcher",
            version: "0.0.0",
        }
    }
    fn register(&self, r: &mut Registry) {
        let gate = self.0.clone();
        r.tool(
            ToolSpec {
                name: "fetch_page".into(),
                description: "Fetch".into(),
                risk: Risk::Read,
                input_schema: json!({}),
            },
            move |_a: Value| {
                gate.check(
                    "page fetch",
                    "https://user:pw@Example.org:8443/secret/path?token=abc123",
                )
                .map(|()| json!("fetched"))
                .map_err(|e| ToolError::Failed(e.to_string()))
            },
        );
    }
}

fn fetch(net: &HostGate, host: &Host) -> Result<Value, String> {
    let params = json!({"agent": "claude-code", "name": "fetch_page", "args": {}});
    net.handle(host, method::TOOLS_CALL, params)
}

#[test]
fn the_cli_host_logs_refusals_from_tool_calls_and_follows_the_setting() {
    let (_d, path) = settings_file(r#"{"network.offline": true}"#);
    let p = path.clone();
    let net = HostGate::new(None, || Ok(p));
    let tool = Fetcher(net.gate().clone());
    let host = crate::open_host(
        Store::open_in_memory().unwrap(),
        1,
        vec![Box::new(tool)],
        &net,
    )
    .unwrap();

    let e = fetch(&net, &host).unwrap_err();
    assert_eq!(e, "Offline: page fetch from example.org refused");
    let log = host.activity(10);
    assert_eq!(log.len(), 2, "the refusal and the call: {log:?}");
    let refusal = &log[1];
    assert_eq!(
        (refusal.agent.as_str(), refusal.tool.as_str()),
        ("claude-code", "fetch_page")
    );
    assert_eq!(refusal.decision, "refused: page fetch from example.org");
    assert_eq!(refusal.outcome, "blocked");
    for row in &log {
        let text = format!("{row:?}");
        assert!(
            !text.contains("secret") && !text.contains("abc123") && !text.contains("pw"),
            "{text}"
        );
    }

    // The desktop turns Offline off while the host runs: the next call goes out.
    std::fs::write(&path, r#"{"network.offline": false}"#).unwrap();
    assert_eq!(fetch(&net, &host).unwrap(), json!("fetched"));
    assert_eq!(host.activity(10).len(), 3, "only the call, no refusal");
}
