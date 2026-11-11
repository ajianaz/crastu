//! Toasts: a short message with an optional action. Success and warning go away by themselves;
//! an error stays until it is dismissed, because it asks for something.

use crastu_ui::tokens::{Color, Colors};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Warning,
    Error,
}

impl ToastKind {
    /// The colour of the icon.
    pub fn color(self, c: &Colors) -> Color {
        match self {
            ToastKind::Success => c.accent,
            ToastKind::Warning => c.warn,
            ToastKind::Error => c.danger,
        }
    }

    /// A one-character mark, so the kind never depends on colour alone.
    pub fn mark(self) -> &'static str {
        match self {
            ToastKind::Success => "\u{2713}",
            ToastKind::Warning => "\u{26A0}",
            ToastKind::Error => "\u{2715}",
        }
    }

    /// How long it stays, in milliseconds; None for one that stays until dismissed.
    pub fn lifetime_ms(self) -> Option<u32> {
        match self {
            ToastKind::Success => Some(4_000),
            ToastKind::Warning => Some(8_000),
            ToastKind::Error => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub message: String,
    /// The label of the action ("Show", "Details", "Retry").
    pub action: Option<String>,
    age_ms: u32,
}

/// The toasts on screen, newest last, at most `MAX_VISIBLE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Toasts {
    items: Vec<Toast>,
    next_id: u64,
}

pub const MAX_VISIBLE: usize = 3;

impl Toasts {
    pub fn items(&self) -> &[Toast] {
        &self.items
    }

    /// Shows a toast and returns its id. When the screen is full the oldest one that is not
    /// an error goes first, and only if all are errors, the oldest.
    pub fn push(&mut self, kind: ToastKind, message: &str, action: Option<&str>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        if self.items.len() == MAX_VISIBLE {
            let at = self
                .items
                .iter()
                .position(|t| t.kind != ToastKind::Error)
                .unwrap_or(0);
            self.items.remove(at);
        }
        self.items.push(Toast {
            id,
            kind,
            message: message.to_string(),
            action: action.map(str::to_string),
            age_ms: 0,
        });
        id
    }

    /// True when the toast was there.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|t| t.id != id);
        self.items.len() != before
    }

    /// Lets time pass and removes the toasts whose time is up.
    pub fn tick(&mut self, elapsed_ms: u32) {
        for t in &mut self.items {
            t.age_ms = t.age_ms.saturating_add(elapsed_ms);
        }
        self.items
            .retain(|t| t.kind.lifetime_ms().is_none_or(|life| t.age_ms < life));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_and_warning_expire_and_an_error_stays() {
        let mut t = Toasts::default();
        t.push(ToastKind::Success, "Exported", Some("Show"));
        t.push(ToastKind::Warning, "Conditional licence", Some("Details"));
        t.push(ToastKind::Error, "Could not read", Some("Retry"));
        t.tick(3_999);
        assert_eq!(t.items().len(), 3);
        t.tick(1);
        assert_eq!(t.items().len(), 2);
        t.tick(4_000);
        assert_eq!(t.items().len(), 1);
        t.tick(1_000_000);
        assert_eq!(t.items()[0].kind, ToastKind::Error);
    }

    #[test]
    fn dismiss_removes_one_by_id() {
        let mut t = Toasts::default();
        let a = t.push(ToastKind::Error, "a", None);
        let b = t.push(ToastKind::Error, "b", None);
        assert!(t.dismiss(a));
        assert!(!t.dismiss(a));
        assert_eq!(t.items()[0].id, b);
    }

    #[test]
    fn a_full_screen_drops_the_oldest_non_error_first() {
        let mut t = Toasts::default();
        t.push(ToastKind::Error, "keep", None);
        t.push(ToastKind::Success, "old", None);
        t.push(ToastKind::Success, "newer", None);
        t.push(ToastKind::Warning, "newest", None);
        let messages: Vec<_> = t.items().iter().map(|i| i.message.as_str()).collect();
        assert_eq!(messages, ["keep", "newer", "newest"]);
        let mut all_errors = Toasts::default();
        for m in ["1", "2", "3", "4"] {
            all_errors.push(ToastKind::Error, m, None);
        }
        assert_eq!(all_errors.items()[0].message, "2");
    }

    #[test]
    fn ids_are_never_reused() {
        let mut t = Toasts::default();
        let a = t.push(ToastKind::Success, "a", None);
        t.dismiss(a);
        assert_ne!(t.push(ToastKind::Success, "b", None), a);
    }
}
