//! Shared startup: channels, worker, config and initial state.
//!
//! Both frontends begin the same way — spawn the [`Worker`](crate::worker::Worker) on the
//! current tokio runtime, load config and caches into an [`AppState`], and pick the starting
//! mode. Only the render loop that follows differs, so everything up to that point lives here.

use tokio::sync::mpsc;

use crate::app::{AppMode, AppState};
use crate::config::AppConfig;
use crate::events::{AppEvent, WorkerEvent};
use crate::worker::Worker;

pub struct Bootstrap {
    pub state: AppState,
    pub config: AppConfig,
    /// Frontend → worker. Unbounded so sends need no async context.
    pub app_tx: mpsc::UnboundedSender<AppEvent>,
    /// Worker → frontend. The frontend owns the receiving end of its own redraw source.
    pub app_rx: mpsc::Receiver<WorkerEvent>,
    /// Cloneable sender for the worker→frontend channel, used by image tasks and
    /// `apply_worker_event`.
    pub worker_tx: mpsc::Sender<WorkerEvent>,
}

/// Must be called from within a tokio runtime — the worker is spawned onto it.
pub fn init() -> Bootstrap {
    let (app_tx, worker_rx) = mpsc::unbounded_channel::<AppEvent>();
    let (worker_tx, app_rx) = mpsc::channel::<WorkerEvent>(32);

    let worker = Worker::new(worker_rx, worker_tx.clone(), app_tx.clone());
    tokio::spawn(async move {
        worker.run().await;
    });

    let config = AppConfig::load();
    let mut state = AppState::new();
    let cache = AppConfig::load_cache();
    state.data.liked_tracks = cache.liked_tracks.clone();
    if let Some(playlists) = cache.get_playlists() {
        state.data.playlists = playlists;
        state.compute_library_view();
    }
    if let Some(albums) = cache.get_saved_albums() {
        state.data.saved_albums = albums;
    }
    if let Some(tracks) = cache.get_top_tracks() {
        state.data.top_tracks = tracks;
    }
    if let Some(tracks) = cache.get_recently_played() {
        state.data.recently_played = tracks;
    }
    if let Some(artists) = cache.get_followed_artists() {
        state.data.followed_artists = artists;
    }
    state.ui.library_config = config.library.clone();

    if config.spotify_credentials.is_some() {
        state.ui.mode = AppMode::Authenticating;
        let _ = app_tx.send(AppEvent::StartAuth);
    } else if config.library.local_music_dir.is_some() {
        state.ui.mode = AppMode::Normal;
    } else {
        state.ui.mode = AppMode::Setup;
    }
    if let Some(path) = config.library.local_music_dir.clone() {
        let _ = app_tx.send(AppEvent::StartLocalLibraryAutoRefresh(path));
    }

    Bootstrap {
        state,
        config,
        app_tx,
        app_rx,
        worker_tx,
    }
}
