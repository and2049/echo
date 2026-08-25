use crate::models::{
    PlaybackItem, PlaybackTarget, Playlist, SearchResults, Track, TrackListContext,
};
use std::path::PathBuf;

pub enum AppEvent {
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
    /// Start a whole Spotify playlist/album from the top — context playback with no track
    /// offset, so it works before the context's tracks are loaded.
    PlayContext {
        context_id: String,
        is_album: bool,
        /// The UI's playing track id at dispatch time — the post-play sync's reference for
        /// telling a real track change apart from Spotify's eventually-consistent echo of the
        /// pre-command track.
        current_track_id: Option<String>,
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
    SeekTo(u32),
    LoadTrackMetadata(String),
    GlobalSearch(String),
    AddToQueue(Vec<String>),
    FetchQueue,
    AddTracksToPlaylist(String, Vec<Track>),
    RemoveTracksFromPlaylist(String, Vec<String>),
    /// Arm (or with `None`, clear) the sleep timer that pauses playback when it fires.
    SetSleepTimer {
        duration: Option<std::time::Duration>,
    },
    /// Reorder one track of an owned playlist. `from`/`to` are positions in the original
    /// (unsorted) track order; `track_id` locates local-playlist entries robustly.
    MoveTrack {
        playlist_id: String,
        track_id: String,
        from: usize,
        to: usize,
    },
    CreatePlaylist(String),
    CreateLocalPlaylist(String),
    RenamePlaylist(String, String),
    DeletePlaylists(Vec<String>),
    SaveAlbums(Vec<String>),
    RemoveAlbums(Vec<String>),
    ToggleTrackLike(String, bool),
    ReloadHeaderImage,
    FetchDevices,
    TransferPlayback(String),
    FetchLyrics(String, String, String, u32),
    ForcePlaybackSync,
    CancelArtistPageLoad,
    FetchTopTracks {
        range: crate::models::TopItemsRange,
    },
    FetchTopArtists {
        range: crate::models::TopItemsRange,
    },
    FetchRecentlyPlayed,
    FetchFollowedArtists,
    FetchWhatsNew,
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
    SpotifyReauthorizationRequired,
    SpotifyAuthenticationFailed {
        message: String,
    },
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
    AudioOutputUnavailable {
        message: String,
    },
    AudioOutputRecovered,
    /// The sleep timer fired and playback was paused worker-side.
    SleepTimerExpired,
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
        context: Option<crate::models::PlayingContext>,
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
        artwork: crate::artwork::SharedArtwork,
    },
    SearchResultsLoaded(SearchResults),
    QueueLoaded(Vec<Track>),
    TracksQueued(usize),
    HeaderImageProcessed(crate::artwork::SharedArtwork),
    ThumbnailProcessed {
        url: String,
        artwork: Option<crate::artwork::SharedArtwork>,
    },
    LikedStatusUpdate(std::collections::HashMap<String, bool>),
    DevicesLoaded(Vec<crate::models::Device>),
    LyricsLoaded(Option<crate::models::Lyrics>),
    TopTracksLoaded(Vec<Track>),
    TopArtistsLoaded(Vec<crate::models::Artist>),
    RecentlyPlayedLoaded(Vec<Track>),
    FollowedArtistsLoaded(Vec<crate::models::Artist>),
    /// Cumulative snapshot of the What's New scan: the full merged album list so far,
    /// plus scan progress. `done == total` marks the final emission.
    WhatsNewLoaded {
        albums: Vec<crate::models::Album>,
        done: usize,
        total: usize,
    },
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
    ArtistTopTracksLoaded {
        artist_id: String,
        tracks: Vec<Track>,
    },
    ArtistTopTracksLoadFailed {
        artist_id: String,
        message: String,
    },
}
