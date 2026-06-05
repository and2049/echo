pub mod api;
pub mod artist_page;
pub mod audio;
pub mod browse;
pub mod errors;
pub mod local_files;
pub mod local_playback;
pub mod media;
pub mod tracks;
pub mod visualization;

use crate::config::AppConfig;
use crate::events::{AppEvent, WorkerEvent};
use crate::models::{
    LocalLibrary, LocalPlaylist, LocalPlaylistEntry, PlaybackItem, PlaybackTarget, Track,
    TrackSource, stable_local_playlist_id,
};
use api::SpotifyWorker;
use local_playback::{LocalPlaybackEngine, LocalPlaybackSnapshot, RepeatMode};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rspotify::clients::OAuthClient;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use tokio::sync::mpsc;

type LocalScanResult = std::result::Result<(LocalLibrary, crate::models::LocalScanReport), String>;

pub struct Worker {
    rx: mpsc::Receiver<AppEvent>,
    tx: mpsc::Sender<WorkerEvent>,
    media_tx: mpsc::Sender<media::MediaUpdate>,
    first_party: Option<api::first_party::SpotifyWebApi>,
    artist_page_generation: Arc<AtomicU64>,
}

fn save_playlists_cache(playlists: Vec<crate::models::Playlist>) {
    let mut cache = AppConfig::load_cache();
    cache.set_playlists(playlists);
    let _ = AppConfig::save_cache(&cache);
}

fn save_saved_albums_cache(albums: Vec<crate::models::Album>) {
    let mut cache = AppConfig::load_cache();
    cache.set_saved_albums(albums);
    let _ = AppConfig::save_cache(&cache);
}

fn invalidate_playlist_context_cache(playlist_id: &str) {
    let mut cache = AppConfig::load_cache();
    cache.invalidate_playlist_context(playlist_id);
    let _ = AppConfig::save_cache(&cache);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ActivePlaybackSource {
    Spotify,
    Local,
}

async fn emit_local_snapshot(
    tx: &mpsc::Sender<WorkerEvent>,
    media_tx: &mpsc::Sender<media::MediaUpdate>,
    snapshot: LocalPlaybackSnapshot,
    playback_started: bool,
) {
    if playback_started && let Some(item) = snapshot.item.clone() {
        let _ = tx.send(WorkerEvent::PlaybackStarted { item }).await;
    }
    if let Some(item) = snapshot.item.clone() {
        let _ = media_tx
            .send(media::MediaUpdate::Metadata {
                title: item.title.clone(),
                artist: item.artist.clone(),
                album: "Local Music".to_string(),
                duration_ms: item.duration_ms,
                cover_url: item.image_url.clone(),
            })
            .await;
    }
    let _ = media_tx
        .send(media::MediaUpdate::Playback(
            snapshot.is_playing,
            snapshot.progress_ms,
        ))
        .await;
    let _ = tx
        .send(WorkerEvent::SyncPlaybackState {
            is_playing: snapshot.is_playing,
            is_shuffled: snapshot.is_shuffled,
            repeat_mode: snapshot.repeat_mode,
            volume: Some(snapshot.volume),
            device_name: "Local".to_string(),
            progress_ms: snapshot.progress_ms,
            item: snapshot.item,
        })
        .await;
    let _ = tx.send(WorkerEvent::QueueLoaded(snapshot.queue)).await;
}

fn resolve_local_queue_tracks(track_ids: &[String], library: &LocalLibrary) -> Vec<Track> {
    track_ids
        .iter()
        .filter_map(|track_id| {
            library
                .track_by_id(track_id)
                .map(crate::models::LocalTrack::to_track)
        })
        .collect()
}

fn merged_search_results(
    spotify: Option<crate::models::SearchResults>,
    local: crate::models::SearchResults,
) -> crate::models::SearchResults {
    let mut results = spotify.unwrap_or_default();
    results.tracks.extend(local.tracks);
    results.albums.extend(local.albums);
    results.artists.extend(local.artists);
    results
}

fn current_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn schedule_local_scan(
    path: PathBuf,
    scan_tx: &mpsc::Sender<(PathBuf, LocalScanResult)>,
    scan_inflight: &mut bool,
    pending_scan: &mut Option<PathBuf>,
) {
    if *scan_inflight {
        *pending_scan = Some(path);
        return;
    }

    *scan_inflight = true;
    let tx = scan_tx.clone();
    tokio::task::spawn_blocking(move || {
        let result = run_local_scan(&path);
        let _ = tx.blocking_send((path, result));
    });
}

fn run_local_scan(path: &Path) -> LocalScanResult {
    let previous = AppConfig::load_local_library();
    let (library, report) =
        local_files::scan_local_library(path, &previous).map_err(|error| error.to_string())?;
    AppConfig::save_local_library(&library).map_err(|error| error.to_string())?;
    Ok((library, report))
}

fn start_local_watcher(
    root: PathBuf,
    watch_tx: mpsc::Sender<PathBuf>,
) -> anyhow::Result<RecommendedWatcher> {
    let callback_root = root.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else {
            return;
        };
        if event.paths.is_empty()
            || event
                .paths
                .iter()
                .any(|path| local_watch_path_relevant(path))
        {
            let _ = watch_tx.blocking_send(callback_root.clone());
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    Ok(watcher)
}

fn local_watch_path_relevant(path: &Path) -> bool {
    local_files::is_supported_audio_file(path) || is_folder_artwork_candidate(path)
}

fn is_folder_artwork_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "cover.jpg"
            | "cover.jpeg"
            | "cover.png"
            | "folder.jpg"
            | "folder.jpeg"
            | "folder.png"
            | "front.jpg"
            | "front.png"
    )
}

async fn hydrate_library_lists(sp: &SpotifyWorker, tx: mpsc::Sender<WorkerEvent>) {
    let cache = AppConfig::load_cache();
    let playlists_need_refresh = if let Some(entry) = cache.get_playlists_entry() {
        let needs_refresh = crate::config::CacheData::library_list_needs_refresh(&entry);
        let _ = tx.send(WorkerEvent::PlaylistsLoaded(entry.value)).await;
        needs_refresh
    } else {
        true
    };

    let albums_need_refresh = if let Some(entry) = cache.get_saved_albums_entry() {
        let needs_refresh = crate::config::CacheData::library_list_needs_refresh(&entry);
        let _ = tx.send(WorkerEvent::AlbumsLoaded(entry.value)).await;
        needs_refresh
    } else {
        true
    };

    if playlists_need_refresh {
        let sp = sp.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Ok(playlists) = sp.fetch_playlists().await {
                save_playlists_cache(playlists.clone());
                let _ = tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
            }
        });
    }

    if albums_need_refresh {
        let sp = sp.clone();
        tokio::spawn(async move {
            if let Ok(albums) = sp.fetch_albums().await {
                save_saved_albums_cache(albums.clone());
                let _ = tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn resolves_local_queue_tracks_from_library_ids() {
        let library = crate::models::LocalLibrary {
            tracks: vec![
                crate::models::LocalTrack {
                    id: "local:a".to_string(),
                    path: PathBuf::from("/music/a.wav"),
                    title: "A".to_string(),
                    artist: "Artist A".to_string(),
                    album: "Album A".to_string(),
                    duration_ms: 1_000,
                    artwork_path: None,
                    file_size: 10,
                    modified_unix_secs: 20,
                },
                crate::models::LocalTrack {
                    id: "local:b".to_string(),
                    path: PathBuf::from("/music/b.wav"),
                    title: "B".to_string(),
                    artist: "Artist B".to_string(),
                    album: "Album B".to_string(),
                    duration_ms: 2_000,
                    artwork_path: None,
                    file_size: 11,
                    modified_unix_secs: 21,
                },
            ],
        };

        let tracks = resolve_local_queue_tracks(
            &["local:b".to_string(), "local:missing".to_string()],
            &library,
        );

        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].id, "local:b");
        assert_eq!(tracks[0].source, TrackSource::Local);
        assert_eq!(
            tracks[0].local_path.as_deref(),
            Some(Path::new("/music/b.wav"))
        );
    }

    #[test]
    fn merged_search_results_keep_spotify_and_local_tracks() {
        let spotify = crate::models::SearchResults {
            tracks: vec![crate::models::SearchTrack {
                id: "spotify".to_string(),
                source: TrackSource::Spotify,
                local_path: None,
                name: "Spotify".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                duration_ms: 1,
                image_url: None,
                album_id: None,
                artist_id: None,
            }],
            albums: Vec::new(),
            artists: Vec::new(),
        };
        let local = crate::models::SearchResults {
            tracks: vec![crate::models::SearchTrack {
                id: "local:a".to_string(),
                source: TrackSource::Local,
                local_path: Some(PathBuf::from("/music/a.wav")),
                name: "Local".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                duration_ms: 1,
                image_url: None,
                album_id: None,
                artist_id: None,
            }],
            albums: Vec::new(),
            artists: Vec::new(),
        };

        let merged = merged_search_results(Some(spotify), local);

        assert_eq!(merged.tracks.len(), 2);
        assert_eq!(merged.tracks[1].source, TrackSource::Local);
    }

    #[test]
    fn local_watch_filter_accepts_audio_and_folder_artwork() {
        assert!(local_watch_path_relevant(Path::new("/music/song.FLAC")));
        assert!(local_watch_path_relevant(Path::new(
            "/music/Album/cover.jpg"
        )));
        assert!(local_watch_path_relevant(Path::new(
            "/music/Album/FOLDER.PNG"
        )));
    }

    #[test]
    fn local_watch_filter_ignores_unrelated_files() {
        assert!(!local_watch_path_relevant(Path::new("/music/notes.txt")));
        assert!(!local_watch_path_relevant(Path::new("/music/cover.gif")));
    }
}

fn spawn_refresh_library_lists(sp: SpotifyWorker, tx: mpsc::Sender<WorkerEvent>) {
    tokio::spawn(async move {
        if let Ok(playlists) = sp.fetch_playlists().await {
            save_playlists_cache(playlists.clone());
            let _ = tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
        }
        if let Ok(albums) = sp.fetch_albums().await {
            save_saved_albums_cache(albums.clone());
            let _ = tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
        }
    });
}

impl Worker {
    pub fn new(
        rx: mpsc::Receiver<AppEvent>,
        tx: mpsc::Sender<WorkerEvent>,
        app_tx: mpsc::Sender<AppEvent>,
    ) -> Self {
        let (media_tx, media_rx) = mpsc::channel(32);
        media::spawn_media_thread(media_rx, app_tx);
        let first_party = api::first_party::SpotifySessionManager::new(tx.clone())
            .map(api::first_party::SpotifyWebApi::new)
            .ok();
        Self {
            rx,
            tx,
            media_tx,
            first_party,
            artist_page_generation: Arc::new(AtomicU64::new(0)),
        }
    }

    fn spawn_playback_sync(
        client: rspotify::AuthCodeSpotify,
        tx: mpsc::Sender<WorkerEvent>,
        sync_inflight: Arc<AtomicBool>,
        previous_track_id: Option<String>,
        allow_same_track_reset: bool,
    ) {
        if sync_inflight.swap(true, Ordering::SeqCst) {
            return;
        }

        tokio::spawn(async move {
            let mut log = String::from("=== spawn_playback_sync started ===\n");

            for attempt in 0..3u32 {
                if attempt > 0 {
                    log.push_str(&format!("Attempt {}: waiting 500ms...\n", attempt));
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }

                let result = SpotifyWorker::playback_snapshot_from_client(&client).await;

                match result {
                    Ok(Some((
                        is_playing,
                        is_shuffled,
                        repeat_mode,
                        volume,
                        device_name,
                        progress_ms,
                        item,
                    ))) => {
                        let item_id = item.as_ref().map(|item| item.id.clone());
                        if let Some(item) = item.as_ref() {
                            log.push_str(&format!(
                                "  → Item: '{}', duration_ms={}, progress_ms={}, is_playing={}\n",
                                item.title, item.duration_ms, progress_ms, is_playing
                            ));
                        } else {
                            log.push_str("  → playback.item is missing or unparseable\n");
                        }

                        let track_changed = previous_track_id.as_ref() != item_id.as_ref();
                        let same_track_reset =
                            allow_same_track_reset && item_id.is_some() && progress_ms <= 3_000;

                        if item.is_some()
                            && (track_changed || same_track_reset || previous_track_id.is_none())
                        {
                            log.push_str(&format!(
                                "  → Sending SyncPlaybackState (track_id={:?})\n",
                                item_id
                            ));
                            let _ = std::fs::write("echo-debug-sync.log", &log);
                            sync_inflight.store(false, Ordering::SeqCst);
                            let _ = tx
                                .send(WorkerEvent::SyncPlaybackState {
                                    is_playing,
                                    is_shuffled,
                                    repeat_mode,
                                    volume,
                                    device_name,
                                    progress_ms,
                                    item,
                                })
                                .await;
                            return;
                        }
                        // Missing item or Spotify still reported the previous track - retry
                    }
                    Ok(None) => {
                        log.push_str(
                            "  → current_playback returned Ok(None) — no active playback\n",
                        );
                    }
                    Err(e) => {
                        log.push_str(&format!("  → current_playback returned Err: {:?}\n", e));
                    }
                }
            }

            log.push_str("All 3 attempts exhausted without a valid duration_ms. Giving up.\n");
            let _ = std::fs::write("echo-debug-sync.log", &log);
            sync_inflight.store(false, Ordering::SeqCst);
        });
    }

    pub async fn run(mut self) {
        let mut spotify_opt: Option<SpotifyWorker> = None;
        let mut api_client: Option<api::client::EchoSpotifyClient> = None;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(60));
        let is_playing = Arc::new(AtomicBool::new(false));
        let sync_inflight = Arc::new(AtomicBool::new(false));
        let mut current_track_id: Option<String> = None;
        let mut active_playback_source: Option<ActivePlaybackSource> = None;
        let mut local_playback = LocalPlaybackEngine::default();
        let (local_scan_tx, mut local_scan_rx) = mpsc::channel::<(PathBuf, LocalScanResult)>(4);
        let (local_watch_tx, mut local_watch_rx) = mpsc::channel::<PathBuf>(32);
        let mut local_scan_inflight = false;
        let mut pending_local_scan: Option<PathBuf> = None;
        let mut pending_watch_scan: Option<PathBuf> = None;
        let mut _local_watcher: Option<RecommendedWatcher> = None;
        let mut local_watch_debounce = tokio::time::interval(std::time::Duration::from_secs(2));

        loop {
            tokio::select! {
                scan_result = local_scan_rx.recv() => {
                    if let Some((path, result)) = scan_result {
                        local_scan_inflight = false;
                        match result {
                            Ok((library, report)) => {
                                let _ = self.tx.send(WorkerEvent::LocalLibraryLoaded {
                                    library,
                                    report,
                                }).await;
                            }
                            Err(message) => {
                                let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                    label: "Local scan".to_string(),
                                    message,
                                }).await;
                            }
                        }

                        if let Some(path) = pending_local_scan.take() {
                            schedule_local_scan(path, &local_scan_tx, &mut local_scan_inflight, &mut pending_local_scan);
                        } else {
                            let _ = path;
                        }
                    }
                }
                watch_path = local_watch_rx.recv() => {
                    if let Some(path) = watch_path {
                        pending_watch_scan = Some(path);
                    }
                }
                _ = local_watch_debounce.tick() => {
                    if let Some(path) = pending_watch_scan.take() {
                        schedule_local_scan(path, &local_scan_tx, &mut local_scan_inflight, &mut pending_local_scan);
                    }
                }
                _ = sync_interval.tick() => {
                    if let Some(ref sp) = spotify_opt
                        && active_playback_source != Some(ActivePlaybackSource::Local)
                        && !sync_inflight.swap(true, Ordering::SeqCst) {
                        let client = sp.client.clone();
                        let tx = self.tx.clone();
                        let media_tx = self.media_tx.clone();
                        let is_playing_clone = is_playing.clone();
                        let sync_inflight_clone = sync_inflight.clone();
                        tokio::spawn(async move {
                            if let Ok(Some((playing, shuffled, repeat, vol, dev_name, progress_ms, item))) = SpotifyWorker::playback_snapshot_from_client(&client).await {
                                is_playing_clone.store(playing, std::sync::atomic::Ordering::SeqCst);
                                let _ = media_tx.send(media::MediaUpdate::Playback(playing, progress_ms)).await;
                                if let Some(ref track) = item {
                                    let _ = media_tx.send(media::MediaUpdate::Metadata {
                                        title: track.title.clone(),
                                        artist: track.artist.clone(),
                                        album: "Unknown Album".to_string(),
                                        duration_ms: track.duration_ms,
                                        cover_url: track.image_url.clone(),
                                    }).await;
                                }
                                let _ = tx.send(WorkerEvent::SyncPlaybackState { is_playing: playing, is_shuffled: shuffled, repeat_mode: repeat, volume: vol, device_name: dev_name, progress_ms, item }).await;
                            }
                            sync_inflight_clone.store(false, Ordering::SeqCst);
                        });
                    }
                }
                _ = interval.tick() => {
                    if active_playback_source == Some(ActivePlaybackSource::Local) {
                        match local_playback.tick() {
                            Ok(Some(snapshot)) => {
                                is_playing.store(snapshot.is_playing, std::sync::atomic::Ordering::SeqCst);
                                if let Some(item) = snapshot.item.as_ref() {
                                    current_track_id = Some(item.id.clone());
                                }
                                emit_local_snapshot(&self.tx, &self.media_tx, snapshot, false).await;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                active_playback_source = None;
                                is_playing.store(false, std::sync::atomic::Ordering::SeqCst);
                                let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                    label: "Local playback".to_string(),
                                    message: e.to_string(),
                                }).await;
                            }
                        }
                    } else if is_playing.load(std::sync::atomic::Ordering::SeqCst) {
                        let _ = self.tx.send(WorkerEvent::Tick).await;
                    }
                }
                event_opt = self.rx.recv() => {
                    if let Some(event) = event_opt {
                        match event {
                            AppEvent::Quit => break,
                            AppEvent::StartAuth => {
                                let config = AppConfig::load();
                                if config.spotify_credentials.is_some()
                                    && let Ok(client) = SpotifyWorker::new(&config).await {
                                        api_client = Some(api::client::EchoSpotifyClient::new(
                                            client.client.clone(),
                                            self.first_party.clone(),
                                        ));
                                        spotify_opt = Some(client);
                                        let _ = self.tx.send(WorkerEvent::AuthenticationComplete).await;

                                        if let Some(ref sp) = spotify_opt {
                                            use rspotify::prelude::OAuthClient;
                                            use rspotify::prelude::Id;

                                            // Eagerly fetch and cache Liked Songs in background
                                            let client = sp.client.clone();
                                            let tx = self.tx.clone();
                                            tokio::spawn(async move {
                                                use futures_util::stream::StreamExt;
                                                let mut cache = crate::config::AppConfig::load_cache();
                                                let mut tracks = cache.liked_tracks.clone();
                                                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                                                let should_fetch = cache.last_liked_sync_time.map(|t| now > t + 3600).unwrap_or(true);

                                                if should_fetch {
                                                    let mut stream = client.current_user_saved_tracks(None);
                                                    let mut fetched_count = 0;

                                                    while let Some(item) = stream.next().await {
                                                        if let Ok(saved_track) = item {
                                                            if let Some(id) = saved_track.track.id {
                                                                tracks.insert(id.id().to_string());
                                                            }
                                                        }
                                                        fetched_count += 1;
                                                        if fetched_count >= 100 {
                                                            break; // Only fetch the 100 most recent liked songs on startup to avoid rate limits
                                                        }
                                                    }

                                                    cache.last_liked_sync_time = Some(now);
                                                    cache.liked_tracks = tracks.clone();
                                                    let _ = crate::config::AppConfig::save_cache(&cache);
                                                }

                                                    let mut results = std::collections::HashMap::new();
                                                    for tid in tracks {
                                                        results.insert(tid, true);
                                                    }
                                                    let _ = tx.send(WorkerEvent::LikedStatusUpdate(results)).await;
                                            });

                                            if let Ok(user) = sp.client.current_user().await {
                                                let _ = self.tx.send(WorkerEvent::UserIdentityLoaded(user.id.id().to_string())).await;
                                            }
                                        }

                                        audio::spawn_librespot_daemon(String::new(), "echo-rs".to_string(), self.tx.clone()).await;

                                        // Hydrate library lists from cache immediately, then refresh stale entries in background.
                                        if let Some(ref mut sp) = spotify_opt {
                                            hydrate_library_lists(sp, self.tx.clone()).await;
                                            // Initial State Sync (Seamless Handoff)
                                            // Try up to 5 times to sync, allowing the librespot daemon to authenticate
                                            let mut found_playback = false;
                                            for _ in 0..5 {
                                                if let Ok(Some((playing, is_shuffled, repeat, vol, dev_name, progress_ms, item))) = sp.sync_playback_state().await {
                                                    let mut actual_playing = playing;

                                                    // On first boot, if librespot automatically resumed playing, force it to pause
                                                    if playing && dev_name == "echo-rs" {
                                                        let _ = sp.toggle_playback(false).await;
                                                        actual_playing = false;
                                                    }

                                                    is_playing.store(actual_playing, std::sync::atomic::Ordering::SeqCst);
                                                    if let Some(item) = item.as_ref() {
                                                        current_track_id = Some(item.id.clone());
                                                    }
                                                    let _ = self.tx.send(WorkerEvent::SyncPlaybackState { is_playing: actual_playing, is_shuffled, repeat_mode: repeat, volume: vol, device_name: dev_name, progress_ms, item }).await;
                                                    found_playback = true;
                                                    break;
                                                }

                                                // If no active session exists, forcefully wake up our integrated device
                                                let _ = sp.wake_up_device().await;
                                                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                                            }

                                            // Fetch queue initially only if we have an active session
                                            if found_playback
                                                && let Ok(queue) = sp.fetch_queue().await {
                                                    let _ = self.tx.send(WorkerEvent::QueueLoaded(queue)).await;
                                                }
                                        }
                                    }
                            }
                            AppEvent::LoadContextTracks(context) => {
                                if let Some(sp) = spotify_opt.as_ref() {
                                    let sp = sp.clone();
                                    let tx = self.tx.clone();
                                    tokio::spawn(async move {
                                        tracks::load_context_tracks(Some(&sp), context, &tx).await;
                                    });
                                }
                            }
                            AppEvent::RefreshContextTracks(context) => {
                                if let Some(sp) = spotify_opt.as_ref() {
                                    let sp = sp.clone();
                                    let tx = self.tx.clone();
                                    tokio::spawn(async move {
                                        tracks::refresh_context_tracks(Some(&sp), context, &tx).await;
                                    });
                                }
                            }
                            AppEvent::RefreshLibraryLists => {
                                if let Some(sp) = spotify_opt.as_ref() {
                                    spawn_refresh_library_lists(sp.clone(), self.tx.clone());
                                }
                            }
                            AppEvent::ScanLocalLibrary(path) => {
                                match start_local_watcher(path.clone(), local_watch_tx.clone()) {
                                    Ok(watcher) => {
                                        _local_watcher = Some(watcher);
                                    }
                                    Err(e) => {
                                        _local_watcher = None;
                                        let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                            label: "Local watcher".to_string(),
                                            message: e.to_string(),
                                        }).await;
                                    }
                                }
                                schedule_local_scan(path, &local_scan_tx, &mut local_scan_inflight, &mut pending_local_scan);
                            }
                            AppEvent::RescanLocalLibrary => {
                                let config = AppConfig::load();
                                if let Some(path) = config.library.local_music_dir {
                                    schedule_local_scan(path, &local_scan_tx, &mut local_scan_inflight, &mut pending_local_scan);
                                } else {
                                    let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                        label: "Local scan".to_string(),
                                        message: "no local music path configured".to_string(),
                                    }).await;
                                }
                            }
                            AppEvent::StartLocalLibraryAutoRefresh(path) => {
                                match start_local_watcher(path.clone(), local_watch_tx.clone()) {
                                    Ok(watcher) => {
                                        _local_watcher = Some(watcher);
                                    }
                                    Err(e) => {
                                        _local_watcher = None;
                                        let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                            label: "Local watcher".to_string(),
                                            message: e.to_string(),
                                        }).await;
                                    }
                                }
                                schedule_local_scan(path, &local_scan_tx, &mut local_scan_inflight, &mut pending_local_scan);
                            }
                            AppEvent::PlayTrack { target, track_id, title, artist, duration_ms, image_url, album_id } => {
                                if let PlaybackTarget::LocalTrack { track_id: _, path } = target.clone() {
                                    let track = crate::models::Track {
                                        id: track_id.clone(),
                                        source: TrackSource::Local,
                                        local_path: Some(path),
                                        name: title.clone(),
                                        artist: artist.clone(),
                                        duration_ms,
                                        image_url: image_url.clone(),
                                        album_id: album_id.clone(),
                                        artist_id: None,
                                    };
                                    if let Some(ref mut sp) = spotify_opt {
                                        let _ = sp.toggle_playback(false).await;
                                    }
                                    local_playback.stop();
                                    match local_playback.play_context(vec![track], 0) {
                                        Ok(snapshot) => {
                                            active_playback_source = Some(ActivePlaybackSource::Local);
                                            is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                            current_track_id = Some(track_id);
                                            emit_local_snapshot(&self.tx, &self.media_tx, snapshot, true).await;
                                        }
                                        Err(e) => {
                                            active_playback_source = None;
                                            let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                                label: "Local playback".to_string(),
                                                message: e.to_string(),
                                            }).await;
                                        }
                                    }
                                    continue;
                                }

                                if let PlaybackTarget::LocalContext { tracks, selected_index } = target.clone() {
                                    if let Some(ref mut sp) = spotify_opt {
                                        let _ = sp.toggle_playback(false).await;
                                    }
                                    local_playback.stop();
                                    match local_playback.play_context(tracks, selected_index) {
                                        Ok(snapshot) => {
                                            active_playback_source = Some(ActivePlaybackSource::Local);
                                            is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                            current_track_id = Some(track_id);
                                            emit_local_snapshot(&self.tx, &self.media_tx, snapshot, true).await;
                                        }
                                        Err(e) => {
                                            active_playback_source = None;
                                            let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                                label: "Local playback".to_string(),
                                                message: e.to_string(),
                                            }).await;
                                        }
                                    }
                                    continue;
                                }

                                if let Some(ref mut sp) = spotify_opt {
                                    if active_playback_source == Some(ActivePlaybackSource::Local) {
                                        local_playback.stop();
                                    }
                                    active_playback_source = Some(ActivePlaybackSource::Spotify);
                                    let play_result = match &target {
                                        PlaybackTarget::SpotifyContext { context_id, is_album } => {
                                            sp.play_track(context_id, &track_id, *is_album).await
                                        }
                                        PlaybackTarget::SpotifyTrack { track_id } => {
                                            sp.play_track("LIKED_SONGS", track_id, false).await
                                        }
                                        PlaybackTarget::LocalTrack { .. } | PlaybackTarget::LocalContext { .. } => unreachable!(),
                                    };
                                    match play_result {
                                        Ok(_) => {
                                            is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                            current_track_id = Some(track_id.clone());
                                            let item = PlaybackItem {
                                                id: track_id,
                                                source: TrackSource::Spotify,
                                                local_path: None,
                                                title: title.clone(),
                                                artist: artist.clone(),
                                                duration_ms,
                                                image_url: image_url.clone(),
                                                album_id: album_id.clone(),
                                                artist_id: None,
                                            };
                                            let _ = self.tx.send(WorkerEvent::PlaybackStarted {
                                                item: item.clone(),
                                            }).await;
                                            let _ = self.media_tx.send(media::MediaUpdate::Metadata {
                                                title: title.clone(),
                                                artist: artist.clone(),
                                                album: "Unknown Album".to_string(),
                                                duration_ms,
                                                cover_url: image_url.clone(),
                                            }).await;
                                        }
                                        Err(e) => {
                                            let _ = std::fs::write("echo-debug-worker.log", format!("Worker PlayTrack failed: {:?}", e));
                                        }
                                    }
                                }
                            }
                            AppEvent::TogglePlayback(playing) => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    local_playback.toggle_playback(playing);
                                    is_playing.store(playing, std::sync::atomic::Ordering::SeqCst);
                                    emit_local_snapshot(&self.tx, &self.media_tx, local_playback.snapshot(), false).await;
                                } else if let Some(ref mut sp) = spotify_opt {
                                    if sp.toggle_playback(playing).await.is_ok() {
                                        is_playing.store(playing, std::sync::atomic::Ordering::SeqCst);
                                        let _ = self.media_tx.send(media::MediaUpdate::Playback(playing, 0)).await;
                                        let _ = self.tx.send(WorkerEvent::PlaybackControlState { is_playing: playing }).await;
                                    }
                                }
                            }
                            AppEvent::ToggleShuffle(is_shuffled) => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    emit_local_snapshot(&self.tx, &self.media_tx, local_playback.set_shuffle(is_shuffled), false).await;
                                } else if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.toggle_shuffle(is_shuffled).await;
                                }
                            }
                            AppEvent::SetRepeatMode(mode) => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    let snapshot = local_playback.set_repeat_mode(RepeatMode::from_label(&mode));
                                    emit_local_snapshot(&self.tx, &self.media_tx, snapshot, false).await;
                                } else if let Some(ref mut sp) = spotify_opt {
                                    let state = match mode.as_str() {
                                        "Track" => rspotify::model::RepeatState::Track,
                                        "Context" => rspotify::model::RepeatState::Context,
                                        _ => rspotify::model::RepeatState::Off,
                                    };
                                    let _ = sp.set_repeat_mode(state).await;
                                }
                            }
                            AppEvent::SetVolume(vol) => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    let snapshot = local_playback.set_volume(u32::from(vol));
                                    emit_local_snapshot(&self.tx, &self.media_tx, snapshot, false).await;
                                } else if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.set_volume(vol).await;
                                }
                            }
                            AppEvent::NextTrack { current_track_id: ui_current_track_id } => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    match local_playback.next() {
                                        Ok(snapshot) => {
                                            is_playing.store(snapshot.is_playing, std::sync::atomic::Ordering::SeqCst);
                                            if let Some(item) = snapshot.item.as_ref() {
                                                current_track_id = Some(item.id.clone());
                                            }
                                            emit_local_snapshot(&self.tx, &self.media_tx, snapshot, true).await;
                                        }
                                        Err(e) => {
                                            let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                                label: "Local playback".to_string(),
                                                message: e.to_string(),
                                            }).await;
                                        }
                                    }
                                } else if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.next_track().await;
                                    is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                    let previous_track_id = ui_current_track_id.or_else(|| current_track_id.clone());
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), sync_inflight.clone(), previous_track_id, false);
                                }
                            }
                            AppEvent::PreviousTrack { current_track_id: ui_current_track_id } => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    match local_playback.previous() {
                                        Ok(snapshot) => {
                                            is_playing.store(snapshot.is_playing, std::sync::atomic::Ordering::SeqCst);
                                            if let Some(item) = snapshot.item.as_ref() {
                                                current_track_id = Some(item.id.clone());
                                            }
                                            emit_local_snapshot(&self.tx, &self.media_tx, snapshot, true).await;
                                        }
                                        Err(e) => {
                                            let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                                label: "Local playback".to_string(),
                                                message: e.to_string(),
                                            }).await;
                                        }
                                    }
                                } else if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.previous_track().await;
                                    is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                    let previous_track_id = ui_current_track_id.or_else(|| current_track_id.clone());
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), sync_inflight.clone(), previous_track_id, true);
                                }
                            }
                            AppEvent::LoadTrackMetadata(tid) => {
                                if let Some(ref mut sp) = spotify_opt
                                    && let Ok((title, artist, image_url)) = sp.get_track_metadata(&tid).await {
                                        let _ = self.tx.send(WorkerEvent::TrackMetadataLoaded { track_id: tid, title, artist, image_url }).await;
                                    }
                            }
                            AppEvent::GlobalSearch(query) => {
                                let local_results = AppConfig::load_local_library().search(&query);
                                if let Some(ref mut sp) = spotify_opt {
                                    match sp.search_catalog(&query).await {
                                        Ok(results) => {
                                            let _ = self.tx.send(WorkerEvent::SearchResultsLoaded(
                                                merged_search_results(Some(results), local_results),
                                            )).await;
                                        }
                                        Err(e) => {
                                            let _ = std::fs::write("echo-debug-search.log", format!("Search error: {:?}", e));
                                            let _ = self.tx.send(WorkerEvent::SearchResultsLoaded(local_results)).await;
                                        }
                                    }
                                } else {
                                    let _ = self.tx.send(WorkerEvent::SearchResultsLoaded(local_results)).await;
                                }
                            }
                            AppEvent::AddToQueue(track_ids) => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    if track_ids.iter().any(|id| !id.starts_with("local:")) {
                                        let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                            label: "Local queue".to_string(),
                                            message: "cross-source live queueing is not supported yet".to_string(),
                                        }).await;
                                        continue;
                                    }
                                    let library = AppConfig::load_local_library();
                                    let tracks = resolve_local_queue_tracks(&track_ids, &library);
                                    if tracks.is_empty() {
                                        let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                            label: "Local queue".to_string(),
                                            message: "no queued local tracks were found in the local library".to_string(),
                                        }).await;
                                        continue;
                                    }
                                    let count = tracks.len();
                                    let snapshot = local_playback.add_to_queue(tracks);
                                    emit_local_snapshot(&self.tx, &self.media_tx, snapshot, false).await;
                                    let _ = self.tx.send(WorkerEvent::TracksQueued(count)).await;
                                } else if track_ids.iter().any(|id| id.starts_with("local:")) {
                                    let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                        label: "Spotify queue".to_string(),
                                        message: "local tracks can only be queued while local playback is active".to_string(),
                                    }).await;
                                } else if let Some(ref sp) = spotify_opt {
                                    let count = track_ids.len();
                                    let _ = sp.add_to_queue(track_ids).await;
                                    let _ = self.tx.send(WorkerEvent::TracksQueued(count)).await;
                                }
                            }
                            AppEvent::AddTracksToPlaylist(playlist_id, track_ids) => {
                                if playlist_id.starts_with("local-playlist:") {
                                    let mut local_playlists = AppConfig::load_local_playlists();
                                    if let Some(playlist) = local_playlists.playlists.iter_mut().find(|playlist| playlist.id == playlist_id) {
                                        let entries: Vec<_> = track_ids
                                            .iter()
                                            .filter_map(LocalPlaylistEntry::from_track)
                                            .collect();
                                        let count = entries.len();
                                        playlist.entries.extend(entries);
                                        playlist.updated_unix_secs = current_unix_secs();
                                        let _ = AppConfig::save_local_playlists(&local_playlists);
                                        let _ = self.tx.send(WorkerEvent::LocalPlaylistsLoaded(local_playlists)).await;
                                        let _ = self.tx.send(WorkerEvent::TracksQueued(count)).await;
                                    } else {
                                        let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                            label: "Local playlist".to_string(),
                                            message: "playlist not found".to_string(),
                                        }).await;
                                    }
                                } else if track_ids.iter().any(|track| track.source == TrackSource::Local) {
                                    let _ = self.tx.send(WorkerEvent::ApiRequestFailed {
                                        label: "Spotify playlist".to_string(),
                                        message: "local tracks cannot be added to Spotify playlists".to_string(),
                                    }).await;
                                } else if let Some(ref sp) = spotify_opt {
                                    use rspotify::prelude::OAuthClient;
                                    use rspotify::model::{PlaylistId, PlayableId, TrackId};

                                    if let Ok(pid) = PlaylistId::from_id(&playlist_id) {
                                        let mut items = Vec::new();
                                        for track in &track_ids {
                                            if let Ok(id) = TrackId::from_id(&track.id) {
                                                items.push(PlayableId::Track(id));
                                            }
                                        }
                                        if !items.is_empty() {
                                            let res = sp.client.playlist_add_items(pid.clone(), items, None).await;
                                            if let Err(e) = res {
                                                let _ = std::fs::write("echo-debug-add.log", format!("Add error: {:?}", e));
                                            } else {
                                                invalidate_playlist_context_cache(&playlist_id);
                                                // Trigger a refresh of the playlists to show the new tracks count
                                                if let Ok(playlists) = sp.fetch_playlists().await {
                                                    save_playlists_cache(playlists.clone());
                                                    let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                                }
                                                let _ = self.tx.send(WorkerEvent::ForceContextRefresh).await;
                                            }
                                        }
                                    }
                                }
                            }
                            AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids) => {
                                if playlist_id.starts_with("local-playlist:") {
                                    let mut local_playlists = AppConfig::load_local_playlists();
                                    if let Some(playlist) = local_playlists.playlists.iter_mut().find(|playlist| playlist.id == playlist_id) {
                                        for track_id in &track_ids {
                                            if let Some(pos) = playlist.entries.iter().position(|entry| entry.track_id() == track_id) {
                                                playlist.entries.remove(pos);
                                            }
                                        }
                                        playlist.updated_unix_secs = current_unix_secs();
                                        let _ = AppConfig::save_local_playlists(&local_playlists);
                                        let _ = self.tx.send(WorkerEvent::LocalPlaylistsLoaded(local_playlists)).await;
                                    }
                                } else if let Some(ref sp) = spotify_opt {
                                    use rspotify::prelude::OAuthClient;
                                    use rspotify::model::{PlaylistId, PlayableId, TrackId};

                                    if let Ok(pid) = PlaylistId::from_id(&playlist_id) {
                                        let mut items = Vec::new();
                                        for t_id in &track_ids {
                                            if let Ok(id) = TrackId::from_id(t_id) {
                                                items.push(PlayableId::Track(id));
                                            }
                                        }
                                        let _ = std::fs::write("echo-debug-remove.log", format!("Attempting remove on {} with {} items (raw ids: {:?})", playlist_id, items.len(), track_ids));
                                        if !items.is_empty() {
                                            let res = sp.client.playlist_remove_all_occurrences_of_items(pid.clone(), items, None).await;

                                            if let Ok(_) = res {
                                                let _ = std::fs::write("echo-debug-remove-success.log", "Remove succeeded API call");
                                                invalidate_playlist_context_cache(&playlist_id);
                                                if let Ok(playlists) = sp.fetch_playlists().await {
                                                    save_playlists_cache(playlists.clone());
                                                    let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                                }
                                                let _ = self.tx.send(WorkerEvent::ForceContextRefresh).await;
                                            } else if let Err(e) = res {
                                                let _ = std::fs::write("echo-debug-remove-err.log", format!("Remove error API: {:?}", e));
                                            }
                                        } else {
                                            let _ = std::fs::write("echo-debug-remove.log", format!("No items parsed! track_ids: {:?}", track_ids));
                                        }
                                    }
                                }
                            }
                            AppEvent::CreatePlaylist(name) => {
                                if let Some(ref sp) = spotify_opt {
                                    let client = sp.client.clone();
                                    let tx = self.tx.clone();
                                    tokio::spawn(async move {
                                        use rspotify::prelude::Id;

                                        let mut created = false;

                                        // 1. Try standard current_user approach first
                                        if let Ok(me) = client.current_user().await {
                                            if client.user_playlist_create(
                                                me.id.clone(),
                                                &name,
                                                Some(false),
                                                Some(false),
                                                Some("Created by echo-rs"),
                                            ).await.is_ok() {
                                                created = true;
                                            }
                                        }

                                        // 2. Workaround: If current_user failed (e.g. 429 rate limit on /me),
                                        // fetch playlists and try creating with the owner ID of existing playlists.
                                        // current_user_playlists_manual is on /me/playlists which often escapes the /me block.
                                        if !created {
                                            if let Ok(page) = client.current_user_playlists_manual(None, None).await {
                                                // Collect unique owner IDs from playlists
                                                let mut owner_ids = std::collections::HashSet::new();
                                                for p in &page.items {
                                                    owner_ids.insert(p.owner.id.clone());
                                                }

                                                // Try creating the playlist with each unique owner ID.
                                                // Only the actual user's ID will succeed (others will 403 Forbidden).
                                                for uid in owner_ids {
                                                    if client.user_playlist_create(
                                                        uid,
                                                        &name,
                                                        Some(false),
                                                        Some(false),
                                                        Some("Created by echo-rs"),
                                                    ).await.is_ok() {
                                                        created = true;
                                                        break;
                                                    }
                                                }
                                            }
                                        }

                                        if created {
                                            // Refresh playlists
                                            let page_res = client.current_user_playlists_manual(None, None).await;
                                            if let Ok(page) = page_res {
                                                let mut out = Vec::new();
                                                for p in page.items {
                                                    let owner = p.owner.display_name.clone().unwrap_or_else(|| p.owner.id.id().to_string());
                                                    let owner_id = p.owner.id.id().to_string();
                                                    out.push(crate::models::Playlist {
                                                        id: p.id.id().to_string(),
                                                        name: p.name,
                                                        owner,
                                                        owner_id,
                                                        image_url: p.images.first().map(|i| i.url.clone()),
                                                    });
                                                }
                                                save_playlists_cache(out.clone());
                                                let _ = tx.send(WorkerEvent::PlaylistsLoaded(out)).await;
                                            }
                                        }
                                    });
                                }
                            }
                            AppEvent::CreateLocalPlaylist(name) => {
                                let mut local_playlists = AppConfig::load_local_playlists();
                                let now = current_unix_secs();
                                let id = stable_local_playlist_id(&name, now);
                                local_playlists.playlists.push(LocalPlaylist {
                                    id,
                                    name,
                                    created_unix_secs: now,
                                    updated_unix_secs: now,
                                    entries: Vec::new(),
                                });
                                let _ = AppConfig::save_local_playlists(&local_playlists);
                                let _ = self.tx.send(WorkerEvent::LocalPlaylistsLoaded(local_playlists)).await;
                            }
                            AppEvent::RenamePlaylist(playlist_id, new_name) => {
                                if playlist_id.starts_with("local-playlist:") {
                                    let mut local_playlists = AppConfig::load_local_playlists();
                                    if let Some(playlist) = local_playlists.playlists.iter_mut().find(|playlist| playlist.id == playlist_id) {
                                        playlist.name = new_name;
                                        playlist.updated_unix_secs = current_unix_secs();
                                        let _ = AppConfig::save_local_playlists(&local_playlists);
                                        let _ = self.tx.send(WorkerEvent::LocalPlaylistsLoaded(local_playlists)).await;
                                    }
                                } else if let Some(ref sp) = spotify_opt
                                    && let Ok(pid) = rspotify::model::PlaylistId::from_id(&playlist_id) {
                                        let res = sp.client.playlist_change_detail(
                                            pid,
                                            Some(&new_name),
                                            None,
                                            None,
                                            None,
                                        ).await;
                                        if res.is_ok()
                                            && let Ok(playlists) = sp.fetch_playlists().await {
                                                save_playlists_cache(playlists.clone());
                                                let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                            }
                                    }
                            }
                            AppEvent::DeletePlaylists(playlist_ids) => {
                                let has_local = playlist_ids.iter().any(|id| id.starts_with("local-playlist:"));
                                if has_local {
                                    let mut local_playlists = AppConfig::load_local_playlists();
                                    local_playlists.playlists.retain(|playlist| !playlist_ids.contains(&playlist.id));
                                    let _ = AppConfig::save_local_playlists(&local_playlists);
                                    let _ = self.tx.send(WorkerEvent::LocalPlaylistsLoaded(local_playlists)).await;
                                }
                                if let Some(ref sp) = spotify_opt {
                                    for id_str in playlist_ids.iter().filter(|id| !id.starts_with("local-playlist:")) {
                                        if let Ok(pid) = rspotify::model::PlaylistId::from_id(id_str) {
                                            let _ = sp.client.library_remove([rspotify::model::LibraryId::Playlist(pid)]).await;
                                            invalidate_playlist_context_cache(id_str);
                                        }
                                    }
                                    if let Ok(playlists) = sp.fetch_playlists().await {
                                        save_playlists_cache(playlists.clone());
                                        let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                    }
                                }
                            }
                            AppEvent::SaveAlbums(album_ids) => {
                                if let Some(ref sp) = spotify_opt {
                                    let ids: Vec<_> = album_ids.iter().filter_map(|id_str| rspotify::model::AlbumId::from_id(id_str).ok().map(|id| rspotify::model::LibraryId::Album(id))).collect();
                                    if !ids.is_empty() {
                                        let _ = sp.client.library_add(ids).await;
                                        if let Ok(albums) = sp.fetch_albums().await {
                                            save_saved_albums_cache(albums.clone());
                                            let _ = self.tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
                                        }
                                    }
                                }
                            }
                            AppEvent::RemoveAlbums(album_ids) => {
                                if let Some(ref sp) = spotify_opt {
                                    let ids: Vec<_> = album_ids.iter().filter_map(|id_str| rspotify::model::AlbumId::from_id(id_str).ok().map(|id| rspotify::model::LibraryId::Album(id))).collect();
                                    if !ids.is_empty() {
                                        let _ = sp.client.library_remove(ids).await;
                                        if let Ok(albums) = sp.fetch_albums().await {
                                            save_saved_albums_cache(albums.clone());
                                            let _ = self.tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
                                        }
                                    }
                                }
                            }
                            AppEvent::FetchQueue => {
                                if active_playback_source == Some(ActivePlaybackSource::Local) {
                                    let _ = self.tx.send(WorkerEvent::QueueLoaded(local_playback.snapshot().queue)).await;
                                } else if let Some(ref sp) = spotify_opt {
                                    match sp.fetch_queue().await {
                                        Ok(tracks) => {
                                            let _ = self.tx.send(WorkerEvent::QueueLoaded(tracks)).await;
                                        }
                                        Err(e) => {
                                            let _ = std::fs::write("echo-debug-queue.log", format!("Queue error: {:?}", e));
                                        }
                                    }
                                }
                            }
                            AppEvent::FetchLyrics(_track_id, title, artist, duration_ms) => {
                                match api::lyrics::fetch_lyrics(&title, &artist, duration_ms).await {
                                    Ok(lyrics) => {
                                        let _ = self.tx.send(WorkerEvent::LyricsLoaded(lyrics)).await;
                                    }
                                    Err(_) => {
                                        let _ = self.tx.send(WorkerEvent::LyricsLoaded(None)).await;
                                    }
                                }
                            }
                            AppEvent::FetchDevices => {
                                if let Some(ref sp) = spotify_opt {
                                    if let Ok(devices) = sp.fetch_devices().await {
                                        let _ = self.tx.send(WorkerEvent::DevicesLoaded(devices)).await;
                                    }
                                }
                            }
                            AppEvent::TransferPlayback(device_id) => {
                                if let Some(ref sp) = spotify_opt {
                                    let _ = sp.transfer_playback(&device_id).await;
                                    // Trigger a full context sync so UI updates its active device quickly
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), sync_inflight.clone(), current_track_id.clone(), true);
                                }
                            }
                            AppEvent::ToggleTrackLike(track_id, like) => {
                                if track_id.starts_with("local:") {
                                    let mut cache = AppConfig::load_cache();
                                    if like {
                                        cache.liked_tracks.insert(track_id.clone());
                                    } else {
                                        cache.liked_tracks.remove(&track_id);
                                    }
                                    let _ = AppConfig::save_cache(&cache);
                                    let mut update = std::collections::HashMap::new();
                                    update.insert(track_id, like);
                                    let _ = self.tx.send(WorkerEvent::LikedStatusUpdate(update)).await;
                                } else if let Some(ref sp) = spotify_opt {
                                    use rspotify::model::{TrackId, LibraryId};
                                    if let Ok(tid) = TrackId::from_id(&track_id) {
                                        let lib_id = LibraryId::Track(tid);
                                        if like {
                                            let _ = sp.client.library_add([lib_id]).await;
                                        } else {
                                            let _ = sp.client.library_remove([lib_id]).await;
                                        }
                                        let mut cache = AppConfig::load_cache();
                                        if like {
                                            cache.liked_tracks.insert(track_id.clone());
                                        } else {
                                            cache.liked_tracks.remove(&track_id);
                                        }
                                        let _ = AppConfig::save_cache(&cache);
                                    }
                                }
                            }
                            AppEvent::ForcePlaybackSync => {
                                if let Some(ref sp) = spotify_opt {
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), sync_inflight.clone(), current_track_id.clone(), true);
                                }
                            }
                            AppEvent::CancelArtistPageLoad => {
                                artist_page::cancel_pending_artist_page(&self.artist_page_generation);
                            }
                            AppEvent::FetchTopTracks => {
                                browse::spawn_top_tracks(api_client.clone(), self.tx.clone());
                            }
                            AppEvent::FetchRecentlyPlayed => {
                                browse::spawn_recently_played(api_client.clone(), self.tx.clone());
                            }
                            AppEvent::FetchFollowedArtists => {
                                browse::spawn_followed_artists(api_client.clone(), self.tx.clone());
                            }
                            AppEvent::LoadArtistPage {
                                artist_id,
                                artist_name,
                                artist_image_url,
                            } => {
                                artist_page::spawn_load_artist_page(
                                    api_client.as_ref(),
                                    self.tx.clone(),
                                    artist_id,
                                    artist_name,
                                    artist_image_url,
                                );
                            }
                            AppEvent::RefreshArtistAlbums { artist_id } => {
                                artist_page::spawn_refresh_artist_albums(
                                    api_client.as_ref(),
                                    self.tx.clone(),
                                    artist_id,
                                );
                            }
                            _ => {}
                        }
                    } else {
                        break; // Channel closed
                    }
                }
            }
        }
    }
}
