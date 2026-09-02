//! Frontend-neutral theme colors.
//!
//! Theme files store colors as strings (`config::Theme`); this module resolves them into
//! [`ThemeColor`] values that carry no rendering dependency, so the same resolved theme serves
//! any frontend. The ratatui views convert with an extension trait in `tui`; a GPUI frontend
//! maps named colors through its own palette.

use crate::config::Theme;
use std::str::FromStr;

/// The sixteen ANSI palette entries, by conventional name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

/// A theme color with no rendering dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeColor {
    /// The terminal's (or window's) default for that slot.
    Reset,
    Named(NamedColor),
    /// An xterm-256 palette index.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl FromStr for ThemeColor {
    type Err = ();

    /// Mirrors ratatui's `Color::from_str`: named colors (case- and separator-insensitive),
    /// `#rrggbb` hex, or a bare palette index.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized: String = s
            .chars()
            .filter(|c| !matches!(c, ' ' | '-' | '_'))
            .collect::<String>()
            .to_lowercase();

        use NamedColor::*;
        Ok(match normalized.as_str() {
            "reset" | "default" => Self::Reset,
            "black" => Self::Named(Black),
            "red" => Self::Named(Red),
            "green" => Self::Named(Green),
            "yellow" => Self::Named(Yellow),
            "blue" => Self::Named(Blue),
            "magenta" => Self::Named(Magenta),
            "cyan" => Self::Named(Cyan),
            "gray" | "grey" => Self::Named(Gray),
            "darkgray" | "darkgrey" => Self::Named(DarkGray),
            "lightred" => Self::Named(LightRed),
            "lightgreen" => Self::Named(LightGreen),
            "lightyellow" => Self::Named(LightYellow),
            "lightblue" => Self::Named(LightBlue),
            "lightmagenta" => Self::Named(LightMagenta),
            "lightcyan" => Self::Named(LightCyan),
            "white" => Self::Named(White),
            _ => {
                if let Some(hex) = normalized.strip_prefix('#') {
                    if hex.len() != 6 {
                        return Err(());
                    }
                    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| ())?;
                    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| ())?;
                    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| ())?;
                    Self::Rgb(r, g, b)
                } else if let Ok(index) = normalized.parse::<u8>() {
                    Self::Indexed(index)
                } else {
                    return Err(());
                }
            }
        })
    }
}

/// A theme with every color parsed, falling back per-slot like the old ratatui-typed version.
#[derive(Clone)]
pub struct ResolvedTheme {
    pub primary: ThemeColor,
    pub secondary: ThemeColor,
    pub background: ThemeColor,
    /// Elevated panels (menus, modals); `Reset` when the theme doesn't define one.
    pub surface: ThemeColor,
    pub text: ThemeColor,
    pub text_muted: ThemeColor,
    pub highlight_bg: ThemeColor,
    pub highlight_fg: ThemeColor,
    pub selection_bg: ThemeColor,
    pub selected_item: ThemeColor,
    pub error: ThemeColor,
    /// The desktop's derived washes/borders/hovers when the theme spells them out — top-level
    /// keys and `[desktop]` table entries merged (the table wins on conflicts). Keys the
    /// desktop doesn't know, and values that fail to parse, are dropped. Empty for themes
    /// listing only base colors (the desktop then derives everything).
    pub desktop_overrides: std::collections::HashMap<String, ThemeColor>,
}

impl ResolvedTheme {
    pub fn from_theme(theme: &Theme) -> Self {
        use NamedColor::*;
        let parse = |s: &str, fallback: ThemeColor| ThemeColor::from_str(s).unwrap_or(fallback);
        Self {
            primary: parse(&theme.primary, ThemeColor::Named(Cyan)),
            secondary: parse(&theme.secondary, ThemeColor::Named(Yellow)),
            background: parse(&theme.background, ThemeColor::Reset),
            surface: parse(&theme.surface, ThemeColor::Reset),
            text: parse(&theme.text, ThemeColor::Named(White)),
            text_muted: parse(&theme.text_muted, ThemeColor::Named(DarkGray)),
            highlight_bg: parse(&theme.highlight_bg, ThemeColor::Named(White)),
            highlight_fg: parse(&theme.highlight_fg, ThemeColor::Named(Black)),
            selection_bg: parse(&theme.highlight_bg, ThemeColor::Named(White)),
            selected_item: parse(&theme.highlight_fg, ThemeColor::Named(Black)),
            error: parse(&theme.error, ThemeColor::Named(Red)),
            desktop_overrides: theme
                .extra
                .iter()
                .chain(theme.desktop.iter())
                .filter_map(|(key, value)| {
                    ThemeColor::from_str(value).ok().map(|c| (key.clone(), c))
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_overrides_come_from_flat_keys_and_the_table() {
        let theme: crate::config::Theme = toml::from_str(
            r##"
            primary = "cyan"
            secondary = "yellow"
            background = "#101014"
            text = "white"
            text_muted = "darkgray"
            highlight_bg = "white"
            highlight_fg = "black"
            error = "red"
            row_selected = "#222233" # flat layout
            row_hover = "not a color"
            [desktop]
            row_selected = "#333344"
            menu_hover = "#444455"
            "##,
        )
        .unwrap();
        let resolved = ResolvedTheme::from_theme(&theme);
        let overrides = &resolved.desktop_overrides;
        assert_eq!(
            overrides.get("row_selected"),
            Some(&ThemeColor::Rgb(0x33, 0x33, 0x44))
        );
        assert_eq!(
            overrides.get("menu_hover"),
            Some(&ThemeColor::Rgb(0x44, 0x44, 0x55))
        );
        assert!(!overrides.contains_key("row_hover"));
    }

    #[test]
    fn named_colors_parse_with_any_separator_style() {
        assert_eq!("cyan".parse(), Ok(ThemeColor::Named(NamedColor::Cyan)));
        assert_eq!(
            "Light-Red".parse(),
            Ok(ThemeColor::Named(NamedColor::LightRed))
        );
        assert_eq!(
            "dark_gray".parse(),
            Ok(ThemeColor::Named(NamedColor::DarkGray))
        );
        assert_eq!("grey".parse(), Ok(ThemeColor::Named(NamedColor::Gray)));
    }

    #[test]
    fn hex_and_indexed_forms_parse() {
        assert_eq!("#1db954".parse(), Ok(ThemeColor::Rgb(0x1d, 0xb9, 0x54)));
        assert_eq!("42".parse(), Ok(ThemeColor::Indexed(42)));
        assert_eq!("reset".parse(), Ok(ThemeColor::Reset));
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert_eq!(ThemeColor::from_str("#12345"), Err(()));
        assert_eq!(ThemeColor::from_str("#gggggg"), Err(()));
        assert_eq!(ThemeColor::from_str("not a color"), Err(()));
    }

    #[test]
    fn resolved_theme_falls_back_per_slot() {
        let theme = Theme {
            primary: "#ff0000".to_string(),
            secondary: "bogus".to_string(),
            ..Theme::default()
        };
        let resolved = ResolvedTheme::from_theme(&theme);
        assert_eq!(resolved.primary, ThemeColor::Rgb(255, 0, 0));
        assert_eq!(resolved.secondary, ThemeColor::Named(NamedColor::Yellow));
    }
}
