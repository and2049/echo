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
