use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::{
    Album, Artist, ArtistPageData, LocalLibrary, LocalPlaylists, Playlist, Track, TrackListContext,
    TrackListContextKind,
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
pub const ARTIST_ALBUMS_REFRESH_TTL: Duration = Duration::from_secs(6 * 60 * 60);
pub const ALBUM_TRACKS_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const PLAYLIST_TRACKS_CACHE_TTL: Duration = Duration::from_secs(60 * 60);
pub const LIBRARY_LIST_CACHE_TTL: Duration = Duration::from_secs(60 * 60);
pub const LIBRARY_LIST_REFRESH_TTL: Duration = Duration::from_secs(15 * 60);
pub const PLAYLIST_TRACKS_REFRESH_TTL: Duration = Duration::from_secs(15 * 60);
pub const ALBUM_TRACKS_REFRESH_TTL: Duration = Duration::from_secs(6 * 60 * 60);

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

    pub fn get_playlists_entry(&self) -> Option<CachedEntry<Vec<Playlist>>> {
        self.playlists.clone()
    }

    pub fn library_list_needs_refresh<T>(entry: &CachedEntry<T>) -> bool {
        !is_fresh(entry.fetched_at, LIBRARY_LIST_REFRESH_TTL)
    }

    pub fn set_playlists(&mut self, playlists: Vec<Playlist>) {
        self.playlists = Some(CachedEntry::new(playlists));
    }

    pub fn get_saved_albums(&self) -> Option<Vec<Album>> {
        self.saved_albums
            .as_ref()
            .and_then(|entry| entry.fresh_value(LIBRARY_LIST_CACHE_TTL))
    }

    pub fn get_saved_albums_entry(&self) -> Option<CachedEntry<Vec<Album>>> {
        self.saved_albums.clone()
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

    pub fn get_context_tracks_entry(
        &self,
        context: &TrackListContext,
    ) -> Option<CachedEntry<ContextTracksCacheEntry>> {
        self.context_tracks
            .get(&context_cache_key(context)?)
            .cloned()
    }

    pub fn context_tracks_need_refresh(entry: &CachedEntry<ContextTracksCacheEntry>) -> bool {
        let ttl = context_refresh_ttl(&entry.value.context);
        !is_fresh(entry.fetched_at, ttl)
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

    pub fn get_artist_page_entry(&self, artist_id: &str) -> Option<CachedEntry<ArtistPageData>> {
        self.artist_pages.get(artist_id).cloned()
    }

    pub fn artist_page_needs_album_refresh(entry: &CachedEntry<ArtistPageData>) -> bool {
        !is_fresh(entry.fetched_at, ARTIST_ALBUMS_REFRESH_TTL)
            || !artist_album_metadata_complete(&entry.value.albums)
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

pub fn artist_album_metadata_complete(albums: &[Album]) -> bool {
    albums.iter().all(|album| album.track_count.is_some())
}

pub fn context_cache_key(context: &TrackListContext) -> Option<String> {
    match context.kind {
        TrackListContextKind::Playlist => Some(format!("playlist:{}", context.id)),
        TrackListContextKind::Album => Some(format!("album:{}", context.id)),
        TrackListContextKind::Generated
        | TrackListContextKind::LocalLibrary
        | TrackListContextKind::LocalPlaylist => None,
    }
}

fn context_cache_ttl(context: &TrackListContext) -> Option<Duration> {
    match context.kind {
        TrackListContextKind::Playlist => Some(PLAYLIST_TRACKS_CACHE_TTL),
        TrackListContextKind::Album => Some(ALBUM_TRACKS_CACHE_TTL),
        TrackListContextKind::Generated
        | TrackListContextKind::LocalLibrary
        | TrackListContextKind::LocalPlaylist => None,
    }
}

fn context_refresh_ttl(context: &TrackListContext) -> Duration {
    match context.kind {
        TrackListContextKind::Playlist => PLAYLIST_TRACKS_REFRESH_TTL,
        TrackListContextKind::Album => ALBUM_TRACKS_REFRESH_TTL,
        TrackListContextKind::Generated
        | TrackListContextKind::LocalLibrary
        | TrackListContextKind::LocalPlaylist => Duration::MAX,
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
    #[serde(default)]
    pub local_music_dir: Option<PathBuf>,
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
            local_music_dir: None,
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

    pub fn local_library_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("echo");
        path.push("local_library.json");
        path
    }

    pub fn load_local_library() -> LocalLibrary {
        let path = Self::local_library_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            LocalLibrary::default()
        }
    }

    pub fn save_local_library(library: &LocalLibrary) -> Result<()> {
        let path = Self::local_library_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(library)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn local_playlists_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("echo");
        path.push("local_playlists.json");
        path
    }

    pub fn load_local_playlists() -> LocalPlaylists {
        let path = Self::local_playlists_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            LocalPlaylists::default()
        }
    }

    pub fn save_local_playlists(playlists: &LocalPlaylists) -> Result<()> {
        let path = Self::local_playlists_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = serde_json::to_string_pretty(playlists)?;
        fs::write(path, contents)?;
        Ok(())
    }

    pub fn local_artwork_dir() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("echo");
        path.push("local_artwork");
        path
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

    #[allow(dead_code)]
    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("echo-{}-{}-{}", name, std::process::id(), nanos))
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
    fn local_contexts_are_not_persisted_as_spotify_track_contexts() {
        let mut cache = CacheData::default();
        let library = TrackListContext::local_library();
        let playlist =
            TrackListContext::local_playlist("local-playlist:one".to_string(), "Local".to_string());

        cache.set_context_tracks(library.clone(), Vec::new());
        cache.set_context_tracks(playlist.clone(), Vec::new());

        assert!(cache.get_context_tracks(&library).is_none());
        assert!(cache.get_context_tracks(&playlist).is_none());
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

    fn artist_page_entry(track_count: Option<u32>, age: Duration) -> CachedEntry<ArtistPageData> {
        let mut entry = CachedEntry::new(ArtistPageData {
            artist_id: "artist".to_string(),
            artist_name: "Artist".to_string(),
            image_url: None,
            albums: vec![Album {
                id: "album".to_string(),
                name: "Album".to_string(),
                artists: "Artist".to_string(),
                image_url: None,
                release_year: "2024".to_string(),
                track_count,
            }],
        });
        entry.fetched_at = now_epoch_secs().saturating_sub(age.as_secs());
        entry
    }

    #[test]
    fn artist_album_cache_missing_track_counts_requires_refresh() {
        let entry = artist_page_entry(None, Duration::from_secs(60));

        assert!(CacheData::artist_page_needs_album_refresh(&entry));
    }

    #[test]
    fn artist_album_cache_recent_complete_entry_does_not_refresh() {
        let entry = artist_page_entry(Some(12), Duration::from_secs(60));

        assert!(!CacheData::artist_page_needs_album_refresh(&entry));
    }

    #[test]
    fn artist_album_cache_soft_stale_entry_requires_refresh() {
        let entry = artist_page_entry(Some(12), ARTIST_ALBUMS_REFRESH_TTL + Duration::from_secs(1));

        assert!(CacheData::artist_page_needs_album_refresh(&entry));
    }

    #[test]
    fn expired_artist_page_entry_can_still_be_read_for_display() {
        let mut cache = CacheData::default();
        let mut entry = artist_page_entry(Some(12), ARTIST_PAGE_CACHE_TTL + Duration::from_secs(1));
        entry.value.albums[0].name = "Cached Album".to_string();
        cache
            .artist_pages
            .insert("artist".to_string(), entry.clone());

        assert!(cache.get_artist_page("artist").is_none());
        assert_eq!(
            cache.get_artist_page_entry("artist").and_then(|entry| entry
                .value
                .albums
                .first()
                .map(|album| album.name.clone())),
            Some("Cached Album".to_string())
        );
    }

    #[test]
    fn library_list_soft_stale_entry_requires_refresh() {
        let mut entry = CachedEntry::new(vec![Playlist {
            id: "playlist".to_string(),
            name: "Playlist".to_string(),
            owner: "Owner".to_string(),
            owner_id: "owner".to_string(),
            image_url: None,
        }]);
        entry.fetched_at = now_epoch_secs().saturating_sub(LIBRARY_LIST_REFRESH_TTL.as_secs() + 1);

        assert!(CacheData::library_list_needs_refresh(&entry));
    }

    #[test]
    fn recent_library_list_entry_does_not_refresh() {
        let entry = CachedEntry::new(Vec::<Playlist>::new());

        assert!(!CacheData::library_list_needs_refresh(&entry));
    }

    #[test]
    fn expired_context_track_entry_can_still_be_read_for_display() {
        let mut cache = CacheData::default();
        let context = TrackListContext::playlist(
            "playlist".to_string(),
            "Playlist".to_string(),
            "Owner".to_string(),
            "owner".to_string(),
            None,
        );
        cache.set_context_tracks(context.clone(), Vec::new());
        let key = context_cache_key(&context).expect("cacheable context");
        cache
            .context_tracks
            .get_mut(&key)
            .expect("context cache entry")
            .fetched_at = now_epoch_secs().saturating_sub(PLAYLIST_TRACKS_CACHE_TTL.as_secs() + 1);

        assert!(cache.get_context_tracks(&context).is_none());
        assert!(cache.get_context_tracks_entry(&context).is_some());
    }

    #[test]
    fn playlist_context_soft_stale_entry_requires_refresh() {
        let context = TrackListContext::playlist(
            "playlist".to_string(),
            "Playlist".to_string(),
            "Owner".to_string(),
            "owner".to_string(),
            None,
        );
        let mut entry = CachedEntry::new(ContextTracksCacheEntry {
            context,
            tracks: Vec::new(),
        });
        entry.fetched_at =
            now_epoch_secs().saturating_sub(PLAYLIST_TRACKS_REFRESH_TTL.as_secs() + 1);

        assert!(CacheData::context_tracks_need_refresh(&entry));
    }
}
