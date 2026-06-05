use crate::models::{
    PlaybackItem, PlaybackTarget, Playlist, SearchResults, Track, TrackListContext,
};
use crossterm::event::KeyEvent;
use std::path::PathBuf;

pub enum AppEvent {
    Key(KeyEvent),
    LoadContextTracks(TrackListContext),
    RefreshContextTracks(TrackListContext),
    RefreshLibraryLists,
    ScanLocalLibrary(PathBuf),
    RescanLocalLibrary,
    StartLocalLibraryAutoRefresh(PathBuf),
    PlayTrack {
        target: PlaybackTarget,
        track_id: String,
        title: String,
        artist: String,
        duration_ms: u32,
        image_url: Option<String>,
        album_id: Option<String>,
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
    AddTracksToPlaylist(String, Vec<Track>),
    RemoveTracksFromPlaylist(String, Vec<String>),
    CreatePlaylist(String),
    CreateLocalPlaylist(String),
    RenamePlaylist(String, String),
    DeletePlaylists(Vec<String>),
    DeletePlaylistTracks(String, Vec<String>),
    SaveAlbums(Vec<String>),
    RemoveAlbums(Vec<String>),
    ToggleTrackLike(String, bool),
    ReloadHeaderImage,
    FetchDevices,
    TransferPlayback(String),
    ToggleLyricsModal,
    ToggleCondensedLyrics,
    FetchLyrics(String, String, String, u32),
    ForcePlaybackSync,
    CancelArtistPageLoad,
    FetchTopTracks,
    FetchRecentlyPlayed,
    FetchFollowedArtists,
    LoadArtistPage {
        artist_id: String,
        artist_name: Option<String>,
        artist_image_url: Option<String>,
    },
    RefreshArtistAlbums {
        artist_id: String,
    },
}

pub enum WorkerEvent {
    Tick,
    AuthenticationComplete,
    UserIdentityLoaded(String),
    PlaylistsLoaded(Vec<Playlist>),
    AlbumsLoaded(Vec<crate::models::Album>),
    LocalLibraryLoaded {
        library: crate::models::LocalLibrary,
        report: crate::models::LocalScanReport,
    },
    LocalPlaylistsLoaded(crate::models::LocalPlaylists),
    TracksLoaded(Vec<Track>, TrackListContext),
    TracksLoadFailed {
        context_id: String,
        message: String,
    },
    ApiRequestFailed {
        label: String,
        message: String,
    },
    AudioVisualizationReady(
        std::sync::Arc<parking_lot::Mutex<[f32; 32]>>,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
    ),
    PlaybackStarted {
        item: PlaybackItem,
    },
    PlaybackControlState {
        is_playing: bool,
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
    ForceContextRefresh,
    TrackMetadataLoaded {
        track_id: String,
        title: String,
        artist: String,
        image_url: Option<String>,
    },
    TrackImageProcessed {
        track_id: String,
        protocol: ratatui_image::protocol::StatefulProtocol,
    },
    SearchResultsLoaded(SearchResults),
    QueueLoaded(Vec<Track>),
    TracksQueued(usize),
    HeaderImageProcessed(ratatui_image::protocol::StatefulProtocol),
    LikedStatusUpdate(std::collections::HashMap<String, bool>),
    DevicesLoaded(Vec<crate::models::Device>),
    LyricsLoaded(Option<crate::models::Lyrics>),
    TopTracksLoaded(Vec<Track>),
    RecentlyPlayedLoaded(Vec<Track>),
    FollowedArtistsLoaded(Vec<crate::models::Artist>),
    ArtistPageOpened {
        artist_id: String,
        artist_name: String,
        artist_image_url: Option<String>,
    },
    ArtistAlbumsLoaded {
        artist_id: String,
        albums: Vec<crate::models::Album>,
    },
    ArtistAlbumsLoadFailed {
        artist_id: String,
        message: String,
    },
    ArtistAlbumsRateLimited {
        artist_id: String,
        retry_after_secs: u64,
    },
    /// Fired when an artist's profile image has been resolved (e.g. from a
    /// secondary API call when the image URL wasn't known at page-open time).
    ArtistImageResolved {
        artist_id: String,
        image_url: String,
    },
}
