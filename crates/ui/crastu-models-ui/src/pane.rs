//! The state the shell keeps for the page (the model, the focus handles) and how an intent is
//! applied: against the model as it is now, then its effects are carried out.

use std::rc::Rc;

use crastu_models_model::{Effect, Intent, ModelsModel, ModelsPlatform};
use gpui::{App, Context, FocusHandle, WeakEntity, Window};

/// Sends an intent of the page to its owner from a callback of a component.
pub type ModelsSend = Rc<dyn Fn(Intent, &mut Window, &mut App) + 'static>;

/// A [`ModelsSend`] that calls `apply` on the owner entity, which applies the intent to the page
/// as it is then.
pub fn sender<T: 'static>(
    owner: WeakEntity<T>,
    apply: fn(&mut T, Intent, &mut Window, &mut Context<T>),
) -> ModelsSend {
    Rc::new(move |intent, window, app| {
        let _ = owner.update(app, |t, cx| apply(t, intent, window, cx));
    })
}

/// The focus handles of one model row.
pub(crate) struct RowFocus {
    pub action: FocusHandle,
}

pub(crate) struct Focus {
    pub refresh: FocusHandle,
    pub dialog: FocusHandle,
    /// One per catalogue row, in the order of the rows.
    pub rows: Vec<RowFocus>,
}

pub struct ModelsPane {
    pub(crate) model: ModelsModel,
    pub(crate) focus: Focus,
    /// The row whose Remove button opened the dialog, to take the focus back to it.
    return_row: Option<usize>,
}

impl ModelsPane {
    pub fn new(cx: &App) -> ModelsPane {
        let model = ModelsModel::shipped();
        let focus = Focus {
            refresh: cx.focus_handle(),
            dialog: cx.focus_handle(),
            rows: model
                .rows
                .iter()
                .map(|_| RowFocus {
                    action: cx.focus_handle(),
                })
                .collect(),
        };
        ModelsPane {
            model,
            focus,
            return_row: None,
        }
    }

    pub fn model(&self) -> &ModelsModel {
        &self.model
    }

    /// Applies an intent to the page as it is now and carries out what it asks. Returns the
    /// messages to show as error toasts.
    pub fn apply(
        &mut self,
        intent: Intent,
        platform: &dyn ModelsPlatform,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<String> {
        if let Intent::AskRemove(id) = &intent {
            self.return_row = self.model.rows.iter().position(|r| &r.id == id);
        }
        let effects = self.model.apply(intent, platform);
        self.run(effects, Some(window), cx)
    }

    /// Called after every app action with whether the page is showing.
    pub fn sync(
        &mut self,
        on_page: bool,
        platform: &dyn ModelsPlatform,
        cx: &mut App,
    ) -> Vec<String> {
        let effects = self.model.sync(on_page, platform);
        self.run(effects, None, cx)
    }

    /// Called on a timer: takes the downloads' events. Returns whether to draw again and the
    /// toasts.
    pub fn tick(&mut self, platform: &dyn ModelsPlatform, cx: &mut App) -> (bool, Vec<String>) {
        let tick = self.model.tick(platform);
        (tick.changed, self.run(tick.effects, None, cx))
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
                Effect::FocusDialog => {
                    if let Some(w) = window.as_deref_mut() {
                        w.focus(&self.focus.dialog, cx);
                    }
                }
                Effect::ReturnFocus => {
                    let target = self
                        .return_row
                        .take()
                        .and_then(|i| self.focus.rows.get(i))
                        .map_or(&self.focus.refresh, |r| &r.action);
                    if let Some(w) = window.as_deref_mut() {
                        w.focus(target, cx);
                    }
                }
            }
        }
        toasts
    }
}
