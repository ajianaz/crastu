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
