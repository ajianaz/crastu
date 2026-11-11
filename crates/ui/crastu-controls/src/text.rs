//! The text and cursor of a single-line field, with no drawing. Positions count characters.
//! There is no selection or input-method support yet.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextModel {
    text: String,
    cursor: usize,
    max_chars: Option<usize>,
}

impl TextModel {
    pub fn new(text: &str) -> TextModel {
        TextModel {
            text: text.to_string(),
            cursor: text.chars().count(),
            max_chars: None,
        }
    }

    pub fn with_max_chars(mut self, max: usize) -> TextModel {
        self.max_chars = Some(max);
        self.text = self.text.chars().take(max).collect();
        self.cursor = self.cursor.min(self.len());
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn byte(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map_or(self.text.len(), |(i, _)| i)
    }

    /// Inserts at the cursor. Control characters (and newlines: the field is one line) are
    /// dropped, and so is whatever would go past the character limit.
    pub fn insert(&mut self, s: &str) {
        for c in s.chars().filter(|c| !c.is_control()) {
            if self.max_chars.is_some_and(|m| self.len() >= m) {
                break;
            }
            let at = self.byte(self.cursor);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let at = self.byte(self.cursor - 1);
            self.text.remove(at);
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.len() {
            let at = self.byte(self.cursor);
            self.text.remove(at);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut t = TextModel::new("Podcast #12");
        assert_eq!(t.cursor(), 11);
        t.home();
        t.insert("My ");
        assert_eq!(t.text(), "My Podcast #12");
        assert_eq!(t.cursor(), 3);
    }

    #[test]
    fn backspace_and_delete_stop_at_the_ends() {
        let mut t = TextModel::new("ab");
        t.backspace();
        t.backspace();
        t.backspace();
        assert_eq!(t.text(), "");
        let mut t = TextModel::new("ab");
        t.delete();
        assert_eq!(t.text(), "ab");
        t.home();
        t.delete();
        assert_eq!(t.text(), "b");
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        let mut t = TextModel::new("é日");
        assert_eq!(t.cursor(), 2);
        t.left();
        t.insert("x");
        assert_eq!(t.text(), "éx日");
        t.backspace();
        t.backspace();
        assert_eq!(t.text(), "日");
        t.end();
        t.right();
        assert_eq!(t.cursor(), 1);
    }

    #[test]
    fn control_characters_and_the_limit_are_enforced() {
        let mut t = TextModel::default().with_max_chars(3);
        t.insert("a\nb\tc");
        assert_eq!(t.text(), "abc");
        t.insert("d");
        assert_eq!(t.text(), "abc");
        let t = TextModel::new("abcdef").with_max_chars(2);
        assert_eq!((t.text(), t.cursor()), ("ab", 2));
    }
}
