//! The Settings sidebar after Offline (Privacy) and the agents page landed together.

use std::cell::RefCell;

use crastu_app::{Action, App, Settings, SettingsCategory};

use crate::agents::{
    AgentRow, AgentsModel, AgentsPlatform, AgentsSnapshot, Class, Intent, Level, Secret,
    is_agents_page,
};
use crate::palette;

/// Pairs agents in memory.
#[derive(Default)]
struct Pairs(RefCell<Vec<String>>);

impl AgentsPlatform for Pairs {
    fn list_agents(&self) -> Result<AgentsSnapshot, String> {
        let names = self.0.borrow();
        let agents = names.iter().map(|n| AgentRow {
            name: n.clone(),
            created_at: String::new(),
            revoked: false,
            change: None,
            delete: None,
            last_call: None,
        });
        Ok(AgentsSnapshot {
            agents: agents.collect(),
            ..AgentsSnapshot::default()
        })
    }
    fn pair_agent(&self, name: &str) -> Result<Secret, String> {
        self.0.borrow_mut().push(name.into());
        Ok(Secret::new("crastu_secret"))
    }
    fn rotate_agent(&self, _: &str) -> Result<Secret, String> {
        Ok(Secret::new("crastu_secret"))
    }
    fn revoke_agent(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn reset_policy(&self) -> Result<bool, String> {
        Ok(true)
    }
    fn set_default_policy(&self, _: Class, _: Level) -> Result<(), String> {
        Ok(())
    }
    fn set_agent_override(&self, _: &str, _: Class, _: Option<Level>) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn settings_lists_privacy_and_agents_and_moving_between_them_clears_the_token() {
    let cats = SettingsCategory::available();
    assert!(cats.contains(&SettingsCategory::Privacy) && cats.contains(&SettingsCategory::Agents));
    let titles: Vec<String> = palette::entries()
        .into_iter()
        .filter(|e| e.group == "Settings")
        .map(|e| e.label)
        .collect();
    assert!(titles.iter().any(|t| t == "Privacy") && titles.iter().any(|t| t == "Agents (MCP)"));

    let platform = Pairs::default();
    let mut app = App::new(Settings::in_memory());
    let mut m = AgentsModel::default();
    let sync = |app: &App, m: &mut AgentsModel| m.sync(is_agents_page(&app.state().nav), &platform);
    app.dispatch(Action::OpenSettings(SettingsCategory::Agents));
    sync(&app, &mut m);
    for ch in "tole".chars() {
        m.apply(
            Intent::Name(crastu_controls::Edit::Insert(ch.to_string())),
            &platform,
        );
    }
    m.apply(Intent::Pair, &platform);
    assert!(m.reveal.is_some(), "the token box is open");
    app.dispatch(Action::OpenSettings(SettingsCategory::Privacy));
    sync(&app, &mut m);
    assert!(m.reveal.is_none(), "the token box is closed on Privacy");
    app.dispatch(Action::OpenSettings(SettingsCategory::Agents));
    sync(&app, &mut m);
    assert!(m.reveal.is_none(), "coming back does not bring it back");
}
