//! The plain data of Settings, Agents: what the platform reports and the labels of its classes.

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

    /// What Auto lets a caller do, for the confirmation.
    pub fn verb(self) -> &'static str {
        match self {
            Class::Change => "change things",
            Class::Delete => "delete things",
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
    /// A stored policy row is not understood. The host then lets reads through and nothing else
    /// until the policy is reset; `defaults` and the overrides above are not what it applies.
    pub policy_damaged: bool,
}

/// The longest caller label shown, and the longest decision or outcome text.
pub const LABEL_MAX: usize = 64;
const DETAIL_MAX: usize = 160;

/// A label an unpaired caller chose, made safe to draw: control characters and the characters
/// that reorder or hide text are dropped, and it is cut at [`LABEL_MAX`] characters with an
/// ellipsis.
pub fn clean_label(raw: &str) -> String {
    clean_to(raw, LABEL_MAX)
}

fn clean_to(raw: &str, max: usize) -> String {
    let hidden = |c: char| {
        c.is_control()
            || matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
    };
    let mut out: String = raw.chars().filter(|c| !hidden(*c)).take(max + 1).collect();
    if out.chars().count() > max {
        out = out.chars().take(max - 1).collect();
        out.push('\u{2026}');
    }
    out
}

impl CallRow {
    fn cleaned(mut self) -> CallRow {
        self.agent = clean_label(&self.agent);
        self.tool = clean_label(&self.tool);
        self.decision = clean_to(&self.decision, DETAIL_MAX);
        self.outcome = clean_to(&self.outcome, DETAIL_MAX);
        self
    }
}

impl AgentsSnapshot {
    /// The snapshot with every caller-supplied label cleaned by [`clean_label`]. The model does
    /// this on every read, so no view ever draws a raw label.
    pub fn cleaned(mut self) -> AgentsSnapshot {
        self.recent = self.recent.into_iter().map(CallRow::cleaned).collect();
        for a in &mut self.agents {
            a.last_call = a.last_call.take().map(CallRow::cleaned);
        }
        self
    }
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
