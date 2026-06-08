use crate::config::{LibraryConfig, Theme};
use crate::models::{
    ActionMenuContext, ArtistPageData, BrowseNode, LocalLibrary, LocalPlaylists, Playlist,
    SearchResults, Track, TrackListContext, TrackSource,
};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Sub-states
// ---------------------------------------------------------------------------

pub struct UIState {
    pub mode: AppMode,
    pub active_view: ActiveView,
    pub is_running: bool,
    pub prev_view: Option<ActiveView>,
    pub active_library_tab: LibraryTab,
    pub active_search_tab: SearchTab,
    pub active_browse_node: BrowseNode,
    // Selection indices
    pub selected_playlist_index: usize,
    pub selected_artist_index: usize,
    pub selected_track_index: usize,
    pub selected_search_index: usize,
    pub selected_queue_index: usize,
    pub selected_device_index: usize,
    pub artist_page_album_index: usize,
    pub selected_action_index: usize,
    pub selected_playlist_modal_index: usize,
    // Prompts / modals
    pub folder_delete_prompt: Option<String>,
    pub playlist_delete_prompt: Option<Vec<String>>,
    pub album_mass_delete_prompt: Option<Vec<String>>,
    pub track_delete_prompt: Option<(String, Vec<String>)>,
    pub liked_track_remove_prompt: Option<String>,
    pub playlist_add_modal_open: bool,
    pub device_modal_open: bool,
    pub lyrics_modal_open: bool,
    pub action_menu_open: bool,
    pub action_menu_context: Option<ActionMenuContext>,
    pub visual_selection_start: Option<usize>,
    pub pending_d_press: bool,
    // Text input (command & search)
    pub command_buffer: String,
    pub command_suggestions: Vec<String>,
    pub command_suggestion_index: Option<usize>,
    pub command_base_buffer: String,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_context_query: String,
    // Status
    pub status_message: Option<String>,
    pub status_message_expiry: Option<std::time::Instant>,
    pub recent_queue_count: usize,
    // Theme / display
    pub themes: HashMap<String, Theme>,
    pub active_theme: ResolvedTheme,
    pub needs_terminal_clear: bool,
    pub vis_bins: usize,
    pub condensed_lyrics_enabled: bool,
    // Setup
    pub setup_client_id: String,
    pub setup_client_secret: String,
    pub setup_focus_secret: bool,
    // Library config (mutable user settings)
    pub library_config: LibraryConfig,
    // Image rendering
    pub image_picker: Option<ratatui_image::picker::Picker>,
    pub active_library_header_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub header_image_cache: Option<Buffer>,
    pub header_image_dirty: bool,
    // Operation register (cut/paste)
    pub operation_register: Vec<String>,
}

impl UIState {
    fn new(
        initial_mode: AppMode,
        condensed_lyrics_enabled: bool,
        vis_bins: usize,
        themes: HashMap<String, Theme>,
        active_theme_config: Theme,
        library_config: LibraryConfig,
    ) -> Self {
        Self {
            mode: initial_mode,
            active_view: ActiveView::Library,
            is_running: true,
            prev_view: None,
            active_library_tab: LibraryTab::Playlists,
            active_search_tab: SearchTab::Tracks,
            active_browse_node: BrowseNode::TopTracks,
            selected_playlist_index: 0,
            selected_artist_index: 0,
            selected_track_index: 0,
            selected_search_index: 0,
            selected_queue_index: 0,
            selected_device_index: 0,
            artist_page_album_index: 0,
            selected_action_index: 0,
            selected_playlist_modal_index: 0,
            folder_delete_prompt: None,
            playlist_delete_prompt: None,
            album_mass_delete_prompt: None,
            track_delete_prompt: None,
            liked_track_remove_prompt: None,
            playlist_add_modal_open: false,
            device_modal_open: false,
            lyrics_modal_open: false,
            action_menu_open: false,
            action_menu_context: None,
            visual_selection_start: None,
            pending_d_press: false,
            command_buffer: String::new(),
            command_suggestions: vec![],
            command_suggestion_index: None,
            command_base_buffer: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_context_query: String::new(),
            status_message: None,
            status_message_expiry: None,
            recent_queue_count: 0,
            themes,
            active_theme: ResolvedTheme::from_theme(&active_theme_config),
            needs_terminal_clear: false,
            vis_bins,
            condensed_lyrics_enabled,
            setup_client_id: String::new(),
            setup_client_secret: String::new(),
            setup_focus_secret: false,
            library_config,
            image_picker: None,
            active_library_header_image: None,
            header_image_cache: None,
            header_image_dirty: false,
            operation_register: vec![],
        }
    }
}

pub struct PlaybackState {
    pub is_playing: bool,
    pub is_shuffled: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub playing_track_id: Option<String>,
    pub playing_track_title: String,
    pub playing_track_artist: String,
    pub playing_track_album_id: Option<String>,
    pub playing_track_artist_id: Option<String>,
    pub playing_track_source: Option<TrackSource>,
    pub playing_track_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub previous_track_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub playing_track_image_cache: Option<ratatui::buffer::Buffer>,
    pub fetching_track_id: Option<String>,
    pub device_name: String,
    pub repeat_mode: String,
    pub volume: u32,
    pub audio_visualization: Option<std::sync::Arc<parking_lot::Mutex<[f32; 32]>>>,
    pub enable_visualizer: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub current_lyrics: Option<crate::models::Lyrics>,
    pub current_lyric_track_id: Option<String>,
    pub is_fetching_lyrics: bool,
    pub playback_last_updated_at: Option<std::time::Instant>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: false,
            is_shuffled: false,
            progress_ms: 0,
            duration_ms: 0,
            playing_track_id: None,
            playing_track_title: String::new(),
            playing_track_artist: String::new(),
            playing_track_album_id: None,
            playing_track_artist_id: None,
            playing_track_source: None,
            playing_track_image: None,
            previous_track_image: None,
            playing_track_image_cache: None,
            fetching_track_id: None,
            device_name: "echo-rs".to_string(),
            repeat_mode: "Off".to_string(),
            volume: 100,
            audio_visualization: None,
            enable_visualizer: None,
            current_lyrics: None,
            current_lyric_track_id: None,
            is_fetching_lyrics: false,
            playback_last_updated_at: None,
        }
    }
}

pub struct DataState {
    pub playlists: Vec<Playlist>,
    pub local_library: LocalLibrary,
    pub local_playlists: LocalPlaylists,
    pub library_view: Vec<crate::models::LibraryNode>,
    pub saved_albums: Vec<crate::models::Album>,
    pub liked_tracks: std::collections::HashSet<String>,
    pub user_id: Option<String>,
    // Browse data
    pub top_tracks: Vec<Track>,
    pub recently_played: Vec<Track>,
    pub followed_artists: Vec<crate::models::Artist>,
    // Search
    pub search_results: SearchResults,
    // TrackList
    pub tracks: Vec<Track>,
    pub active_tracklist_context: Option<TrackListContext>,
    pub tracklist_image_url: Option<String>,
    // Queue
    pub queue: Vec<Track>,
    // Devices
    pub devices: Vec<crate::models::Device>,
    // Artist page
    pub artist_page_data: Option<ArtistPageData>,
    pub pending_artist_page_id: Option<String>,
    pub artist_page_loading: bool,
    pub artist_albums_loading: bool,
}

impl DataState {
    fn new() -> Self {
        Self {
            playlists: Vec::new(),
            local_library: crate::config::AppConfig::load_local_library(),
            local_playlists: crate::config::AppConfig::load_local_playlists(),
            library_view: Vec::new(),
            saved_albums: Vec::new(),
            liked_tracks: std::collections::HashSet::new(),
            user_id: None,
            top_tracks: Vec::new(),
            recently_played: Vec::new(),
            followed_artists: Vec::new(),
            search_results: SearchResults::default(),
            tracks: Vec::new(),
            active_tracklist_context: None,
            tracklist_image_url: None,
            queue: Vec::new(),
            devices: Vec::new(),
            artist_page_data: None,
            pending_artist_page_id: None,
            artist_page_loading: false,
            artist_albums_loading: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum ActiveView {
    Library,
    TrackList,
    SearchResults,
    Queue,
    Devices,
    ArtistList,
    ArtistPage,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SearchTab {
    Tracks,
    Albums,
    Artists,
}

#[derive(PartialEq)]
pub enum AppMode {
    Setup,
    Authenticating,
    Normal,
    Command,
    Search,
    Visual,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum LibraryTab {
    Playlists,
    Albums,
    Browse,
}

// ---------------------------------------------------------------------------
// ResolvedTheme
// ---------------------------------------------------------------------------

pub struct ResolvedTheme {
    pub primary: Color,
    pub secondary: Color,
    pub background: Color,
    pub text: Color,
    pub text_muted: Color,
    pub highlight_bg: Color,
    pub highlight_fg: Color,
    pub selection_bg: Color,
    pub selected_item: Color,
    pub error: Color,
}

impl ResolvedTheme {
    pub fn from_theme(theme: &Theme) -> Self {
        use std::str::FromStr;
        Self {
            primary: Color::from_str(&theme.primary).unwrap_or(Color::Cyan),
            secondary: Color::from_str(&theme.secondary).unwrap_or(Color::Yellow),
            background: Color::from_str(&theme.background).unwrap_or(Color::Reset),
            text: Color::from_str(&theme.text).unwrap_or(Color::White),
            text_muted: Color::from_str(&theme.text_muted).unwrap_or(Color::DarkGray),
            highlight_bg: Color::from_str(&theme.highlight_bg).unwrap_or(Color::White),
            highlight_fg: Color::from_str(&theme.highlight_fg).unwrap_or(Color::Black),
            selection_bg: Color::from_str(&theme.highlight_bg).unwrap_or(Color::White),
            selected_item: Color::from_str(&theme.highlight_fg).unwrap_or(Color::Black),
            error: Color::from_str(&theme.error).unwrap_or(Color::Red),
        }
    }

    pub fn base_style(&self) -> Style {
        Style::default().fg(self.text).bg(self.background)
    }

    pub fn muted_style(&self) -> Style {
        self.base_style().fg(self.text_muted)
    }

    pub fn primary_style(&self) -> Style {
        self.base_style().fg(self.primary)
    }

    pub fn secondary_style(&self) -> Style {
        self.base_style().fg(self.secondary)
    }

    pub fn error_style(&self) -> Style {
        self.base_style().fg(self.error)
    }

    pub fn selected_style(&self) -> Style {
        Style::default().fg(self.highlight_fg).bg(self.highlight_bg)
    }

    pub fn gauge_style(&self) -> Style {
        Style::default().fg(self.text).bg(self.text_muted)
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub ui: UIState,
    pub playback: PlaybackState,
    pub data: DataState,
}

impl AppState {
    pub fn new() -> Self {
        let config = crate::config::AppConfig::load();
        let condensed_lyrics_enabled = config.library.condensed_lyrics_enabled;
        let vis_bins = config.library.vis_bins;
        let has_local_music_dir = config.library.local_music_dir.is_some();
        let initial_mode = if config.spotify_credentials.is_none() && has_local_music_dir {
            AppMode::Normal
        } else {
            AppMode::Setup
        };

        let themes = crate::config::load_themes().unwrap_or_else(|_| {
            let mut fallback = HashMap::new();
            fallback.insert("default".to_string(), crate::config::bundled_default_theme());
            fallback
        });

        let default_theme = crate::config::bundled_default_theme();
        let active_theme_name = config
            .library
            .active_theme
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let active_theme_config = themes
            .get(&active_theme_name)
            .or_else(|| themes.get("default"))
            .unwrap_or(&default_theme)
            .clone();

        Self {
            ui: UIState::new(
                initial_mode,
                condensed_lyrics_enabled,
                vis_bins,
                themes,
                active_theme_config,
                config.library,
            ),
            playback: PlaybackState::default(),
            data: DataState::new(),
        }
    }

    pub fn get_visual_selection_range(&self) -> Option<(usize, usize)> {
        if self.ui.mode != AppMode::Visual {
            return None;
        }

        if let Some(start) = self.ui.visual_selection_start {
            let current = match self.ui.active_view {
                ActiveView::TrackList => self.ui.selected_track_index,
                ActiveView::SearchResults => self.ui.selected_search_index,
                ActiveView::Queue => self.ui.selected_queue_index,
                ActiveView::Library => self.ui.selected_playlist_index,
                ActiveView::Devices => self.ui.selected_device_index,
                ActiveView::ArtistList => self.ui.selected_artist_index,
                ActiveView::ArtistPage => self.ui.artist_page_album_index,
            };
            Some((std::cmp::min(start, current), std::cmp::max(start, current)))
        } else {
            None
        }
    }

    pub fn compute_library_view(&mut self) {
        use crate::config::SortMode;
        use crate::models::LibraryNode;
        use std::collections::HashSet;

        let mut view = Vec::new();

        // 0. Liked Songs (Always at the top)
        view.push(LibraryNode::Playlist {
            playlist: crate::models::Playlist {
                id: "LIKED_SONGS".to_string(),
                name: "♥️ Liked Songs".to_string(),
                owner: String::new(),
                owner_id: "spotify".to_string(),
                image_url: None,
            },
            indent: 0,
        });

        if self.ui.library_config.local_music_dir.is_some()
            || !self.data.local_library.tracks.is_empty()
        {
            view.push(LibraryNode::Playlist {
                playlist: crate::models::Playlist {
                    id: "local-library".to_string(),
                    name: "📁 Local Music".to_string(),
                    owner: "Local".to_string(),
                    owner_id: "local".to_string(),
                    image_url: None,
                },
                indent: 0,
            });
        }

        for playlist in self.data.local_playlists.to_library_playlists() {
            view.push(LibraryNode::Playlist {
                playlist,
                indent: 0,
            });
        }

        let pinned_set: HashSet<String> =
            self.ui.library_config.pinned.iter().cloned().collect();
        let mut folder_playlists: HashSet<String> = HashSet::new();

        // 1. Pinned Playlists
        for pid in &self.ui.library_config.pinned {
            if let Some(p) = self.data.playlists.iter().find(|p| &p.id == pid) {
                view.push(LibraryNode::Playlist {
                    playlist: p.clone(),
                    indent: 0,
                });
            }
        }

        // 2. Folders
        for folder in &self.ui.library_config.folders {
            for pid in &folder.playlists {
                folder_playlists.insert(pid.clone());
            }
            view.push(LibraryNode::Folder(folder.clone()));
            if folder.is_open {
                for pid in &folder.playlists {
                    if let Some(p) = self.data.playlists.iter().find(|p| &p.id == pid) {
                        view.push(LibraryNode::Playlist {
                            playlist: p.clone(),
                            indent: 1,
                        });
                    }
                }
            }
        }

        // 3. Loose playlists
        let mut loose: Vec<_> = self
            .data
            .playlists
            .iter()
            .filter(|p| !pinned_set.contains(&p.id) && !folder_playlists.contains(&p.id))
            .cloned()
            .collect();

        match self.ui.library_config.sort_mode {
            SortMode::Alphabetical => loose.sort_by_key(|a| a.name.to_lowercase()),
            SortMode::Creator => loose.sort_by_key(|a| a.owner.to_lowercase()),
            SortMode::Default => {}
        }

        for p in loose {
            view.push(LibraryNode::Playlist {
                playlist: p,
                indent: 0,
            });
        }

        self.data.library_view = view;
    }

    pub fn save_library_config(&self) {
        let mut config = crate::config::AppConfig::load();
        config.library = self.ui.library_config.clone();
        let _ = config.save();
    }

    pub fn begin_tracklist_load(&mut self, context: TrackListContext) {
        self.ui.active_view = ActiveView::TrackList;
        self.data.tracks.clear();
        self.ui.selected_track_index = 0;
        self.data.active_tracklist_context = Some(context.clone());
        self.data.tracklist_image_url = context.image_url.clone();
        self.clear_header_image();
        self.clear_pending_artist_page();
    }

    pub fn show_generated_tracks(&mut self, tracks: Vec<Track>, context: TrackListContext) {
        self.ui.active_view = ActiveView::TrackList;
        self.data.tracks = tracks;
        self.ui.selected_track_index = 0;
        self.data.active_tracklist_context = Some(context);
        self.data.tracklist_image_url = None;
        self.clear_header_image();
        self.clear_pending_artist_page();
    }

    pub fn show_local_library(&mut self) {
        let context = TrackListContext::local_library();
        self.ui.active_view = ActiveView::TrackList;
        self.data.tracks = self.data.local_library.to_tracks();
        self.ui.selected_track_index = 0;
        self.data.active_tracklist_context = Some(context);
        self.data.tracklist_image_url = None;
        self.clear_header_image();
        self.clear_pending_artist_page();
    }

    pub fn show_local_playlist(&mut self, playlist_id: &str, title: String) {
        let context = TrackListContext::local_playlist(playlist_id.to_string(), title);
        self.ui.active_view = ActiveView::TrackList;
        self.data.tracks = self
            .data
            .local_playlists
            .tracks_for_playlist(playlist_id, &self.data.local_library);
        self.ui.selected_track_index = 0;
        self.data.active_tracklist_context = Some(context);
        self.data.tracklist_image_url = None;
        self.clear_header_image();
        self.clear_pending_artist_page();
    }

    pub fn begin_artist_page_load(
        &mut self,
        artist_id: String,
        artist_name: String,
        image_url: Option<String>,
    ) {
        self.data.artist_page_data = Some(ArtistPageData {
            artist_id: artist_id.clone(),
            artist_name,
            image_url,
            albums: Vec::new(),
        });
        self.data.pending_artist_page_id = Some(artist_id);
        self.ui.artist_page_album_index = 0;
        self.data.artist_page_loading = true;
        self.data.artist_albums_loading = true;
        self.ui.active_view = ActiveView::ArtistPage;
        self.clear_header_image();
    }

    pub fn clear_pending_artist_page(&mut self) {
        self.data.pending_artist_page_id = None;
        self.data.artist_page_loading = false;
        self.data.artist_albums_loading = false;
    }

    fn clear_header_image(&mut self) {
        self.ui.active_library_header_image = None;
        self.ui.header_image_cache = None;
        self.ui.header_image_dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContextKind;

    #[test]
    fn begin_tracklist_load_preserves_album_context() {
        let mut state = AppState::new();
        let context = TrackListContext::album(
            "album-id".to_string(),
            "Album".to_string(),
            "Artist".to_string(),
            Some("cover".to_string()),
        );

        state.begin_tracklist_load(context.clone());

        assert_eq!(state.data.active_tracklist_context, Some(context));
        assert_eq!(
            state.data.active_tracklist_context.as_ref().map(|ctx| ctx.kind),
            Some(TrackListContextKind::Album)
        );
        assert_eq!(state.data.tracklist_image_url.as_deref(), Some("cover"));
    }

    #[test]
    fn generated_tracklists_do_not_look_like_playlists() {
        let mut state = AppState::new();
        state.show_generated_tracks(
            Vec::new(),
            TrackListContext::generated("TOP_TRACKS", "Top Tracks"),
        );

        let context = state.data.active_tracklist_context.as_ref().unwrap();
        assert_eq!(context.kind, TrackListContextKind::Generated);
        assert!(!context.can_modify_playlist(Some(&"user".to_string())));
    }

    #[test]
    fn beginning_tracklist_load_clears_pending_artist() {
        let mut state = AppState::new();
        state.begin_artist_page_load("artist-id".to_string(), "Artist".to_string(), None);

        state.begin_tracklist_load(TrackListContext::playlist(
            "playlist-id".to_string(),
            "Playlist".to_string(),
            "Owner".to_string(),
            "owner-id".to_string(),
            None,
        ));

        assert_eq!(state.data.pending_artist_page_id, None);
        assert!(!state.data.artist_page_loading);
    }

    #[test]
    fn local_music_entry_is_shown_when_library_has_tracks() {
        let mut state = AppState::new();
        state.data.local_library.tracks = vec![crate::models::LocalTrack {
            id: "local:track".to_string(),
            path: std::path::PathBuf::from("/music/track.mp3"),
            title: "Track".to_string(),
            artist: String::new(),
            album: String::new(),
            duration_ms: 0,
            artwork_path: None,
            file_size: 1,
            modified_unix_secs: 1,
        }];

        state.compute_library_view();

        assert!(state.data.library_view.iter().any(|node| matches!(
            node,
            crate::models::LibraryNode::Playlist { playlist, .. }
                if playlist.id == "local-library"
        )));
    }

    #[test]
    fn local_playlists_are_shown_in_library_view() {
        let mut state = AppState::new();
        state.data.local_playlists = crate::models::LocalPlaylists {
            playlists: vec![crate::models::LocalPlaylist {
                id: "local-playlist:one".to_string(),
                name: "Road".to_string(),
                created_unix_secs: 1,
                updated_unix_secs: 1,
                entries: Vec::new(),
            }],
        };

        state.compute_library_view();

        assert!(state.data.library_view.iter().any(|node| matches!(
            node,
            crate::models::LibraryNode::Playlist { playlist, .. }
                if playlist.id == "local-playlist:one" && playlist.owner_id == "local"
        )));
    }

    #[test]
    fn progress_estimation_when_playing() {
        let state = AppState::new();
        let pb = &state.playback;
        let effective = effective_progress_ms(pb);
        assert_eq!(effective, 0);
    }

    #[test]
    fn progress_capped_at_duration() {
        let mut state = AppState::new();
        state.playback.duration_ms = 5000;
        state.playback.progress_ms = 4900;
        state.playback.is_playing = true;
        state.playback.playback_last_updated_at = Some(
            std::time::Instant::now() - std::time::Duration::from_secs(10),
        );
        let effective = effective_progress_ms(&state.playback);
        assert_eq!(effective, 5000);
    }

    #[test]
    fn progress_paused_uses_stored_value() {
        let mut state = AppState::new();
        state.playback.duration_ms = 10000;
        state.playback.progress_ms = 3000;
        state.playback.is_playing = false;
        state.playback.playback_last_updated_at = Some(
            std::time::Instant::now() - std::time::Duration::from_secs(5),
        );
        let effective = effective_progress_ms(&state.playback);
        assert_eq!(effective, 3000);
    }

    #[test]
    fn progress_no_timestamp_defaults_to_stored() {
        let mut state = AppState::new();
        state.playback.duration_ms = 10000;
        state.playback.progress_ms = 5000;
        state.playback.is_playing = true;
        state.playback.playback_last_updated_at = None;
        let effective = effective_progress_ms(&state.playback);
        assert_eq!(effective, 5000);
    }

    #[test]
    fn progress_advances_with_elapsed_time() {
        let mut state = AppState::new();
        state.playback.duration_ms = 300000;
        state.playback.progress_ms = 10000;
        state.playback.is_playing = true;
        state.playback.playback_last_updated_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(3));
        let effective = effective_progress_ms(&state.playback);
        // 10000 + ~3000ms = ~13000, allow 120ms tolerance
        assert!(
            effective >= 12940 && effective <= 13060,
            "expected ~13000, got {effective}"
        );
    }
}

fn effective_progress_ms(pb: &PlaybackState) -> u32 {
    if pb.is_playing {
        if let Some(last_updated) = pb.playback_last_updated_at {
            let elapsed = last_updated.elapsed().as_millis() as u32;
            (pb.progress_ms + elapsed).min(pb.duration_ms)
        } else {
            pb.progress_ms
        }
    } else {
        pb.progress_ms
    }
}
