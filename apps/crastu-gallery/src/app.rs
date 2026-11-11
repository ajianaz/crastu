//! The gallery window: the appearance switches, the page tabs and the state each page keeps.

use crastu_app::{Appearance, Density, FontPairing, Theme};
use crastu_controls::{Choice, DialogModel, RangeSlider, SelectModel, Slider, TextModel, Toasts};
use crastu_ui::tokens::Tokens;
use crastu_widgets::{ChoiceKind, Choices, label_text};
use gpui::{
    App, Context, FocusHandle, FontWeight, IntoElement, Render, Window, div, prelude::*, px,
};

use crate::{core_page, feedback_page};

pub struct Gallery {
    pub appearance: Appearance,
    pub installed_fonts: Vec<String>,
    pub page: Choice,
    pub page_focus: FocusHandle,
    pub theme: Choice,
    pub theme_focus: FocusHandle,
    pub font: Choice,
    pub font_focus: FocusHandle,
    pub density: Choice,
    pub density_focus: FocusHandle,
    // Core page.
    pub name: TextModel,
    pub name_focus: FocusHandle,
    pub language: SelectModel,
    pub language_focus: FocusHandle,
    pub ratio: Choice,
    pub ratio_focus: FocusHandle,
    pub tab: Choice,
    pub tab_focus: FocusHandle,
    pub follow_focus: FocusHandle,
    pub filler_focus: FocusHandle,
    pub keep_focus: FocusHandle,
    pub follow: bool,
    pub filler: bool,
    pub keep_loaded: bool,
    pub volume: Slider,
    pub volume_focus: FocusHandle,
    pub clip: RangeSlider,
    pub clip_focus: (FocusHandle, FocusHandle),
    // Feedback page.
    pub toasts: Toasts,
    pub toast_focus: Vec<FocusHandle>,
    pub dialog: DialogModel,
    pub dialog_focus: FocusHandle,
    pub opener_focus: FocusHandle,
    pub dont_ask: bool,
    pub dont_ask_focus: FocusHandle,
}

impl Gallery {
    pub fn new(cx: &mut Context<Self>) -> Gallery {
        let mut toasts = Toasts::default();
        for (kind, text, action) in [
            (
                crastu_controls::ToastKind::Success,
                "3 clips exported to the Clips folder",
                "Show",
            ),
            (
                crastu_controls::ToastKind::Warning,
                "Supertonic uses a conditional licence",
                "Details",
            ),
            (
                crastu_controls::ToastKind::Error,
                "Could not read webinar-budgeting.mov",
                "Retry",
            ),
        ] {
            toasts.push(kind, text, Some(action));
        }
        Gallery {
            appearance: Appearance::default(),
            installed_fonts: cx.text_system().all_font_names(),
            page: Choice::new(2, 0),
            page_focus: cx.focus_handle(),
            theme: Choice::new(3, 0),
            theme_focus: cx.focus_handle(),
            font: Choice::new(3, 0),
            font_focus: cx.focus_handle(),
            density: Choice::new(2, 0),
            density_focus: cx.focus_handle(),
            name: TextModel::new("Podcast #12").with_max_chars(40),
            name_focus: cx.focus_handle(),
            language: SelectModel::new(3, 0),
            language_focus: cx.focus_handle(),
            ratio: Choice::new(3, 0),
            ratio_focus: cx.focus_handle(),
            tab: Choice::new(3, 0),
            tab_focus: cx.focus_handle(),
            follow_focus: cx.focus_handle(),
            filler_focus: cx.focus_handle(),
            keep_focus: cx.focus_handle(),
            follow: true,
            filler: false,
            keep_loaded: true,
            volume: Slider::new(-60.0, 6.0, 0.5, -6.0),
            volume_focus: cx.focus_handle(),
            clip: RangeSlider::new(5.0, 90.0, 1.0, 5.0, 30.0, 60.0),
            clip_focus: (cx.focus_handle(), cx.focus_handle()),
            toasts,
            toast_focus: (0..3).map(|_| cx.focus_handle()).collect(),
            dialog: DialogModel::default(),
            dialog_focus: cx.focus_handle(),
            opener_focus: cx.focus_handle(),
            dont_ask: false,
            dont_ask_focus: cx.focus_handle(),
        }
    }
}

/// A callback for a control: runs `f` on the gallery with the value the control sends.
pub fn hook<V: 'static>(
    cx: &Context<Gallery>,
    f: impl Fn(&mut Gallery, V, &mut Context<Gallery>) + 'static,
) -> impl Fn(V, &mut Window, &mut App) + 'static {
    let this = cx.entity().downgrade();
    move |v, _, app| {
        this.update(app, |g, cx| f(g, v, cx)).ok();
    }
}

/// Like `hook`, for a control that sends no value.
pub fn hook0(
    cx: &Context<Gallery>,
    f: impl Fn(&mut Gallery, &mut Context<Gallery>) + 'static,
) -> impl Fn(&mut Window, &mut App) + 'static {
    let this = cx.entity().downgrade();
    move |_, app| {
        this.update(app, |g, cx| f(g, cx)).ok();
    }
}

pub fn heading(t: &Tokens, text: &str) -> gpui::Div {
    label_text(text.to_string(), t.colors.text, t)
        .text_size(px(16.0))
        .font_weight(FontWeight::SEMIBOLD)
}

pub fn caption(t: &Tokens, text: &str) -> gpui::Div {
    label_text(text.to_string(), t.colors.text_2, t).text_size(px(12.0))
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Tokens::new(&self.appearance, false).with_installed_fonts(&self.installed_fonts);
        let c = t.colors;
        let switch = |id: &'static str, labels: &[&str], choice: &Choice, focus: &FocusHandle| {
            Choices::new(id, ChoiceKind::Segmented, labels, choice, t).track_focus(focus)
        };

        let page_tabs = Choices::new(
            "pages",
            ChoiceKind::Tabs,
            &["Core", "AI and feedback"],
            &self.page,
            t,
        )
        .track_focus(&self.page_focus)
        .on_select(hook(cx, |g, i, cx| {
            g.page.apply(&i);
            cx.notify();
        }));
        let theme = switch(
            "theme",
            &["Graphite", "Paper", "Signal"],
            &self.theme,
            &self.theme_focus,
        )
        .on_select(hook(cx, |g, i, cx| {
            g.theme.apply(&i);
            g.appearance.theme = Theme::ALL[g.theme.selected];
            cx.notify();
        }));
        let font = switch(
            "font",
            &["Inter", "Space Grotesk", "IBM Plex"],
            &self.font,
            &self.font_focus,
        )
        .on_select(hook(cx, |g, i, cx| {
            g.font.apply(&i);
            g.appearance.font = FontPairing::ALL[g.font.selected];
            cx.notify();
        }));
        let density = switch(
            "density",
            &["Compact", "Comfortable"],
            &self.density,
            &self.density_focus,
        )
        .on_select(hook(cx, |g, i, cx| {
            g.density.apply(&i);
            g.appearance.density = [Density::Compact, Density::Comfortable][g.density.selected];
            cx.notify();
        }));

        let page = if self.page.selected == 0 {
            core_page::core_page(self, t, cx)
        } else {
            feedback_page::feedback_page(self, t, cx)
        };

        div()
            .relative()
            .size_full()
            .bg(c.bg.rgb())
            .child(
                div()
                    .id("page")
                    .size_full()
                    .overflow_y_scroll()
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(16.0))
                            .child(heading(&t, "Crastu components"))
                            .child(theme)
                            .child(font)
                            .child(density),
                    )
                    .child(page_tabs)
                    .child(page),
            )
            .children(feedback_page::dialog_overlay(self, t, cx))
    }
}
