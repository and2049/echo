#[derive(PartialEq)]
pub enum ActiveView {
    Library,
    TrackList,
}

pub enum AppMode {
    Normal,
    Visual,
    Command,
}

pub struct AppState {
    pub mode: AppMode,
    pub active_view: ActiveView,
    pub is_running: bool,
    pub playlists: Vec<String>,
    pub selected_playlist_index: usize,
    pub tracks: Vec<String>,
    pub selected_track_index: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: AppMode::Normal,
            active_view: ActiveView::Library,
            is_running: true,
            playlists: vec![
                "Liked Songs".to_string(),
                "Discover Weekly".to_string(),
                "Release Radar".to_string(),
                "Daily Mix 1".to_string(),
            ],
            selected_playlist_index: 0,
            tracks: vec![
                "Track 1 - Artist A".to_string(),
                "Track 2 - Artist B".to_string(),
                "Track 3 - Artist C".to_string(),
                "Track 4 - Artist D".to_string(),
                "Track 5 - Artist E".to_string(),
            ],
            selected_track_index: 0,
        }
    }
}
