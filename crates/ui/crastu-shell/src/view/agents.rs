//! Settings, Agents in the shell: the page is `crastu-agents-ui`; the shell owns its state.

use crastu_agents_ui::AgentSend;
use crastu_controls::ToastKind;
use crastu_ui::tokens::Tokens;
use gpui::{AnyElement, Context, Window};

use super::Shell;
use crate::agents::{Intent, is_agents_page};

impl Shell {
    fn agents_sender(cx: &Context<Self>) -> AgentSend {
        crastu_agents_ui::sender(cx.entity().downgrade(), Self::agents_intent)
    }
    fn agents_intent(&mut self, intent: Intent, window: &mut Window, cx: &mut Context<Self>) {
        let toasts = self.agents.apply(intent, &*self.platform, window, cx);
        self.show_toasts(toasts);
        self.check_focus = true; // the pressed button can be gone next frame (Done, Revoke)
        cx.notify();
    }

    pub(super) fn show_toasts(&mut self, messages: Vec<String>) {
        messages.iter().for_each(|m| {
            self.toasts.push(ToastKind::Error, m, None);
        });
    }
    pub(super) fn agents_page(&self, t: Tokens, cx: &Context<Self>) -> gpui::Stateful<gpui::Div> {
        crastu_agents_ui::page(&self.agents, t, &Self::agents_sender(cx))
    }
    pub(super) fn agents_dialog(&self, t: Tokens, cx: &Context<Self>) -> Option<AnyElement> {
        crastu_agents_ui::dialog(&self.agents, t, &Self::agents_sender(cx))
    }

    pub(super) fn sync_agents(&mut self, cx: &mut Context<Self>) {
        let on = is_agents_page(&self.model.state().nav);
        let toasts = self.agents.sync(on, &*self.platform, cx);
        self.show_toasts(toasts);
    }
}
