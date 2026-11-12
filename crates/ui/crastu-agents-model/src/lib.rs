//! Settings, Agents as plain data: the paired agents, the approval policy, the one-time token
//! reveal and the rules for each button. The view draws it and sends [`Intent`]s; the owner
//! applies them against the state as it is then and carries out the [`Effect`]s. The platform
//! (the desktop app, over the host's pairing and policy stores) does the work behind
//! [`AgentsPlatform`].
//!
//! A token is a secret. [`Secret`] never prints, the model keeps it only while the reveal is
//! open, and the clipboard is touched only by [`Intent::CopyToken`].

use crastu_app::{ModuleId, Nav, SettingsCategory};
use crastu_controls::{
    Choice, ChoiceInput, DialogEffect, DialogInput, DialogModel, Edit, TextModel,
};

mod data;
pub mod platform;
pub use data::{
    AgentRow, AgentsSnapshot, CallRow, Class, Defaults, LABEL_MAX, Level, Secret, clean_label,
};
pub use platform::AgentsPlatform;

/// The environment variable the MCP shim reads the token from.
pub const TOKEN_ENV: &str = "CRASTU_AGENT_TOKEN";

/// The longest agent name.
pub const NAME_MAX: usize = 64;

/// Shown when a new token is asked for while one is still on screen: it would be lost.
const TOKEN_OPEN: &str = "Copy the token and press Done first: it is shown only once.";

/// Shown when a reset was confirmed but the policy had been fixed in the meantime.
const POLICY_OK: &str = "Policy is no longer damaged; nothing was reset.";

/// Shown when a policy change is asked for while the policy is damaged.
const POLICY_DAMAGED: &str = "The policy is damaged: reset it before changing it.";

/// How many recent calls the page lists.
pub const RECENT_CALLS: usize = 8;

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
    /// Reset a damaged stored policy to the defaults (asks first).
    AskResetPolicy,
    /// Carry out what the open dialog asks about.
    Confirm,
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
    /// Move the focus to the one-time token box (it just opened).
    FocusToken,
}

/// What the confirmation dialog asks about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pending {
    Revoke(String),
    /// Replace the stored policy with the defaults.
    ResetPolicy,
    /// Set a class to Auto for everything without an override (`agent` None) or for one agent.
    Auto {
        agent: Option<String>,
        class: Class,
    },
}

impl Pending {
    pub fn title(&self) -> String {
        match self {
            Pending::Revoke(name) => format!("Revoke {name}?"),
            Pending::ResetPolicy => "Reset the policy to the defaults?".to_string(),
            Pending::Auto { agent: None, class } => {
                format!("Let anything {} without asking?", class.verb())
            }
            Pending::Auto {
                agent: Some(name),
                class,
            } => format!("Let {name} {} without asking?", class.verb()),
        }
    }

    pub fn subtitle(&self) -> String {
        match self {
            Pending::Revoke(_) => "Its token stops working at once. This cannot be undone; pair a new agent to give access again.".to_string(),
            Pending::ResetPolicy => "Changes and deletions go back to Ask and every per-agent setting is removed.".to_string(),
            Pending::Auto { agent: None, class } => format!(
                "Auto lets any local process {} without asking, including callers that have no token.",
                class.verb()
            ),
            Pending::Auto { agent: Some(_), class } => format!(
                "Auto lets this agent {} without asking you first.",
                class.verb()
            ),
        }
    }

    /// The label of the button that confirms.
    pub fn action(&self) -> &'static str {
        match self {
            Pending::Revoke(_) => "Revoke",
            Pending::ResetPolicy => "Reset policy",
            Pending::Auto { .. } => "Allow Auto",
        }
    }
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
    /// What the dialog asks about.
    pub confirm: Option<Pending>,
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

    /// Whether Pair can run: the typed name is allowed and no token box is open (a second token
    /// would replace the one on screen, which is shown only once).
    pub fn can_pair(&self) -> bool {
        self.reveal.is_none() && name_problem(self.name.text()).is_none()
    }

    /// Whether Rotate can run: no token box is open.
    pub fn can_rotate(&self) -> bool {
        self.reveal.is_none()
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
    pub fn refresh(&mut self, platform: &dyn AgentsPlatform) -> Vec<Effect> {
        match platform.list_agents() {
            Ok(snapshot) => {
                self.snapshot = snapshot.cleaned();
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
    pub fn sync(&mut self, on_page: bool, platform: &dyn AgentsPlatform) -> Vec<Effect> {
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

    pub fn apply(&mut self, intent: Intent, platform: &dyn AgentsPlatform) -> Vec<Effect> {
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
                self.ask(Pending::Revoke(name))
            }
            Intent::AskResetPolicy => {
                if !self.snapshot.policy_damaged {
                    return self.refresh(platform);
                }
                self.ask(Pending::ResetPolicy)
            }
            Intent::Confirm => self.confirmed(platform),
            Intent::Dialog(input) => {
                let fx = self.dialog.apply(&input);
                if !self.dialog.open {
                    self.confirm = None;
                }
                dialog_effect(fx)
            }
            Intent::Default(..) | Intent::Override(..) if self.snapshot.policy_damaged => {
                vec![Effect::Toast(POLICY_DAMAGED.to_string())]
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
                if level == Level::Auto {
                    return self.ask(Pending::Auto { agent: None, class });
                }
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
                let level = levels[choice.selected];
                if level == Some(Level::Auto) && class == Class::Delete {
                    return self.ask(Pending::Auto {
                        agent: Some(agent),
                        class,
                    });
                }
                let done = platform.set_agent_override(&agent, class, level);
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
    fn after_write(
        &mut self,
        done: Result<(), String>,
        platform: &dyn AgentsPlatform,
    ) -> Vec<Effect> {
        let mut fx = self.refresh(platform);
        if let Err(why) = done {
            fx.insert(
                0,
                Effect::Toast(format!("Could not save the policy: {why}")),
            );
        }
        fx
    }

    fn pair(&mut self, platform: &dyn AgentsPlatform) -> Vec<Effect> {
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
                let mut fx = self.refresh(platform);
                fx.push(Effect::FocusToken);
                fx
            }
            Err(why) => vec![Effect::Toast(format!("Could not pair the agent: {why}"))],
        }
    }

    fn rotate(&mut self, name: &str, platform: &dyn AgentsPlatform) -> Vec<Effect> {
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
                let mut fx = self.refresh(platform);
                fx.push(Effect::FocusToken);
                fx
            }
            Err(why) => vec![Effect::Toast(format!("Could not rotate the token: {why}"))],
        }
    }

    fn ask(&mut self, what: Pending) -> Vec<Effect> {
        self.confirm = Some(what);
        dialog_effect(self.dialog.apply(&DialogInput::Open))
    }

    /// Carries out only what the dialog asked about, and only after it was confirmed.
    fn confirmed(&mut self, platform: &dyn AgentsPlatform) -> Vec<Effect> {
        let Some(what) = self.confirm.take() else {
            return Vec::new();
        };
        self.dialog.open = false;
        let mut fx = vec![Effect::ReturnFocus];
        match what {
            Pending::Revoke(name) => match platform.revoke_agent(&name) {
                Ok(()) => {
                    if self.reveal.as_ref().is_some_and(|r| r.agent == name) {
                        self.reveal = None;
                    }
                }
                Err(why) => fx.push(Effect::Toast(format!("Could not revoke {name}: {why}"))),
            },
            Pending::ResetPolicy => match platform.reset_policy() {
                Ok(true) => {}
                Ok(false) => fx.push(Effect::Toast(POLICY_OK.to_string())),
                Err(why) => fx.push(Effect::Toast(format!("Could not reset the policy: {why}"))),
            },
            Pending::Auto { agent, class } => {
                let done = match &agent {
                    None => platform.set_default_policy(class, Level::Auto),
                    Some(name) if self.active(name).is_some() => {
                        platform.set_agent_override(name, class, Some(Level::Auto))
                    }
                    Some(name) => Err(format!("{name} is not an active agent")),
                };
                fx.extend(self.after_write(done, platform));
                return fx;
            }
        }
        fx.extend(self.refresh(platform));
        fx
    }
}

#[cfg(test)]
mod tests;
