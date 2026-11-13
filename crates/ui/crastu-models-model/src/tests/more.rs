//! Offline, Remove and the page rules.

use super::*;

#[test]
fn offline_disables_download_with_the_reason_and_starts_nothing() {
    let f = Fake::default();
    f.offline.set(true);
    let mut m = model();
    m.refresh(&f);
    let row = m.row(VAD).unwrap();
    assert!(!m.can_download(row));
    assert_eq!(m.download_blocked(row), Some("Offline is on"));
    let effects = m.apply(Intent::Download(VAD.into()), &f);
    assert!(matches!(&effects[..], [Effect::Toast(t)] if t.contains("Offline is on")));
    assert!(f.calls().is_empty());
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
}

#[test]
fn offline_switched_on_after_the_page_was_read_still_blocks_the_press() {
    let f = Fake::default();
    let mut m = model();
    m.refresh(&f);
    f.offline.set(true);
    m.apply(Intent::Download(VAD.into()), &f);
    assert!(f.calls().is_empty());
    assert!(m.offline);
}

#[test]
fn installed_models_stay_installed_and_removable_while_offline() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    f.offline.set(true);
    let mut m = model();
    m.refresh(&f);
    assert_eq!(state(&m, VAD), ModelState::Installed);
    assert_eq!(m.download_blocked(m.row(VAD).unwrap()), None);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    assert!(m.dialog.open);
    m.apply(Intent::Confirm, &f);
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
}

#[test]
fn turning_offline_on_cancels_the_running_downloads_and_turning_it_off_does_not_restart() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    downloading(&mut m, &f, "yunet");
    f.offline.set(true);
    let tick = m.tick(&f);
    assert!(tick.changed);
    let calls = f.calls();
    assert!(calls.contains(&"cancel silero-vad".to_string()));
    assert!(calls.contains(&"cancel yunet".to_string()));
    f.push(DownloadEvent::Cancelled { id: VAD.into() });
    f.push(DownloadEvent::Cancelled { id: "yunet".into() });
    m.tick(&f);
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
    f.offline.set(false);
    m.tick(&f);
    assert!(!m.offline);
    assert_eq!(f.calls().len(), calls.len(), "nothing restarts by itself");
    // A second tick while Offline stays on does not cancel again.
    f.offline.set(true);
    downloading_while_offline_is_refused(&mut m, &f);
}

fn downloading_while_offline_is_refused(m: &mut ModelsModel, f: &Fake) {
    let before = f.calls().len();
    m.tick(f);
    m.tick(f);
    assert_eq!(f.calls().len(), before);
}

#[test]
fn remove_asks_first_and_only_for_an_installed_model() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    m.refresh(&f);
    m.apply(Intent::AskRemove("yunet".into()), &f);
    assert!(!m.dialog.open && m.confirm.is_none());
    assert_eq!(
        m.apply(Intent::AskRemove(VAD.into()), &f),
        [Effect::FocusDialog]
    );
    assert_eq!(m.confirm.as_deref(), Some(VAD));
    assert!(f.calls().is_empty(), "asking removes nothing");
    let effects = m.apply(Intent::Confirm, &f);
    assert_eq!(effects, [Effect::ReturnFocus]);
    assert_eq!(f.calls(), ["remove silero-vad"]);
    assert_eq!(state(&m, VAD), ModelState::NotInstalled);
    assert!(!m.dialog.open && m.confirm.is_none());
}

#[test]
fn cancelling_or_escaping_the_dialog_removes_nothing() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    m.refresh(&f);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    assert_eq!(
        m.apply(Intent::Dialog(DialogInput::Close), &f),
        [Effect::ReturnFocus]
    );
    assert!(m.confirm.is_none());
    m.apply(Intent::AskRemove(VAD.into()), &f);
    m.apply(Intent::Dialog(DialogInput::Key("escape".into())), &f);
    assert!(f.calls().is_empty());
    // A Confirm with no dialog open does nothing.
    assert!(m.apply(Intent::Confirm, &f).is_empty());
    assert!(f.calls().is_empty());
    assert_eq!(state(&m, VAD), ModelState::Installed);
}

#[test]
fn a_remove_that_fails_keeps_the_model_installed_and_says_why() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    *f.fail_remove.borrow_mut() = Some("in use".into());
    let mut m = model();
    m.refresh(&f);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    let effects = m.apply(Intent::Confirm, &f);
    assert!(effects.contains(&Effect::Toast("Could not remove Silero VAD: in use".into())));
    assert_eq!(state(&m, VAD), ModelState::Installed);
}

#[test]
fn a_model_that_started_downloading_behind_an_open_dialog_is_not_removed() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    m.refresh(&f);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    m.row_mut(VAD).unwrap().state = ModelState::Verifying;
    m.apply(Intent::Confirm, &f);
    assert!(f.calls().is_empty());
}

#[test]
fn leaving_the_page_drops_a_pending_remove_and_entering_reads_the_folder() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    m.sync(true, &f);
    assert_eq!(state(&m, VAD), ModelState::Installed);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    m.sync(false, &f);
    assert!(!m.dialog.open && m.confirm.is_none());
    // Syncing again with the same value reads nothing.
    f.installed.borrow_mut().clear();
    m.sync(false, &f);
    assert_eq!(state(&m, VAD), ModelState::Installed);
}

#[test]
fn download_and_remove_are_not_offered_while_a_download_runs() {
    let f = Fake::default();
    let mut m = model();
    downloading(&mut m, &f, VAD);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    assert!(!m.dialog.open);
    assert!(!m.can_download(m.row(VAD).unwrap()));
    assert_eq!(m.row("yunet").unwrap().state, ModelState::NotInstalled);
}

#[test]
fn unknown_ids_do_nothing() {
    let f = Fake::default();
    let mut m = model();
    m.apply(Intent::Download("../../etc/passwd".into()), &f);
    m.apply(Intent::AskRemove("nope".into()), &f);
    assert!(f.calls().is_empty());
}

#[test]
fn sizes_and_percentages_print_as_the_page_shows_them() {
    assert_eq!(bytes(0), "0 B");
    assert_eq!(bytes(999), "999 B");
    assert_eq!(bytes(232_589), "232.6 KB");
    assert_eq!(bytes(639_387_718), "639.4 MB");
    assert_eq!(bytes(1_500_000_000), "1.5 GB");
    assert_eq!(percent(50, 200), "25%");
    assert_eq!(percent(300, 200), "100%");
    assert_eq!(percent(1, 0), "0%");
}

#[test]
fn the_models_page_is_found_in_the_nav() {
    let mut nav = Nav::default();
    assert!(!is_models_page(&nav));
    nav.active = ModuleId::Settings;
    nav.settings_category = SettingsCategory::ModelsAndLicences;
    assert!(is_models_page(&nav));
    nav.settings_category = SettingsCategory::Agents;
    assert!(!is_models_page(&nav));
}

#[test]
fn the_remove_dialog_names_the_model_and_its_size() {
    let f = Fake::default();
    f.installed.borrow_mut().push(VAD.into());
    let mut m = model();
    assert_eq!(m.confirm_text(), None);
    m.refresh(&f);
    m.apply(Intent::AskRemove(VAD.into()), &f);
    let (title, about) = m.confirm_text().unwrap();
    assert_eq!(title, "Remove Silero VAD?");
    assert!(about.contains("2.3 MB"), "{about}");
}
