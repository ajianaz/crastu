//! Licence and status badges: one pill per kind, with the colours of the design system.

use crastu_ui::tokens::{Color, Colors};

/// The badges of `DESIGN.md`: every model and engine carries one licence badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    CommercialOk,
    Conditional,
    NonCommercial,
    UserInstalled,
    ProviderTerms,
    Local,
    CloudYourKey,
}

impl Badge {
    pub const ALL: [Badge; 7] = [
        Badge::CommercialOk,
        Badge::Conditional,
        Badge::NonCommercial,
        Badge::UserInstalled,
        Badge::ProviderTerms,
        Badge::Local,
        Badge::CloudYourKey,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Badge::CommercialOk => "Commercial OK",
            Badge::Conditional => "Conditional",
            Badge::NonCommercial => "Non-commercial",
            Badge::UserInstalled => "User-installed",
            Badge::ProviderTerms => "Provider terms",
            Badge::Local => "Local",
            Badge::CloudYourKey => "Cloud, your key",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadgeLook {
    /// A soft fill for the three licence verdicts; the others are outlined.
    pub fill: Option<Color>,
    pub fg: Color,
    pub edge: Color,
}

pub fn badge(kind: Badge, c: &Colors) -> BadgeLook {
    let (fill, fg) = match kind {
        Badge::CommercialOk => (Some(c.accent_soft), c.accent),
        Badge::Conditional => (Some(c.warn_soft), c.warn),
        Badge::NonCommercial => (Some(c.danger_soft), c.danger),
        _ => (None, c.text_2),
    };
    BadgeLook {
        fill,
        fg,
        edge: fill.map_or(c.border, |_| fg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crastu_app::Theme;
    use crastu_ui::tokens::colors;

    #[test]
    fn the_three_verdicts_are_filled_and_the_rest_outlined() {
        for theme in Theme::ALL {
            let c = colors(theme);
            for kind in Badge::ALL {
                let look = badge(kind, c);
                let verdict = matches!(
                    kind,
                    Badge::CommercialOk | Badge::Conditional | Badge::NonCommercial
                );
                assert_eq!(look.fill.is_some(), verdict, "{theme:?} {kind:?}");
                assert_ne!(Some(look.fg), look.fill, "{theme:?} {kind:?}");
            }
        }
    }

    #[test]
    fn verdict_colours_follow_the_design() {
        let c = colors(Theme::Graphite);
        assert_eq!(badge(Badge::CommercialOk, c).fill, Some(c.accent_soft));
        assert_eq!(badge(Badge::Conditional, c).fg, c.warn);
        assert_eq!(badge(Badge::NonCommercial, c).fg, c.danger);
        assert_eq!(badge(Badge::Local, c).edge, c.border);
    }

    #[test]
    fn labels_are_the_design_wording_and_unique() {
        let labels: Vec<_> = Badge::ALL.iter().map(|b| b.label()).collect();
        assert_eq!(labels[0], "Commercial OK");
        assert_eq!(labels[6], "Cloud, your key");
        let mut sorted = labels.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), labels.len());
    }
}
