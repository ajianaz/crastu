//! The state the shell keeps for the page (the model, the focus handles) and how an intent is
//! applied: against the model as it is now, then its effects are carried out.

use std::rc::Rc;

use crastu_shell_model::agents::{AgentsModel, Effect, Intent};
use crastu_shell_model::platform::Platform;
use gpui::{App, ClipboardItem, FocusHandle, Window};

/// Sends an intent of the page to its owner from a callback of a component.
pub type AgentSend = Rc<dyn Fn(Intent, &mut Window, &mut App) + 'static>;

/// The focus handles of one agent row.
pub(crate) struct RowFocus {
    pub rotate: FocusHandle,
    pub revoke: FocusHandle,
    pub change: FocusHandle,
    pub delete: FocusHandle,
}

pub(crate) struct Focus {
    pub name: FocusHandle,
    pub pair: FocusHandle,
    pub copy: FocusHandle,
    pub done: FocusHandle,
    pub default_change: FocusHandle,
    pub default_delete: FocusHandle,
    pub refresh: FocusHandle,
    pub dialog: FocusHandle,
    /// One per listed agent; grows with the list.
    pub rows: Vec<RowFocus>,
}

pub struct AgentsPane {
    pub(crate) model: AgentsModel,
    pub(crate) focus: Focus,
    /// The list row whose Revoke button opened the dialog, to take the focus back to it.
    return_row: Option<usize>,
}

impl AgentsPane {
    pub fn new(cx: &App) -> AgentsPane {
        let focus = Focus {
            name: cx.focus_handle(),
            pair: cx.focus_handle(),
            copy: cx.focus_handle(),
            done: cx.focus_handle(),
            default_change: cx.focus_handle(),
            default_delete: cx.focus_handle(),
            refresh: cx.focus_handle(),
            dialog: cx.focus_handle(),
            rows: Vec::new(),
        };
        AgentsPane {
            model: AgentsModel::default(),
            focus,
            return_row: None,
        }
    }

    pub fn model(&self) -> &AgentsModel {
        &self.model
    }

    fn ensure_rows(&mut self, cx: &App) {
        while self.focus.rows.len() < self.model.snapshot.agents.len() {
            self.focus.rows.push(RowFocus {
                rotate: cx.focus_handle(),
                revoke: cx.focus_handle(),
                change: cx.focus_handle(),
                delete: cx.focus_handle(),
            });
        }
    }

    /// Applies an intent to the page as it is now and carries out what it asks. Returns the
    /// messages to show as error toasts.
    pub fn apply(
        &mut self,
        intent: Intent,
        platform: &dyn Platform,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<String> {
        if let Intent::AskRevoke(name) = &intent {
            self.return_row = self
                .model
                .snapshot
                .agents
                .iter()
                .position(|a| &a.name == name);
        }
        let effects = self.model.apply(intent, platform);
        self.ensure_rows(cx);
        self.run(effects, Some(window), cx)
    }

    /// Called after every app action with whether the Agents page is showing.
    pub fn sync(&mut self, on_page: bool, platform: &dyn Platform, cx: &mut App) -> Vec<String> {
        let effects = self.model.sync(on_page, platform);
        self.ensure_rows(cx);
        self.run(effects, None, cx)
    }

    fn run(
        &mut self,
        effects: Vec<Effect>,
        mut window: Option<&mut Window>,
        cx: &mut App,
    ) -> Vec<String> {
        let mut toasts = Vec::new();
        for effect in effects {
            match effect {
                Effect::Toast(message) => toasts.push(message),
                // The token goes to the clipboard here and only here, on the Copy button.
                Effect::Copy(token) => {
                    cx.write_to_clipboard(ClipboardItem::new_string(token.expose().to_string()))
                }
                Effect::FocusDialog => {
                    if let Some(w) = window.as_deref_mut() {
                        w.focus(&self.focus.dialog, cx);
                    }
                }
                Effect::ReturnFocus => {
                    let row = self.return_row.take().filter(|i| {
                        self.model
                            .snapshot
                            .agents
                            .get(*i)
                            .is_some_and(|a| !a.revoked)
                    });
                    let target = row
                        .and_then(|i| self.focus.rows.get(i))
                        .map_or(&self.focus.name, |r| &r.revoke);
                    if let Some(w) = window.as_deref_mut() {
                        w.focus(target, cx);
                    }
                }
            }
        }
        toasts
    }
}
