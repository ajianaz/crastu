//! One catalogue model as the page shows it.

use crastu_controls::badge::Badge;
use crastu_models::{Commercial, ModelEntry};

use crate::{badge_of, bytes, percent, task_label};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelState {
    NotInstalled,
    Downloading {
        done: u64,
        total: Option<u64>,
        /// Cancel was pressed; the download stops at its next read.
        cancelling: bool,
    },
    /// Every byte is on disk and the checksum is being checked.
    Verifying,
    Installed,
    /// The last download ended with this error; Download tries again and resumes.
    Failed(String),
}

impl ModelState {
    /// A download is in flight (or ending): Download and Remove are not offered.
    pub fn is_running(&self) -> bool {
        matches!(self, ModelState::Downloading { .. } | ModelState::Verifying)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRow {
    pub id: String,
    pub name: String,
    /// What it is for: `Speech to text`, `Voice activity`, ...
    pub kind: &'static str,
    pub size_bytes: Option<u64>,
    pub licence: String,
    pub badge: Badge,
    /// The extra terms of a Conditional licence, shown under the badge.
    pub licence_note: Option<String>,
    pub attribution: Option<String>,
    pub note: Option<String>,
    pub state: ModelState,
}

impl ModelRow {
    pub fn new(entry: &ModelEntry) -> ModelRow {
        ModelRow {
            id: entry.id.clone(),
            name: entry.name.clone(),
            kind: task_label(entry.task),
            size_bytes: entry.size_bytes,
            licence: entry.licence.clone(),
            badge: badge_of(entry),
            licence_note: match &entry.commercial {
                Commercial::Conditional { note } => Some(note.clone()),
                _ => None,
            },
            attribution: entry.attribution.clone(),
            note: entry.note.clone(),
            state: ModelState::NotInstalled,
        }
    }

    /// The size column: `639.4 MB`, or a dash when the catalogue does not say.
    pub fn size_text(&self) -> String {
        self.size_bytes.map_or_else(|| "?".to_string(), bytes)
    }

    /// Progress from 0.0 to 1.0 while downloading (None when the total is unknown).
    pub fn fraction(&self) -> Option<f32> {
        match &self.state {
            ModelState::Downloading {
                done,
                total: Some(t),
                ..
            } if *t > 0 => Some((*done as f64 / *t as f64).clamp(0.0, 1.0) as f32),
            ModelState::Verifying => Some(1.0),
            _ => None,
        }
    }

    /// The status line of the row.
    pub fn status_text(&self) -> String {
        match &self.state {
            ModelState::NotInstalled => "Not installed".to_string(),
            ModelState::Installed => "Installed".to_string(),
            ModelState::Verifying => "Verifying the download".to_string(),
            ModelState::Failed(why) => format!("Failed: {why}"),
            ModelState::Downloading {
                done,
                total,
                cancelling,
            } => {
                let head = if *cancelling {
                    "Cancelling"
                } else {
                    "Downloading"
                };
                match total {
                    Some(t) => format!("{head}: {} of {} ({})", bytes(*done), bytes(*t), {
                        percent(*done, *t)
                    }),
                    None => format!("{head}: {}", bytes(*done)),
                }
            }
        }
    }
}
