use crate::models::{Playlist, Track};

#[derive(Default)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub is_shuffled: bool,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub playing_track_id: Option<String>,
    pub playing_track_title: String,
    pub playing_track_artist: String,
    pub playing_track_image: Option<ratatui_image::protocol::Protocol>,
}

#[derive(PartialEq)]
pub enum ActiveView {
    Library,
    TrackList,
}

#[derive(PartialEq)]
pub enum AppMode {
    Setup,
    Authenticating,
    Normal,
    Visual,
    Command,
}

pub struct AppState {
    pub mode: AppMode,
    pub active_view: ActiveView,
    pub is_running: bool,
    pub playlists: Vec<Playlist>,
    pub selected_playlist_index: usize,
    pub tracks: Vec<Track>,
    pub selected_track_index: usize,
    pub setup_client_id: String,
    pub setup_client_secret: String,
    pub setup_focus_secret: bool,
    pub image_picker: Option<ratatui_image::picker::Picker>,
    pub playback: PlaybackState,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: AppMode::Setup,
            active_view: ActiveView::Library,
            is_running: true,
            playlists: vec![],
            selected_playlist_index: 0,
            tracks: Vec::new(),
            selected_track_index: 0,
            setup_client_id: String::new(),
            setup_client_secret: String::new(),
            setup_focus_secret: false,
            image_picker: None,
            playback: PlaybackState::default(),
        }
    }
}
