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
        Ok(ASSETS
            .iter()
            .filter(|(p, _)| p.starts_with(path))
            .map(|(p, _)| (*p).into())
            .collect())
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
