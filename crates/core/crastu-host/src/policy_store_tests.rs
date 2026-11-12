use super::*;
use crate::tests::{Answer, Clips, Fetcher, host, paired, unpaired};
use std::sync::atomic::AtomicBool;

#[test]
fn a_default_set_in_the_database_applies_to_the_next_call() {
    let h = host(false);
    // Ask by default: the approver says no.
    assert!(h.call(paired("tole"), "clip_reject", json!({})).is_err());
    policy_store::set_default(h.shared().store().conn(), Risk::Change, Decision::Auto).unwrap();
    assert_eq!(
        h.call(paired("tole"), "clip_reject", json!({})).unwrap(),
        json!("rejected")
    );
    policy_store::set_default(h.shared().store().conn(), Risk::Delete, Decision::Deny).unwrap();
    let err = h
        .call(unpaired("x"), "asset_delete", json!({}))
        .unwrap_err();
    assert!(matches!(err, CallError::Denied { .. }), "{err}");
}

#[test]
fn an_override_is_stored_per_agent_and_can_be_cleared() {
    let h = host(false);
    let conn = || h.shared().store();
    policy_store::set_agent(conn().conn(), "tole", Risk::Change, Some(Decision::Auto)).unwrap();
    assert!(h.call(paired("tole"), "clip_reject", json!({})).is_ok());
    assert!(h.call(paired("other"), "clip_reject", json!({})).is_err());
    policy_store::set_agent(conn().conn(), "tole", Risk::Change, None).unwrap();
    assert!(h.call(paired("tole"), "clip_reject", json!({})).is_err());
    let stored = policy_store::load(conn().conn()).unwrap();
    assert_eq!(stored.agent_override("tole", Risk::Change), None);
}

#[test]
fn the_policy_survives_a_reopen_of_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crastu.db");
    {
        let mut store = Store::open(&path).unwrap();
        migrate(&mut store).unwrap();
        let c = store.conn();
        policy_store::set_default(c, Risk::Delete, Decision::Auto).unwrap();
        policy_store::set_agent(c, "tole", Risk::Delete, Some(Decision::Deny)).unwrap();
    }
    let mut store = Store::open(&path).unwrap();
    migrate(&mut store).unwrap();
    let p = policy_store::load(store.conn()).unwrap();
    assert_eq!(p.delete, Decision::Auto);
    assert_eq!(p.change, Decision::Ask);
    assert_eq!(p.agent_override("tole", Risk::Delete), Some(Decision::Deny));
}

#[test]
fn values_the_policy_does_not_allow_are_refused_and_not_stored() {
    let h = host(false);
    let store = h.shared().store();
    let c = store.conn();
    for (risk, d) in [
        (Risk::Read, Decision::Deny),
        (Risk::Read, Decision::Ask),
        (Risk::Change, Decision::Deny),
    ] {
        assert!(matches!(
            policy_store::set_default(c, risk, d),
            Err(policy_store::PolicyError::NotAllowed)
        ));
        assert!(matches!(
            policy_store::set_agent(c, "tole", risk, Some(d)),
            Err(policy_store::PolicyError::NotAllowed)
        ));
    }
    assert!(matches!(
        policy_store::set_agent(c, "Bad Name", Risk::Delete, Some(Decision::Deny)),
        Err(policy_store::PolicyError::Name(_))
    ));
    assert_eq!(policy_store::load(c).unwrap(), Policy::default());
}

#[test]
fn a_damaged_row_fails_the_load_and_the_host_goes_reads_only() {
    for (agent, risk, decision) in [
        ("", "delete", "maybe"),
        ("tole", "delete", "maybe"),
        ("tole", "gamble", "deny"),
        ("tole", "read", "deny"),
        ("tole", "change", "deny"),
        ("", "change", "deny"),
    ] {
        // The stored default is Auto; the damaged row is where a Deny or an Ask was meant to be.
        // Skipping it would leave Auto, so the whole policy must be refused instead.
        let h = host(true);
        {
            let store = h.shared().store();
            let c = store.conn();
            policy_store::set_default(c, Risk::Delete, Decision::Auto).unwrap();
            policy_store::set_default(c, Risk::Change, Decision::Auto).unwrap();
            c.execute(
                "INSERT OR REPLACE INTO agent_policy (agent, risk, decision) VALUES (?1, ?2, ?3)",
                params![agent, risk, decision],
            )
            .unwrap();
            let err = policy_store::load(c).unwrap_err();
            assert!(
                matches!(err, policy_store::PolicyError::Damaged(_)),
                "{agent:?} {risk} {decision}: {err}"
            );
        }
        assert!(h.call(paired("tole"), "project_list", json!({})).is_ok());
        for tool in ["asset_delete", "clip_reject"] {
            let err = h.call(paired("tole"), tool, json!({})).unwrap_err();
            assert!(matches!(err, CallError::Denied { .. }), "{tool}: {err}");
        }
    }
}

#[test]
fn a_failed_replace_leaves_the_old_policy() {
    let h = host(false);
    let mut old = Policy::default();
    old.delete = Decision::Auto;
    old.set_agent("tole", Risk::Delete, Decision::Deny);
    h.set_policy(old.clone()).unwrap();
    h.shared()
        .store()
        .conn()
        .execute_batch(
            "CREATE TRIGGER no_other BEFORE INSERT ON agent_policy WHEN NEW.agent = 'other'
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    let mut new = Policy::default();
    new.change = Decision::Auto;
    new.set_agent("other", Risk::Delete, Decision::Deny);
    assert!(h.set_policy(new).is_err());
    assert_eq!(policy_store::load(h.shared().store().conn()).unwrap(), old);
}

#[test]
fn a_reader_never_sees_the_table_half_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crastu.db");
    let mut writer = Store::open(&path).unwrap();
    migrate(&mut writer).unwrap();
    let mut policy = Policy::default();
    policy.delete = Decision::Auto; // an empty table would read as Ask
    policy.set_agent("tole", Risk::Change, Decision::Auto);
    policy_store::replace(writer.conn(), &policy).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    let reader_path = path.clone();
    let reader = std::thread::spawn(move || {
        let r = Store::open(&reader_path).unwrap();
        r.conn()
            .busy_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let mut seen = 0;
        while !flag.load(std::sync::atomic::Ordering::Relaxed) {
            let p = policy_store::load(r.conn()).unwrap();
            assert_eq!(p.delete, Decision::Auto, "a half-written table was read");
            assert_eq!(p.agent_override("tole", Risk::Change), Some(Decision::Auto));
            seen += 1;
        }
        seen
    });
    writer
        .conn()
        .busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    for _ in 0..300 {
        policy_store::replace(writer.conn(), &policy).unwrap();
    }
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(reader.join().unwrap() > 0);
}

#[test]
fn an_unreadable_policy_lets_reads_through_and_nothing_else() {
    let h = host(true);
    h.shared()
        .store()
        .conn()
        .execute("DROP TABLE agent_policy", [])
        .unwrap();
    assert!(h.call(paired("tole"), "project_list", json!({})).is_ok());
    let err = h
        .call(paired("tole"), "clip_reject", json!({}))
        .unwrap_err();
    assert!(matches!(err, CallError::Denied { .. }), "{err}");
}

#[test]
fn the_activity_of_a_paired_agent_is_not_confused_with_a_label() {
    let h = host(true);
    let token = pairing::add(h.shared().store().conn(), "tole").unwrap();
    h.call(unpaired("tole"), "project_list", json!({})).unwrap();
    let conn = |h: &Host| activity::last_paired_call(h.shared().store().conn(), "tole").unwrap();
    assert_eq!(conn(&h), None);
    let name = pairing::resolve(h.shared().store().conn(), &token).unwrap();
    h.call(paired(&name), "clip_reject", json!({})).unwrap();
    let last = conn(&h).unwrap();
    assert_eq!(last.activity.tool, "clip_reject");
    assert!(last.activity.paired && !last.at.is_empty());
    let all = activity::recent(h.shared().store().conn(), 10).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].activity.tool, "clip_reject");
}

#[test]
fn migrating_a_current_database_takes_no_write_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crastu.db");
    let mut first = Store::open(&path).unwrap();
    migrate(&mut first).unwrap();
    // Another connection holds the write lock (the host writing an activity row).
    first.conn().execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut second = Store::open(&path).unwrap();
    second
        .conn()
        .busy_timeout(std::time::Duration::from_millis(0))
        .unwrap();
    migrate(&mut second).expect("nothing pending, so no lock is needed");
    first.conn().execute_batch("ROLLBACK").unwrap();
}

#[test]
fn a_gated_refusal_and_a_stored_policy_decision_work_in_one_host() {
    let gate = NetGate::new(true);
    let h = Host::new(
        Store::open_in_memory().unwrap(),
        JobQueue::new(1, 1),
        vec![Box::new(Clips), Box::new(Fetcher(gate.clone()))],
        Box::new(Answer(AtomicBool::new(false))),
    )
    .unwrap();
    h.set_net_gate(gate.clone());
    // The stored policy decides: Change is Ask and the approver says no.
    assert!(matches!(
        h.call(paired("tole"), "clip_reject", json!({})),
        Err(CallError::NotApproved { .. })
    ));
    policy_store::set_agent(
        h.shared().store().conn(),
        "tole",
        Risk::Change,
        Some(Decision::Auto),
    )
    .unwrap();
    assert!(h.call(paired("tole"), "clip_reject", json!({})).is_ok());
    // The gate refuses the request of a read tool, and the refusal is drained into the log.
    let err = h.call(paired("tole"), "fetch_page", json!({})).unwrap_err();
    assert!(matches!(err, CallError::Tool(_)), "{err}");
    assert!(
        h.activity(10)
            .iter()
            .any(|a| a.decision.starts_with("refused: page fetch")),
        "the refusal is logged"
    );
    assert!(gate.refusals().is_empty(), "drained");
    // A damaged policy still fails closed, and the gate keeps refusing.
    h.shared()
        .store()
        .conn()
        .execute("UPDATE agent_policy SET decision = 'bogus'", [])
        .unwrap();
    assert!(h.call(paired("tole"), "clip_reject", json!({})).is_err());
    assert!(h.call(paired("tole"), "fetch_page", json!({})).is_err());
}

#[test]
fn a_failed_commit_is_rolled_back_and_leaves_the_connection_usable() {
    let h = host(false);
    let store = h.shared().store();
    let c = store.conn();
    c.execute_batch(
        "CREATE TABLE parent (id INTEGER PRIMARY KEY);
         CREATE TABLE child (p INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER agent_policy_orphan AFTER INSERT ON agent_policy
         BEGIN INSERT INTO child VALUES (999); END;
         PRAGMA foreign_keys = ON;",
    )
    .unwrap();
    // The inserts pass, the deferred foreign key fails at COMMIT.
    assert!(policy_store::replace(c, &Policy::default()).is_err());
    assert!(c.is_autocommit(), "no transaction is left open");
    c.execute_batch("DROP TRIGGER agent_policy_orphan").unwrap();
    policy_store::replace(c, &Policy::default()).unwrap();
}

#[test]
fn reset_replaces_a_damaged_policy_with_the_defaults() {
    let h = host(false);
    let store = h.shared().store();
    let c = store.conn();
    policy_store::set_agent(c, "tole", Risk::Change, Some(Decision::Auto)).unwrap();
    c.execute("UPDATE agent_policy SET decision = 'bogus'", [])
        .unwrap();
    assert!(policy_store::load(c).is_err());
    assert!(policy_store::reset_if_damaged(c).unwrap());
    let p = policy_store::load(c).unwrap();
    assert_eq!((p.change, p.delete), (Decision::Ask, Decision::Ask));
    assert_eq!(p.agent_override("tole", Risk::Change), None);
}

#[test]
fn reset_if_damaged_resets_a_damaged_policy_and_leaves_a_healthy_one_alone() {
    let h = host(false);
    let store = h.shared().store();
    let c = store.conn();
    policy_store::set_agent(c, "tole", Risk::Delete, Some(Decision::Deny)).unwrap();
    assert!(
        !policy_store::reset_if_damaged(c).unwrap(),
        "healthy: nothing happens"
    );
    let p = policy_store::load(c).unwrap();
    assert_eq!(p.agent_override("tole", Risk::Delete), Some(Decision::Deny));
    c.execute("UPDATE agent_policy SET decision = 'bogus'", [])
        .unwrap();
    assert!(policy_store::reset_if_damaged(c).unwrap());
    let p = policy_store::load(c).unwrap();
    assert_eq!((p.change, p.delete), (Decision::Ask, Decision::Ask));
    assert_eq!(p.agent_override("tole", Risk::Delete), None);
    assert!(c.is_autocommit());
}
