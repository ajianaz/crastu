use super::*;

fn labels(query: &str) -> Vec<String> {
    let all = entries();
    matches(query, &all)
        .into_iter()
        .map(|i| all[i].label.clone())
        .collect()
}

#[test]
fn every_command_has_an_action_the_app_understands() {
    let all = entries();
    assert!(
        all.iter()
            .any(|e| e.action == Action::Navigate(ModuleId::Settings))
    );
    assert!(
        all.iter()
            .any(|e| e.action == Action::OpenSettings(SettingsCategory::Appearance))
    );
    assert!(
        all.iter()
            .any(|e| e.action == Action::SetTheme(Theme::Paper))
    );
    assert!(
        all.iter()
            .any(|e| e.action == Action::SetDensity(Density::Comfortable))
    );
    // One entry per module that is built in, per Settings category, and the fixed ones.
    let expected =
        ModuleId::available().len() + SettingsCategory::available().len() + 2 + 2 + 3 + 3 + 2;
    assert_eq!(all.len(), expected);
}

#[test]
fn an_empty_query_lists_everything_in_order() {
    let all = entries();
    let m = matches("", &all);
    assert_eq!(m, (0..all.len()).collect::<Vec<_>>());
    assert_eq!(matches("   ", &all).len(), all.len());
}

#[test]
fn every_word_must_match_and_case_does_not_matter() {
    assert_eq!(labels("PAPER"), ["Theme: Paper"]);
    assert_eq!(labels("theme paper"), ["Theme: Paper"]);
    assert!(labels("theme zzz").is_empty());
    assert!(labels("appearance density").contains(&"Density: Compact".to_string()));
}

#[test]
fn a_label_that_starts_with_the_word_comes_first() {
    let found = labels("set");
    // "Settings" (the module) starts with "set"; the Settings categories only have it in the group.
    assert_eq!(found.first().map(String::as_str), Some("Settings"));
    let a = labels("a");
    assert!(
        a.iter().position(|l| l == "Appearance").unwrap()
            < a.iter().position(|l| l == "Toggle the sidebar").unwrap()
    );
}

#[test]
fn typing_narrows_and_resets_the_selection() {
    let all = entries();
    let mut p = Palette::default();
    assert_eq!(p.apply(&PaletteInput::Open, &all), PaletteEffect::None);
    p.apply(&PaletteInput::Key("down".into()), &all);
    p.apply(&PaletteInput::Key("down".into()), &all);
    assert_eq!(p.selected, 2);
    p.apply(&PaletteInput::Edit(Edit::Insert("pa".into())), &all);
    assert_eq!(p.query.text(), "pa");
    assert_eq!(p.selected, 0);
}

#[test]
fn the_selection_stays_in_the_visible_rows() {
    let all = entries();
    let mut p = Palette::default();
    p.apply(&PaletteInput::Open, &all);
    for _ in 0..50 {
        p.apply(&PaletteInput::Key("down".into()), &all);
    }
    assert_eq!(p.selected, MAX_ROWS - 1);
    assert_eq!(p.rows(&all).len(), MAX_ROWS);
    for _ in 0..50 {
        p.apply(&PaletteInput::Key("up".into()), &all);
    }
    assert_eq!(p.selected, 0);
}

#[test]
fn enter_runs_the_selected_command_and_closes() {
    let all = entries();
    let mut p = Palette::default();
    p.apply(&PaletteInput::Open, &all);
    p.apply(&PaletteInput::Edit(Edit::Insert("signal".into())), &all);
    let effect = p.apply(&PaletteInput::Key("enter".into()), &all);
    assert_eq!(effect, PaletteEffect::Run(Action::SetTheme(Theme::Signal)));
    assert!(!p.open);
}

#[test]
fn enter_with_no_match_does_nothing_and_escape_closes() {
    let all = entries();
    let mut p = Palette::default();
    p.apply(&PaletteInput::Open, &all);
    p.apply(&PaletteInput::Edit(Edit::Insert("qqqq".into())), &all);
    assert_eq!(
        p.apply(&PaletteInput::Key("enter".into()), &all),
        PaletteEffect::None
    );
    assert!(p.open, "no match: the palette stays open");
    p.apply(&PaletteInput::Key("escape".into()), &all);
    assert!(!p.open);
}

#[test]
fn a_closed_palette_ignores_typing_and_keys_and_opening_resets_it() {
    let all = entries();
    let mut p = Palette::default();
    assert_eq!(
        p.apply(&PaletteInput::Key("enter".into()), &all),
        PaletteEffect::None
    );
    p.apply(&PaletteInput::Edit(Edit::Insert("x".into())), &all);
    assert!(p.query.is_empty());
    p.apply(&PaletteInput::Open, &all);
    p.apply(&PaletteInput::Edit(Edit::Insert("abc".into())), &all);
    p.apply(&PaletteInput::Close, &all);
    p.apply(&PaletteInput::Open, &all);
    assert!(p.query.is_empty() && p.selected == 0 && p.open);
}

#[test]
fn a_click_runs_that_command_and_closes() {
    let all = entries();
    let mut p = Palette::default();
    p.apply(&PaletteInput::Open, &all);
    let effect = p.apply(&PaletteInput::Choose(Action::ToggleInspector), &all);
    assert_eq!(effect, PaletteEffect::Run(Action::ToggleInspector));
    assert!(!p.open);
    // Not while closed.
    assert_eq!(
        p.apply(&PaletteInput::Choose(Action::ToggleInspector), &all),
        PaletteEffect::None
    );
}

#[test]
fn opening_an_open_palette_keeps_the_query_and_the_selection() {
    let all = entries();
    let mut p = Palette::default();
    p.apply(&PaletteInput::Open, &all);
    p.apply(&PaletteInput::Edit(Edit::Insert("t".into())), &all);
    p.apply(&PaletteInput::Key("down".into()), &all);
    let before = p.clone();
    p.apply(&PaletteInput::Open, &all);
    assert_eq!(p, before);
}

#[test]
fn moving_the_cursor_keeps_the_selected_row() {
    let all = entries();
    let mut p = Palette::default();
    p.apply(&PaletteInput::Open, &all);
    p.apply(&PaletteInput::Edit(Edit::Insert("t".into())), &all);
    p.apply(&PaletteInput::Key("down".into()), &all);
    assert_eq!(p.selected, 1);
    for e in [Edit::Left, Edit::Home, Edit::End, Edit::Right] {
        p.apply(&PaletteInput::Edit(e), &all);
        assert_eq!(p.selected, 1);
    }
    // Typing does reset it.
    p.apply(&PaletteInput::Edit(Edit::Insert("h".into())), &all);
    assert_eq!(p.selected, 0);
}

#[test]
fn offline_commands_are_left_out_while_the_environment_decides() {
    let has = |all: &[Entry]| {
        all.iter()
            .any(|e| matches!(e.action, Action::SetOffline(_)))
    };
    assert!(has(&entries_for(false)));
    assert!(!has(&entries_for(true)));
    assert_eq!(entries_for(true).len() + 2, entries().len());
}
