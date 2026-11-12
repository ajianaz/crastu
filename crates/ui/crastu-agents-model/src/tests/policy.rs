//! The confirmations (Auto, reset), the damaged-policy banner, cleaned labels and the token box
//! rules.

use super::*;

fn call(agent: &str) -> CallRow {
    CallRow {
        at: "2026-11-12T10:00:00.000Z".into(),
        agent: agent.into(),
        paired: false,
        tool: "project_list".into(),
        decision: "auto".into(),
        outcome: "ok".into(),
    }
}

#[test]
fn an_unpaired_label_is_cut_and_stripped_of_control_characters() {
    let fake = Fake::default();
    let hostile = format!("a\u{1b}[31mred\u{202E}\n{}", "x".repeat(300));
    fake.recent.borrow_mut().push(call(&hostile));
    fake.agents.borrow_mut().push(AgentRow {
        last_call: Some(call("tole\u{7}")),
        ..row("tole")
    });
    let m = model_with(&fake);
    let shown = &m.snapshot.recent[0].agent;
    assert_eq!(shown.chars().count(), LABEL_MAX, "{shown:?}");
    assert!(shown.ends_with('\u{2026}'));
    assert!(shown.starts_with("a[31mred"), "{shown:?}");
    assert!(!shown.chars().any(|c| c.is_control() || c == '\u{202E}'));
    assert_eq!(
        m.snapshot.agents[0].last_call.as_ref().unwrap().agent,
        "tole"
    );
    assert_eq!(clean_label("short"), "short");
}

#[test]
fn auto_for_changes_or_deletions_is_asked_first_and_written_only_on_confirm() {
    for class in Class::ALL {
        let fake = Fake::default();
        let mut m = model_with(&fake);
        let fx = m.apply(Intent::Default(class, right()), &fake);
        assert_eq!(fx, [Effect::FocusDialog]);
        assert_eq!(m.confirm, Some(Pending::Auto { agent: None, class }));
        assert!(m.dialog.open);
        assert_eq!(fake.writes(), 0, "nothing written before the answer");
        assert!(
            m.confirm
                .as_ref()
                .unwrap()
                .subtitle()
                .contains("without asking")
        );
        // Cancel: still nothing.
        m.apply(Intent::Dialog(DialogInput::Close), &fake);
        assert!(m.confirm.is_none() && fake.writes() == 0);
        m.apply(Intent::Default(class, right()), &fake);
        m.apply(Intent::Confirm, &fake);
        assert_eq!(m.snapshot.defaults.get(class), Level::Auto);
    }
}

#[test]
fn deny_and_ask_need_no_confirmation() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    m.apply(
        Intent::Default(Class::Delete, ChoiceInput::Select(2)),
        &fake,
    );
    assert!(m.confirm.is_none());
    assert_eq!(fake.defaults.borrow().delete, Level::Deny);
}

#[test]
fn a_per_agent_auto_is_asked_for_deletions_only() {
    let fake = Fake::with(&["tole"]);
    let mut m = model_with(&fake);
    m.apply(
        Intent::Override("tole".into(), Class::Change, ChoiceInput::Select(2)),
        &fake,
    );
    assert!(m.confirm.is_none());
    assert_eq!(fake.agents.borrow()[0].change, Some(Level::Auto));
    let fx = m.apply(
        Intent::Override("tole".into(), Class::Delete, ChoiceInput::Select(2)),
        &fake,
    );
    assert_eq!(fx, [Effect::FocusDialog]);
    assert_eq!(fake.agents.borrow()[0].delete, None);
    m.apply(Intent::Confirm, &fake);
    assert_eq!(fake.agents.borrow()[0].delete, Some(Level::Auto));
}

#[test]
fn a_damaged_policy_shows_the_flag_keeps_the_agents_and_resets_after_confirm() {
    let fake = Fake::with(&["tole"]);
    *fake.damaged.borrow_mut() = true;
    let mut m = model_with(&fake);
    assert!(m.snapshot.policy_damaged);
    assert_eq!(m.snapshot.agents.len(), 1, "the list still works");
    let fx = m.apply(Intent::AskResetPolicy, &fake);
    assert_eq!(fx, [Effect::FocusDialog]);
    assert_eq!(m.confirm, Some(Pending::ResetPolicy));
    assert!(!fake.calls().iter().any(|c| c == "reset"), "asked first");
    m.apply(Intent::Confirm, &fake);
    assert!(fake.calls().iter().any(|c| c == "reset"));
    assert!(!m.snapshot.policy_damaged);
    // With nothing damaged the button does nothing.
    let fx = m.apply(Intent::AskResetPolicy, &fake);
    assert!(m.confirm.is_none() && !m.dialog.open, "{fx:?}");
}

#[test]
fn pair_and_rotate_are_disabled_while_the_token_box_is_open_and_focus_goes_to_it() {
    let fake = Fake::with(&["tole"]);
    let mut m = model_with(&fake);
    assert!(m.can_rotate());
    type_name(&mut m, &fake, "second");
    assert!(m.can_pair());
    let fx = m.apply(Intent::Rotate("tole".into()), &fake);
    assert!(fx.contains(&Effect::FocusToken), "{fx:?}");
    assert!(!m.can_pair() && !m.can_rotate());
    m.apply(Intent::DismissToken, &fake);
    assert!(m.can_pair() && m.can_rotate());
    let fx = m.apply(Intent::Pair, &fake);
    assert!(fx.contains(&Effect::FocusToken), "{fx:?}");
    assert!(!m.can_pair());
}

#[test]
fn policy_intents_are_refused_while_the_policy_is_damaged() {
    let fake = Fake::with(&["tole"]);
    *fake.damaged.borrow_mut() = true;
    let mut m = model_with(&fake);
    let writes = fake.writes();
    for intent in [
        Intent::Default(Class::Delete, ChoiceInput::Select(2)),
        Intent::Default(Class::Change, right()),
        Intent::Override("tole".into(), Class::Change, ChoiceInput::Select(2)),
    ] {
        let fx = m.apply(intent, &fake);
        assert!(
            matches!(fx.as_slice(), [Effect::Toast(t)] if t.contains("damaged")),
            "{fx:?}"
        );
    }
    assert_eq!(fake.writes(), writes, "nothing was written");
    assert!(m.confirm.is_none() && !m.dialog.open);
    // Rotate and Revoke still work.
    assert!(
        m.apply(Intent::Rotate("tole".into()), &fake)
            .contains(&Effect::FocusToken)
    );
    m.apply(Intent::DismissToken, &fake);
    m.apply(Intent::AskRevoke("tole".into()), &fake);
    m.apply(Intent::Confirm, &fake);
    assert!(fake.agents.borrow()[0].revoked);
}

#[test]
fn a_reset_for_a_policy_that_was_fixed_meanwhile_changes_nothing_and_says_so() {
    let fake = Fake::with(&["tole"]);
    *fake.damaged.borrow_mut() = true;
    let mut m = model_with(&fake);
    m.apply(Intent::AskResetPolicy, &fake);
    // Fixed from the CLI while the dialog was open.
    *fake.damaged.borrow_mut() = false;
    fake.agents.borrow_mut()[0].delete = Some(Level::Deny);
    let fx = m.apply(Intent::Confirm, &fake);
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::Toast(t) if t.contains("no longer damaged"))),
        "{fx:?}"
    );
    assert_eq!(fake.agents.borrow()[0].delete, Some(Level::Deny), "kept");
    assert!(!m.snapshot.policy_damaged);
}
