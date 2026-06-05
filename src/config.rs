use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::{
    Album, Artist, ArtistPageData, Playlist, Track, TrackListContext, TrackListContextKind,
};

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

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct CacheData {
    #[serde(default)]
    pub liked_tracks: HashSet<String>,
    #[serde(default)]
    pub last_liked_sync_time: Option<u64>,
    #[serde(default)]
    pub playlists: Option<CachedEntry<Vec<Playlist>>>,
    #[serde(default)]
    pub saved_albums: Option<CachedEntry<Vec<Album>>>,
    #[serde(default)]
    pub followed_artists: Option<CachedEntry<Vec<Artist>>>,
    #[serde(default)]
    pub top_tracks: Option<CachedEntry<Vec<Track>>>,
    #[serde(default)]
    pub recently_played: Option<CachedEntry<Vec<Track>>>,
    #[serde(default)]
    pub context_tracks: HashMap<String, CachedEntry<ContextTracksCacheEntry>>,
    #[serde(default)]
    pub artist_pages: HashMap<String, CachedEntry<ArtistPageData>>,
    #[serde(default)]
    pub cooldowns: HashMap<String, u64>,
    #[serde(default)]
    pub cooldown_failures: HashMap<String, u32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CachedEntry<T> {
    pub fetched_at: u64,
    pub value: T,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContextTracksCacheEntry {
    pub context: TrackListContext,
    pub tracks: Vec<Track>,
}

pub const FOLLOWED_ARTISTS_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const TOP_TRACKS_CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
pub const RECENTLY_PLAYED_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
pub const ARTIST_PAGE_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const ALBUM_TRACKS_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const PLAYLIST_TRACKS_CACHE_TTL: Duration = Duration::from_secs(60 * 60);
pub const LIBRARY_LIST_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

impl<T> CachedEntry<T> {
    pub fn new(value: T) -> Self {
        Self {
            fetched_at: now_epoch_secs(),
            value,
        }
    }
}

impl<T: Clone> CachedEntry<T> {
    pub fn fresh_value(&self, ttl: Duration) -> Option<T> {
        is_fresh(self.fetched_at, ttl).then(|| self.value.clone())
    }
}

impl CacheData {
    pub fn get_playlists(&self) -> Option<Vec<Playlist>> {
        self.playlists
            .as_ref()
            .and_then(|entry| entry.fresh_value(LIBRARY_LIST_CACHE_TTL))
    }

    pub fn set_playlists(&mut self, playlists: Vec<Playlist>) {
        self.playlists = Some(CachedEntry::new(playlists));
    }

    pub fn get_saved_albums(&self) -> Option<Vec<Album>> {
        self.saved_albums
            .as_ref()
            .and_then(|entry| entry.fresh_value(LIBRARY_LIST_CACHE_TTL))
    }

    pub fn set_saved_albums(&mut self, albums: Vec<Album>) {
        self.saved_albums = Some(CachedEntry::new(albums));
    }

    pub fn get_followed_artists(&self) -> Option<Vec<Artist>> {
        self.followed_artists
            .as_ref()
            .and_then(|entry| entry.fresh_value(FOLLOWED_ARTISTS_CACHE_TTL))
    }

    pub fn set_followed_artists(&mut self, artists: Vec<Artist>) {
        self.clear_cooldown("followed_artists");
        self.followed_artists = Some(CachedEntry::new(artists));
    }

    pub fn get_top_tracks(&self) -> Option<Vec<Track>> {
        self.top_tracks
            .as_ref()
            .and_then(|entry| entry.fresh_value(TOP_TRACKS_CACHE_TTL))
    }

    pub fn set_top_tracks(&mut self, tracks: Vec<Track>) {
        self.clear_cooldown("top_tracks");
        self.top_tracks = Some(CachedEntry::new(tracks));
    }

    pub fn get_recently_played(&self) -> Option<Vec<Track>> {
        self.recently_played
            .as_ref()
            .and_then(|entry| entry.fresh_value(RECENTLY_PLAYED_CACHE_TTL))
    }

    pub fn set_recently_played(&mut self, tracks: Vec<Track>) {
        self.clear_cooldown("recently_played");
        self.recently_played = Some(CachedEntry::new(tracks));
    }

    pub fn get_context_tracks(
        &self,
        context: &TrackListContext,
    ) -> Option<ContextTracksCacheEntry> {
        let ttl = context_cache_ttl(context)?;
        self.context_tracks
            .get(&context_cache_key(context)?)
            .and_then(|entry| entry.fresh_value(ttl))
    }

    pub fn set_context_tracks(&mut self, context: TrackListContext, tracks: Vec<Track>) {
        let Some(key) = context_cache_key(&context) else {
            return;
        };
        self.context_tracks.insert(
            key,
            CachedEntry::new(ContextTracksCacheEntry { context, tracks }),
        );
    }

    pub fn invalidate_playlist_context(&mut self, playlist_id: &str) {
        self.context_tracks
            .remove(&format!("playlist:{playlist_id}"));
    }

    pub fn get_artist_page(&self, artist_id: &str) -> Option<ArtistPageData> {
        self.artist_pages
            .get(artist_id)
            .and_then(|entry| entry.fresh_value(ARTIST_PAGE_CACHE_TTL))
    }

    pub fn set_artist_page(&mut self, artist_id: String, page: ArtistPageData) {
        self.clear_cooldown(&format!("artist_page:{artist_id}"));
        self.artist_pages.insert(artist_id, CachedEntry::new(page));
    }

    pub fn set_cooldown(&mut self, key: impl Into<String>, retry_after: Duration) {
        self.cooldowns.insert(
            key.into(),
            now_epoch_secs().saturating_add(retry_after.as_secs().max(1)),
        );
    }

    pub fn record_rate_limit_cooldown(
        &mut self,
        key: impl Into<String>,
        retry_after: Duration,
    ) -> Duration {
        let key = key.into();
        let failures = self
            .cooldown_failures
            .entry(key.clone())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        let safety_secs = 5u64
            .saturating_mul(2u64.saturating_pow(failures.saturating_sub(1).min(6)))
            .min(5 * 60);
        let cooldown = retry_after.saturating_add(Duration::from_secs(safety_secs));
        self.cooldowns.insert(
            key,
            now_epoch_secs().saturating_add(cooldown.as_secs().max(1)),
        );
        cooldown
    }

    pub fn cooldown_remaining(&mut self, key: &str) -> Option<Duration> {
        let until = *self.cooldowns.get(key)?;
        let now = now_epoch_secs();
        if until > now {
            Some(Duration::from_secs(until - now))
        } else {
            self.cooldowns.remove(key);
            None
        }
    }

    pub fn clear_cooldown(&mut self, key: &str) {
        self.cooldowns.remove(key);
        self.cooldown_failures.remove(key);
    }
}

pub fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_fresh(fetched_at: u64, ttl: Duration) -> bool {
    now_epoch_secs().saturating_sub(fetched_at) <= ttl.as_secs()
}

pub fn context_cache_key(context: &TrackListContext) -> Option<String> {
    match context.kind {
        TrackListContextKind::Playlist => Some(format!("playlist:{}", context.id)),
        TrackListContextKind::Album => Some(format!("album:{}", context.id)),
        TrackListContextKind::Generated => None,
    }
}

fn context_cache_ttl(context: &TrackListContext) -> Option<Duration> {
    match context.kind {
        TrackListContextKind::Playlist => Some(PLAYLIST_TRACKS_CACHE_TTL),
        TrackListContextKind::Album => Some(ALBUM_TRACKS_CACHE_TTL),
        TrackListContextKind::Generated => None,
    }
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
    #[serde(default)]
    pub cover_img_pixels: u32,
    #[serde(default)]
    pub condensed_lyrics_enabled: bool,
    #[serde(default = "default_vis_bins")]
    pub vis_bins: usize,
    #[serde(default = "default_enable_visualizer")]
    pub enable_visualizer: bool,
}

fn default_enable_visualizer() -> bool {
    false
}

fn default_vis_bins() -> usize {
    9
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
            cover_img_pixels: 0,
            condensed_lyrics_enabled: false,
            vis_bins: 7,
            enable_visualizer: false,
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

    #[test]
    fn cached_entry_respects_ttl() {
        let mut entry = CachedEntry::new(vec!["track".to_string()]);
        assert_eq!(
            entry.fresh_value(Duration::from_secs(60)),
            Some(vec!["track".to_string()])
        );

        entry.fetched_at = now_epoch_secs().saturating_sub(120);
        assert_eq!(entry.fresh_value(Duration::from_secs(60)), None);
    }

    #[test]
    fn generated_contexts_are_not_persisted_as_track_contexts() {
        let mut cache = CacheData::default();
        let context = TrackListContext::generated("TOP_TRACKS", "Top Tracks");

        cache.set_context_tracks(context.clone(), Vec::new());

        assert!(cache.get_context_tracks(&context).is_none());
        assert!(cache.context_tracks.is_empty());
    }

    #[test]
    fn persistent_cooldown_reports_remaining_then_expires() {
        let mut cache = CacheData::default();
        cache.set_cooldown("top_tracks", Duration::from_secs(30));

        assert!(cache.cooldown_remaining("top_tracks").is_some());

        cache
            .cooldowns
            .insert("top_tracks".to_string(), now_epoch_secs().saturating_sub(1));
        assert!(cache.cooldown_remaining("top_tracks").is_none());
        assert!(!cache.cooldowns.contains_key("top_tracks"));
    }

    #[test]
    fn rate_limit_cooldown_adds_safety_and_backs_off() {
        let mut cache = CacheData::default();

        let first =
            cache.record_rate_limit_cooldown("artist_albums:artist", Duration::from_secs(30));
        let second =
            cache.record_rate_limit_cooldown("artist_albums:artist", Duration::from_secs(30));

        assert_eq!(first, Duration::from_secs(35));
        assert_eq!(second, Duration::from_secs(40));

        cache.clear_cooldown("artist_albums:artist");
        assert!(!cache.cooldown_failures.contains_key("artist_albums:artist"));
    }
}
