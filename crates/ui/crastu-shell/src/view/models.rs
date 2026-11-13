//! Settings, Models and licences in the shell: the page is `crastu-models-ui`; the shell owns its
//! state, ticks it while downloads run and shows its toasts.

use std::time::Duration;

use crastu_models_ui::ModelsSend;
use crastu_ui::tokens::Tokens;
use gpui::{AnyElement, Context, Window};

use super::Shell;
use crate::models::{Intent, is_models_page};

/// How often the downloads' events are taken from the platform.
const TICK: Duration = Duration::from_millis(250);

impl Shell {
    fn models_sender(cx: &Context<Self>) -> ModelsSend {
        crastu_models_ui::sender(cx.entity().downgrade(), Self::models_intent)
    }

    fn models_intent(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        let toasts = self.models.apply(intent, &*self.platform, window, cx);
        self.show_toasts(toasts);
        self.check_focus = true; // the pressed button changes or goes (Remove, Cancel)
        cx.notify();
    }

    pub(super) fn models_page(&self, t: Tokens, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        crastu_models_ui::page(&self.models, t, &Self::models_sender(cx))
    }

    pub(super) fn models_dialog(&self, t: Tokens, cx: &Context<Self>) -> Option<AnyElement> {
        crastu_models_ui::dialog(&self.models, t, &Self::models_sender(cx))
    }

    pub(super) fn sync_models(&mut self, cx: &mut Context<Self>) {
        let on = is_models_page(&self.model.state().nav);
        let toasts = self.models.sync(on, &*self.platform, cx);
        self.show_toasts(toasts);
    }

    /// Takes the events of the background downloads on the UI thread, and follows Offline, every
    /// 250 ms for as long as the shell lives. The downloads themselves never touch the UI.
    pub(super) fn start_models_pump(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |shell, cx| shell.models_tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn models_tick(&mut self, cx: &mut Context<Self>) {
        let (changed, toasts) = self.models.tick(&*self.platform, cx);
        let toasts_shown = !toasts.is_empty();
        self.show_toasts(toasts);
        if changed || toasts_shown {
            cx.notify();
        }
    }
}
