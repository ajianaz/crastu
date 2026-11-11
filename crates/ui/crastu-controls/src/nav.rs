//! Moving through a list of choices with the keyboard. Shared by tabs, the segmented control and
//! the select list.

/// A key press that moves the current choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Prev,
    Next,
    First,
    Last,
}

impl Move {
    /// The move a key name stands for: arrows, Home and End. `horizontal` picks left and right,
    /// otherwise up and down. Anything else is not a move.
    pub fn from_key(key: &str, horizontal: bool) -> Option<Move> {
        match (key, horizontal) {
            ("left", true) | ("up", false) => Some(Move::Prev),
            ("right", true) | ("down", false) => Some(Move::Next),
            ("home", _) => Some(Move::First),
            ("end", _) => Some(Move::Last),
            _ => None,
        }
    }
}

/// The index after `mv`, skipping entries that are not enabled. With `wrap` the ends meet,
/// otherwise the move stops at the first or last enabled entry. Stays on `current` when nothing
/// is enabled.
pub fn step(current: usize, enabled: &[bool], mv: Move, wrap: bool) -> usize {
    let len = enabled.len();
    if len == 0 || !enabled.iter().any(|e| *e) {
        return current;
    }
    let first = enabled.iter().position(|e| *e).unwrap_or(current);
    let last = enabled.iter().rposition(|e| *e).unwrap_or(current);
    match mv {
        Move::First => first,
        Move::Last => last,
        Move::Next => {
            let mut i = current;
            for _ in 0..len {
                if i + 1 >= len {
                    if !wrap {
                        return current;
                    }
                    i = 0;
                } else {
                    i += 1;
                }
                if enabled[i] {
                    return i;
                }
            }
            current
        }
        Move::Prev => {
            let mut i = current;
            for _ in 0..len {
                if i == 0 {
                    if !wrap {
                        return current;
                    }
                    i = len - 1;
                } else {
                    i -= 1;
                }
                if enabled[i] {
                    return i;
                }
            }
            current
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [bool; 3] = [true; 3];

    #[test]
    fn moves_by_one_and_wraps_when_asked() {
        assert_eq!(step(0, &ALL, Move::Next, false), 1);
        assert_eq!(step(2, &ALL, Move::Next, false), 2);
        assert_eq!(step(2, &ALL, Move::Next, true), 0);
        assert_eq!(step(0, &ALL, Move::Prev, false), 0);
        assert_eq!(step(0, &ALL, Move::Prev, true), 2);
    }

    #[test]
    fn jumps_to_the_ends() {
        assert_eq!(step(1, &ALL, Move::First, false), 0);
        assert_eq!(step(1, &ALL, Move::Last, false), 2);
    }

    #[test]
    fn skips_disabled_entries() {
        let enabled = [true, false, true, false];
        assert_eq!(step(0, &enabled, Move::Next, false), 2);
        assert_eq!(step(2, &enabled, Move::Next, false), 2);
        assert_eq!(step(2, &enabled, Move::Next, true), 0);
        assert_eq!(step(2, &enabled, Move::Prev, false), 0);
        assert_eq!(step(0, &enabled, Move::Last, false), 2);
        assert_eq!(step(2, &[false, true, true, true], Move::First, false), 1);
    }

    #[test]
    fn nothing_enabled_or_empty_keeps_the_index() {
        assert_eq!(step(1, &[false, false], Move::Next, true), 1);
        assert_eq!(step(0, &[], Move::Next, true), 0);
    }

    #[test]
    fn keys_map_to_moves_by_direction() {
        assert_eq!(Move::from_key("left", true), Some(Move::Prev));
        assert_eq!(Move::from_key("down", false), Some(Move::Next));
        assert_eq!(Move::from_key("down", true), None);
        assert_eq!(Move::from_key("end", false), Some(Move::Last));
        assert_eq!(Move::from_key("a", true), None);
    }
}
