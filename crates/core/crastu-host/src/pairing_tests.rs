use super::*;
use crate::tests::{Clips, host};
use crastu_ipc::method;

/// A call as the shim sends it.
fn ipc_call(h: &Host, agent: &str, token: Option<&str>, tool: &str) -> Result<Value, String> {
    let mut p = json!({"agent": agent, "name": tool, "args": {}});
    if let Some(t) = token {
        p["token"] = json!(t);
    }
    h.handle_ipc(method::TOOLS_CALL, p)
}

fn pair(h: &Host, name: &str) -> String {
    pairing::add(h.shared().store().conn(), name).unwrap()
}

/// Change is Auto for the paired agent "tole" and stays Ask for everyone else.
fn tole_may_change(h: &Host) {
    let mut p = Policy::default();
    p.set_agent("tole", Risk::Change, Decision::Auto);
    h.set_policy(p);
}

#[test]
fn an_unpaired_caller_claiming_a_paired_name_gets_the_default() {
    let h = host(false);
    let token = pair(&h, "tole");
    tole_may_change(&h);
    let err = ipc_call(&h, "tole", None, "clip_reject").unwrap_err();
    assert!(err.contains("needs approval"), "{err}");
    // The same name with the token takes the override.
    assert_eq!(
        ipc_call(&h, "tole", Some(&token), "clip_reject").unwrap(),
        json!("rejected")
    );
}

#[test]
fn the_declared_name_is_ignored_when_a_token_is_presented() {
    let h = host(false);
    let token = pair(&h, "tole");
    tole_may_change(&h);
    // Declaring another name does not lose the pairing; the token decides.
    assert!(ipc_call(&h, "someone-else", Some(&token), "clip_reject").is_ok());
    // A paired agent without an override gets the default, whatever it declares.
    let other = pair(&h, "other");
    assert!(ipc_call(&h, "tole", Some(&other), "clip_reject").is_err());
}

#[test]
fn an_invalid_token_is_an_error_not_a_downgrade() {
    let h = host(true); // Ask would be approved, so a downgrade would succeed
    pair(&h, "tole");
    let err = ipc_call(&h, "tole", Some("crastu_wrong"), "clip_reject").unwrap_err();
    assert!(err.contains("not valid"), "{err}");
    let err = ipc_call(&h, "tole", Some(""), "project_list").unwrap_err();
    assert!(err.contains("not valid"), "{err}");
    let log = h.activity(10);
    assert_eq!(log[0].decision, "bad token");
    assert_eq!(log[0].outcome, "blocked");
    assert!(!log[0].paired);
}

#[test]
fn revoke_stops_the_token() {
    let h = host(true);
    let token = pair(&h, "tole");
    assert!(ipc_call(&h, "x", Some(&token), "project_list").is_ok());
    pairing::revoke(h.shared().store().conn(), "tole").unwrap();
    let err = ipc_call(&h, "x", Some(&token), "project_list").unwrap_err();
    assert!(err.contains("revoked"), "{err}");
    let listed = pairing::list(h.shared().store().conn()).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].revoked);
    assert!(matches!(
        pairing::rotate(h.shared().store().conn(), "tole"),
        Err(pairing::PairingError::RevokedAgent(_))
    ));
    assert!(matches!(
        pairing::revoke(h.shared().store().conn(), "nobody"),
        Err(pairing::PairingError::NotFound(_))
    ));
}

#[test]
fn rotate_replaces_the_token() {
    let h = host(true);
    let old = pair(&h, "tole");
    let new = pairing::rotate(h.shared().store().conn(), "tole").unwrap();
    assert_ne!(old, new);
    assert!(ipc_call(&h, "x", Some(&old), "project_list").is_err());
    assert!(ipc_call(&h, "x", Some(&new), "project_list").is_ok());
}

#[test]
fn tokens_are_random_prefixed_and_names_are_checked() {
    let a = pairing::generate_token().unwrap();
    let b = pairing::generate_token().unwrap();
    assert_ne!(a, b);
    assert!(a.starts_with("crastu_"));
    assert_eq!(a.len(), "crastu_".len() + 64);
    assert!(a["crastu_".len()..].bytes().all(|c| c.is_ascii_hexdigit()));
    let h = host(true);
    for bad in ["", "Tole", "a b", "a/b", &"a".repeat(65)] {
        assert!(matches!(
            pairing::add(h.shared().store().conn(), bad),
            Err(pairing::PairingError::InvalidName)
        ));
    }
    pair(&h, "tole");
    assert!(matches!(
        pairing::add(h.shared().store().conn(), "tole"),
        Err(pairing::PairingError::Exists(_))
    ));
}

#[test]
fn constant_time_compare() {
    assert!(pairing::ct_eq(b"abcd", b"abcd"));
    assert!(pairing::ct_eq(b"", b""));
    assert!(!pairing::ct_eq(b"abcd", b"abce"));
    assert!(!pairing::ct_eq(b"abcd", b"bbcd"));
    assert!(!pairing::ct_eq(b"abcd", b"abc"));
}

#[test]
fn the_plain_token_is_not_in_the_database_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("crastu.db")).unwrap();
    let h = Host::new(
        store,
        JobQueue::new(1, 1),
        vec![Box::new(Clips)],
        Box::new(DenyAsk),
    )
    .unwrap();
    let token = pair(&h, "tole");
    assert!(ipc_call(&h, "x", Some(&token), "project_list").is_ok());
    // The database and its journal files: the token could be in either.
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        bytes.extend(std::fs::read(entry.unwrap().path()).unwrap());
    }
    let contains = |needle: &str| bytes.windows(needle.len()).any(|w| w == needle.as_bytes());
    assert!(!contains(&token), "the plain token is on disk");
    assert!(
        !contains(&token["crastu_".len()..]),
        "the token body is on disk"
    );
    assert!(contains(&pairing::hash_token(&token)), "the hash is stored");
}

#[test]
fn the_activity_log_marks_paired_and_unpaired() {
    let h = host(true);
    let token = pair(&h, "tole");
    ipc_call(&h, "claude-code", None, "project_list").unwrap();
    ipc_call(&h, "ignored", Some(&token), "project_list").unwrap();
    let log = h.activity(2);
    assert_eq!((log[0].agent.as_str(), log[0].paired), ("tole", true));
    assert_eq!(
        (log[1].agent.as_str(), log[1].paired),
        ("claude-code", false)
    );
}

#[test]
fn the_token_does_not_reach_debug_output() {
    let p = crastu_ipc::CallParams {
        agent: "a".into(),
        token: Some("crastu_secret".into()),
        name: "n".into(),
        args: Value::Null,
    };
    assert!(!format!("{p:?}").contains("crastu_secret"));
}
