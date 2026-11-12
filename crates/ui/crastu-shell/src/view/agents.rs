//! Settings, Agents inside the shell: the page itself is `crastu-agents-ui`. The shell owns its
//! state, applies the intents it sends back, shows its toasts and tells it when the page opens
//! or closes.

use std::rc::Rc;

use crastu_agents_ui::AgentSend;
use crastu_controls::ToastKind;
use crastu_ui::tokens::Tokens;
use gpui::{AnyElement, Context, Window};

use super::Shell;
use crate::agents::{Intent, is_agents_page};

impl Shell {
    fn agents_sender(cx: &Context<Self>) -> AgentSend {
        let this = cx.entity().downgrade();
        Rc::new(move |intent, window, app| {
            this.update(app, |shell, cx| shell.agents_intent(intent, window, cx))
                .ok();
        })
    }

    /// Applies an intent to the page as it is now.
    fn agents_intent(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        let toasts = self.agents.apply(intent, &*self.platform, window, cx);
        self.show_toasts(toasts);
        // A button that was pressed can be gone in the next frame (Done, Revoke).
        self.check_focus = true;
        cx.notify();
    }

    fn show_toasts(&mut self, messages: Vec<String>) {
        for message in messages {
            self.toasts.push(ToastKind::Error, &message, None);
        }
    }

    pub(super) fn agents_page(&self, t: Tokens, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        crastu_agents_ui::page(&self.agents, t, &Self::agents_sender(cx))
    }

    pub(super) fn agents_dialog(&self, t: Tokens, cx: &Context<Self>) -> Option<AnyElement> {
        crastu_agents_ui::dialog(&self.agents, t, &Self::agents_sender(cx))
    }

    /// Called after every action: the page reads its list when it opens, and forgets a shown
    /// token and a pending revoke when it closes.
    pub(super) fn sync_agents(&mut self, cx: &mut Context<Self>) {
        let on = is_agents_page(&self.model.state().nav);
        let toasts = self.agents.sync(on, &*self.platform, cx);
        self.show_toasts(toasts);
    }
}
