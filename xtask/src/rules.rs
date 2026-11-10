//! Layering rules for the Crastu workspace, as a pure function over package data.
//!
//! Every workspace crate declares `[package.metadata.crastu] kind = "..."`.
//! Feature crates also declare a `group` (the module they belong to) and may list
//! other groups they are allowed to use in `composite`.

use std::collections::HashMap;
use std::fmt;

/// The layer a crate belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Core,
    Engine,
    Integration,
    Os,
    Feature,
    App,
    Ffi,
    Ui,
    FeatureUi,
    Bin,
    Tool,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "core" => Kind::Core,
            "engine" => Kind::Engine,
            "integration" => Kind::Integration,
            "os" => Kind::Os,
            "feature" => Kind::Feature,
            "app" => Kind::App,
            "ffi" => Kind::Ffi,
            "ui" => Kind::Ui,
            "feature-ui" => Kind::FeatureUi,
            "bin" => Kind::Bin,
            "tool" => Kind::Tool,
            _ => return None,
        })
    }

    /// Crates that may use GPUI.
    fn may_use_gpui(self) -> bool {
        matches!(self, Kind::Ui | Kind::FeatureUi | Kind::Bin)
    }

    /// Crates that must compile for iOS and Android.
    pub fn is_portable(self) -> bool {
        matches!(
            self,
            Kind::Core
                | Kind::Engine
                | Kind::Integration
                | Kind::Os
                | Kind::Feature
                | Kind::App
                | Kind::Ffi
        )
    }

    /// Which kinds this kind may depend on. Feature-to-feature edges are refined by groups.
    fn allowed(self) -> &'static [Kind] {
        use Kind::*;
        match self {
            Core => &[Core],
            Engine | Integration => &[Core, Engine],
            Os => &[Core],
            Feature => &[Core, Engine, Integration, Os, Feature],
            App => &[Core, Engine, Integration, Os, Feature],
            Ffi => &[Core, App],
            Ui | FeatureUi => &[Core, App, Ui],
            Bin | Tool => &[
                Core,
                Engine,
                Integration,
                Os,
                Feature,
                App,
                Ffi,
                Ui,
                FeatureUi,
                Bin,
                Tool,
            ],
        }
    }
}

/// One workspace crate with its declared (normal and build) dependencies.
#[derive(Clone, Debug)]
pub struct Package {
    pub name: String,
    /// `None` when the metadata is missing or unknown.
    pub kind: Option<Kind>,
    pub group: Option<String>,
    pub composite: Vec<String>,
    pub deps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub from: String,
    pub to: String,
    pub reason: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.to.is_empty() {
            write!(f, "{}: {}", self.from, self.reason)
        } else {
            write!(f, "{} -> {}: {}", self.from, self.to, self.reason)
        }
    }
}

fn is_gpui(name: &str) -> bool {
    name == "gpui" || name.starts_with("gpui-") || name.starts_with("gpui_")
}

/// Returns every rule violation, sorted for stable output.
pub fn check(packages: &[Package]) -> Vec<Violation> {
    let by_name: HashMap<&str, &Package> = packages.iter().map(|p| (p.name.as_str(), p)).collect();
    let mut out = Vec::new();

    for p in packages {
        let Some(kind) = p.kind else {
            out.push(Violation {
                from: p.name.clone(),
                to: String::new(),
                reason: "missing or unknown [package.metadata.crastu] kind".into(),
            });
            continue;
        };
        if matches!(kind, Kind::Feature | Kind::FeatureUi) && p.group.is_none() {
            out.push(Violation {
                from: p.name.clone(),
                to: String::new(),
                reason: "feature crates must declare a group".into(),
            });
        }

        for dep in &p.deps {
            if is_gpui(dep) {
                if !kind.may_use_gpui() {
                    out.push(Violation {
                        from: p.name.clone(),
                        to: dep.clone(),
                        reason: "only ui, feature-ui and bin crates may depend on GPUI".into(),
                    });
                }
                continue;
            }
            let Some(target) = by_name.get(dep.as_str()) else {
                continue; // external crate
            };
            let Some(tk) = target.kind else {
                continue; // reported on the target itself
            };
            if !kind.allowed().contains(&tk) {
                out.push(Violation {
                    from: p.name.clone(),
                    to: dep.clone(),
                    reason: format!("{kind:?} crates may not depend on {tk:?} crates"),
                });
                continue;
            }
            if kind == Kind::Feature && tk == Kind::Feature {
                let same = p.group.is_some() && p.group == target.group;
                let declared = target
                    .group
                    .as_ref()
                    .is_some_and(|g| p.composite.contains(g));
                if !same && !declared {
                    out.push(Violation {
                        from: p.name.clone(),
                        to: dep.clone(),
                        reason: format!(
                            "feature modules may not depend on each other; add {:?} to `composite` only if this is a composite module",
                            target.group.clone().unwrap_or_default()
                        ),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(
        name: &str,
        kind: &str,
        group: Option<&str>,
        composite: &[&str],
        deps: &[&str],
    ) -> Package {
        Package {
            name: name.into(),
            kind: Kind::parse(kind),
            group: group.map(Into::into),
            composite: composite.iter().map(|s| s.to_string()).collect(),
            deps: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn base() -> Vec<Package> {
        vec![
            pkg("core", "core", None, &[], &[]),
            pkg("timeline", "feature", Some("editor"), &[], &["core"]),
            pkg(
                "render",
                "feature",
                Some("editor"),
                &[],
                &["core", "timeline"],
            ),
            pkg(
                "clipper",
                "feature",
                Some("clipper"),
                &["editor"],
                &["core", "timeline"],
            ),
            pkg("app", "app", None, &[], &["core", "timeline", "clipper"]),
            pkg("ui", "ui", None, &[], &["gpui-component", "app"]),
            pkg(
                "clipper-ui",
                "feature-ui",
                Some("clipper"),
                &[],
                &["ui", "app"],
            ),
            pkg("desktop", "bin", None, &[], &["clipper-ui", "ui", "app"]),
        ]
    }

    #[test]
    fn valid_graph_passes() {
        assert_eq!(check(&base()), vec![]);
    }

    #[test]
    fn feature_to_other_feature_fails() {
        let mut p = base();
        p.push(pkg(
            "dictation",
            "feature",
            Some("dictation"),
            &[],
            &["core", "clipper"],
        ));
        let v = check(&p);
        assert_eq!(v.len(), 1);
        assert_eq!(
            (v[0].from.as_str(), v[0].to.as_str()),
            ("dictation", "clipper")
        );
    }

    #[test]
    fn domain_crate_using_gpui_fails() {
        let mut p = base();
        p[1].deps.push("gpui".into());
        let v = check(&p);
        assert_eq!(v.len(), 1);
        assert!(v[0].reason.contains("GPUI"));
    }

    #[test]
    fn app_using_gpui_fails() {
        let mut p = base();
        p[4].deps.push("gpui-pre".into());
        assert_eq!(check(&p).len(), 1);
    }

    #[test]
    fn core_depending_upward_fails() {
        let mut p = base();
        p[0].deps.push("app".into());
        let v = check(&p);
        assert_eq!(v.len(), 1);
        assert!(v[0].reason.contains("Core crates may not depend on App"));
    }

    #[test]
    fn ui_depending_on_domain_directly_fails() {
        let mut p = base();
        p[6].deps.push("clipper".into());
        assert_eq!(check(&p).len(), 1);
    }

    #[test]
    fn missing_kind_and_group_are_reported() {
        let mut p = base();
        p.push(pkg("mystery", "nope", None, &[], &[]));
        p.push(pkg("voice", "feature", None, &[], &["core"]));
        let v = check(&p);
        assert_eq!(v.len(), 2);
    }
}
