//! GPUI bindings for the neutral theme types.
//!
//! The mirror of `spotify-tui`'s ratatui extension trait: [`echo_core::theme::ThemeColor`]
//! resolves to a [`gpui::Hsla`]. Named ANSI colors map through a fixed palette (the terminal's
//! palette isn't available — or meaningful — in a window), and `Reset` falls back to a slot-
//! appropriate default supplied by the caller.

use echo_core::theme::{NamedColor, ThemeColor};
use gpui::{Hsla, Rgba};

fn rgb8(r: u8, g: u8, b: u8) -> Hsla {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

/// The VGA-ish palette used for named theme colors. Chosen to read well on the dark window
/// background rather than to match any particular terminal.
fn named(color: NamedColor) -> Hsla {
    use NamedColor::*;
    match color {
        Black => rgb8(0x00, 0x00, 0x00),
        Red => rgb8(0xcd, 0x31, 0x31),
        Green => rgb8(0x0d, 0xbc, 0x79),
        Yellow => rgb8(0xe5, 0xe5, 0x10),
        Blue => rgb8(0x24, 0x72, 0xc8),
        Magenta => rgb8(0xbc, 0x3f, 0xbc),
        Cyan => rgb8(0x11, 0xa8, 0xcd),
        Gray => rgb8(0xa0, 0xa0, 0xa0),
        DarkGray => rgb8(0x66, 0x66, 0x66),
        LightRed => rgb8(0xf1, 0x4c, 0x4c),
        LightGreen => rgb8(0x23, 0xd1, 0x8b),
        LightYellow => rgb8(0xf5, 0xf5, 0x43),
        LightBlue => rgb8(0x3b, 0x8e, 0xea),
        LightMagenta => rgb8(0xd6, 0x70, 0xd6),
        LightCyan => rgb8(0x29, 0xb8, 0xdb),
        White => rgb8(0xe5, 0xe5, 0xe5),
    }
}

/// The xterm-256 palette for indexed colors.
fn indexed(index: u8) -> Hsla {
    match index {
        0..=15 => {
            use NamedColor::*;
            const ANSI: [NamedColor; 16] = [
                Black, Red, Green, Yellow, Blue, Magenta, Cyan, Gray, DarkGray, LightRed,
                LightGreen, LightYellow, LightBlue, LightMagenta, LightCyan, White,
            ];
            named(ANSI[index as usize])
        }
        16..=231 => {
            // 6x6x6 color cube.
            let i = index - 16;
            let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            let r = step(i / 36);
            let g = step((i % 36) / 6);
            let b = step(i % 6);
            rgb8(r, g, b)
        }
        232..=255 => {
            // Grayscale ramp.
            let v = 8 + (index - 232) * 10;
            rgb8(v, v, v)
        }
    }
}

/// Converts a neutral theme color to gpui's, with `fallback` standing in for `Reset`.
pub trait ToGpui {
    fn gpui(self, fallback: Hsla) -> Hsla;
}

impl ToGpui for ThemeColor {
    fn gpui(self, fallback: Hsla) -> Hsla {
        match self {
            ThemeColor::Reset => fallback,
            ThemeColor::Named(name) => named(name),
            ThemeColor::Indexed(index) => indexed(index),
            ThemeColor::Rgb(r, g, b) => rgb8(r, g, b),
        }
    }
}

/// The window's own idea of "default background" / "default text", used where a theme says
/// `Reset`.
pub const WINDOW_BG: fn() -> Hsla = || rgb8(0x12, 0x12, 0x12);
pub const WINDOW_FG: fn() -> Hsla = || rgb8(0xe5, 0xe5, 0xe5);
/// Fallback for the theme `surface` slot (menus/modals) when a theme doesn't define one:
/// a step lighter than the window background so panels still separate from the content.
pub const PANEL_BG: fn() -> Hsla = || rgb8(0x1f, 0x1f, 0x22);

/// The Windows close-caption red (system convention, not themed).
pub const CLOSE_RED: fn() -> Hsla = || rgb8(0xe8, 0x11, 0x23);

/// Alpha-composites `color` at `alpha` over `under`, exactly as gpui does when painting a
/// translucent quad: plain sRGB-channel interpolation (verified against on-screen pixels).
/// Used to turn the old runtime `.opacity()` washes into concrete colors.
pub fn blend(color: Hsla, under: Hsla, alpha: f32) -> Hsla {
    let c = Rgba::from(color);
    let u = Rgba::from(under);
    Rgba {
        r: c.r * alpha + u.r * (1.0 - alpha),
        g: c.g * alpha + u.g * (1.0 - alpha),
        b: c.b * alpha + u.b * (1.0 - alpha),
        a: 1.0,
    }
    .into()
}

/// Every derived color the desktop paints, resolved to a concrete opaque value.
///
/// One slot per semantic role — selection, hover, muted fill, border, accent hover, accent
/// selection — once over the window `background` and once over the elevated `surface`, plus
/// the heart, error and danger tints. Each slot has a formula (base theme slot, mix strength,
/// underlay) used when the theme file doesn't spell the color out. A theme's `[desktop]`
/// table overrides any slot by name; the bundled themes list every slot explicitly (values
/// generated with `themes/generate_desktop.py`), so editing the file is the whole story for
/// them.
///
/// Field order mirrors the `[desktop]` table order in the bundled theme files.
#[derive(Clone, Copy)]
pub struct DesktopPalette {
    // --- washes over the window background ---
    /// Selected list row pill. Formula: `highlight_bg` 20% over `background`.
    pub row_selected: Hsla,
    /// Any hovered row over the window: sidebar rows, quick links, list rows, range pills.
    /// Formula: `text_muted` 10% over `background`.
    pub row_hover: Hsla,
    /// Muted fills: placeholder cover boxes, small button hovers, disabled buttons.
    /// Formula: `text_muted` 15% over `background`.
    pub wash: Hsla,
    /// Window-level borders and outlines, and the seek/volume bar track.
    /// Formula: `text_muted` 30% over `background`.
    pub border: Hsla,
    /// Accent-tinted hover: tabs, links, active-state transport toggles.
    /// Formula: `primary` 12% over `background`.
    pub accent_hover: Hsla,
    /// Accent-tinted selection: drag-over drop targets, the sidebar resize handle, the
    /// selected command-bar suggestion chip. Formula: `primary` 20% over `background`.
    pub accent_selected: Hsla,
    /// The faint heart on a track that is not liked (hovering shows `secondary` itself).
    /// Formula: `secondary` 25% over `background`.
    pub like_dim: Hsla,
    /// The audio-device failure banner background. Formula: `error` 15% over `background`.
    pub error_wash: Hsla,
    // --- washes over the elevated surface (menus, modals, popovers, titlebar) ---
    /// Hovered row, button or titlebar caption in menus and modals.
    /// Formula: `text_muted` 10% over `surface`.
    pub menu_hover: Hsla,
    /// Selected menu/picker row. Formula: `highlight_bg` 20% over `surface`.
    pub menu_selected: Hsla,
    /// Accent-tinted hover/selection in pickers, suggestions and modal controls.
    /// Formula: `primary` 12% over `surface`.
    pub menu_accent: Hsla,
    /// Borders in and around menus, modals, popovers and drag chips.
    /// Formula: `text_muted` 35% over `surface`.
    pub menu_border: Hsla,
    /// Confirm-prompt destructive button border. Formula: `error` 50% over `surface`.
    pub danger_border: Hsla,
    /// That button's hover fill. Formula: `error` 12% over `surface`.
    pub danger_wash: Hsla,
}

impl DesktopPalette {
    pub fn resolve(theme: &echo_core::theme::ResolvedTheme) -> Self {
        let bg = theme.background.gpui(WINDOW_BG());
        let surface = theme.surface.gpui(PANEL_BG());
        let muted = theme.text_muted.gpui(WINDOW_FG());
        let accent = theme.primary.gpui(WINDOW_FG());
        let secondary = theme.secondary.gpui(WINDOW_FG());
        let highlight = theme.highlight_bg.gpui(WINDOW_FG());
        let error = theme.error.gpui(WINDOW_FG());

        let slot = |name: &str, derived: Hsla| {
            theme
                .desktop_overrides
                .get(name)
                .map(|color| color.gpui(derived))
                .unwrap_or(derived)
        };

        Self {
            row_selected: slot("row_selected", blend(highlight, bg, 0.20)),
            row_hover: slot("row_hover", blend(muted, bg, 0.10)),
            wash: slot("wash", blend(muted, bg, 0.15)),
            border: slot("border", blend(muted, bg, 0.30)),
            accent_hover: slot("accent_hover", blend(accent, bg, 0.12)),
            accent_selected: slot("accent_selected", blend(accent, bg, 0.20)),
            like_dim: slot("like_dim", blend(secondary, bg, 0.25)),
            error_wash: slot("error_wash", blend(error, bg, 0.15)),
            menu_hover: slot("menu_hover", blend(muted, surface, 0.10)),
            menu_selected: slot("menu_selected", blend(highlight, surface, 0.20)),
            menu_accent: slot("menu_accent", blend(accent, surface, 0.12)),
            menu_border: slot("menu_border", blend(muted, surface, 0.35)),
            danger_border: slot("danger_border", blend(error, surface, 0.50)),
            danger_wash: slot("danger_wash", blend(error, surface, 0.12)),
        }
    }
}
