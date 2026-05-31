use crate::models::{Playlist, Track};

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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: AppMode::Normal, // will be overridden in main if config is missing
            active_view: ActiveView::Library,
            is_running: true,
            playlists: vec![],
            selected_playlist_index: 0,
            tracks: vec![],
            selected_track_index: 0,
            setup_client_id: String::new(),
            setup_client_secret: String::new(),
            setup_focus_secret: false,
        }
    }
}
