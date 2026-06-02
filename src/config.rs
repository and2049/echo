use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuthTokens {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct SpotifyCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub spotify_credentials: Option<SpotifyCredentials>,
    pub auth_tokens: Option<AuthTokens>,
    #[serde(default)]
    pub library: LibraryConfig,
}

fn default_track_index_base() -> isize {
    1
}

fn default_language() -> String {
    "en".to_string()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LibraryConfig {
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub sort_mode: SortMode,
    #[serde(default)]
    pub active_theme: Option<String>,
    #[serde(default = "default_track_index_base")]
    pub track_index_base: isize,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for LibraryConfig {
    fn default() -> Self {
        Self {
            pinned: vec![],
            folders: vec![],
            sort_mode: SortMode::default(),
            active_theme: None,
            track_index_base: 1,
            language: "en".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Folder {
    pub name: String,
    pub is_open: bool,
    pub playlists: Vec<String>,
}

#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
pub enum SortMode {
    #[default]
    Default,
    Alphabetical,
    Creator,
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("echo");
        path.push("config.toml");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            toml::from_str(&contents).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let toml_str = toml::to_string_pretty(self)?;
        fs::write(path, toml_str)?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Theme {
    pub primary: String,
    pub secondary: String,
    pub background: String,
    pub text: String,
    pub text_muted: String,
    pub highlight_bg: String,
    pub highlight_fg: String,
    pub error: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: "Cyan".to_string(),
            secondary: "Yellow".to_string(),
            background: "Reset".to_string(),
            text: "White".to_string(),
            text_muted: "DarkGray".to_string(),
            highlight_bg: "White".to_string(),
            highlight_fg: "Black".to_string(),
            error: "Red".to_string(),
        }
    }
}

pub fn user_theme_dir() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("echo");
    path.push("themes");
    path
}

pub fn workspace_theme_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("themes")
}

pub fn load_themes() -> Result<HashMap<String, Theme>> {
    let mut themes = HashMap::new();

    // Built-in/workspace themes are read-only app themes. User themes are loaded
    // after them, but duplicate names are exposed as `user/<name>` instead of
    // shadowing the app theme.
    load_bundled_theme(
        &mut themes,
        "default",
        include_str!("../themes/default.toml"),
    );
    load_themes_from_dir(&mut themes, workspace_theme_dir(), DuplicateTheme::Replace)?;

    let user_dir = user_theme_dir();
    fs::create_dir_all(&user_dir)?;
    load_themes_from_dir(&mut themes, user_dir, DuplicateTheme::NamespaceUser)?;

    if themes.is_empty() {
        themes.insert("default".to_string(), Theme::default());
    }

    Ok(themes)
}

#[derive(Clone, Copy)]
enum DuplicateTheme {
    Replace,
    NamespaceUser,
}

fn load_bundled_theme(themes: &mut HashMap<String, Theme>, name: &str, contents: &str) {
    if let Ok(theme) = toml::from_str::<Theme>(contents) {
        themes.insert(name.to_string(), theme);
    }
}

fn load_themes_from_dir(
    themes: &mut HashMap<String, Theme>,
    path: PathBuf,
    duplicate_theme: DuplicateTheme,
) -> Result<()> {
    if path.exists() {
        for entry in fs::read_dir(&path)?.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml")
                && let Some(file_stem) = path.file_stem().and_then(|s| s.to_str())
                    && let Ok(contents) = fs::read_to_string(&path)
                        && let Ok(theme) = toml::from_str::<Theme>(&contents) {
                            let theme_name = match duplicate_theme {
                                DuplicateTheme::Replace => file_stem.to_string(),
                                DuplicateTheme::NamespaceUser if themes.contains_key(file_stem) => {
                                    format!("user/{}", file_stem)
                                }
                                DuplicateTheme::NamespaceUser => file_stem.to_string(),
                            };
                            themes.insert(theme_name, theme);
                        }
        }
    }

    Ok(())
}
