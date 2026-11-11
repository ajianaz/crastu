use super::*;
use crastu_app::Theme;
use crastu_ui::tokens::colors;

const THEMES: [Theme; 3] = [Theme::Graphite, Theme::Paper, Theme::Signal];

#[test]
fn mix_goes_from_one_colour_to_the_other() {
    let c = colors(Theme::Graphite);
    assert_eq!(mix(c.bg, c.text, 0), c.bg);
    assert_eq!(mix(c.bg, c.text, 100), c.text);
    assert_eq!(mix(c.text, c.text, 70), c.text);
    // Halfway is the average of each channel, rounded up.
    let half = mix(c.bg, c.text, 50);
    for shift in [16, 8, 0] {
        let (a, b) = ((c.bg.0 >> shift) & 0xFF, (c.text.0 >> shift) & 0xFF);
        assert_eq!((half.0 >> shift) & 0xFF, (a + b).div_ceil(2));
    }
}

#[test]
fn every_button_state_is_distinct_where_it_should_be_in_every_theme() {
    for theme in THEMES {
        let c = colors(theme);
        for v in Variant::ALL {
            let look = |s| button(v, s, c);
            assert_ne!(
                look(State::Hover).bg,
                look(State::Default).bg,
                "{theme:?} {v:?} hover"
            );
            assert_ne!(
                look(State::Pressed).bg,
                look(State::Hover).bg,
                "{theme:?} {v:?} pressed"
            );
            assert_ne!(
                look(State::Pressed).bg,
                look(State::Default).bg,
                "{theme:?} {v:?} pressed"
            );
            // Only focus shows a ring, only disabled dims.
            for s in State::ALL {
                assert_eq!(
                    look(s).ring.is_some(),
                    s == State::Focus,
                    "{theme:?} {v:?} {s:?}"
                );
                assert_eq!(look(s).dim, s == State::Disabled, "{theme:?} {v:?} {s:?}");
            }
            // Loading and disabled keep the resting fill.
            assert_eq!(look(State::Loading).bg, look(State::Default).bg);
            assert_eq!(look(State::Disabled).bg, look(State::Default).bg);
        }
    }
}

#[test]
fn the_primary_button_uses_the_accent_family_and_danger_the_danger_one() {
    let c = colors(Theme::Graphite);
    assert_eq!(
        button(Variant::Primary, State::Default, c).bg,
        Some(c.accent)
    );
    assert_eq!(
        button(Variant::Primary, State::Hover, c).bg,
        Some(c.accent_hover)
    );
    assert_eq!(
        button(Variant::Primary, State::Pressed, c).bg,
        Some(c.accent_press)
    );
    assert_eq!(
        button(Variant::Primary, State::Focus, c).ring,
        Some(c.accent)
    );
    assert_eq!(
        button(Variant::Danger, State::Default, c).bg,
        Some(c.danger)
    );
    assert_eq!(button(Variant::Danger, State::Default, c).fg, c.on_danger);
    assert_eq!(button(Variant::Ghost, State::Default, c).bg, None);
    assert_eq!(
        button(Variant::Secondary, State::Default, c).border,
        Some(c.border)
    );
}

#[test]
fn button_text_stays_readable_in_every_state_and_theme() {
    // Text and fill must not be the same colour anywhere.
    for theme in THEMES {
        let c = colors(theme);
        for v in Variant::ALL {
            for s in State::ALL {
                let l = button(v, s, c);
                assert_ne!(l.bg, Some(l.fg), "{theme:?} {v:?} {s:?}");
            }
        }
    }
}

#[test]
fn fields_mark_focus_and_error_and_dim_when_disabled() {
    for theme in THEMES {
        let c = colors(theme);
        let f = |s| field(s, c);
        assert_eq!(f(FieldState::Default).border, c.border);
        assert_eq!(f(FieldState::Focus).border, c.accent);
        assert!(f(FieldState::Focus).ring.is_some());
        assert_eq!(f(FieldState::Error).border, c.danger);
        assert_eq!(f(FieldState::Error).help, c.danger);
        assert_eq!(f(FieldState::Default).help, c.text_2);
        assert!(f(FieldState::Disabled).dim);
        assert!(
            FieldState::ALL
                .iter()
                .filter(|s| f(**s).ring.is_some())
                .count()
                == 1
        );
    }
}

#[test]
fn toggles_fill_when_on_and_show_the_ring_only_when_focused_and_enabled() {
    let c = colors(Theme::Graphite);
    let s = |on, focus, disabled| ToggleState {
        on,
        focus,
        disabled,
    };
    assert_eq!(checkbox(s(true, false, false), c).fill, Some(c.accent));
    assert_eq!(checkbox(s(false, false, false), c).fill, None);
    assert!(checkbox(s(false, true, false), c).ring.is_some());
    assert!(checkbox(s(false, true, true), c).ring.is_none());
    assert!(checkbox(s(true, false, true), c).dim);
    assert_eq!(switch(s(true, false, false), c).fill, Some(c.accent));
    assert_eq!(switch(s(false, false, false), c).fill, Some(c.raised));
    assert_ne!(
        switch(s(true, false, false), c).mark,
        switch(s(false, false, false), c).mark
    );
    assert!(switch(s(true, true, false), c).ring.is_some());
}

#[test]
fn the_chosen_segment_and_tab_stand_out() {
    let c = colors(Theme::Paper);
    let on = ToggleState {
        on: true,
        focus: false,
        disabled: false,
    };
    let off = ToggleState { on: false, ..on };
    assert_eq!(segment(on, c).bg, Some(c.raised));
    assert_eq!(segment(off, c).bg, None);
    assert_ne!(segment(on, c).fg, segment(off, c).fg);
    assert_eq!(tab(on, c).border, Some(c.accent));
    assert_eq!(tab(off, c).border, None);
    assert_eq!(option_row(true, true, c).fg, c.accent);
    assert_eq!(option_row(false, true, c).bg, Some(c.raised));
    assert_eq!(option_row(false, false, c).bg, None);
}
