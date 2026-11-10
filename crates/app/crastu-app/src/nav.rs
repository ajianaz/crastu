use serde::{Deserialize, Serialize};

/// Destinations in the rail. Module entries only exist when the module is built in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleId {
    Dictation,
    Voice,
    Clipper,
    Editor,
    Memory,
    Settings,
}

impl ModuleId {
    /// Rail order, filtered to the modules compiled into this build.
    pub fn available() -> Vec<ModuleId> {
        let mut v = Vec::new();
        if cfg!(feature = "dictation") {
            v.push(ModuleId::Dictation);
        }
        if cfg!(feature = "voice") {
            v.push(ModuleId::Voice);
        }
        if cfg!(feature = "clipper") {
            v.push(ModuleId::Clipper);
        }
        if cfg!(feature = "editor") {
            v.push(ModuleId::Editor);
        }
        v.push(ModuleId::Memory);
        v.push(ModuleId::Settings);
        v
    }

    pub fn is_available(self) -> bool {
        Self::available().contains(&self)
    }

    pub fn title(self) -> &'static str {
        match self {
            ModuleId::Dictation => "Dictation",
            ModuleId::Voice => "Voice",
            ModuleId::Clipper => "Auto-clipper",
            ModuleId::Editor => "Editor",
            ModuleId::Memory => "Memory",
            ModuleId::Settings => "Settings",
        }
    }

    /// The short label under the rail icon.
    pub fn rail_label(self) -> &'static str {
        match self {
            ModuleId::Dictation => "Dictate",
            ModuleId::Voice => "Voice",
            ModuleId::Clipper => "Clip",
            ModuleId::Editor => "Edit",
            ModuleId::Memory => "Memory",
            ModuleId::Settings => "Settings",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsCategory {
    Appearance,
    AiProviders,
    ModelsAndLicences,
    VoiceEngines,
    Agents,
    Memory,
    Shortcuts,
    Storage,
    About,
}

impl SettingsCategory {
    /// Sidebar order; Voice engines only appears when the voice module is built in.
    pub fn available() -> Vec<SettingsCategory> {
        use SettingsCategory::*;
        [
            Appearance,
            AiProviders,
            ModelsAndLicences,
            VoiceEngines,
            Agents,
            Memory,
            Shortcuts,
            Storage,
            About,
        ]
        .into_iter()
        .filter(|c| *c != VoiceEngines || cfg!(feature = "voice"))
        .collect()
    }

    pub fn title(self) -> &'static str {
        match self {
            SettingsCategory::Appearance => "Appearance",
            SettingsCategory::AiProviders => "AI providers",
            SettingsCategory::ModelsAndLicences => "Models and licences",
            SettingsCategory::VoiceEngines => "Voice engines",
            SettingsCategory::Agents => "Agents (MCP)",
            SettingsCategory::Memory => "Memory",
            SettingsCategory::Shortcuts => "Shortcuts",
            SettingsCategory::Storage => "Storage",
            SettingsCategory::About => "About",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Nav {
    pub active: ModuleId,
    pub settings_category: SettingsCategory,
    pub left_open: bool,
    pub inspector_open: bool,
}

impl Default for Nav {
    fn default() -> Self {
        Nav {
            active: ModuleId::available()[0],
            settings_category: SettingsCategory::Appearance,
            left_open: true,
            inspector_open: true,
        }
    }
}

impl Nav {
    /// Ignores modules that are not built in.
    pub(crate) fn navigate(&mut self, m: ModuleId) {
        if m.is_available() {
            self.active = m;
        }
    }

    pub(crate) fn open_settings(&mut self, c: SettingsCategory) {
        if SettingsCategory::available().contains(&c) {
            self.active = ModuleId::Settings;
            self.settings_category = c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_follows_the_build() {
        let a = ModuleId::available();
        assert_eq!(
            a.contains(&ModuleId::Dictation),
            cfg!(feature = "dictation")
        );
        assert_eq!(a.contains(&ModuleId::Editor), cfg!(feature = "editor"));
        assert_eq!(a.last(), Some(&ModuleId::Settings));
        assert_eq!(
            SettingsCategory::available().contains(&SettingsCategory::VoiceEngines),
            cfg!(feature = "voice")
        );
    }

    #[test]
    fn navigation_ignores_missing_modules() {
        let mut nav = Nav::default();
        nav.navigate(ModuleId::Memory);
        assert_eq!(nav.active, ModuleId::Memory);
        nav.navigate(ModuleId::Editor);
        let expected = if cfg!(feature = "editor") {
            ModuleId::Editor
        } else {
            ModuleId::Memory
        };
        assert_eq!(nav.active, expected);
        nav.open_settings(SettingsCategory::Agents);
        assert_eq!(
            (nav.active, nav.settings_category),
            (ModuleId::Settings, SettingsCategory::Agents)
        );
    }
}
