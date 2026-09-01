//! Embedded assets served to gpui — the app's SVG icons.
//!
//! The icons are the free stroke-rounded set from Hugeicons (hugeicons.com), distributed as
//! `@hugeicons/core-free-icons` under the MIT license, converted to minimal single-color
//! SVGs; gpui's `svg()` element renders them as alpha masks tinted with
//! the element's text color, so they follow the active theme like any text glyph. Bytes are
//! compiled in so the binary stays self-contained.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

macro_rules! icons {
    ($($name:literal),* $(,)?) => {
        const ICONS: &[(&str, &[u8])] = &[
            $((
                concat!("icons/", $name, ".svg"),
                include_bytes!(concat!("../icons/", $name, ".svg")),
            )),*
        ];
    };
}

icons!(
    "arrow-down",
    "arrow-left",
    "arrow-right",
    "clock",
    "computer",
    "full-screen",
    // Filled, unlike the rest of the set — it marks liked state, not an action.
    "heart",
    "mic",
    "music-note",
    "next",
    "paint-board",
    "pause",
    "pin",
    "play",
    "playlist",
    "previous",
    "repeat",
    "repeat-one",
    "search",
    // Sliders rather than a gear — hand-drawn in the same stroke style, like the win-* icons.
    "settings",
    "shuffle",
    "sidebar-left",
    "sparkles",
    "star",
    "volume-high",
    "volume-off",
    // Titlebar caption buttons (hand-drawn in the same stroke style, not Hugeicons —
    // SVGs rather than Segoe glyph codepoints so they render identically on Win10/11).
    "win-close",
    "win-maximize",
    "win-minimize",
    "win-restore",
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
