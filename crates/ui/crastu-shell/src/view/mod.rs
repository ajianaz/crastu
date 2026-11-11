//! The GPUI shell: it owns the `crastu_app::App`, draws its state and sends actions back. All
//! rules live in `crastu-app`; what is here is layout and wiring.

mod chrome;
mod pages;

use std::rc::Rc;

pub use crastu_app::Settings;
use crastu_app::{Action, App as Model, Effect};
use crastu_controls::{Choice, ChoiceInput, ToastKind, Toasts};
use crastu_ui::tokens::Tokens;
use gpui::{
    App, Context, FocusHandle, IntoElement, KeyBinding, Render, Window, actions, div, prelude::*,
    px,
};

use crate::appearance::{self, Setting};
use crate::layout::Columns;

actions!(crastu_shell, [ToggleSidebar, ToggleInspector, OpenSettings]);

/// Key bindings: sidebar, inspector, Settings. Cmd on macOS, Ctrl elsewhere.
#[cfg(target_os = "macos")]
const KEYS: [&str; 3] = ["cmd-b", "cmd-alt-b", "cmd-,"];
#[cfg(not(target_os = "macos"))]
const KEYS: [&str; 3] = ["ctrl-b", "ctrl-alt-b", "ctrl-,"];

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(KEYS[0], ToggleSidebar, None),
        KeyBinding::new(KEYS[1], ToggleInspector, None),
        KeyBinding::new(KEYS[2], OpenSettings, None),
    ]);
}

/// Sends an action to the model from a callback of a component.
pub(crate) type Dispatch = Rc<dyn Fn(Action, &mut App) + 'static>;

pub(crate) type Pick = Rc<dyn Fn(Setting, ChoiceInput, &mut App) + 'static>;

/// The focus handles the shell keeps between frames.
pub(crate) struct Focus {
    pub root: FocusHandle,
    pub rail: Vec<FocusHandle>,
    pub categories: Vec<FocusHandle>,
    pub sidebar_toggle: FocusHandle,
    pub inspector_toggle: FocusHandle,
    pub theme: FocusHandle,
    pub font: FocusHandle,
    pub density: FocusHandle,
    pub motion: FocusHandle,
    pub continue_setup: FocusHandle,
}

#[allow(missing_debug_implementations, reason = "it holds GPUI handles")]
pub struct Shell {
    pub(crate) model: Model,
    fonts: Vec<String>,
    pub(crate) focus: Focus,
    pub(crate) toasts: Toasts,
    /// Until the onboarding screens exist (#55) a development build can step past the stub.
    pub(crate) past_stub: bool,
}

impl Shell {
    /// `notice` is shown as an error toast (a problem found while starting).
    pub fn new(
        settings: Settings,
        notice: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Shell {
        let handle = |cx: &mut Context<Self>| cx.focus_handle();
        let focus = Focus {
            root: handle(cx),
            rail: (0..8).map(|_| handle(cx)).collect(),
            categories: (0..12).map(|_| handle(cx)).collect(),
            sidebar_toggle: handle(cx),
            inspector_toggle: handle(cx),
            theme: handle(cx),
            font: handle(cx),
            density: handle(cx),
            motion: handle(cx),
            continue_setup: handle(cx),
        };
        window.focus(&focus.root, cx);
        let mut toasts = Toasts::default();
        if let Some(message) = notice {
            toasts.push(ToastKind::Error, &message, None);
        }
        Shell {
            model: Model::new(settings),
            fonts: cx.text_system().all_font_names(),
            focus,
            toasts,
            past_stub: false,
        }
    }

    /// The tokens for the appearance in the state, with fonts that are not installed replaced.
    pub(crate) fn tokens(&self) -> Tokens {
        Tokens::new(&self.model.state().appearance, false).with_installed_fonts(&self.fonts)
    }

    pub(crate) fn dispatch(&mut self, action: Action, cx: &mut Context<Self>) {
        for effect in self.model.dispatch(action) {
            // The permission and download effects are carried out by the onboarding slices.
            if let Effect::SettingsNotSaved(why) = effect {
                let message = format!("Could not save the setting: {why}");
                self.toasts.push(ToastKind::Error, &message, None);
            }
        }
        cx.notify();
    }

    /// A choice made in an Appearance control: the key or click is resolved against the option
    /// that is chosen *now*, so two key presses between frames both count.
    pub(crate) fn choose(&mut self, setting: Setting, input: &ChoiceInput, cx: &mut Context<Self>) {
        let labels = appearance::labels(setting);
        let now = appearance::selected(setting, &self.model.state().appearance);
        let mut choice = Choice::new(labels.len(), now);
        if let (true, Some(action)) = (
            choice.apply(input),
            appearance::action_for(setting, choice.selected),
        ) {
            self.dispatch(action, cx);
        }
    }

    pub(crate) fn chooser(cx: &Context<Self>) -> Pick {
        let this = cx.entity().downgrade();
        Rc::new(move |setting, input, app| {
            this.update(app, |shell, cx| shell.choose(setting, &input, cx))
                .ok();
        })
    }

    pub(crate) fn dispatcher(cx: &Context<Self>) -> Dispatch {
        let this = cx.entity().downgrade();
        Rc::new(move |action, app| {
            this.update(app, |shell, cx| shell.dispatch(action, cx))
                .ok();
        })
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The shortcuts are actions on the root, which only run while the focus is inside it.
        // When the focused element goes away (the stub's button after it is pressed, a sidebar
        // item that collapses) nothing is focused any more, so take the focus back.
        if window.focused(cx).is_none() {
            window.focus(&self.focus.root, cx);
        }
        let t = self.tokens();
        let c = t.colors;
        let state = self.model.state().clone();
        if state.onboarding.is_some() && !self.past_stub {
            return pages::onboarding_stub(self, t, cx).into_any_element();
        }
        let cols = Columns::of(&state.nav);

        let mut body = div()
            .flex()
            .flex_1()
            .min_h_0()
            .child(chrome::rail(self, &state.nav, t, cx));
        if cols.left.is_some() {
            body = body.child(chrome::left_sidebar(self, &state.nav, t, cx));
        }
        body = body.child(pages::content(self, &state.nav, t, cx));
        if cols.inspector.is_some() {
            body = body.child(chrome::inspector(t));
        }

        div()
            .id("shell")
            .key_context("Shell")
            .track_focus(&self.focus.root)
            .on_action(
                cx.listener(|s, _: &ToggleSidebar, _, cx| {
                    s.dispatch(Action::ToggleLeftSidebar, cx)
                }),
            )
            .on_action(
                cx.listener(|s, _: &ToggleInspector, _, cx| {
                    s.dispatch(Action::ToggleInspector, cx)
                }),
            )
            .on_action(cx.listener(|s, _: &OpenSettings, _, cx| {
                let category = s.model.state().nav.settings_category;
                s.dispatch(Action::OpenSettings(category), cx)
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(c.bg.rgb())
            .child(chrome::header(self, &state.nav, t, cx))
            .child(body)
            .child(pages::toast_stack(self, t, cx))
            .into_any_element()
    }
}

/// A caption line in the secondary text colour.
pub(crate) fn caption(text: impl Into<gpui::SharedString>, t: &Tokens) -> gpui::Div {
    crastu_widgets::label_text(text, t.colors.text_2, t).text_size(px(12.0))
}
