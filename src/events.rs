use crossterm::event::KeyEvent;
use crate::models::{Playlist, Track};

pub enum AppEvent {
    Key(KeyEvent),
    LoadPlaylistTracks(String),
    StartAuth,
    Quit,
}

pub enum WorkerEvent {
    Tick,
    AuthenticationComplete,
    PlaylistsLoaded(Vec<Playlist>),
    TracksLoaded(Vec<Track>),
}
