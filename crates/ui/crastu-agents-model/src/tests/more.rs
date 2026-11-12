//! More model tests: one token box at a time, and every way of leaving the page.

use super::*;

#[test]
fn a_new_token_is_refused_while_one_is_on_screen() {
    let fake = Fake::with(&["tole", "other"]);
    let mut m = model_with(&fake);
    m.apply(Intent::Rotate("tole".into()), &fake);
    let writes = fake.writes();
    for intent in [
        Intent::Rotate("other".into()),
        Intent::Rotate("tole".into()),
    ] {
        let fx = m.apply(intent, &fake);
        assert!(
            matches!(fx.as_slice(), [Effect::Toast(t)] if t.contains("Done first")),
            "{fx:?}"
        );
    }
    type_name(&mut m, &fake, "third");
    let fx = m.apply(Intent::Pair, &fake);
    assert!(
        matches!(fx.as_slice(), [Effect::Toast(t)] if t.contains("Done first")),
        "{fx:?}"
    );
    assert_eq!(fake.writes(), writes, "nothing was asked of the platform");
    assert_eq!(
        m.reveal.as_ref().unwrap().agent,
        "tole",
        "the box is untouched"
    );
    m.apply(Intent::DismissToken, &fake);
    m.apply(Intent::Pair, &fake);
    assert_eq!(m.reveal.as_ref().unwrap().agent, "third");
}

#[test]
fn a_refresh_that_shows_the_agent_revoked_drops_the_box() {
    let fake = Fake::with(&["tole"]);
    let mut m = model_with(&fake);
    m.apply(Intent::Rotate("tole".into()), &fake);
    assert!(m.reveal.is_some());
    fake.agents.borrow_mut()[0].revoked = true; // revoked from the CLI
    m.apply(Intent::Refresh, &fake);
    assert!(m.reveal.is_none());
}

#[test]
fn the_token_is_forgotten_whichever_way_the_page_is_left() {
    use crastu_app::{Action, App, ModuleId, Settings, SettingsCategory};
    let fake = Fake::with(&["tole"]);
    let leave: [Action; 3] = [
        Action::OpenSettings(SettingsCategory::Appearance),
        Action::Navigate(ModuleId::Memory),
        Action::OpenSettings(SettingsCategory::About),
    ];
    for away in leave {
        let mut app = App::new(Settings::in_memory());
        let mut m = AgentsModel::default();
        app.dispatch(Action::OpenSettings(SettingsCategory::Agents));
        m.sync(is_agents_page(&app.state().nav), &fake);
        assert_eq!(
            m.snapshot.agents.len(),
            1,
            "the list is read when the page opens"
        );
        m.apply(Intent::Rotate("tole".into()), &fake);
        m.apply(Intent::AskRevoke("tole".into()), &fake);
        assert!(m.reveal.is_some() && m.dialog.open);
        // Still on the page after an unrelated action: nothing is forgotten.
        app.dispatch(Action::ToggleInspector);
        m.sync(is_agents_page(&app.state().nav), &fake);
        assert!(m.reveal.is_some());
        app.dispatch(away.clone());
        m.sync(is_agents_page(&app.state().nav), &fake);
        assert!(m.reveal.is_none(), "{away:?}");
        assert!(m.confirm.is_none() && !m.dialog.open, "{away:?}");
    }
}
