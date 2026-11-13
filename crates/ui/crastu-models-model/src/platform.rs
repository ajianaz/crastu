//! What Settings, Models and licences needs from the app. The desktop app implements it over
//! `crastu-models` and `crastu-models-http`; `crastu-shell-model`'s `Platform` includes it.
//!
//! Ids always come from the catalogue rows, never from text the user typed.

use crate::DownloadEvent;

pub trait ModelsPlatform {
    /// The ids of the catalogue models that are on disk, complete.
    fn installed_models(&self) -> Result<Vec<String>, String>;

    /// Starts a download in the background and returns at once; the progress and the end come
    /// back through [`ModelsPlatform::download_events`]. An error means it did not start.
    fn start_download(&self, id: &str) -> Result<(), String>;

    /// Asks a running download to stop. Its partial file stays for a later resume.
    fn cancel_download(&self, id: &str);

    /// Deletes the model's files from the models folder. Refused while it is downloading.
    fn remove_model(&self, id: &str) -> Result<(), String>;

    /// Takes the events the background downloads produced since the last call. Called on the UI
    /// thread; the downloads never touch the UI.
    fn download_events(&self) -> Vec<DownloadEvent>;

    /// Whether Offline mode is on right now.
    fn is_offline(&self) -> bool;
}
