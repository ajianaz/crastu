use std::cell::RefCell;

use super::*;

const TOKEN: &str = "crastu_0123456789abcdef";

/// A platform that keeps the pairings in memory and counts what is asked of it.
#[derive(Default)]
struct Fake {
    agents: RefCell<Vec<AgentRow>>,
    defaults: RefCell<Defaults>,
    calls: RefCell<Vec<String>>,
    fail_writes: RefCell<bool>,
    damaged: RefCell<bool>,
    recent: RefCell<Vec<CallRow>>,
}

impl Fake {
    fn with(names: &[&str]) -> Fake {
        let f = Fake::default();
        for n in names {
            f.agents.borrow_mut().push(row(n));
        }
        f
    }

    fn log(&self, what: String) {
        self.calls.borrow_mut().push(what);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn writes(&self) -> usize {
        self.calls().iter().filter(|c| *c != "list").count()
    }
}

fn row(name: &str) -> AgentRow {
    AgentRow {
        name: name.into(),
        created_at: "2026-11-12T10:00:00.000Z".into(),
        revoked: false,
        change: None,
        delete: None,
        last_call: None,
    }
}

impl AgentsPlatform for Fake {
    fn list_agents(&self) -> Result<AgentsSnapshot, String> {
        self.log("list".into());
        Ok(AgentsSnapshot {
            agents: self.agents.borrow().clone(),
            defaults: *self.defaults.borrow(),
            recent: self.recent.borrow().clone(),
            policy_damaged: *self.damaged.borrow(),
        })
    }
    fn pair_agent(&self, name: &str) -> Result<Secret, String> {
        self.log(format!("pair {name}"));
        if self.agents.borrow().iter().any(|a| a.name == name) {
            return Err(format!("an agent named {name:?} already exists"));
        }
        self.agents.borrow_mut().push(row(name));
        Ok(Secret::new(TOKEN))
    }
    fn rotate_agent(&self, name: &str) -> Result<Secret, String> {
        self.log(format!("rotate {name}"));
        Ok(Secret::new(TOKEN))
    }
    fn revoke_agent(&self, name: &str) -> Result<(), String> {
        self.log(format!("revoke {name}"));
        for a in self
            .agents
            .borrow_mut()
            .iter_mut()
            .filter(|a| a.name == name)
        {
            a.revoked = true;
        }
        Ok(())
    }
    fn reset_policy(&self) -> Result<(), String> {
        self.log("reset".into());
        *self.damaged.borrow_mut() = false;
        *self.defaults.borrow_mut() = Defaults::default();
        for a in self.agents.borrow_mut().iter_mut() {
            (a.change, a.delete) = (None, None);
        }
        Ok(())
    }
    fn set_default_policy(&self, class: Class, level: Level) -> Result<(), String> {
        self.log(format!("default {class:?} {level:?}"));
        if *self.fail_writes.borrow() {
            return Err("disk is full".into());
        }
        match class {
            Class::Change => self.defaults.borrow_mut().change = level,
            Class::Delete => self.defaults.borrow_mut().delete = level,
        }
        Ok(())
    }
    fn set_agent_override(
        &self,
        agent: &str,
        class: Class,
        level: Option<Level>,
    ) -> Result<(), String> {
        self.log(format!("override {agent} {class:?} {level:?}"));
        for a in self
            .agents
            .borrow_mut()
            .iter_mut()
            .filter(|a| a.name == agent)
        {
            match class {
                Class::Change => a.change = level,
                Class::Delete => a.delete = level,
            }
        }
        Ok(())
    }
}

fn model_with(fake: &Fake) -> AgentsModel {
    let mut m = AgentsModel::default();
    assert!(m.refresh(fake).is_empty());
    m
}

fn type_name(m: &mut AgentsModel, fake: &Fake, name: &str) {
    m.apply(Intent::Name(Edit::Insert(name.into())), fake);
}

fn right() -> ChoiceInput {
    ChoiceInput::Key("right".into())
}

#[test]
fn pairing_shows_the_token_once_and_lists_the_agent() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    type_name(&mut m, &fake, "claude-code");
    assert!(m.can_pair());
    assert_eq!(m.apply(Intent::Pair, &fake), [Effect::FocusToken]);
    let reveal = m.reveal.as_ref().expect("the token is revealed");
    assert_eq!(
        (reveal.agent.as_str(), reveal.rotated),
        ("claude-code", false)
    );
    assert_eq!(reveal.token.expose(), TOKEN);
    assert_eq!(m.snapshot.agents.len(), 1);
    assert!(m.name.is_empty(), "the field is ready for the next name");
}

#[test]
fn an_invalid_name_never_reaches_the_platform() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    for bad in ["Claude", "has space", "ünï", "a/b"] {
        m.name = TextModel::new(bad);
        assert!(!m.can_pair(), "{bad}");
        assert!(m.name_error().is_some(), "{bad}");
        let fx = m.apply(Intent::Pair, &fake);
        assert!(matches!(fx.as_slice(), [Effect::Toast(_)]), "{bad}: {fx:?}");
    }
    m.name = TextModel::default();
    assert!(!m.can_pair());
    assert!(m.name_error().is_none(), "an empty field shows no error");
    assert!(matches!(
        m.apply(Intent::Pair, &fake).as_slice(),
        [Effect::Toast(_)]
    ));
    assert_eq!(fake.writes(), 0);
    assert!(m.reveal.is_none());
}

#[test]
fn the_name_rules_are_the_hosts() {
    let long = "a".repeat(NAME_MAX);
    for ok in ["a", "claude-code", "a.b_c-9", long.as_str()] {
        assert!(name_problem(ok).is_none(), "{ok}");
    }
    let too_long = "a".repeat(NAME_MAX + 1);
    for bad in ["", "A", "a b", "é", too_long.as_str()] {
        assert!(name_problem(bad).is_some(), "{bad}");
    }
    let m = AgentsModel::default();
    let long_input = "a".repeat(NAME_MAX + 10);
    let mut m2 = AgentsModel::default();
    m2.apply(Intent::Name(Edit::Insert(long_input)), &NoFake);
    assert_eq!(m2.name.len(), NAME_MAX, "the field stops at the limit");
    assert!(m.name.is_empty());
}

struct NoFake;
impl AgentsPlatform for NoFake {
    fn list_agents(&self) -> Result<AgentsSnapshot, String> {
        Ok(AgentsSnapshot::default())
    }
    fn pair_agent(&self, _: &str) -> Result<Secret, String> {
        Err("no".into())
    }
    fn rotate_agent(&self, _: &str) -> Result<Secret, String> {
        Err("no".into())
    }
    fn revoke_agent(&self, _: &str) -> Result<(), String> {
        Err("no".into())
    }
    fn reset_policy(&self) -> Result<(), String> {
        Err("no".into())
    }
    fn set_default_policy(&self, _: Class, _: Level) -> Result<(), String> {
        Err("no".into())
    }
    fn set_agent_override(&self, _: &str, _: Class, _: Option<Level>) -> Result<(), String> {
        Err("no".into())
    }
}

#[test]
fn a_platform_error_becomes_a_toast_and_no_token() {
    let fake = Fake::with(&["tole"]);
    let mut m = model_with(&fake);
    type_name(&mut m, &fake, "tole");
    let fx = m.apply(Intent::Pair, &fake);
    assert!(
        matches!(fx.as_slice(), [Effect::Toast(t)] if t.contains("already exists")),
        "{fx:?}"
    );
    assert!(m.reveal.is_none());
    assert_eq!(m.name.text(), "tole", "the name stays for a correction");
}

#[test]
fn dismissing_the_reveal_clears_the_token() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    type_name(&mut m, &fake, "tole");
    m.apply(Intent::Pair, &fake);
    assert!(m.reveal.is_some());
    m.apply(Intent::DismissToken, &fake);
    assert!(m.reveal.is_none());
    assert!(
        m.apply(Intent::CopyToken, &fake).is_empty(),
        "nothing left to copy"
    );
}

#[test]
fn leaving_the_page_clears_the_token() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    type_name(&mut m, &fake, "tole");
    m.apply(Intent::Pair, &fake);
    m.dismiss_token();
    assert!(m.reveal.is_none());
}

#[test]
fn the_token_is_copied_only_by_the_copy_button_and_never_printed() {
    let fake = Fake::with(&["tole", "other"]);
    let mut m = model_with(&fake);
    m.apply(Intent::Rotate("tole".into()), &fake);
    let mut fx = Vec::new();
    for i in [
        Intent::Name(Edit::Insert("x".into())),
        Intent::Default(Class::Change, right()),
        Intent::Override("other".into(), Class::Delete, right()),
        Intent::Refresh,
    ] {
        fx.extend(m.apply(i, &fake));
    }
    assert!(!fx.iter().any(|e| matches!(e, Effect::Copy(_))), "{fx:?}");
    let shown = format!("{m:?} {fx:?} {:?}", m.reveal);
    assert!(
        !shown.contains(TOKEN) && !shown.contains("crastu_"),
        "{shown}"
    );
    assert_eq!(
        m.apply(Intent::CopyToken, &fake),
        vec![Effect::Copy(Secret::new(TOKEN))]
    );
}

#[test]
fn rotating_an_agent_reveals_a_new_token_and_marks_it_rotated() {
    let fake = Fake::with(&["tole"]);
    let mut m = model_with(&fake);
    m.apply(Intent::Rotate("tole".into()), &fake);
    let r = m.reveal.as_ref().unwrap();
    assert!(r.rotated && r.agent == "tole");
    assert!(fake.calls().contains(&"rotate tole".to_string()));
}

#[test]
fn a_revoked_or_missing_agent_cannot_be_rotated() {
    let fake = Fake::with(&["tole"]);
    let mut m = model_with(&fake);
    fake.agents.borrow_mut()[0].revoked = true;
    m.refresh(&fake);
    let fx = m.apply(Intent::Rotate("tole".into()), &fake);
    assert!(matches!(fx.first(), Some(Effect::Toast(_))), "{fx:?}");
    m.apply(Intent::Rotate("ghost".into()), &fake);
    assert!(m.reveal.is_none());
    assert!(!fake.calls().iter().any(|c| c.starts_with("rotate")));
}

#[test]
fn revoking_needs_the_confirmation() {
    let fake = Fake::with(&["tole", "other"]);
    let mut m = model_with(&fake);
    // Asking opens the dialog and revokes nothing.
    assert_eq!(
        m.apply(Intent::AskRevoke("tole".into()), &fake),
        vec![Effect::FocusDialog]
    );
    assert!(m.dialog.open && m.confirm == Some(Pending::Revoke("tole".into())));
    assert_eq!(fake.writes(), 0);
    // Escape and the scrim cancel.
    let fx = m.apply(Intent::Dialog(DialogInput::Key("escape".into())), &fake);
    assert_eq!(fx, vec![Effect::ReturnFocus]);
    assert!(!m.dialog.open && m.confirm.is_none());
    m.apply(Intent::AskRevoke("tole".into()), &fake);
    m.apply(Intent::Dialog(DialogInput::Close), &fake);
    assert_eq!(fake.writes(), 0);
    // A confirm with nothing asked does nothing.
    assert!(m.apply(Intent::Confirm, &fake).is_empty());
    assert_eq!(fake.writes(), 0);
    // Ask and confirm revokes exactly that agent.
    m.apply(Intent::AskRevoke("tole".into()), &fake);
    let fx = m.apply(Intent::Confirm, &fake);
    assert_eq!(fx, vec![Effect::ReturnFocus]);
    let rows = &m.snapshot.agents;
    assert!(rows[0].revoked && !rows[1].revoked);
    assert_eq!(
        fake.calls()
            .iter()
            .filter(|c| c.starts_with("revoke"))
            .count(),
        1
    );
    assert!(!m.dialog.open && m.confirm.is_none());
}

#[test]
fn revoking_the_agent_whose_token_is_shown_closes_the_box() {
    let fake = Fake::with(&["tole", "other"]);
    let mut m = model_with(&fake);
    m.apply(Intent::Rotate("tole".into()), &fake);
    m.apply(Intent::AskRevoke("other".into()), &fake);
    m.apply(Intent::Confirm, &fake);
    assert!(m.reveal.is_some(), "another agent's revoke leaves the box");
    m.apply(Intent::AskRevoke("tole".into()), &fake);
    m.apply(Intent::Confirm, &fake);
    assert!(m.reveal.is_none());
}

#[test]
fn a_default_choice_is_resolved_against_the_state_when_it_arrives() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    // Delete: Ask, Auto, Deny. Auto is asked about first; Deny is not.
    m.apply(Intent::Default(Class::Delete, right()), &fake);
    assert_eq!(
        fake.defaults.borrow().delete,
        Level::Ask,
        "not before the dialog"
    );
    m.apply(Intent::Confirm, &fake);
    m.apply(Intent::Default(Class::Delete, right()), &fake);
    assert_eq!(m.snapshot.defaults.delete, Level::Deny);
    assert_eq!(fake.defaults.borrow().delete, Level::Deny);
    // At the end the row does not wrap and nothing is written.
    let before = fake.writes();
    m.apply(Intent::Default(Class::Delete, right()), &fake);
    assert_eq!(fake.writes(), before);
    // A click names an option by index.
    m.apply(
        Intent::Default(Class::Delete, ChoiceInput::Select(0)),
        &fake,
    );
    assert_eq!(fake.defaults.borrow().delete, Level::Ask);
    // Change offers Ask and Auto only.
    assert_eq!(default_labels(Class::Change), ["Ask", "Auto"]);
    m.apply(
        Intent::Default(Class::Change, ChoiceInput::Select(2)),
        &fake,
    );
    assert_eq!(fake.defaults.borrow().change, Level::Ask);
}

#[test]
fn a_failed_policy_write_says_so_and_shows_what_is_stored() {
    let fake = Fake::default();
    let mut m = model_with(&fake);
    *fake.fail_writes.borrow_mut() = true;
    m.apply(Intent::Default(Class::Change, right()), &fake);
    let fx = m.apply(Intent::Confirm, &fake);
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::Toast(t) if t.contains("disk is full"))),
        "{fx:?}"
    );
    assert_eq!(
        m.snapshot.defaults.change,
        Level::Ask,
        "the control did not move"
    );
}

#[test]
fn an_override_is_set_and_cleared_for_the_chosen_agent_only() {
    let fake = Fake::with(&["tole", "other"]);
    let mut m = model_with(&fake);
    // Options: Default, Ask, Auto, Deny.
    assert_eq!(
        override_labels(Class::Delete),
        ["Default", "Ask", "Auto", "Deny"]
    );
    assert_eq!(override_labels(Class::Change), ["Default", "Ask", "Auto"]);
    m.apply(
        Intent::Override("tole".into(), Class::Delete, ChoiceInput::Select(3)),
        &fake,
    );
    assert_eq!(m.snapshot.agents[0].delete, Some(Level::Deny));
    assert_eq!(m.snapshot.agents[1].delete, None);
    assert_eq!(override_selected(&m.snapshot.agents[0], Class::Delete), 3);
    m.apply(
        Intent::Override("tole".into(), Class::Delete, ChoiceInput::Select(0)),
        &fake,
    );
    assert_eq!(m.snapshot.agents[0].delete, None);
    assert_eq!(override_selected(&m.snapshot.agents[0], Class::Delete), 0);
    // A revoked agent takes no override.
    fake.agents.borrow_mut()[1].revoked = true;
    m.refresh(&fake);
    let before = fake.writes();
    m.apply(
        Intent::Override("other".into(), Class::Delete, ChoiceInput::Select(2)),
        &fake,
    );
    assert_eq!(fake.writes(), before);
}

#[test]
fn times_are_shown_short() {
    assert_eq!(short_time("2026-11-12T10:03:22.123Z"), "2026-11-12 10:03");
    assert_eq!(short_time("later"), "later");
    assert_eq!(short_time(""), "");
}

mod more;
mod policy;
