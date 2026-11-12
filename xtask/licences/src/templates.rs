//! Standard licence texts for crates whose package ships no licence file.
//!
//! Only licences whose standard text is fixed apart from the copyright line are covered. The
//! text starts with a marker saying it was generated, so nobody takes it for the crate's own file.

const TEMPLATES: &[(&str, &str, bool)] = &[
    ("MIT", include_str!("../templates/MIT.txt"), true),
    ("ISC", include_str!("../templates/ISC.txt"), true),
    (
        "BSD-2-Clause",
        include_str!("../templates/BSD-2-Clause.txt"),
        true,
    ),
    (
        "BSD-3-Clause",
        include_str!("../templates/BSD-3-Clause.txt"),
        true,
    ),
    (
        "Apache-2.0",
        include_str!("../templates/Apache-2.0.txt"),
        false,
    ),
];

/// The crate's authors without e-mail addresses.
pub fn author_names(authors: &[String]) -> Vec<String> {
    authors
        .iter()
        .map(|a| a.split('<').next().unwrap_or(a).trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

/// The canonical id and the generated text for a declared licence, or `None` when there is no
/// template for it.
pub fn generate(id: &str, crate_name: &str, authors: &[String]) -> Option<(String, String)> {
    let (canon, body, has_copyright) = TEMPLATES
        .iter()
        .find(|(t, _, _)| t.eq_ignore_ascii_case(id))?;
    let names = author_names(authors);
    let holder = if names.is_empty() {
        format!("the authors of {crate_name}")
    } else {
        names.join(", ")
    };
    let marker = if *has_copyright {
        format!(
            "[Generated from the declared licence ({canon}); the crate ships no licence file.\n \
             The copyright line is built from the authors in the crate's manifest.]"
        )
    } else {
        format!(
            "[Generated from the declared licence ({canon}); the crate ships no licence file.\n \
             This is the standard text, without a copyright line.]"
        )
    };
    Some((
        (*canon).to_string(),
        format!("{marker}\n\n{}", body.replace("{copyright}", &holder)),
    ))
}
