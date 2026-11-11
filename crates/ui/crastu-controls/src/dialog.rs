//! A modal dialog: open or closed, closed by Escape, with the focus handed back to whatever
//! opened it. The view draws it; the owner moves the focus using [`DialogEffect`].

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DialogModel {
    pub open: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogInput {
    Open,
    Close,
    Key(String),
}

/// What the owner has to do with the focus after an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogEffect {
    None,
    /// The dialog opened: move the focus into it.
    FocusDialog,
    /// The dialog closed: give the focus back to what opened it.
    ReturnFocus,
}

impl DialogModel {
    pub fn apply(&mut self, input: &DialogInput) -> DialogEffect {
        match input {
            DialogInput::Open if !self.open => {
                self.open = true;
                DialogEffect::FocusDialog
            }
            DialogInput::Close => self.close(),
            DialogInput::Key(k) if k == "escape" => self.close(),
            _ => DialogEffect::None,
        }
    }

    fn close(&mut self) -> DialogEffect {
        if self.open {
            self.open = false;
            DialogEffect::ReturnFocus
        } else {
            DialogEffect::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_moves_focus_in_and_closing_gives_it_back() {
        let mut d = DialogModel::default();
        assert_eq!(d.apply(&DialogInput::Open), DialogEffect::FocusDialog);
        assert!(d.open);
        assert_eq!(d.apply(&DialogInput::Open), DialogEffect::None);
        assert_eq!(d.apply(&DialogInput::Close), DialogEffect::ReturnFocus);
        assert!(!d.open);
        assert_eq!(d.apply(&DialogInput::Close), DialogEffect::None);
    }

    #[test]
    fn escape_closes_and_other_keys_do_not() {
        let mut d = DialogModel { open: true };
        assert_eq!(d.apply(&DialogInput::Key("a".into())), DialogEffect::None);
        assert!(d.open);
        assert_eq!(
            d.apply(&DialogInput::Key("escape".into())),
            DialogEffect::ReturnFocus
        );
        assert_eq!(
            d.apply(&DialogInput::Key("escape".into())),
            DialogEffect::None
        );
    }
}
