//! What the shell shows for a navigation state, as plain data: which columns are open and how
//! wide, the rail entries and the breadcrumb. The GPUI views only draw it.

use crastu_app::{ModuleId, Nav};
use crastu_ui::tokens::layout;

/// The column widths of the main window; a collapsed sidebar or inspector has none.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Columns {
    pub header: f32,
    pub rail: f32,
    pub left: Option<f32>,
    pub inspector: Option<f32>,
}

impl Columns {
    pub fn of(nav: &Nav) -> Columns {
        Columns {
            header: layout::SHELL_HEADER,
            rail: layout::SHELL_RAIL,
            left: nav.left_open.then_some(layout::SHELL_LEFT_SIDEBAR),
            inspector: nav.inspector_open.then_some(layout::SHELL_INSPECTOR),
        }
    }

    /// The width taken by everything except the content, for a window `width` wide; the content
    /// gets the rest.
    pub fn content_width(&self, width: f32) -> f32 {
        (width - self.rail - self.left.unwrap_or(0.0) - self.inspector.unwrap_or(0.0)).max(0.0)
    }
}

/// The modules in the rail, in order. Settings is pinned at the bottom, apart from these.
pub fn rail_modules() -> Vec<ModuleId> {
    ModuleId::available()
        .into_iter()
        .filter(|m| *m != ModuleId::Settings)
        .collect()
}

/// A rail icon. Each one is an SVG embedded in `crastu-ui` (`crastu_ui::assets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Mic,
    Wave,
    Cut,
    Film,
    Memory,
    Settings,
}

impl Icon {
    pub const ALL: [Icon; 6] = [
        Icon::Mic,
        Icon::Wave,
        Icon::Cut,
        Icon::Film,
        Icon::Memory,
        Icon::Settings,
    ];

    /// The asset path GPUI loads the icon from.
    pub fn path(self) -> &'static str {
        match self {
            Icon::Mic => "icons/mic.svg",
            Icon::Wave => "icons/wave.svg",
            Icon::Cut => "icons/cut.svg",
            Icon::Film => "icons/film.svg",
            Icon::Memory => "icons/memory.svg",
            Icon::Settings => "icons/settings.svg",
        }
    }
}

/// The icon of a module, as the design boards draw them.
pub fn icon_for(module: ModuleId) -> Icon {
    match module {
        ModuleId::Dictation => Icon::Mic,
        ModuleId::Voice => Icon::Wave,
        ModuleId::Clipper => Icon::Cut,
        ModuleId::Editor => Icon::Film,
        ModuleId::Memory => Icon::Memory,
        ModuleId::Settings => Icon::Settings,
    }
}

/// Where the user is: the module, and for Settings the category.
pub fn breadcrumb(nav: &Nav) -> Vec<&'static str> {
    match nav.active {
        ModuleId::Settings => vec![ModuleId::Settings.title(), nav.settings_category.title()],
        m => vec![m.title()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crastu_app::SettingsCategory;

    // Not `ModuleId::available()`, which depends on the build's features.
    const ALL_MODULES: [ModuleId; 6] = [
        ModuleId::Dictation,
        ModuleId::Voice,
        ModuleId::Clipper,
        ModuleId::Editor,
        ModuleId::Memory,
        ModuleId::Settings,
    ];

    #[test]
    fn collapsed_columns_have_no_width_and_the_content_takes_the_rest() {
        let mut nav = Nav::default();
        let open = Columns::of(&nav);
        assert_eq!(open.left, Some(236.0));
        assert_eq!(open.inspector, Some(316.0));
        assert_eq!(open.content_width(1280.0), 1280.0 - 64.0 - 236.0 - 316.0);
        nav.left_open = false;
        nav.inspector_open = false;
        let closed = Columns::of(&nav);
        assert_eq!((closed.left, closed.inspector), (None, None));
        assert_eq!(closed.content_width(1280.0), 1280.0 - 64.0);
        assert_eq!(closed.header, 44.0);
    }

    #[test]
    fn a_narrow_window_never_gives_the_content_a_negative_width() {
        assert_eq!(Columns::of(&Nav::default()).content_width(300.0), 0.0);
    }

    #[test]
    fn the_rail_lists_modules_without_settings() {
        let rail = rail_modules();
        assert!(!rail.contains(&ModuleId::Settings));
        assert!(rail.contains(&ModuleId::Memory));
        assert_eq!(rail.len(), ModuleId::available().len() - 1);
    }

    #[test]
    fn every_module_has_an_icon_and_every_icon_an_embedded_asset() {
        for m in ALL_MODULES {
            assert!(Icon::ALL.contains(&icon_for(m)), "{m:?}");
        }
        for icon in Icon::ALL {
            assert!(crastu_ui::assets::asset(icon.path()).is_some(), "{icon:?}");
        }
        // No embedded icon is left without a name in the enum.
        for (path, _) in crastu_ui::assets::ASSETS {
            assert!(Icon::ALL.iter().any(|i| i.path() == path), "{path}");
        }
    }

    #[test]
    fn modules_have_different_icons() {
        let mut seen = Vec::new();
        for m in ALL_MODULES {
            let icon = icon_for(m);
            assert!(!seen.contains(&icon), "{icon:?} used twice");
            seen.push(icon);
        }
    }

    #[test]
    fn the_breadcrumb_names_the_module_and_the_settings_category() {
        let mut nav = Nav {
            active: ModuleId::Memory,
            ..Nav::default()
        };
        assert_eq!(breadcrumb(&nav), ["Memory"]);
        nav.active = ModuleId::Settings;
        nav.settings_category = SettingsCategory::Agents;
        assert_eq!(breadcrumb(&nav), ["Settings", "Agents (MCP)"]);
    }
}
