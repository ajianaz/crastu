//! The Cmd+K command palette as plain data: the commands, how a query narrows them, and the
//! open / typing / selecting state. The view only draws it and sends [`PaletteInput`]s; running a
//! command means handing its `crastu_app::Action` to the app, so the palette holds no business
//! state of its own.

use crastu_app::{Action, Density, FontPairing, ModuleId, SettingsCategory, Theme};
use crastu_controls::{Edit, TextModel};
use crastu_ui::tokens::fonts;

/// The most rows the palette shows; a longer result list is narrowed by typing.
pub const MAX_ROWS: usize = 8;

/// One command in the palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// What kind of command it is (shown dimmed before the label).
    pub group: &'static str,
    pub label: String,
    pub action: Action,
}

fn entry(group: &'static str, label: impl Into<String>, action: Action) -> Entry {
    Entry {
        group,
        label: label.into(),
        action,
    }
}

/// Every command, in the order they are listed when the query is empty: where to go, the Settings
/// categories, what to show, and the appearance choices. Only what is built into this build.
pub fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    for m in ModuleId::available() {
        out.push(entry("Go to", m.title(), Action::Navigate(m)));
    }
    for c in SettingsCategory::available() {
        out.push(entry("Settings", c.title(), Action::OpenSettings(c)));
    }
    out.push(entry(
        "View",
        "Toggle the sidebar",
        Action::ToggleLeftSidebar,
    ));
    out.push(entry(
        "View",
        "Toggle the inspector",
        Action::ToggleInspector,
    ));
    let theme_names = ["Graphite", "Paper", "Signal"];
    for (t, name) in Theme::ALL.iter().zip(theme_names) {
        out.push(entry(
            "Appearance",
            format!("Theme: {name}"),
            Action::SetTheme(*t),
        ));
    }
    for f in FontPairing::ALL {
        out.push(entry(
            "Appearance",
            format!("Font: {}", fonts(f).label),
            Action::SetFont(f),
        ));
    }
    out.push(entry(
        "Appearance",
        "Density: Compact",
        Action::SetDensity(Density::Compact),
    ));
    out.push(entry(
        "Appearance",
        "Density: Comfortable",
        Action::SetDensity(Density::Comfortable),
    ));
    out
}

/// The entries that match `query`, best first, as indexes into `entries`. Every word of the query
/// must appear in the group or the label (case does not matter); a label that starts with the first
/// word comes first, then shorter labels, then the original order. An empty query matches all.
pub fn matches(query: &str, entries: &[Entry]) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return (0..entries.len()).collect();
    }
    let mut hits: Vec<(bool, usize, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let label = e.label.to_lowercase();
            let hay = format!("{} {label}", e.group.to_lowercase());
            words.iter().all(|w| hay.contains(w.as_str())).then(|| {
                (
                    !words.first().is_some_and(|w| label.starts_with(w.as_str())),
                    label.len(),
                    i,
                )
            })
        })
        .collect();
    hits.sort();
    hits.into_iter().map(|(_, _, i)| i).collect()
}

/// What the view tells the palette.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteInput {
    Open,
    Close,
    Edit(Edit),
    /// A key by name: `up`, `down`, `enter`, `escape`.
    Key(String),
    /// A click on a row: run that command.
    Choose(Action),
}

/// What the owner has to do after an input.
#[derive(Clone, Debug, PartialEq)]
pub enum PaletteEffect {
    None,
    /// Run this command (the palette has closed).
    Run(Action),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Palette {
    pub open: bool,
    pub query: TextModel,
    /// Index into the visible rows (the first [`MAX_ROWS`] matches).
    pub selected: usize,
}

impl Palette {
    /// The visible rows for the current query.
    pub fn rows(&self, entries: &[Entry]) -> Vec<usize> {
        let mut m = matches(self.query.text(), entries);
        m.truncate(MAX_ROWS);
        m
    }

    pub fn apply(&mut self, input: &PaletteInput, entries: &[Entry]) -> PaletteEffect {
        match input {
            PaletteInput::Open => {
                *self = Palette {
                    open: true,
                    ..Palette::default()
                };
            }
            PaletteInput::Close => self.open = false,
            _ if !self.open => {}
            PaletteInput::Edit(e) => {
                self.query.apply(e);
                self.selected = 0;
            }
            PaletteInput::Choose(action) => {
                self.open = false;
                return PaletteEffect::Run(action.clone());
            }
            PaletteInput::Key(k) => {
                let rows = self.rows(entries);
                match k.as_str() {
                    "escape" => self.open = false,
                    "down" => self.selected = (self.selected + 1).min(rows.len().saturating_sub(1)),
                    "up" => self.selected = self.selected.saturating_sub(1),
                    "enter" => {
                        if let Some(i) = rows.get(self.selected) {
                            let action = entries[*i].action.clone();
                            self.open = false;
                            return PaletteEffect::Run(action);
                        }
                    }
                    _ => {}
                }
            }
        }
        PaletteEffect::None
    }
}

#[cfg(test)]
mod tests;
