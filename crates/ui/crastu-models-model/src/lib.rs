//! Settings, Models and licences as plain data: one row per catalogue model with its licence
//! badge and its state, and the rules for each button. The view draws it and sends [`Intent`]s;
//! the owner applies them against the state as it is then and carries out the [`Effect`]s. The
//! platform (the desktop app, over `crastu-models`) does the work behind [`ModelsPlatform`].
//!
//! Downloads run on a background thread of the platform. They never touch this state: they leave
//! [`DownloadEvent`]s that the owner drains on the UI thread with [`ModelsModel::tick`].
//!
//! Offline mode: downloads are disabled with the reason "Offline is on", a download that is
//! running when Offline turns on is cancelled (its partial file stays), and installed models are
//! untouched and stay usable.

use crastu_app::{ModuleId, Nav, SettingsCategory};
use crastu_controls::badge::Badge;
use crastu_controls::{DialogEffect, DialogInput, DialogModel};
use crastu_models::{Catalogue, Commercial, ModelEntry, Task};

mod format;
pub mod platform;
mod row;
#[cfg(test)]
mod tests;

pub use format::{bytes, percent};
pub use platform::ModelsPlatform;
pub use row::{ModelRow, ModelState};

/// The reason shown next to a disabled Download while Offline is on.
pub const OFFLINE_REASON: &str = "Offline is on";

/// The toast for a Download pressed while Offline is on.
const OFFLINE_TOAST: &str = "Offline is on: turn it off in Settings, Privacy to download models.";

/// What a background download reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadEvent {
    /// Bytes on disk so far, out of the total when it is known.
    Progress {
        id: String,
        done: u64,
        total: Option<u64>,
    },
    /// The file is complete, verified and in place.
    Done { id: String },
    /// The download ended with an error (the message says why).
    Failed { id: String, message: String },
    /// The download stopped because it was cancelled; the partial file stays.
    Cancelled { id: String },
}

/// What the view asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Download(String),
    Cancel(String),
    /// Ask before removing an installed model.
    AskRemove(String),
    /// Carry out what the open dialog asks about.
    Confirm,
    Dialog(DialogInput),
    Refresh,
}

/// What the owner has to do after an intent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// An error to show as a toast.
    Toast(String),
    FocusDialog,
    ReturnFocus,
}

/// What a [`ModelsModel::tick`] changed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Tick {
    /// The page needs to be drawn again.
    pub changed: bool,
    pub effects: Vec<Effect>,
}

/// Whether the app is showing Settings, Models and licences.
pub fn is_models_page(nav: &Nav) -> bool {
    nav.active == ModuleId::Settings && nav.settings_category == SettingsCategory::ModelsAndLicences
}

pub fn task_label(task: Task) -> &'static str {
    match task {
        Task::SpeechToText => "Speech to text",
        Task::VoiceActivity => "Voice activity",
        Task::TextToSpeech => "Text to speech",
        Task::FaceDetection => "Face detection",
        Task::Embedding => "Embedding",
    }
}

/// The licence badge of an entry: Commercial OK, or Conditional when the licence has extra
/// terms. (A `Denied` entry cannot be in a loaded catalogue; the badge says so anyway.)
pub fn badge_of(entry: &ModelEntry) -> Badge {
    match entry.commercial {
        Commercial::Allowed => Badge::CommercialOk,
        Commercial::Conditional { .. } => Badge::Conditional,
        Commercial::Denied => Badge::NonCommercial,
    }
}

#[derive(Debug)]
pub struct ModelsModel {
    pub rows: Vec<ModelRow>,
    /// Offline mode as last read from the platform.
    pub offline: bool,
    /// Why the installed models could not be read; the page shows it with a Refresh button.
    pub load_error: Option<String>,
    pub dialog: DialogModel,
    /// The model the open dialog asks about.
    pub confirm: Option<String>,
    on_page: bool,
}

impl ModelsModel {
    /// The rows of the catalogue that ships with Crastu.
    pub fn shipped() -> ModelsModel {
        ModelsModel::new(&crastu_models::catalogue())
    }

    pub fn new(catalogue: &Catalogue) -> ModelsModel {
        ModelsModel {
            rows: catalogue.entries().iter().map(ModelRow::new).collect(),
            offline: false,
            load_error: None,
            dialog: DialogModel::default(),
            confirm: None,
            on_page: false,
        }
    }

    pub fn row(&self, id: &str) -> Option<&ModelRow> {
        self.rows.iter().find(|r| r.id == id)
    }

    fn row_mut(&mut self, id: &str) -> Option<&mut ModelRow> {
        self.rows.iter_mut().find(|r| r.id == id)
    }

    /// Why Download is disabled for a row, or None when it is available.
    pub fn download_blocked(&self, row: &ModelRow) -> Option<&'static str> {
        match row.state {
            ModelState::NotInstalled | ModelState::Failed(_) if self.offline => {
                Some(OFFLINE_REASON)
            }
            _ => None,
        }
    }

    pub fn can_download(&self, row: &ModelRow) -> bool {
        matches!(row.state, ModelState::NotInstalled | ModelState::Failed(_)) && !self.offline
    }

    /// True while any download is running, so the owner keeps ticking.
    pub fn busy(&self) -> bool {
        self.rows.iter().any(|r| r.state.is_running())
    }

    /// How many models are installed and how many bytes they take (as the catalogue states).
    pub fn installed_summary(&self) -> (usize, u64) {
        self.rows
            .iter()
            .filter(|r| r.state == ModelState::Installed)
            .fold((0, 0), |(n, b), r| (n + 1, b + r.size_bytes.unwrap_or(0)))
    }

    /// The line above the list: `1 of 3 installed, 2.3 MB on disk`.
    pub fn summary(&self) -> String {
        let (n, size) = self.installed_summary();
        format!(
            "{n} of {} installed, {} on disk",
            self.rows.len(),
            bytes(size)
        )
    }

    /// The title and the explanation of the open Remove dialog.
    pub fn confirm_text(&self) -> Option<(String, String)> {
        let row = self.row(self.confirm.as_deref()?)?;
        Some((
            format!("Remove {}?", row.name),
            format!(
                "The downloaded file ({}) is deleted from this computer. Whatever uses the model stops working until you download it again.",
                row.size_text()
            ),
        ))
    }

    /// Reads which models are installed. A failure keeps what is shown and says why.
    pub fn refresh(&mut self, platform: &dyn ModelsPlatform) -> Vec<Effect> {
        self.offline = platform.is_offline();
        match platform.installed_models() {
            Ok(ids) => {
                self.load_error = None;
                for row in &mut self.rows {
                    if row.state.is_running() {
                        continue;
                    }
                    let installed = ids.contains(&row.id);
                    match (&row.state, installed) {
                        (_, true) => row.state = ModelState::Installed,
                        (ModelState::Installed, false) => row.state = ModelState::NotInstalled,
                        _ => {}
                    }
                }
                Vec::new()
            }
            Err(why) => {
                self.load_error = Some(format!("Could not read the models folder: {why}"));
                Vec::new()
            }
        }
    }

    /// Called after every app action with whether the page is showing: reads the folder when the
    /// page opens and drops a pending Remove when it closes.
    pub fn sync(&mut self, on_page: bool, platform: &dyn ModelsPlatform) -> Vec<Effect> {
        if on_page == self.on_page {
            return Vec::new();
        }
        self.on_page = on_page;
        if on_page {
            return self.refresh(platform);
        }
        self.confirm = None;
        self.dialog.open = false;
        Vec::new()
    }

    /// Drains the background downloads' events and follows Offline. Called on a timer on the UI
    /// thread.
    pub fn tick(&mut self, platform: &dyn ModelsPlatform) -> Tick {
        let mut tick = Tick::default();
        let offline = platform.is_offline();
        if offline != self.offline {
            self.offline = offline;
            tick.changed = true;
            if offline {
                // Offline refuses every request, also the one that is already running.
                for row in &mut self.rows {
                    if let ModelState::Downloading { cancelling, .. } = &mut row.state
                        && !*cancelling
                    {
                        *cancelling = true;
                        platform.cancel_download(&row.id);
                    }
                }
            }
        }
        for event in platform.download_events() {
            tick.changed = true;
            if let Some(effect) = self.event(event) {
                tick.effects.push(effect);
            }
        }
        tick
    }

    fn event(&mut self, event: DownloadEvent) -> Option<Effect> {
        match event {
            DownloadEvent::Progress { id, done, total } => {
                let row = self.row_mut(&id)?;
                if let ModelState::Downloading { cancelling, .. } = row.state {
                    let total = total.or(row.size_bytes);
                    row.state = if total.is_some_and(|t| done >= t) && !cancelling {
                        ModelState::Verifying
                    } else {
                        ModelState::Downloading {
                            done,
                            total,
                            cancelling,
                        }
                    };
                }
                None
            }
            DownloadEvent::Done { id } => {
                let row = self.row_mut(&id)?;
                row.state = ModelState::Installed;
                None
            }
            DownloadEvent::Failed { id, message } => {
                let row = self.row_mut(&id)?;
                if !row.state.is_running() {
                    return None;
                }
                let toast = format!("{}: {message}", row.name);
                row.state = ModelState::Failed(message);
                Some(Effect::Toast(toast))
            }
            DownloadEvent::Cancelled { id } => {
                let row = self.row_mut(&id)?;
                if row.state.is_running() {
                    row.state = ModelState::NotInstalled;
                }
                None
            }
        }
    }

    pub fn apply(&mut self, intent: Intent, platform: &dyn ModelsPlatform) -> Vec<Effect> {
        match intent {
            Intent::Download(id) => self.download(&id, platform),
            Intent::Cancel(id) => {
                if let Some(row) = self.row_mut(&id)
                    && let ModelState::Downloading { cancelling, .. } = &mut row.state
                    && !*cancelling
                {
                    *cancelling = true;
                    platform.cancel_download(&id);
                }
                Vec::new()
            }
            Intent::AskRemove(id) => {
                if self
                    .row(&id)
                    .is_none_or(|r| r.state != ModelState::Installed)
                {
                    return Vec::new();
                }
                self.confirm = Some(id);
                self.dialog_effects(DialogInput::Open)
            }
            Intent::Confirm => self.confirmed(platform),
            Intent::Dialog(input) => {
                let effects = self.dialog_effects(input);
                if !self.dialog.open {
                    self.confirm = None;
                }
                effects
            }
            Intent::Refresh => self.refresh(platform),
        }
    }

    fn dialog_effects(&mut self, input: DialogInput) -> Vec<Effect> {
        match self.dialog.apply(&input) {
            DialogEffect::None => Vec::new(),
            DialogEffect::FocusDialog => vec![Effect::FocusDialog],
            DialogEffect::ReturnFocus => vec![Effect::ReturnFocus],
        }
    }

    fn download(&mut self, id: &str, platform: &dyn ModelsPlatform) -> Vec<Effect> {
        // Offline may have been switched on since the last tick.
        self.offline = platform.is_offline();
        let Some(row) = self.row(id) else {
            return Vec::new();
        };
        if !matches!(row.state, ModelState::NotInstalled | ModelState::Failed(_)) {
            return Vec::new();
        }
        if self.offline {
            return vec![Effect::Toast(OFFLINE_TOAST.to_string())];
        }
        let (name, total) = (row.name.clone(), row.size_bytes);
        let result = platform.start_download(id);
        let Some(row) = self.row_mut(id) else {
            return Vec::new();
        };
        match result {
            Ok(()) => {
                row.state = ModelState::Downloading {
                    done: 0,
                    total,
                    cancelling: false,
                };
                Vec::new()
            }
            Err(why) => {
                row.state = ModelState::Failed(why.clone());
                vec![Effect::Toast(format!("{name}: {why}"))]
            }
        }
    }

    fn confirmed(&mut self, platform: &dyn ModelsPlatform) -> Vec<Effect> {
        let Some(id) = self.confirm.take().filter(|_| self.dialog.open) else {
            return Vec::new();
        };
        let mut effects = self.dialog_effects(DialogInput::Close);
        let Some(row) = self.row_mut(&id) else {
            return effects;
        };
        // The state may have changed while the dialog was open.
        if row.state != ModelState::Installed {
            return effects;
        }
        let name = row.name.clone();
        match platform.remove_model(&id) {
            Ok(()) => {
                if let Some(row) = self.row_mut(&id) {
                    row.state = ModelState::NotInstalled;
                }
            }
            Err(why) => effects.push(Effect::Toast(format!("Could not remove {name}: {why}"))),
        }
        effects
    }
}
