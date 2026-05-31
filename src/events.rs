use crossterm::event::KeyEvent;
use crate::models::{Playlist, Track};

pub enum AppEvent {
    Key(KeyEvent),
    LoadPlaylistTracks(String),
    PlayTrack { playlist_id: String, track_id: String, duration_ms: u32 },
    StartAuth,
    Quit,
    TogglePlayback(bool),
    NextTrack,
    PreviousTrack,
    ToggleShuffle(bool),
    LoadTrackMetadata(String),
}

pub enum WorkerEvent {
    Tick,
    AuthenticationComplete,
    PlaylistsLoaded(Vec<Playlist>),
    TracksLoaded(Vec<Track>),
    PlaybackStarted(u32),
    SyncPlaybackState {
        is_playing: bool,
        is_shuffled: bool,
        progress_ms: u32,
        duration_ms: u32,
        track_id: Option<String>,
    },
    ForceRedraw,
    TrackMetadataLoaded {
        title: String,
        artist: String,
        image_url: Option<String>,
    },
    TrackImageProcessed(ratatui_image::protocol::Protocol),
}
