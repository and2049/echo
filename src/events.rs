use crate::models::{PlaybackItem, Playlist, Track};
use crossterm::event::KeyEvent;

pub enum AppEvent {
    Key(KeyEvent),
    LoadPlaylistTracks(String),
    PlayTrack {
        playlist_id: String,
        track_id: String,
        title: String,
        artist: String,
        duration_ms: u32,
        image_url: Option<String>,
    },
    StartAuth,
    Quit,
    TogglePlayback(bool),
    NextTrack {
        current_track_id: Option<String>,
    },
    PreviousTrack {
        current_track_id: Option<String>,
    },
    ToggleShuffle(bool),
    LoadTrackMetadata(String),
}

pub enum WorkerEvent {
    Tick,
    AuthenticationComplete,
    PlaylistsLoaded(Vec<Playlist>),
    TracksLoaded(Vec<Track>),
    PlaybackStarted {
        item: PlaybackItem,
    },
    SyncPlaybackState {
        is_playing: bool,
        is_shuffled: bool,
        progress_ms: u32,
        item: Option<PlaybackItem>,
    },
    ForceRedraw,
    TrackMetadataLoaded {
        track_id: String,
        title: String,
        artist: String,
        image_url: Option<String>,
    },
    TrackImageProcessed {
        track_id: String,
        protocol: ratatui_image::protocol::Protocol,
    },
}
