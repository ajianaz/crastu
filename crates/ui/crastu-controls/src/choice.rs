//! The state of the choosing controls: segmented control, tabs and the select list. Plain data
//! with the keyboard rules, so the views only draw it.

use crate::nav::{Move, step};

/// One chosen entry out of a row (segmented control, tabs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub enabled: Vec<bool>,
    pub selected: usize,
}

impl Choice {
    pub fn new(count: usize, selected: usize) -> Choice {
        Choice {
            enabled: vec![true; count],
            selected: selected.min(count.saturating_sub(1)),
        }
    }

    /// Chooses `index` unless it is out of range or disabled. True when the choice changed.
    pub fn select(&mut self, index: usize) -> bool {
        let ok = self.enabled.get(index).copied().unwrap_or(false);
        let changed = ok && index != self.selected;
        if ok {
            self.selected = index;
        }
        changed
    }

    /// Left, right, Home and End move the choice (the row does not wrap). True when it changed.
    pub fn key(&mut self, key: &str) -> bool {
        match Move::from_key(key, true) {
            Some(mv) => self.select(step(self.selected, &self.enabled, mv, false)),
            None => false,
        }
    }
}

/// What a key did to a select.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectEvent {
    None,
    Opened,
    Closed,
    /// A different option was chosen (the list closed).
    Chosen(usize),
}

/// A select: a closed trigger showing the chosen option, or the open list with one highlighted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectModel {
    pub enabled: Vec<bool>,
    pub selected: usize,
    pub highlighted: usize,
    pub open: bool,
}

impl SelectModel {
    pub fn new(count: usize, selected: usize) -> SelectModel {
        let selected = selected.min(count.saturating_sub(1));
        SelectModel {
            enabled: vec![true; count],
            selected,
            highlighted: selected,
            open: false,
        }
    }

    pub fn open(&mut self) -> SelectEvent {
        if self.open || self.enabled.is_empty() {
            return SelectEvent::None;
        }
        self.open = true;
        self.highlighted = self.selected;
        SelectEvent::Opened
    }

    pub fn close(&mut self) -> SelectEvent {
        let was = self.open;
        self.open = false;
        if was {
            SelectEvent::Closed
        } else {
            SelectEvent::None
        }
    }

    /// Picks an option (a click, or Enter on the highlighted one). A disabled one is ignored.
    pub fn choose(&mut self, index: usize) -> SelectEvent {
        if !self.enabled.get(index).copied().unwrap_or(false) {
            return SelectEvent::None;
        }
        let changed = index != self.selected;
        self.selected = index;
        self.highlighted = index;
        self.open = false;
        if changed {
            SelectEvent::Chosen(index)
        } else {
            SelectEvent::Closed
        }
    }

    /// Closed: Enter, Space, up and down open the list. Open: up, down, Home and End move the
    /// highlight (no wrap), Enter and Space choose it, Escape and Tab close without choosing.
    pub fn key(&mut self, key: &str) -> SelectEvent {
        if !self.open {
            return match key {
                "enter" | "space" | "down" | "up" => self.open(),
                _ => SelectEvent::None,
            };
        }
        match key {
            "enter" | "space" => self.choose(self.highlighted),
            "escape" | "tab" => self.close(),
            _ => {
                if let Some(mv) = Move::from_key(key, false) {
                    self.highlighted = step(self.highlighted, &self.enabled, mv, false);
                }
                SelectEvent::None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_moves_with_the_arrows_and_skips_disabled_entries() {
        let mut c = Choice::new(4, 0);
        c.enabled[1] = false;
        assert!(c.key("right"));
        assert_eq!(c.selected, 2);
        assert!(c.key("end"));
        assert_eq!(c.selected, 3);
        assert!(!c.key("right"));
        assert!(c.key("home"));
        assert_eq!(c.selected, 0);
        assert!(!c.key("a"));
        assert!(!c.key("down"));
    }

    #[test]
    fn selecting_reports_changes_and_refuses_disabled_or_missing_entries() {
        let mut c = Choice::new(3, 1);
        c.enabled[2] = false;
        assert!(!c.select(1));
        assert!(c.select(0));
        assert!(!c.select(2));
        assert!(!c.select(9));
        assert_eq!(c.selected, 0);
        assert_eq!(Choice::new(3, 9).selected, 2);
    }

    #[test]
    fn the_select_opens_from_the_keyboard_and_highlights_the_chosen_option() {
        let mut s = SelectModel::new(3, 1);
        assert_eq!(s.key("a"), SelectEvent::None);
        assert_eq!(s.key("down"), SelectEvent::Opened);
        assert!(s.open);
        assert_eq!(s.highlighted, 1);
        assert_eq!(s.open(), SelectEvent::None);
    }

    #[test]
    fn the_open_list_moves_chooses_and_closes() {
        let mut s = SelectModel::new(4, 0);
        s.enabled[1] = false;
        s.key("enter");
        assert_eq!(s.key("down"), SelectEvent::None);
        assert_eq!(s.highlighted, 2);
        assert_eq!(s.key("end"), SelectEvent::None);
        assert_eq!(s.highlighted, 3);
        assert_eq!(s.key("enter"), SelectEvent::Chosen(3));
        assert!(!s.open);
        assert_eq!(s.selected, 3);
        // Escape closes without choosing.
        s.key("space");
        s.key("up");
        assert_eq!(s.key("escape"), SelectEvent::Closed);
        assert_eq!(s.selected, 3);
        // Choosing the same option only closes.
        s.key("space");
        assert_eq!(s.key("enter"), SelectEvent::Closed);
    }

    #[test]
    fn a_click_on_a_disabled_option_does_nothing() {
        let mut s = SelectModel::new(3, 0);
        s.enabled[2] = false;
        s.open();
        assert_eq!(s.choose(2), SelectEvent::None);
        assert!(s.open);
        assert_eq!(s.choose(1), SelectEvent::Chosen(1));
        assert_eq!(SelectModel::new(0, 0).open(), SelectEvent::None);
    }
}
