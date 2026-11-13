use std::cell::{Cell, RefCell};

use super::*;

/// A platform in memory: it records what is asked and hands out the events it was given.
#[derive(Default)]
struct Fake {
    installed: RefCell<Vec<String>>,
    calls: RefCell<Vec<String>>,
    events: RefCell<Vec<DownloadEvent>>,
    offline: Cell<bool>,
    fail_start: RefCell<Option<String>>,
    fail_remove: RefCell<Option<String>>,
    fail_list: Cell<bool>,
}

impl Fake {
    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
    fn push(&self, e: DownloadEvent) {
        self.events.borrow_mut().push(e);
    }
}

impl ModelsPlatform for Fake {
    fn installed_models(&self) -> Result<Vec<String>, String> {
        if self.fail_list.get() {
            return Err("permission denied".into());
        }
        Ok(self.installed.borrow().clone())
    }
    fn start_download(&self, id: &str) -> Result<(), String> {
        self.calls.borrow_mut().push(format!("start {id}"));
        self.fail_start.borrow().clone().map_or(Ok(()), Err)
    }
    fn cancel_download(&self, id: &str) {
        self.calls.borrow_mut().push(format!("cancel {id}"));
    }
    fn remove_model(&self, id: &str) -> Result<(), String> {
        self.calls.borrow_mut().push(format!("remove {id}"));
        self.fail_remove.borrow().clone().map_or(Ok(()), Err)
    }
    fn download_events(&self) -> Vec<DownloadEvent> {
        std::mem::take(&mut *self.events.borrow_mut())
    }
    fn is_offline(&self) -> bool {
        self.offline.get()
    }
}

fn model() -> ModelsModel {
    ModelsModel::new(&crastu_models::catalogue())
}

fn state(m: &ModelsModel, id: &str) -> ModelState {
    m.row(id).unwrap().state.clone()
}

fn progress(id: &str, done: u64, total: Option<u64>) -> DownloadEvent {
    DownloadEvent::Progress {
        id: id.into(),
        done,
        total,
    }
}

const VAD: &str = "silero-vad";
const VAD_SIZE: u64 = 2_327_524;

fn downloading(m: &mut ModelsModel, f: &Fake, id: &str) {
    assert!(m.apply(Intent::Download(id.into()), f).is_empty());
}

#[test]
fn rows_follow_the_catalogue_with_the_licence_badge_rule() {
    let m = model();
    assert_eq!(m.rows.len(), 3);
    let whisper = m.row("whisper-small").unwrap();
    assert_eq!(whisper.kind, "Speech to text");
    assert_eq!(whisper.badge, Badge::CommercialOk);
    assert_eq!(whisper.licence, "MIT");
    assert_eq!(whisper.size_text(), "639.4 MB");
    assert_eq!(whisper.state, ModelState::NotInstalled);
    assert_eq!(whisper.licence_note, None);
}

#[test]
fn a_conditional_entry_gets_the_conditional_badge_and_its_note() {
    let mut e = crastu_models::catalogue().entries()[0].clone();
    e.id = "tts".into();
    e.file_name = "tts.bin".into();
    e.commercial = Commercial::Conditional {
        note: "OpenRAIL-M use restrictions".into(),
    };
    let row = ModelRow::new(&e);
    assert_eq!(row.badge, Badge::Conditional);
    assert_eq!(
        row.licence_note.as_deref(),
        Some("OpenRAIL-M use restrictions")
    );
    e.commercial = Commercial::Allowed;
    assert_eq!(ModelRow::new(&e).badge, Badge::CommercialOk);
}

#[test]
fn refresh_marks_the_installed_models_and_the_summary_adds_their_size() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    assert_eq!(m.summary(), "0 of 3 installed, 0 B on disk");
    assert!(m.refresh(&f).is_empty());
    assert_eq!(state(&m, VAD), ModelState::Installed);
    assert_eq!(state(&m, "yunet"), ModelState::NotInstalled);
    assert_eq!(m.installed_summary(), (1, Some(VAD_SIZE)));
    assert_eq!(m.summary(), "1 of 3 installed, 2.3 MB on disk");
    // Removed behind our back: the next read shows it.
    f.installed.borrow_mut().clear();
    m.refresh(&f);
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
}

#[test]
fn a_failed_read_keeps_the_rows_and_says_why() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    m.refresh(&f);
    f.fail_list.set(true);
    m.refresh(&f);
    assert_eq!(state(&m, VAD), ModelState::Installed);
    assert!(
        m.load_error
            .as_deref()
            .unwrap()
            .contains("permission denied")
    );
    f.fail_list.set(false);
    m.refresh(&f);
    assert_eq!(m.load_error, None);
}

#[test]
fn refresh_does_not_disturb_a_running_download() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    f.installed.borrow_mut().push(VAD.into()); // a stale or half-written file
    m.refresh(&f);
    assert!(matches!(state(&m, VAD), ModelState::Downloading { .. }));
}

#[test]
fn download_starts_the_platform_and_shows_zero_progress() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    assert_eq!(f.calls(), ["start silero-vad"]);
    assert_eq!(
        state(&m, VAD),
        ModelState::Downloading {
            done: 0,
            total: Some(VAD_SIZE),
            cancelling: false
        }
    );
    assert!(m.busy());
    // A second press while it runs does nothing.
    downloading(&mut m, &f, VAD);
    assert_eq!(f.calls().len(), 1);
}

#[test]
fn progress_events_move_the_bar_and_the_last_byte_means_verifying() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    f.push(progress(VAD, VAD_SIZE / 2, Some(VAD_SIZE)));
    let tick = m.tick(&f);
    assert!(tick.changed && tick.effects.is_empty());
    let row = m.row(VAD).unwrap();
    assert_eq!(row.fraction(), Some(0.5));
    assert_eq!(row.status_text(), "Downloading: 1.2 MB of 2.3 MB (50%)");
    f.push(progress(VAD, VAD_SIZE, Some(VAD_SIZE)));
    m.tick(&f);
    assert_eq!(state(&m, VAD), ModelState::Verifying);
    assert!(m.busy());
    assert_eq!(m.row(VAD).unwrap().fraction(), Some(1.0));
    f.push(DownloadEvent::Done { id: VAD.into() });
    m.tick(&f);
    assert_eq!(state(&m, VAD), ModelState::Installed);
    assert!(!m.busy());
}

#[test]
fn a_progress_event_without_a_total_uses_the_catalogue_size() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    f.push(progress(VAD, 1000, None));
    m.tick(&f);
    assert_eq!(
        state(&m, VAD),
        ModelState::Downloading {
            done: 1000,
            total: Some(VAD_SIZE),
            cancelling: false
        }
    );
}

#[test]
fn events_for_a_row_that_is_not_downloading_are_ignored() {
    let f = Fake::default();
    let mut m = model();
    f.push(progress(VAD, 10, Some(VAD_SIZE)));
    f.push(DownloadEvent::Failed {
        id: VAD.into(),
        message: "late".into(),
    });
    f.push(DownloadEvent::Cancelled { id: "nope".into() });
    let tick = m.tick(&f);
    assert!(tick.effects.is_empty());
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
}

#[test]
fn cancel_asks_the_platform_once_and_the_event_ends_it() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    m.apply(Intent::Cancel(VAD.into()), &f);
    m.apply(Intent::Cancel(VAD.into()), &f);
    assert_eq!(f.calls(), ["start silero-vad", "cancel silero-vad"]);
    let row = m.row(VAD).unwrap();
    assert!(row.status_text().starts_with("Cancelling"));
    // Bytes that arrive before the stop do not undo it, even a complete count.
    f.push(progress(VAD, VAD_SIZE, Some(VAD_SIZE)));
    m.tick(&f);
    assert!(matches!(
        state(&m, VAD),
        ModelState::Downloading {
            cancelling: true,
            ..
        }
    ));
    f.push(DownloadEvent::Cancelled { id: VAD.into() });
    m.tick(&f);
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
    // And it can start again.
    downloading(&mut m, &f, VAD);
    assert_eq!(f.calls().last().unwrap(), "start silero-vad");
}

#[test]
fn cancel_does_nothing_for_a_model_that_is_not_downloading() {
    let f = Fake::default();
    let mut m = model();
    m.apply(Intent::Cancel(VAD.into()), &f);
    assert!(f.calls().is_empty());
}

#[test]
fn a_failed_download_shows_the_error_and_can_be_retried() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    f.push(DownloadEvent::Failed {
        id: VAD.into(),
        message: "checksum mismatch".into(),
    });
    let tick = m.tick(&f);
    assert_eq!(
        tick.effects,
        [Effect::Toast("Silero VAD: checksum mismatch".into())]
    );
    assert_eq!(
        state(&m, VAD),
        ModelState::Failed("checksum mismatch".into())
    );
    assert_eq!(
        m.row(VAD).unwrap().status_text(),
        "Failed: checksum mismatch"
    );
    assert!(!m.busy());
    assert!(m.can_download(m.row(VAD).unwrap()));
    downloading(&mut m, &f, VAD);
    assert!(matches!(state(&m, VAD), ModelState::Downloading { .. }));
}

#[test]
fn a_download_that_cannot_start_is_a_failure_with_a_toast() {
    let f = Fake::default();
    *f.fail_start.borrow_mut() = Some("disk full".into());
    let mut m = model();
    let effects = m.apply(Intent::Download(VAD.into()), &f);
    assert_eq!(effects, [Effect::Toast("Silero VAD: disk full".into())]);
    assert_eq!(state(&m, VAD), ModelState::Failed("disk full".into()));
}

mod more;
