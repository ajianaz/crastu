//! The frame around the content: header, rail, left sidebar and inspector.

use crastu_app::{Action, ModuleId, Nav, SettingsCategory};
use crastu_ui::tokens::{Tokens, layout};
use crastu_widgets::{Button, Kbd, label_text};
use gpui::{
    App, Context, ElementId, FocusHandle, FontWeight, IntoElement, RenderOnce, SharedString,
    Window, div, prelude::*, px,
};

use super::{Dispatch, Shell, caption};
use crate::layout::{breadcrumb, rail_modules};
use crate::palette::PaletteInput;

/// A destination: a rail entry or a Settings category. Draws the active one on a raised chip
/// with an accent edge, and a focus ring from the keyboard.
#[derive(IntoElement)]
struct NavItem {
    id: ElementId,
    label: SharedString,
    active: bool,
    compact: bool,
    tokens: Tokens,
    focus: Option<FocusHandle>,
    on_click: Dispatch,
    action: Action,
}

impl RenderOnce for NavItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let c = self.tokens.colors;
        let (hover_bg, ring) = (c.raised, c.accent);
        let mut el = div()
            .id(self.id)
            .flex()
            .items_center()
            .rounded(px(layout::RADIUS_CONTROL))
            .border_l_2()
            .border_color(if self.active {
                c.accent.rgb().into()
            } else {
                gpui::transparent_black()
            })
            .font_family(self.tokens.fonts.ui)
            .text_color(if self.active {
                c.text.rgb()
            } else {
                c.text_2.rgb()
            })
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg.rgb()))
            .focus_visible(move |s| s.border_color(ring.rgb()))
            .child(self.label);
        el = if self.compact {
            el.h(px(32.0)).px(px(12.0)).text_size(px(13.0))
        } else {
            el.h(px(44.0))
                .w(px(56.0))
                .justify_center()
                .text_size(px(11.0))
        };
        if self.active {
            el = el.bg(c.raised.rgb()).font_weight(FontWeight::MEDIUM);
        }
        if let Some(h) = &self.focus {
            el = el.track_focus(h).tab_stop(true);
        }
        let (send, action) = (self.on_click, self.action);
        el.on_click(move |_, _, cx| send(action.clone(), cx))
    }
}

pub fn header(shell: &Shell, nav: &Nav, t: Tokens, cx: &mut Context<Shell>) -> gpui::Div {
    let c = t.colors;
    let send = Shell::dispatcher(cx);
    let (s1, s2) = (send.clone(), send);
    let crumbs = breadcrumb(nav).join("  /  ");
    let sidebar = Button::ghost(
        "toggle-sidebar",
        if nav.left_open {
            "Hide sidebar"
        } else {
            "Show sidebar"
        },
        t,
    )
    .track_focus(&shell.focus.sidebar_toggle)
    .on_click(move |_, _, cx| s1(Action::ToggleLeftSidebar, cx));
    let inspector = Button::ghost(
        "toggle-inspector",
        if nav.inspector_open {
            "Hide inspector"
        } else {
            "Show inspector"
        },
        t,
    )
    .track_focus(&shell.focus.inspector_toggle)
    .on_click(move |_, _, cx| s2(Action::ToggleInspector, cx));
    // The search box opens the command palette; searching memory comes with memory (P1). The MCP
    // status is a stub until the host status exists.
    let open_palette = Shell::palette_sender(cx);
    let hover_bg = c.raised;
    let search = div()
        .id("search")
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .w(px(280.0))
        .h(px(28.0))
        .px(px(10.0))
        .rounded(px(layout::RADIUS_CONTROL))
        .border_1()
        .border_color(c.border.rgb())
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg.rgb()))
        .on_click(move |_, window, app| open_palette(PaletteInput::Open, window, app))
        .child(label_text("Search commands", c.text_3, &t))
        .child(Kbd::new("\u{2318}K", t));
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .h(px(layout::SHELL_HEADER))
        .px(px(16.0))
        .border_b_1()
        .border_color(c.border.rgb())
        .child(
            div()
                .flex_1()
                .child(label_text(crumbs, c.text, &t).font_weight(FontWeight::MEDIUM)),
        )
        .child(search)
        .child(label_text("MCP: not connected", c.text_3, &t).text_size(px(12.0)))
        .child(sidebar)
        .child(inspector)
}

pub fn rail(shell: &Shell, nav: &Nav, t: Tokens, cx: &mut Context<Shell>) -> gpui::Div {
    let c = t.colors;
    let send = Shell::dispatcher(cx);
    let item = |i: usize, m: ModuleId| NavItem {
        id: ElementId::NamedInteger("rail".into(), i as u64),
        label: m.rail_label().into(),
        active: nav.active == m,
        compact: false,
        tokens: t,
        focus: shell.focus.rail.get(i).cloned(),
        on_click: send.clone(),
        action: Action::Navigate(m),
    };
    let mut top = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.0))
        .pt(px(8.0));
    let modules = rail_modules();
    for (i, m) in modules.iter().enumerate() {
        top = top.child(item(i, *m));
    }
    div()
        .flex()
        .flex_col()
        .justify_between()
        .items_center()
        .w(px(layout::SHELL_RAIL))
        .pb(px(8.0))
        .bg(c.surface.rgb())
        .border_r_1()
        .border_color(c.border.rgb())
        .child(top)
        .child(item(modules.len(), ModuleId::Settings))
}

pub fn left_sidebar(shell: &Shell, nav: &Nav, t: Tokens, cx: &mut Context<Shell>) -> gpui::Div {
    let c = t.colors;
    let send = Shell::dispatcher(cx);
    let mut col = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .w(px(layout::SHELL_LEFT_SIDEBAR))
        .p(px(12.0))
        .bg(c.surface.rgb())
        .border_r_1()
        .border_color(c.border.rgb());
    col = col.child(
        label_text(nav.active.title(), c.text, &t)
            .font_weight(FontWeight::SEMIBOLD)
            .pb(px(8.0)),
    );
    if nav.active == ModuleId::Settings {
        for (i, cat) in SettingsCategory::available().into_iter().enumerate() {
            col = col.child(NavItem {
                id: ElementId::NamedInteger("category".into(), i as u64),
                label: cat.title().into(),
                active: nav.settings_category == cat,
                compact: true,
                tokens: t,
                focus: shell.focus.categories.get(i).cloned(),
                on_click: send.clone(),
                action: Action::OpenSettings(cat),
            });
        }
    } else {
        col = col.child(caption(
            "The sources and sections of this module appear here when its screen is built.",
            &t,
        ));
    }
    col
}

pub fn inspector(t: Tokens) -> gpui::Div {
    let c = t.colors;
    div()
        .flex()
        .flex_col()
        .w(px(layout::SHELL_INSPECTOR))
        .p(px(16.0))
        .bg(c.surface.rgb())
        .border_l_1()
        .border_color(c.border.rgb())
        .child(
            label_text("Inspector", c.text, &t)
                .font_weight(FontWeight::SEMIBOLD)
                .pb(px(8.0)),
        )
        .child(caption("Nothing is selected.", &t))
}
