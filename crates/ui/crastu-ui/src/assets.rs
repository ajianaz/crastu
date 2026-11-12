//! The images that ship with the app: the rail icons, 24x24 SVGs drawn with a 1.8 px stroke and
//! `currentColor`, so the colour comes from the text tokens. The table is plain data and builds
//! everywhere; on macOS and Windows [`Assets`] serves it to GPUI (`svg().path("icons/mic.svg")`).

/// Every embedded asset: its path as GPUI asks for it, and its bytes.
pub const ASSETS: [(&str, &[u8]); 6] = [
    ("icons/mic.svg", include_bytes!("../assets/icons/mic.svg")),
    ("icons/wave.svg", include_bytes!("../assets/icons/wave.svg")),
    ("icons/cut.svg", include_bytes!("../assets/icons/cut.svg")),
    ("icons/film.svg", include_bytes!("../assets/icons/film.svg")),
    (
        "icons/memory.svg",
        include_bytes!("../assets/icons/memory.svg"),
    ),
    (
        "icons/settings.svg",
        include_bytes!("../assets/icons/settings.svg"),
    ),
];

/// The bytes of the asset at `path`, if one is embedded.
pub fn asset(path: &str) -> Option<&'static [u8]> {
    ASSETS.iter().find(|(p, _)| *p == path).map(|(_, b)| *b)
}

/// The paths of the assets inside the directory `dir` (`"icons"` or `"icons/"`). A directory
/// matches on a whole path segment, so `"icons/m"` lists nothing; the empty string is the root
/// and lists nothing too, as the assets are not at the top level.
pub fn list(dir: &str) -> Vec<&'static str> {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        return Vec::new();
    }
    ASSETS
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| {
            p.strip_prefix(dir)
                .is_some_and(|rest| rest.starts_with('/'))
        })
        .collect()
}

/// The asset source to register with the application (`Application::with_assets`).
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(Debug)]
pub struct Assets;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Ok(asset(path).map(std::borrow::Cow::Borrowed))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        Ok(list(path).into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::layout;

    #[test]
    fn every_asset_is_a_24_grid_svg_with_the_token_stroke() {
        let stroke = format!("stroke-width=\"{}\"", layout::ICON_STROKE);
        for (path, bytes) in ASSETS {
            let svg = std::str::from_utf8(bytes).expect("utf-8");
            assert!(svg.contains("viewBox=\"0 0 24 24\""), "{path}: 24 grid");
            assert!(
                svg.contains(&stroke),
                "{path}: stroke {}",
                layout::ICON_STROKE
            );
            assert!(
                svg.contains("stroke=\"currentColor\""),
                "{path}: currentColor"
            );
            assert!(
                svg.contains("stroke-linecap=\"round\""),
                "{path}: round caps"
            );
            assert!(
                svg.contains("stroke-linejoin=\"round\""),
                "{path}: round joins"
            );
        }
    }

    #[test]
    fn listing_matches_a_directory_not_a_string_prefix() {
        assert_eq!(list("icons").len(), ASSETS.len());
        assert_eq!(list("icons/"), list("icons"));
        assert!(list("icons/m").is_empty());
        assert!(list("icon").is_empty());
        assert!(list("").is_empty());
        assert!(list("/").is_empty());
    }

    #[test]
    fn paths_are_unique_and_lookup_finds_them() {
        for (i, (path, bytes)) in ASSETS.iter().enumerate() {
            assert_eq!(asset(path), Some(*bytes));
            assert!(
                ASSETS[i + 1..].iter().all(|(p, _)| p != path),
                "{path} twice"
            );
        }
        assert_eq!(asset("icons/none.svg"), None);
    }
}
