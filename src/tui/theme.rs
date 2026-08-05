//! Ratatui bindings for the neutral theme types.
//!
//! [`crate::theme::ThemeColor`] carries no rendering dependency; these extension traits give the
//! ratatui views back the `Color`/`Style` vocabulary they render with. Extension traits rather
//! than `From` impls so this keeps compiling when theme types and the TUI live in separate
//! crates (orphan rule).

use crate::theme::{NamedColor, ResolvedTheme, ThemeColor};
use ratatui::style::{Color, Style};

/// Converts a neutral theme color to the ratatui equivalent.
pub trait ToRatatui {
    fn rat(self) -> Color;
}

impl ToRatatui for ThemeColor {
    fn rat(self) -> Color {
        match self {
            ThemeColor::Reset => Color::Reset,
            ThemeColor::Indexed(i) => Color::Indexed(i),
            ThemeColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
            ThemeColor::Named(named) => match named {
                NamedColor::Black => Color::Black,
                NamedColor::Red => Color::Red,
                NamedColor::Green => Color::Green,
                NamedColor::Yellow => Color::Yellow,
                NamedColor::Blue => Color::Blue,
                NamedColor::Magenta => Color::Magenta,
                NamedColor::Cyan => Color::Cyan,
                NamedColor::Gray => Color::Gray,
                NamedColor::DarkGray => Color::DarkGray,
                NamedColor::LightRed => Color::LightRed,
                NamedColor::LightGreen => Color::LightGreen,
                NamedColor::LightYellow => Color::LightYellow,
                NamedColor::LightBlue => Color::LightBlue,
                NamedColor::LightMagenta => Color::LightMagenta,
                NamedColor::LightCyan => Color::LightCyan,
                NamedColor::White => Color::White,
            },
        }
    }
}

/// The style vocabulary the views render with, derived from a resolved theme.
pub trait ThemeStyles {
    fn base_style(&self) -> Style;
    fn muted_style(&self) -> Style;
    fn primary_style(&self) -> Style;
    fn secondary_style(&self) -> Style;
    fn error_style(&self) -> Style;
    fn selected_style(&self) -> Style;
    fn gauge_style(&self) -> Style;
}

impl ThemeStyles for ResolvedTheme {
    fn base_style(&self) -> Style {
        Style::default().fg(self.text.rat()).bg(self.background.rat())
    }

    fn muted_style(&self) -> Style {
        self.base_style().fg(self.text_muted.rat())
    }

    fn primary_style(&self) -> Style {
        self.base_style().fg(self.primary.rat())
    }

    fn secondary_style(&self) -> Style {
        self.base_style().fg(self.secondary.rat())
    }

    fn error_style(&self) -> Style {
        self.base_style().fg(self.error.rat())
    }

    fn selected_style(&self) -> Style {
        Style::default()
            .fg(self.highlight_fg.rat())
            .bg(self.highlight_bg.rat())
    }

    fn gauge_style(&self) -> Style {
        Style::default().fg(self.text.rat()).bg(self.text_muted.rat())
    }
}
