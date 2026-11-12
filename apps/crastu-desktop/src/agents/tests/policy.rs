//! A damaged policy row, the reset, and the mode of a database that already exists.

use super::*;

fn damage(dir: &tempfile::TempDir) {
    let db = Store::open(dir.path().join("data/crastu.db")).unwrap();
    db.conn()
        .execute(
            "INSERT INTO agent_policy (agent, risk, decision) VALUES ('', 'change', 'deny')",
            [],
        )
        .unwrap();
}

#[test]
fn a_damaged_policy_row_keeps_the_agent_list_and_flags_it() {
    let (dir, s) = store();
    let t = s.pair("tole").unwrap();
    damage(&dir);
    let snap = s.snapshot().expect("the snapshot still works");
    assert!(snap.policy_damaged);
    assert_eq!(snap.agents.len(), 1);
    assert_eq!(snap.defaults, Defaults::default());
    assert!(!format!("{snap:?}").contains(t.expose()));
    // Revoke and Rotate still work.
    assert!(s.rotate("tole").is_ok());
    s.revoke("tole").unwrap();
    assert!(s.snapshot().unwrap().agents[0].revoked);
}

#[test]
fn resetting_the_policy_clears_the_damage_and_every_override() {
    let (dir, s) = store();
    s.pair("tole").unwrap();
    s.set_override("tole", Class::Delete, Some(Level::Auto))
        .unwrap();
    damage(&dir);
    assert!(s.snapshot().unwrap().policy_damaged);
    assert!(s.reset_policy().unwrap());
    let snap = s.snapshot().unwrap();
    assert!(!snap.policy_damaged);
    assert_eq!(snap.defaults, Defaults::default());
    assert_eq!(snap.agents[0].delete, None);
}

#[cfg(unix)]
#[test]
fn a_folder_and_a_database_that_are_too_open_are_tightened_on_open() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let db = data.join("crastu.db");
    std::fs::write(&db, b"").unwrap();
    std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).unwrap();
    AgentStore::new(&data).snapshot().unwrap();
    assert_eq!(mode(&data), 0o700);
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(mode(&path), 0o600, "{}", path.display());
    }
}

#[test]
fn a_reset_does_not_wipe_a_policy_that_is_no_longer_damaged() {
    let (dir, s) = store();
    s.pair("tole").unwrap();
    s.set_override("tole", Class::Delete, Some(Level::Auto))
        .unwrap();
    damage(&dir);
    assert!(s.snapshot().unwrap().policy_damaged);
    // Fixed from the CLI: the bad row is gone.
    let db = Store::open(dir.path().join("data/crastu.db")).unwrap();
    db.conn()
        .execute("DELETE FROM agent_policy WHERE agent = ''", [])
        .unwrap();
    assert!(!s.reset_policy().unwrap(), "nothing to reset");
    assert_eq!(s.snapshot().unwrap().agents[0].delete, Some(Level::Auto));
}
