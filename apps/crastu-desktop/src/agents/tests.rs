use super::*;

fn store() -> (tempfile::TempDir, AgentStore) {
    let dir = tempfile::tempdir().unwrap();
    let s = AgentStore::new(dir.path().join("data"));
    (dir, s)
}

#[test]
fn an_empty_database_shows_no_agents_and_the_default_policy() {
    let (_dir, s) = store();
    let snap = s.snapshot().unwrap();
    assert!(snap.agents.is_empty() && snap.recent.is_empty());
    assert_eq!(snap.defaults, Defaults::default());
}

#[test]
fn pairing_rotating_and_revoking_reach_the_hosts_store() {
    let (dir, s) = store();
    let t1 = s.pair("claude-code").unwrap();
    assert!(t1.expose().starts_with(pairing::TOKEN_PREFIX));
    assert!(
        s.pair("claude-code")
            .unwrap_err()
            .contains("already exists")
    );
    let t2 = s.rotate("claude-code").unwrap();
    assert_ne!(t1.expose(), t2.expose());
    // The host resolves the new token, not the old one.
    let mut db = Store::open(dir.path().join("data/crastu.db")).unwrap();
    crastu_host::migrate(&mut db).unwrap();
    assert_eq!(
        pairing::resolve(db.conn(), t2.expose()).unwrap(),
        "claude-code"
    );
    assert!(pairing::resolve(db.conn(), t1.expose()).is_err());
    drop(db);
    s.revoke("claude-code").unwrap();
    let snap = s.snapshot().unwrap();
    assert_eq!(snap.agents.len(), 1);
    assert!(snap.agents[0].revoked);
    assert!(s.rotate("claude-code").unwrap_err().contains("revoked"));
    assert!(s.revoke("ghost").unwrap_err().contains("no agent"));
}

#[test]
fn the_snapshot_never_carries_a_token() {
    let (_dir, s) = store();
    let token = s.pair("tole").unwrap();
    let shown = format!("{:?}", s.snapshot().unwrap());
    assert!(!shown.contains(token.expose()), "{shown}");
    assert!(!shown.contains(pairing::TOKEN_PREFIX), "{shown}");
}

#[test]
fn the_shells_name_rules_equal_the_hosts() {
    let long = "a".repeat(64);
    let too_long = "a".repeat(65);
    for name in [
        "",
        "a",
        "A",
        "a b",
        "a_b-c.d",
        "é",
        "tole/../x",
        long.as_str(),
        too_long.as_str(),
        "UPPER",
        "with\nnewline",
        "9lives",
        "-",
        ".",
        "a\0b",
    ] {
        assert_eq!(
            crastu_shell::agents::name_problem(name).is_none(),
            pairing::check_name(name).is_ok(),
            "{name:?}"
        );
    }
}

#[test]
fn a_bad_name_is_refused_by_the_store_too() {
    let (_dir, s) = store();
    assert!(s.pair("Bad Name").is_err());
    assert!(s.snapshot().unwrap().agents.is_empty());
}

#[test]
fn the_default_policy_persists_across_a_reopen() {
    let (dir, s) = store();
    s.set_default(Class::Delete, Level::Deny).unwrap();
    s.set_default(Class::Change, Level::Auto).unwrap();
    let again = AgentStore::new(dir.path().join("data"));
    let snap = again.snapshot().unwrap();
    assert_eq!(snap.defaults.delete, Level::Deny);
    assert_eq!(snap.defaults.change, Level::Auto);
}

#[test]
fn a_default_the_host_would_refuse_is_refused_here() {
    let (_dir, s) = store();
    assert!(s.set_default(Class::Change, Level::Deny).is_err());
    assert_eq!(s.snapshot().unwrap().defaults, Defaults::default());
}

#[test]
fn an_override_persists_per_agent_and_clears() {
    let (dir, s) = store();
    s.pair("tole").unwrap();
    s.pair("other").unwrap();
    s.set_override("tole", Class::Delete, Some(Level::Auto))
        .unwrap();
    let snap = AgentStore::new(dir.path().join("data")).snapshot().unwrap();
    assert_eq!(snap.agents[0].delete, Some(Level::Auto));
    assert_eq!(snap.agents[1].delete, None);
    s.set_override("tole", Class::Delete, None).unwrap();
    assert_eq!(s.snapshot().unwrap().agents[0].delete, None);
    assert!(
        s.set_override("tole", Class::Change, Some(Level::Deny))
            .is_err()
    );
}

#[test]
fn the_last_call_comes_from_the_activity_log_of_the_paired_agent() {
    let (dir, s) = store();
    s.pair("tole").unwrap();
    {
        let mut db = Store::open(dir.path().join("data/crastu.db")).unwrap();
        crastu_host::migrate(&mut db).unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO activity (agent, tool, risk, decision, outcome, paired)
                 VALUES ('tole', 'project_list', 'read', 'auto', 'ok', 0),
                        ('tole', 'clip_reject', 'change', 'approved', 'ok', 1);",
            )
            .unwrap();
    }
    let snap = s.snapshot().unwrap();
    let last = snap.agents[0].last_call.as_ref().unwrap();
    assert_eq!((last.tool.as_str(), last.paired), ("clip_reject", true));
    assert_eq!(snap.recent.len(), 2);
    assert!(
        !snap.recent[1].paired,
        "the unpaired label is marked as such"
    );
}

#[cfg(unix)]
#[test]
fn the_database_and_its_sidecars_are_private_and_the_umask_is_untouched() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let (dir, s) = store();
    s.pair("tole").unwrap();
    s.set_default(Class::Delete, Level::Auto).unwrap();
    s.snapshot().unwrap();
    let data = dir.path().join("data");
    assert_eq!(mode(&data), 0o700);
    let mut files = 0;
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        assert_eq!(mode(&path), 0o600, "{}", path.display());
        files += 1;
    }
    assert!(files >= 1);
}

/// The umask is process-wide, so it is checked in a process of its own: this one runs only the
/// probe below, before and after the store is used, so no other test can have changed it first.
#[cfg(unix)]
#[test]
fn the_process_umask_is_unchanged_by_using_the_store() {
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .args(["--exact", "agents::tests::umask_probe", "--nocapture"])
        .env("CRASTU_UMASK_PROBE", "1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && text.contains("umask probe ran"),
        "{text}"
    );
}

#[cfg(unix)]
#[test]
fn umask_probe() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("CRASTU_UMASK_PROBE").is_none() {
        return;
    }
    let probe = |dir: &Path| {
        let f = dir.join("probe");
        std::fs::File::create(&f).unwrap();
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777
    };
    let a = tempfile::tempdir().unwrap();
    let before = probe(a.path());
    let (_dir, s) = store();
    s.pair("tole").unwrap();
    s.snapshot().unwrap();
    let b = tempfile::tempdir().unwrap();
    assert_eq!(probe(b.path()), before, "the process umask changed");
    println!("umask probe ran");
}

#[test]
fn the_connection_is_kept_and_sees_what_another_process_writes() {
    let (dir, s) = store();
    let opens = OPENS.with(|n| n.get());
    s.snapshot().unwrap();
    // Another process (the CLI) changes the same file; the kept connection sees it.
    let mut other = Store::open(dir.path().join("data/crastu.db")).unwrap();
    crastu_host::migrate(&mut other).unwrap();
    pairing::add(other.conn(), "from-cli").unwrap();
    for _ in 0..3 {
        assert_eq!(s.snapshot().unwrap().agents.len(), 1);
    }
    s.pair("second").unwrap();
    s.set_default(Class::Delete, Level::Deny).unwrap();
    assert_eq!(OPENS.with(|n| n.get()) - opens, 1, "opened once, then kept");
    assert_eq!(other.version("host").unwrap(), 4);
}

mod policy;
