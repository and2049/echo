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
    pub pending_d_press: bool,
    pub folder_delete_prompt: Option<String>,
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
            library_config: crate::config::LibraryConfig::default(),
            library_view: vec![],
            saved_albums: vec![],
            active_library_tab: LibraryTab::Playlists,
            operation_register: vec![],
            command_buffer: String::new(),
            pending_d_press: false,
            folder_delete_prompt: None,
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

impl AppState {
    pub fn compute_library_view(&mut self) {
        use crate::models::LibraryNode;
        use crate::config::SortMode;
        use std::collections::HashSet;

        let mut view = Vec::new();

        // 0. Liked Songs (Always at the top)
        view.push(LibraryNode::Playlist {
            playlist: crate::models::Playlist {
                id: "LIKED_SONGS".to_string(),
                name: "♥️ Liked Songs".to_string(),
                owner: "Spotify".to_string(),
            },
            indent: 0,
        });

        let pinned_set: HashSet<String> = self.library_config.pinned.iter().cloned().collect();
        let mut folder_playlists: HashSet<String> = HashSet::new();

        // 1. Pinned Playlists
        for pid in &self.library_config.pinned {
            if let Some(p) = self.playlists.iter().find(|p| &p.id == pid) {
                view.push(LibraryNode::Playlist { playlist: p.clone(), indent: 0 });
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
                        view.push(LibraryNode::Playlist { playlist: p.clone(), indent: 1 });
                    }
                }
            }
        }

        // 3. Loose playlists
        let mut loose: Vec<_> = self.playlists.iter()
            .filter(|p| !pinned_set.contains(&p.id) && !folder_playlists.contains(&p.id))
            .cloned()
            .collect();

        match self.library_config.sort_mode {
            SortMode::Alphabetical => loose.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortMode::Creator => loose.sort_by(|a, b| a.owner.to_lowercase().cmp(&b.owner.to_lowercase())),
            SortMode::Default => {}
        }

        for p in loose {
            view.push(LibraryNode::Playlist { playlist: p, indent: 0 });
        }

        self.library_view = view;
    }

    pub fn save_library_config(&self) {
        let mut config = crate::config::AppConfig::load();
        config.library = self.library_config.clone();
        let _ = config.save();
    }
}
