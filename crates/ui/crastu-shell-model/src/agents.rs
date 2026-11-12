//! Settings, Agents as plain data: the paired agents, the approval policy, the one-time token
//! reveal and the rules for each button. The view draws it and sends [`Intent`]s; the owner
//! applies them against the state as it is then and carries out the [`Effect`]s. The platform
//! (the desktop app, over the host's pairing and policy stores) does the work behind
//! [`Platform`].
//!
//! A token is a secret. [`Secret`] never prints, the model keeps it only while the reveal is
//! open, and the clipboard is touched only by [`Intent::CopyToken`].

use crastu_app::{ModuleId, Nav, SettingsCategory};
use crastu_controls::{
    Choice, ChoiceInput, DialogEffect, DialogInput, DialogModel, Edit, TextModel,
};

use crate::platform::Platform;

/// The environment variable the MCP shim reads the token from.
pub const TOKEN_ENV: &str = "CRASTU_AGENT_TOKEN";

/// The longest agent name.
pub const NAME_MAX: usize = 64;

/// Shown when a new token is asked for while one is still on screen: it would be lost.
const TOKEN_OPEN: &str = "Copy the token and press Done first: it is shown only once.";

/// How many recent calls the page lists.
pub const RECENT_CALLS: usize = 8;

/// What happens when an agent calls a tool of a class. Reads are always Auto and not editable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Auto,
    Ask,
    Deny,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Auto => "Auto",
            Level::Ask => "Ask",
            Level::Deny => "Deny",
        }
    }
}

/// The classes of tool call whose policy can be edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Change,
    Delete,
}

impl Class {
    pub const ALL: [Class; 2] = [Class::Change, Class::Delete];

    pub fn label(self) -> &'static str {
        match self {
            Class::Change => "Changes",
            Class::Delete => "Deletions",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Class::Change => "Edits, renames and other changes that can be undone.",
            Class::Delete => "Deletions; the files go to the trash.",
        }
    }

    /// The levels that can be chosen, in the order the control shows them. A change cannot be
    /// denied outright (the host refuses it).
    pub fn levels(self) -> &'static [Level] {
        match self {
            Class::Change => &[Level::Ask, Level::Auto],
            Class::Delete => &[Level::Ask, Level::Auto, Level::Deny],
        }
    }
}

/// The default decision per class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defaults {
    pub change: Level,
    pub delete: Level,
}

impl Default for Defaults {
    fn default() -> Self {
        Defaults {
            change: Level::Ask,
            delete: Level::Ask,
        }
    }
}

impl Defaults {
    pub fn get(&self, class: Class) -> Level {
        match class {
            Class::Change => self.change,
            Class::Delete => self.delete,
        }
    }
}

/// One logged call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallRow {
    /// UTC, ISO 8601.
    pub at: String,
    pub agent: String,
    /// False for a caller without a valid token: its name is only a label.
    pub paired: bool,
    pub tool: String,
    pub decision: String,
    pub outcome: String,
}

/// One paired agent. The token is never part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRow {
    pub name: String,
    pub created_at: String,
    pub revoked: bool,
    pub change: Option<Level>,
    pub delete: Option<Level>,
    pub last_call: Option<CallRow>,
}

impl AgentRow {
    pub fn override_of(&self, class: Class) -> Option<Level> {
        match class {
            Class::Change => self.change,
            Class::Delete => self.delete,
        }
    }
}

/// Everything the page shows, as read from the platform.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentsSnapshot {
    pub agents: Vec<AgentRow>,
    pub defaults: Defaults,
    /// The newest calls of every caller, newest first.
    pub recent: Vec<CallRow>,
}

/// A token, shown once. It prints as a placeholder, so a debug print or a log line never has it.
/// It is not wiped from memory when dropped (that needs `unsafe` or a new dependency); it is
/// held only while the box is open and is not written anywhere else.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(token: impl Into<String>) -> Secret {
        Secret(token.into())
    }

    /// The token itself: for the one-time box and for Copy, nowhere else.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<hidden>)")
    }
}

/// The open one-time token box.
#[derive(Debug)]
pub struct Reveal {
    pub agent: String,
    pub token: Secret,
    /// True for a rotation, so the box says the old token stopped working.
    pub rotated: bool,
}

/// What the view asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Name(Edit),
    Pair,
    Rotate(String),
    AskRevoke(String),
    ConfirmRevoke,
    Dialog(DialogInput),
    Default(Class, ChoiceInput),
    Override(String, Class, ChoiceInput),
    CopyToken,
    DismissToken,
    Refresh,
}

/// What the owner has to do after an intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// An error to show as a toast.
    Toast(String),
    /// Put this on the clipboard (only ever the token, only from Copy).
    Copy(Secret),
    FocusDialog,
    ReturnFocus,
}

/// Whether the app is showing Settings, Agents.
pub fn is_agents_page(nav: &Nav) -> bool {
    nav.active == ModuleId::Settings && nav.settings_category == SettingsCategory::Agents
}

/// The message for a name that is not allowed, or None when it is. The same rules as the host's
/// pairing (`crastu_host::pairing::check_name`; a test in the desktop app keeps them equal).
pub fn name_problem(name: &str) -> Option<&'static str> {
    let ok = !name.is_empty()
        && name.len() <= NAME_MAX
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
        });
    (!ok).then_some("Use 1 to 64 characters of a-z, 0-9, '-', '_' and '.'.")
}

/// `2026-11-12T10:03:22.123Z` as `2026-11-12 10:03`; anything else is shown as it is.
pub fn short_time(at: &str) -> String {
    match (at.get(..10), at.get(11..16)) {
        (Some(d), Some(t)) if at.as_bytes().get(10) == Some(&b'T') => format!("{d} {t}"),
        _ => at.to_string(),
    }
}

/// The labels of a per-agent override control: the default first, then the levels of the class.
pub fn override_labels(class: Class) -> Vec<&'static str> {
    std::iter::once("Default")
        .chain(class.levels().iter().map(|l| l.label()))
        .collect()
}

fn override_levels(class: Class) -> Vec<Option<Level>> {
    std::iter::once(None)
        .chain(class.levels().iter().copied().map(Some))
        .collect()
}

/// The labels of a default control.
pub fn default_labels(class: Class) -> Vec<&'static str> {
    class.levels().iter().map(|l| l.label()).collect()
}

/// The chosen option of a default control.
pub fn default_selected(defaults: &Defaults, class: Class) -> usize {
    class
        .levels()
        .iter()
        .position(|l| *l == defaults.get(class))
        .unwrap_or(0)
}

/// The chosen option of an override control.
pub fn override_selected(agent: &AgentRow, class: Class) -> usize {
    override_levels(class)
        .iter()
        .position(|l| *l == agent.override_of(class))
        .unwrap_or(0)
}

fn dialog_effect(fx: DialogEffect) -> Vec<Effect> {
    match fx {
        DialogEffect::None => Vec::new(),
        DialogEffect::FocusDialog => vec![Effect::FocusDialog],
        DialogEffect::ReturnFocus => vec![Effect::ReturnFocus],
    }
}

/// The state of the page.
#[derive(Debug)]
pub struct AgentsModel {
    pub snapshot: AgentsSnapshot,
    pub name: TextModel,
    pub reveal: Option<Reveal>,
    /// The agent the revoke dialog asks about.
    pub confirm: Option<String>,
    pub dialog: DialogModel,
    /// Whether the Agents page was showing at the last `sync`.
    on_page: bool,
}

impl Default for AgentsModel {
    fn default() -> Self {
        AgentsModel {
            snapshot: AgentsSnapshot::default(),
            name: TextModel::default().with_max_chars(NAME_MAX),
            reveal: None,
            confirm: None,
            dialog: DialogModel::default(),
            on_page: false,
        }
    }
}

impl AgentsModel {
    /// The active agent with this name, as listed now.
    fn active(&self, name: &str) -> Option<&AgentRow> {
        self.snapshot
            .agents
            .iter()
            .find(|a| a.name == name && !a.revoked)
    }

    /// Whether Pair can run: the typed name is allowed.
    pub fn can_pair(&self) -> bool {
        name_problem(self.name.text()).is_none()
    }

    /// The message under the name field, once something is typed and it is not allowed.
    pub fn name_error(&self) -> Option<&'static str> {
        if self.name.is_empty() {
            None
        } else {
            name_problem(self.name.text())
        }
    }

    /// Closes the token box and forgets the token. The page does this when it is left.
    pub fn dismiss_token(&mut self) {
        self.reveal = None;
    }

    /// Reads the platform again. A failure keeps what is shown and says why.
    pub fn refresh(&mut self, platform: &dyn Platform) -> Vec<Effect> {
        match platform.list_agents() {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                // A token for an agent that was revoked elsewhere (the CLI) is of no use.
                if let Some(r) = &self.reveal
                    && self.active(&r.agent).is_none()
                {
                    self.reveal = None;
                }
                Vec::new()
            }
            Err(why) => vec![Effect::Toast(format!("Could not read the agents: {why}"))],
        }
    }

    /// Called after every app action with whether the Agents page is showing: reads the list
    /// when the page opens, and forgets a shown token and a pending revoke when it closes.
    pub fn sync(&mut self, on_page: bool, platform: &dyn Platform) -> Vec<Effect> {
        if on_page == self.on_page {
            return Vec::new();
        }
        self.on_page = on_page;
        if on_page {
            return self.refresh(platform);
        }
        self.dismiss_token();
        self.confirm = None;
        self.dialog.open = false;
        Vec::new()
    }

    pub fn apply(&mut self, intent: Intent, platform: &dyn Platform) -> Vec<Effect> {
        match intent {
            Intent::Name(edit) => {
                self.name.apply(&edit);
                Vec::new()
            }
            Intent::Pair => self.pair(platform),
            Intent::Rotate(name) => self.rotate(&name, platform),
            Intent::AskRevoke(name) => {
                if self.active(&name).is_none() {
                    return self.refresh(platform);
                }
                self.confirm = Some(name);
                dialog_effect(self.dialog.apply(&DialogInput::Open))
            }
            Intent::ConfirmRevoke => self.revoke(platform),
            Intent::Dialog(input) => {
                let fx = self.dialog.apply(&input);
                if !self.dialog.open {
                    self.confirm = None;
                }
                dialog_effect(fx)
            }
            Intent::Default(class, input) => {
                let labels = default_labels(class);
                let mut choice = Choice::new(
                    labels.len(),
                    default_selected(&self.snapshot.defaults, class),
                );
                if !choice.apply(&input) {
                    return Vec::new();
                }
                let level = class.levels()[choice.selected];
                let done = platform.set_default_policy(class, level);
                self.after_write(done, platform)
            }
            Intent::Override(agent, class, input) => {
                let Some(row) = self.active(&agent) else {
                    return self.refresh(platform);
                };
                let levels = override_levels(class);
                let mut choice = Choice::new(levels.len(), override_selected(row, class));
                if !choice.apply(&input) {
                    return Vec::new();
                }
                let done = platform.set_agent_override(&agent, class, levels[choice.selected]);
                self.after_write(done, platform)
            }
            Intent::CopyToken => self
                .reveal
                .as_ref()
                .map(|r| vec![Effect::Copy(r.token.clone())])
                .unwrap_or_default(),
            Intent::DismissToken => {
                self.dismiss_token();
                Vec::new()
            }
            Intent::Refresh => self.refresh(platform),
        }
    }

    /// After a write: show what the platform really holds now, and say if it failed.
    fn after_write(&mut self, done: Result<(), String>, platform: &dyn Platform) -> Vec<Effect> {
        let mut fx = self.refresh(platform);
        if let Err(why) = done {
            fx.insert(
                0,
                Effect::Toast(format!("Could not save the policy: {why}")),
            );
        }
        fx
    }

    fn pair(&mut self, platform: &dyn Platform) -> Vec<Effect> {
        if self.reveal.is_some() {
            return vec![Effect::Toast(TOKEN_OPEN.to_string())];
        }
        let name = self.name.text().to_string();
        if let Some(problem) = name_problem(&name) {
            return vec![Effect::Toast(problem.to_string())];
        }
        match platform.pair_agent(&name) {
            Ok(token) => {
                self.reveal = Some(Reveal {
                    agent: name,
                    token,
                    rotated: false,
                });
                self.name = TextModel::default().with_max_chars(NAME_MAX);
                self.refresh(platform)
            }
            Err(why) => vec![Effect::Toast(format!("Could not pair the agent: {why}"))],
        }
    }

    fn rotate(&mut self, name: &str, platform: &dyn Platform) -> Vec<Effect> {
        if self.reveal.is_some() {
            return vec![Effect::Toast(TOKEN_OPEN.to_string())];
        }
        if self.active(name).is_none() {
            let mut fx = self.refresh(platform);
            fx.insert(0, Effect::Toast(format!("{name} is not an active agent.")));
            return fx;
        }
        match platform.rotate_agent(name) {
            Ok(token) => {
                self.reveal = Some(Reveal {
                    agent: name.to_string(),
                    token,
                    rotated: true,
                });
                self.refresh(platform)
            }
            Err(why) => vec![Effect::Toast(format!("Could not rotate the token: {why}"))],
        }
    }

    /// Revokes only the agent the dialog asked about, and only after it was confirmed.
    fn revoke(&mut self, platform: &dyn Platform) -> Vec<Effect> {
        let Some(name) = self.confirm.take() else {
            return Vec::new();
        };
        self.dialog.open = false;
        let mut fx = vec![Effect::ReturnFocus];
        match platform.revoke_agent(&name) {
            Ok(()) => {
                if self.reveal.as_ref().is_some_and(|r| r.agent == name) {
                    self.reveal = None;
                }
            }
            Err(why) => fx.push(Effect::Toast(format!("Could not revoke {name}: {why}"))),
        }
        fx.extend(self.refresh(platform));
        fx
    }
}

#[cfg(test)]
mod tests;
