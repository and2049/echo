use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
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

#[derive(Serialize, Deserialize, Default)]
pub struct CacheData {
    #[serde(default)]
    pub liked_tracks: std::collections::HashSet<String>,
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

    pub fn cache_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("echo");
        path.push("cache.json");
        path
    }

    pub fn load_cache() -> CacheData {
        let path = Self::cache_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            CacheData::default()
        }
    }

    pub fn save_cache(cache: &CacheData) -> Result<()> {
        let path = Self::cache_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string(cache)?;
        fs::write(path, contents)?;
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

const BUNDLED_DEFAULT_THEME: &str = include_str!("../themes/default.toml");

pub fn bundled_default_theme() -> Theme {
    toml::from_str::<Theme>(BUNDLED_DEFAULT_THEME).unwrap_or_else(|_| Theme::default())
}

fn app_theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        dirs.push(exe_dir.join("themes"));

        if exe_dir.file_name().and_then(|name| name.to_str()) == Some("MacOS")
            && let Some(contents_dir) = exe_dir.parent()
        {
            dirs.push(contents_dir.join("Resources").join("themes"));
        }
    }

    if let Ok(appdir) = env::var("APPDIR") {
        let appdir = PathBuf::from(appdir);
        dirs.push(appdir.join("themes"));
        dirs.push(appdir.join("usr").join("lib").join("echo").join("themes"));
    }

    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/lib/echo/themes"));
    }

    dirs.push(workspace_theme_dir());
    dedupe_paths(dirs)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.iter().any(|existing| existing == &path) {
            deduped.push(path);
        }
    }
    deduped
}

pub fn load_themes() -> Result<HashMap<String, Theme>> {
    load_themes_from_paths(app_theme_dirs(), user_theme_dir())
}

fn load_themes_from_paths(
    app_theme_dirs: Vec<PathBuf>,
    user_theme_dir: PathBuf,
) -> Result<HashMap<String, Theme>> {
    let mut themes = HashMap::new();

    // Packaged/dev themes are read-only app themes. User themes are loaded after
    // them, but duplicate names are exposed as `user/<name>` instead of shadowing
    // the app theme.
    themes.insert("default".to_string(), bundled_default_theme());
    for dir in app_theme_dirs {
        load_themes_from_dir(&mut themes, dir, DuplicateTheme::Replace)?;
    }

    fs::create_dir_all(&user_theme_dir)?;
    load_themes_from_dir(&mut themes, user_theme_dir, DuplicateTheme::NamespaceUser)?;

    if themes.is_empty() {
        themes.insert("default".to_string(), bundled_default_theme());
    }

    Ok(themes)
}

#[derive(Clone, Copy)]
enum DuplicateTheme {
    Replace,
    NamespaceUser,
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
                && let Ok(theme) = toml::from_str::<Theme>(&contents)
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("echo-{}-{}-{}", name, std::process::id(), nanos))
    }

    fn theme_contents(primary: &str, background: &str) -> String {
        format!(
            r##"primary = "{primary}"
secondary = "#222222"
background = "{background}"
text = "#eeeeee"
text_muted = "#999999"
highlight_bg = "#333333"
highlight_fg = "#ffffff"
error = "Red"
"##
        )
    }

    #[test]
    fn bundled_default_theme_uses_packaged_toml_values() {
        let theme = bundled_default_theme();

        assert_eq!(theme.background, "#121114");
        assert_ne!(theme.background, Theme::default().background);
    }

    #[test]
    fn load_themes_from_paths_loads_app_theme_dirs() {
        let root = unique_temp_dir("app-themes");
        let app_dir = root.join("app-themes");
        let user_dir = root.join("user-themes");
        fs::create_dir_all(&app_dir).expect("create app theme dir");
        fs::write(
            app_dir.join("ocean.toml"),
            theme_contents("#111111", "#010203"),
        )
        .expect("write app theme");

        let themes = load_themes_from_paths(vec![app_dir], user_dir).expect("load themes");

        assert_eq!(themes["default"].background, "#121114");
        assert_eq!(themes["ocean"].background, "#010203");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn user_theme_duplicates_are_namespaced() {
        let root = unique_temp_dir("user-theme-namespace");
        let user_dir = root.join("user-themes");
        fs::create_dir_all(&user_dir).expect("create user theme dir");
        fs::write(
            user_dir.join("default.toml"),
            theme_contents("#444444", "#040506"),
        )
        .expect("write user default theme");

        let themes = load_themes_from_paths(Vec::new(), user_dir).expect("load themes");

        assert_eq!(themes["default"].background, "#121114");
        assert_eq!(themes["user/default"].background, "#040506");
        let _ = fs::remove_dir_all(root);
    }
}
