use crate::models::{PlaybackItem, Playlist, SearchResults, Track};
use crossterm::event::KeyEvent;

pub enum AppEvent {
    Key(KeyEvent),
    LoadContextTracks(String, bool),
    PlayTrack {
        context_id: String,
        track_id: String,
        is_album: bool,
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
    SetRepeatMode(String),
    SetVolume(u8),
    LoadTrackMetadata(String),
    GlobalSearch(String),
    AddToQueue(Vec<String>),
    FetchQueue,
}

pub enum WorkerEvent {
    Tick,
    AuthenticationComplete,
    PlaylistsLoaded(Vec<Playlist>),
    AlbumsLoaded(Vec<crate::models::Album>),
    TracksLoaded(Vec<Track>),
    PlaybackStarted {
        item: PlaybackItem,
    },
    SyncPlaybackState {
        is_playing: bool,
        is_shuffled: bool,
        repeat_mode: String,
        volume: Option<u32>,
        device_name: String,
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
    SearchResultsLoaded(SearchResults),
    QueueLoaded(Vec<Track>),
    StatusMessage(String),
    TracksQueued(usize),
}
