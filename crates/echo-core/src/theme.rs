//! Frontend-neutral theme colors.
//!
//! Theme files store colors as strings (`config::Theme`); this module resolves them into
//! [`ThemeColor`] values that carry no rendering dependency. The desktop maps named colors
//! through its own GPUI palette.

use crate::config::{LibraryConfig, Scheme, Theme, ThemeMode};
use std::collections::HashMap;
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

    /// Accepts named colors (case- and separator-insensitive), `#rrggbb` hex, or a bare palette
    /// index.
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

/// The themes a fresh install uses for each appearance.
pub const DEFAULT_DARK: &str = "echo";
pub const DEFAULT_LIGHT: &str = "chime";

/// Bundled dark/light siblings, as `(dark, light)`. A pair is one row in the theme picker and
/// seeds the other appearance when a config from before per-appearance themes is migrated;
/// each appearance's theme is still chosen on its own.
const PAIRS: &[(&str, &str)] = &[
    ("echo", "chime"),
    ("everforest", "glade"),
    ("gruvbox", "parchment"),
    ("nimbus", "cloud"),
    ("tide", "wave"),
];

/// Which appearance `theme` belongs to: the `mode` it declares, else a `-dark` / `-light`
/// suffix on its name, else whether its background is bright. A background that is not a
/// plain `#rrggbb` (the window default, a named color) counts as dark, like the window's own.
pub fn scheme_of(name: &str, theme: &Theme) -> Scheme {
    if let Some(scheme) = theme.mode.as_deref().and_then(Scheme::parse) {
        return scheme;
    }
    if name.ends_with("-light") {
        return Scheme::Light;
    }
    if name.ends_with("-dark") {
        return Scheme::Dark;
    }
    match ThemeColor::from_str(&theme.background) {
        Ok(ThemeColor::Rgb(r, g, b)) => {
            // Rec. 709 luma on the sRGB values: close enough to tell paper from ink.
            let luma = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32;
            if luma > 140.0 {
                Scheme::Light
            } else {
                Scheme::Dark
            }
        }
        _ => Scheme::Dark,
    }
}

/// The name of `name`'s sibling in the other appearance, whether or not it is loaded: a
/// bundled pair, or a user's `foo-dark` / `foo-light`.
pub fn sibling_name(name: &str) -> Option<String> {
    for (dark, light) in PAIRS {
        if name == *dark {
            return Some(light.to_string());
        }
        if name == *light {
            return Some(dark.to_string());
        }
    }
    if let Some(stem) = name.strip_suffix("-dark") {
        return Some(format!("{stem}-light"));
    }
    name.strip_suffix("-light")
        .map(|stem| format!("{stem}-dark"))
}

/// One row of the theme picker: a theme of the listed appearance and, when it has one, its
/// sibling in the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeRow {
    pub name: String,
    pub sibling: Option<String>,
}

/// The themes of one appearance, in picker order. Siblings sort together under the dark
/// member's name, so a pair sits on the same row of both lists; a theme with no sibling
/// only appears in the list for its own appearance.
pub fn themes_for(themes: &HashMap<String, Theme>, scheme: Scheme) -> Vec<ThemeRow> {
    let mut rows: Vec<(String, ThemeRow)> = themes
        .iter()
        .filter(|(name, theme)| scheme_of(name, theme) == scheme)
        .map(|(name, _)| {
            // A sibling only counts when it is loaded and really is the other appearance.
            let sibling = sibling_name(name).filter(|other| {
                themes
                    .get(other)
                    .is_some_and(|theme| scheme_of(other, theme) != scheme)
            });
            let key = match (&sibling, scheme) {
                (Some(dark), Scheme::Light) => dark.clone(),
                _ => name.clone(),
            };
            (
                key.to_lowercase(),
                ThemeRow {
                    name: name.clone(),
                    sibling,
                },
            )
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.name.cmp(&b.1.name)));
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The theme chosen for `scheme`, as a loaded theme's name: the saved choice, else that
/// appearance's default, else the always-present `echo`.
pub fn selected_name(
    config: &LibraryConfig,
    themes: &HashMap<String, Theme>,
    scheme: Scheme,
) -> String {
    let (saved, default) = match scheme {
        Scheme::Dark => (config.dark_theme.as_deref(), DEFAULT_DARK),
        Scheme::Light => (config.light_theme.as_deref(), DEFAULT_LIGHT),
    };
    saved
        .filter(|name| themes.contains_key(*name))
        .or_else(|| themes.contains_key(default).then_some(default))
        .unwrap_or(DEFAULT_DARK)
        .to_string()
}

/// The appearance in effect: the pinned one, or the system's under `Auto`.
pub fn active_scheme(config: &LibraryConfig, system: Scheme) -> Scheme {
    config.theme_mode.unwrap_or(ThemeMode::Auto).scheme(system)
}

/// Fills in the appearance settings for a config written before they existed.
///
/// Someone who had picked a theme keeps exactly that look: the mode is pinned to that
/// theme's appearance, and the other appearance starts on its sibling when it has one.
/// Otherwise (a fresh install, or the theme was never changed) the app follows the system.
/// A saved theme that no longer exists used to fall back to the dark default, so it still
/// does.
pub fn migrate_selection(config: &mut LibraryConfig, themes: &HashMap<String, Theme>) {
    if config.theme_mode.is_some() {
        return;
    }
    // "default" was the pre-workspace alias for the bundled echo theme.
    let legacy = match config.active_theme.as_deref() {
        Some("default") => Some(DEFAULT_DARK.to_string()),
        other => other.map(str::to_string),
    };
    let Some(name) = legacy else {
        config.theme_mode = Some(ThemeMode::Auto);
        return;
    };
    let Some(theme) = themes.get(&name) else {
        config.theme_mode = Some(ThemeMode::Dark);
        return;
    };
    let scheme = scheme_of(&name, theme);
    let sibling = sibling_name(&name).filter(|other| themes.contains_key(other));
    let (own, other) = match scheme {
        Scheme::Dark => (&mut config.dark_theme, &mut config.light_theme),
        Scheme::Light => (&mut config.light_theme, &mut config.dark_theme),
    };
    *own = Some(name);
    if other.is_none() {
        *other = sibling;
    }
    config.theme_mode = Some(match scheme {
        Scheme::Dark => ThemeMode::Dark,
        Scheme::Light => ThemeMode::Light,
    });
}

/// A theme with every color parsed and a fallback for each slot.
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

    fn theme(mode: Option<&str>, background: &str) -> Theme {
        Theme {
            mode: mode.map(str::to_string),
            background: background.to_string(),
            ..Theme::default()
        }
    }

    fn sample_themes() -> HashMap<String, Theme> {
        [
            ("echo", theme(Some("dark"), "#202130")),
            ("chime", theme(Some("light"), "#eef0f6")),
            ("nimbus", theme(Some("dark"), "#1d2723")),
            ("cloud", theme(Some("light"), "#d5e2d8")),
            ("noir", theme(Some("dark"), "#141314")),
            ("vanilla", theme(Some("light"), "#ecead9")),
        ]
        .into_iter()
        .map(|(name, theme)| (name.to_string(), theme))
        .collect()
    }

    #[test]
    fn scheme_comes_from_the_declared_mode_then_the_name_then_the_background() {
        // Wave is a light theme on a mid-blue background: the declaration wins.
        assert_eq!(
            scheme_of("wave", &theme(Some("light"), "#4474b3")),
            Scheme::Light
        );
        assert_eq!(
            scheme_of("mine-light", &theme(None, "#101010")),
            Scheme::Light
        );
        assert_eq!(
            scheme_of("mine-dark", &theme(None, "#f0f0f0")),
            Scheme::Dark
        );
        assert_eq!(scheme_of("paper", &theme(None, "#ecead9")), Scheme::Light);
        assert_eq!(scheme_of("ink", &theme(None, "#141314")), Scheme::Dark);
        assert_eq!(scheme_of("plain", &theme(None, "Reset")), Scheme::Dark);
        assert_eq!(
            scheme_of("typo", &theme(Some("drak"), "#ecead9")),
            Scheme::Light
        );
    }

    #[test]
    fn each_list_holds_only_its_appearance_with_pairs_on_the_same_row() {
        let themes = sample_themes();
        let names = |scheme| {
            themes_for(&themes, scheme)
                .into_iter()
                .map(|row| (row.name, row.sibling))
                .collect::<Vec<_>>()
        };
        let s = |name: &str| name.to_string();
        assert_eq!(
            names(Scheme::Dark),
            vec![
                (s("echo"), Some(s("chime"))),
                (s("nimbus"), Some(s("cloud"))),
                (s("noir"), None),
            ]
        );
        // Light rows sort under their dark sibling's name: chime (echo), cloud (nimbus).
        assert_eq!(
            names(Scheme::Light),
            vec![
                (s("chime"), Some(s("echo"))),
                (s("cloud"), Some(s("nimbus"))),
                (s("vanilla"), None),
            ]
        );
    }

    #[test]
    fn a_sibling_that_is_not_loaded_leaves_the_theme_on_its_own() {
        let mut themes = sample_themes();
        themes.remove("cloud");
        let dark = themes_for(&themes, Scheme::Dark);
        let nimbus = dark.iter().find(|row| row.name == "nimbus").unwrap();
        assert_eq!(nimbus.sibling, None);
    }

    #[test]
    fn user_themes_pair_by_suffix() {
        assert_eq!(sibling_name("mine-dark").as_deref(), Some("mine-light"));
        assert_eq!(sibling_name("mine-light").as_deref(), Some("mine-dark"));
        assert_eq!(sibling_name("cloud").as_deref(), Some("nimbus"));
        assert_eq!(sibling_name("noir"), None);
    }

    #[test]
    fn a_saved_theme_keeps_its_look_and_seeds_the_other_appearance() {
        let themes = sample_themes();
        let mut config = LibraryConfig {
            active_theme: Some("cloud".to_string()),
            ..LibraryConfig::default()
        };
        migrate_selection(&mut config, &themes);
        assert_eq!(config.theme_mode, Some(ThemeMode::Light));
        assert_eq!(config.light_theme.as_deref(), Some("cloud"));
        assert_eq!(config.dark_theme.as_deref(), Some("nimbus"));

        // No sibling: the other appearance is left to its default.
        let mut config = LibraryConfig {
            active_theme: Some("noir".to_string()),
            ..LibraryConfig::default()
        };
        migrate_selection(&mut config, &themes);
        assert_eq!(config.theme_mode, Some(ThemeMode::Dark));
        assert_eq!(config.dark_theme.as_deref(), Some("noir"));
        assert_eq!(config.light_theme, None);
        assert_eq!(selected_name(&config, &themes, Scheme::Light), "chime");
    }

    #[test]
    fn migration_follows_the_system_only_when_no_theme_was_picked() {
        let themes = sample_themes();
        let mut fresh = LibraryConfig::default();
        migrate_selection(&mut fresh, &themes);
        assert_eq!(fresh.theme_mode, Some(ThemeMode::Auto));

        let mut alias = LibraryConfig {
            active_theme: Some("default".to_string()),
            ..LibraryConfig::default()
        };
        migrate_selection(&mut alias, &themes);
        assert_eq!(alias.theme_mode, Some(ThemeMode::Dark));
        assert_eq!(alias.dark_theme.as_deref(), Some("echo"));

        // A theme that was removed fell back to dark echo before, and still does.
        let mut removed = LibraryConfig {
            active_theme: Some("crimson".to_string()),
            ..LibraryConfig::default()
        };
        migrate_selection(&mut removed, &themes);
        assert_eq!(removed.theme_mode, Some(ThemeMode::Dark));
        assert_eq!(selected_name(&removed, &themes, Scheme::Dark), "echo");

        // Already migrated: left alone.
        let mut done = LibraryConfig {
            active_theme: Some("cloud".to_string()),
            theme_mode: Some(ThemeMode::Auto),
            ..LibraryConfig::default()
        };
        migrate_selection(&mut done, &themes);
        assert_eq!(done.theme_mode, Some(ThemeMode::Auto));
        assert_eq!(done.light_theme, None);
    }

    #[test]
    fn a_missing_choice_falls_back_to_the_default_then_echo() {
        let mut themes = sample_themes();
        let config = LibraryConfig {
            light_theme: Some("gone".to_string()),
            ..LibraryConfig::default()
        };
        assert_eq!(selected_name(&config, &themes, Scheme::Light), "chime");
        themes.remove("chime");
        assert_eq!(selected_name(&config, &themes, Scheme::Light), "echo");
    }

    #[test]
    fn auto_follows_the_system_and_a_pinned_mode_ignores_it() {
        let mut config = LibraryConfig::default();
        assert_eq!(active_scheme(&config, Scheme::Light), Scheme::Light);
        config.theme_mode = Some(ThemeMode::Dark);
        assert_eq!(active_scheme(&config, Scheme::Light), Scheme::Dark);
    }

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
