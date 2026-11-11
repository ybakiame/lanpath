//! Compile-time product identity generated from the independent GPUI metadata.
//! Stable Tauri metadata is intentionally not an input.
include!(concat!(env!("OUT_DIR"), "/product_constants.rs"));

/// Window copy is separate from the experimental distribution identity.
pub(crate) fn window_title() -> &'static str {
    DISPLAY_NAME.strip_suffix(" GPUI").unwrap_or(DISPLAY_NAME)
}
pub(crate) const SLOGAN: &str = "循径而行，径达悠远";

/// Preserve the product mark while retaining GPUI Kit's component/icon bundle.
pub(crate) struct ProductAssets;

impl gpui_kit::AssetSource for ProductAssets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if path == "brand/lanpath.svg" {
            return Ok(Some(std::borrow::Cow::Borrowed(include_bytes!(
                "../../client/public/brand/lanpath.svg"
            ))));
        }
        gpui_kit::AssetSource::load(&gpui_kit::assets::AllAssets, path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<gpui_kit::SharedString>> {
        let mut assets = gpui_kit::AssetSource::list(&gpui_kit::assets::AllAssets, path)?;
        if "brand/lanpath.svg".starts_with(path) {
            assets.push("brand/lanpath.svg".into());
        }
        Ok(assets)
    }
}
