use crate::models::{Playlist, SearchResults, Track};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Style};

pub struct PlaybackState {
    pub is_playing: bool,
    pub is_shuffled: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub playing_track_id: Option<String>,
    pub playing_track_title: String,
    pub playing_track_artist: String,
    pub playing_track_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub previous_track_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub fetching_track_id: Option<String>,
    pub device_name: String,
    pub repeat_mode: String,
    pub volume: u32,
    pub audio_visualization: Option<std::sync::Arc<parking_lot::Mutex<[f32; 32]>>>,
    pub enable_visualizer: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

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
    pub fn from_theme(theme: &crate::config::Theme) -> Self {
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
            playing_track_image: None,
            previous_track_image: None,
            fetching_track_id: None,
            device_name: "echo-rs".to_string(),
            repeat_mode: "Off".to_string(),
            volume: 100,
            audio_visualization: None,
            enable_visualizer: None,
        }
    }
}

#[derive(PartialEq)]
pub enum ActiveView {
    Library,
    TrackList,
    SearchResults,
    Queue,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum SearchTab {
    Tracks,
    Albums,
}

#[derive(PartialEq)]
pub enum AppMode {
    Setup,
    Authenticating,
    Normal,
    Command,
    Search,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum LibraryTab {
    Playlists,
    Albums,
}

pub struct AppState {
    pub mode: AppMode,
    pub active_view: ActiveView,
    pub is_running: bool,
    pub playlists: Vec<Playlist>,
    pub library_config: crate::config::LibraryConfig,
    pub library_view: Vec<crate::models::LibraryNode>,
    pub saved_albums: Vec<crate::models::Album>,
    pub active_library_tab: LibraryTab,
    pub operation_register: Vec<String>,
    pub command_buffer: String,
    pub command_suggestions: Vec<String>,
    pub command_suggestion_index: Option<usize>,
    pub command_base_buffer: String,
    pub status_message: Option<String>,
    pub status_message_expiry: Option<std::time::Instant>,
    pub recent_queue_count: usize,
    pub pending_d_press: bool,
    pub folder_delete_prompt: Option<String>,
    pub selected_playlist_index: usize,
    pub tracks: Vec<Track>,
    pub selected_track_index: usize,
    pub setup_client_id: String,
    pub setup_client_secret: String,
    pub setup_focus_secret: bool,
    pub image_picker: Option<ratatui_image::picker::Picker>,
    pub active_library_header_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub header_image_cache: Option<Buffer>,
    pub header_image_dirty: bool,
    pub playback: PlaybackState,
    pub themes: std::collections::HashMap<String, crate::config::Theme>,
    pub active_theme: ResolvedTheme,
    pub needs_terminal_clear: bool,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_results: SearchResults,
    pub search_context_query: String,
    pub active_search_tab: SearchTab,
    pub selected_search_index: usize,
    pub prev_view: Option<ActiveView>,
    pub queue: Vec<crate::models::Track>,
    pub selected_queue_index: usize,
    pub tracklist_context_metadata: Option<(String, String)>,
}

impl AppState {
    pub fn new() -> Self {
        let config = crate::config::AppConfig::load();

        let themes = crate::config::load_themes().unwrap_or_else(|_| {
            let mut fallback = std::collections::HashMap::new();
            fallback.insert("default".to_string(), crate::config::Theme::default());
            fallback
        });

        let active_theme_name = config
            .library
            .active_theme
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let active_theme_config = themes
            .get(&active_theme_name)
            .unwrap_or(&crate::config::Theme::default())
            .clone();

        let active_theme = ResolvedTheme::from_theme(&active_theme_config);

        Self {
            mode: AppMode::Setup,
            active_view: ActiveView::Library,
            is_running: true,
            playlists: vec![],
            library_config: config.library,
            library_view: vec![],
            saved_albums: vec![],
            active_library_tab: LibraryTab::Playlists,
            operation_register: vec![],
            command_buffer: String::new(),
            command_suggestions: vec![],
            command_suggestion_index: None,
            command_base_buffer: String::new(),
            status_message: None,
            status_message_expiry: None,
            recent_queue_count: 0,
            pending_d_press: false,
            folder_delete_prompt: None,
            selected_playlist_index: 0,
            tracks: Vec::new(),
            selected_track_index: 0,
            setup_client_id: String::new(),
            setup_client_secret: String::new(),
            setup_focus_secret: false,
            image_picker: None,
            active_library_header_image: None,
            header_image_cache: None,
            header_image_dirty: false,
            playback: PlaybackState::default(),
            themes,
            active_theme,
            needs_terminal_clear: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_results: SearchResults::default(),
            search_context_query: String::new(),
            active_search_tab: SearchTab::Tracks,
            selected_search_index: 0,
            prev_view: None,
            queue: Vec::new(),
            selected_queue_index: 0,
            tracklist_context_metadata: None,
        }
    }
}

impl AppState {
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
                owner: "Spotify".to_string(),
                image_url: None,
            },
            indent: 0,
        });

        let pinned_set: HashSet<String> = self.library_config.pinned.iter().cloned().collect();
        let mut folder_playlists: HashSet<String> = HashSet::new();

        // 1. Pinned Playlists
        for pid in &self.library_config.pinned {
            if let Some(p) = self.playlists.iter().find(|p| &p.id == pid) {
                view.push(LibraryNode::Playlist {
                    playlist: p.clone(),
                    indent: 0,
                });
            }
        }

        // 2. Folders
        for folder in &self.library_config.folders {
            for pid in &folder.playlists {
                folder_playlists.insert(pid.clone());
            }
            view.push(LibraryNode::Folder(folder.clone()));
            if folder.is_open {
                for pid in &folder.playlists {
                    if let Some(p) = self.playlists.iter().find(|p| &p.id == pid) {
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
            .playlists
            .iter()
            .filter(|p| !pinned_set.contains(&p.id) && !folder_playlists.contains(&p.id))
            .cloned()
            .collect();

        match self.library_config.sort_mode {
            SortMode::Alphabetical => {
                loose.sort_by_key(|a| a.name.to_lowercase())
            }
            SortMode::Creator => {
                loose.sort_by_key(|a| a.owner.to_lowercase())
            }
            SortMode::Default => {}
        }

        for p in loose {
            view.push(LibraryNode::Playlist {
                playlist: p,
                indent: 0,
            });
        }

        self.library_view = view;
    }

    pub fn save_library_config(&self) {
        let mut config = crate::config::AppConfig::load();
        config.library = self.library_config.clone();
        let _ = config.save();
    }
}
