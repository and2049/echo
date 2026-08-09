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
/// The destructive-action red used by menus and confirm prompts.
pub const DANGER_RED: fn() -> Hsla = || Hsla {
    h: 0.0,
    s: 0.7,
    l: 0.6,
    a: 1.0,
};

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
/// Each slot has a formula — base theme slot, mix strength, and what it composites over
/// (the window `background` or the elevated `surface`) — used when the theme file doesn't
/// spell the color out. A theme's `[desktop]` table overrides any slot by name; the bundled
/// themes list every slot explicitly (values generated with `themes/generate_desktop.py`),
/// so editing the file is the whole story for them.
///
/// Field order mirrors the `[desktop]` table order in the bundled theme files.
#[derive(Clone, Copy)]
pub struct DesktopPalette {
    // --- washes over the window background ---
    /// Selected list row pill. Formula: `highlight_bg` 20% over `background`.
    pub row_selected: Hsla,
    /// Hovered sidebar row / quick link / range pill. Formula: `text_muted` 10% over `background`.
    pub row_hover: Hsla,
    /// Hovered main-area list row (slightly fainter). Formula: `text_muted` 8% over `background`.
    pub row_hover_faint: Hsla,
    /// Placeholder cover/thumbnail boxes, small round-button hover, disabled setup button.
    /// Formula: `text_muted` 15% over `background`.
    pub wash_muted: Hsla,
    /// Seek and volume bar track. Formula: `text_muted` 25% over `background`.
    pub bar_track: Hsla,
    /// Window-level borders: sidebar edge, playback/command bar top, input outlines.
    /// Formula: `text_muted` 30% over `background`.
    pub border_soft: Hsla,
    /// Inactive pill outlines in the main area (range switcher). Formula: `text_muted` 40%
    /// over `background`.
    pub outline: Hsla,
    /// Hovered tab / link. Formula: `primary` 10% over `background`.
    pub accent_wash: Hsla,
    /// Hovered active-state transport toggle (shuffle/repeat/queue/lyrics when on).
    /// Formula: `primary` 15% over `background`.
    pub accent_wash_icon: Hsla,
    /// Drag-over drop target rows and the sidebar resize handle. Formula: `primary` 20%
    /// over `background`.
    pub accent_wash_strong: Hsla,
    /// Selected command-bar suggestion chip. Formula: `primary` 25% over `background`.
    pub suggestion_selected: Hsla,
    /// Hovered play/pause and prev/next transport buttons. Formula: `text` 15% over
    /// `background`.
    pub wash_fg: Hsla,
    /// The faint heart on a track that is not liked. Formula: `secondary` 25% over
    /// `background`.
    pub like_dim: Hsla,
    /// That heart while hovered. Formula: `secondary` 70% over `background`.
    pub like_hover: Hsla,
    /// The audio-device failure banner background. Formula: `error` 15% over `background`.
    pub error_wash: Hsla,
    // --- washes over the elevated surface (menus, modals, popovers, titlebar) ---
    /// Hovered menu/modal row. Formula: `text_muted` 10% over `surface`.
    pub menu_hover: Hsla,
    /// Selected context-menu/track-menu row. Formula: `highlight_bg` 20% over `surface`.
    pub menu_row_selected: Hsla,
    /// Selected sort-menu row. Formula: `text_muted` 18% over `surface`.
    pub menu_selected: Hsla,
    /// Input outlines inside modals (settings fields). Formula: `text_muted` 30% over
    /// `surface`.
    pub menu_border_soft: Hsla,
    /// Popover, modal and drag-chip borders. Formula: `text_muted` 40% over `surface`.
    pub border_strong: Hsla,
    /// Selected/hovered row in pickers (theme modal, playlist-add) and search suggestions.
    /// Formula: `primary` 12% over `surface`.
    pub picker_selected: Hsla,
    /// Hovered control row inside modals (settings buttons). Formula: `primary` 10% over
    /// `surface`.
    pub menu_accent_wash: Hsla,
    /// Confirm-prompt destructive button border. Formula: the fixed danger red 50% over
    /// `surface`.
    pub prompt_confirm_border: Hsla,
    /// That button's hover fill. Formula: the fixed danger red 12% over `surface`.
    pub prompt_confirm_wash: Hsla,
    /// Confirm-prompt cancel button border. Formula: `text_muted` 50% over `surface`.
    pub prompt_cancel_border: Hsla,
    /// That button's hover fill. Formula: `text_muted` 12% over `surface`.
    pub prompt_cancel_wash: Hsla,
    /// Titlebar minimize/maximize caption hover. Formula: `text` 8% over `surface`.
    pub titlebar_hover: Hsla,
    /// Those captions while pressed. Formula: `text` 12% over `surface`.
    pub titlebar_active: Hsla,
    /// The close caption while pressed. Formula: fixed `#E81123` 80% over `surface`
    /// (its hover is the opaque system red and is not themed).
    pub close_active: Hsla,
}

impl DesktopPalette {
    pub fn resolve(theme: &echo_core::theme::ResolvedTheme) -> Self {
        let bg = theme.background.gpui(WINDOW_BG());
        let surface = theme.surface.gpui(PANEL_BG());
        let fg = theme.text.gpui(WINDOW_FG());
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
            row_hover_faint: slot("row_hover_faint", blend(muted, bg, 0.08)),
            wash_muted: slot("wash_muted", blend(muted, bg, 0.15)),
            bar_track: slot("bar_track", blend(muted, bg, 0.25)),
            border_soft: slot("border_soft", blend(muted, bg, 0.30)),
            outline: slot("outline", blend(muted, bg, 0.40)),
            accent_wash: slot("accent_wash", blend(accent, bg, 0.10)),
            accent_wash_icon: slot("accent_wash_icon", blend(accent, bg, 0.15)),
            accent_wash_strong: slot("accent_wash_strong", blend(accent, bg, 0.20)),
            suggestion_selected: slot("suggestion_selected", blend(accent, bg, 0.25)),
            wash_fg: slot("wash_fg", blend(fg, bg, 0.15)),
            like_dim: slot("like_dim", blend(secondary, bg, 0.25)),
            like_hover: slot("like_hover", blend(secondary, bg, 0.70)),
            error_wash: slot("error_wash", blend(error, bg, 0.15)),
            menu_hover: slot("menu_hover", blend(muted, surface, 0.10)),
            menu_row_selected: slot("menu_row_selected", blend(highlight, surface, 0.20)),
            menu_selected: slot("menu_selected", blend(muted, surface, 0.18)),
            menu_border_soft: slot("menu_border_soft", blend(muted, surface, 0.30)),
            border_strong: slot("border_strong", blend(muted, surface, 0.40)),
            picker_selected: slot("picker_selected", blend(accent, surface, 0.12)),
            menu_accent_wash: slot("menu_accent_wash", blend(accent, surface, 0.10)),
            prompt_confirm_border: slot("prompt_confirm_border", blend(DANGER_RED(), surface, 0.50)),
            prompt_confirm_wash: slot("prompt_confirm_wash", blend(DANGER_RED(), surface, 0.12)),
            prompt_cancel_border: slot("prompt_cancel_border", blend(muted, surface, 0.50)),
            prompt_cancel_wash: slot("prompt_cancel_wash", blend(muted, surface, 0.12)),
            titlebar_hover: slot("titlebar_hover", blend(fg, surface, 0.08)),
            titlebar_active: slot("titlebar_active", blend(fg, surface, 0.12)),
            close_active: slot("close_active", blend(CLOSE_RED(), surface, 0.80)),
        }
    }
}
