pub mod audio;
pub mod api;
pub mod visualization;
pub mod media;

use crate::config::AppConfig;
use crate::events::{AppEvent, WorkerEvent};
use crate::models::PlaybackItem;
use api::SpotifyWorker;
use tokio::sync::mpsc;
use rspotify::clients::OAuthClient;

pub struct Worker {
    rx: mpsc::Receiver<AppEvent>,
    tx: mpsc::Sender<WorkerEvent>,
    media_tx: mpsc::Sender<media::MediaUpdate>,
}

impl Worker {
    pub fn new(rx: mpsc::Receiver<AppEvent>, tx: mpsc::Sender<WorkerEvent>, app_tx: mpsc::Sender<AppEvent>) -> Self {
        let (media_tx, media_rx) = mpsc::channel(32);
        media::spawn_media_thread(media_rx, app_tx);
        Self { rx, tx, media_tx }
    }

    fn spawn_playback_sync(
        client: rspotify::AuthCodeSpotify,
        tx: mpsc::Sender<WorkerEvent>,
        previous_track_id: Option<String>,
        allow_same_track_reset: bool,
    ) {
        tokio::spawn(async move {
            let mut log = String::from("=== spawn_playback_sync started ===\n");

            for attempt in 0..5u32 {
                let wait_secs = attempt + 1;
                log.push_str(&format!("Attempt {}: waiting {}s...\n", attempt, wait_secs));
                tokio::time::sleep(std::time::Duration::from_secs(wait_secs as u64)).await;

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

            log.push_str("All 5 attempts exhausted without a valid duration_ms. Giving up.\n");
            let _ = std::fs::write("echo-debug-sync.log", &log);
        });
    }

    pub async fn run(mut self) {
        let mut spotify_opt: Option<SpotifyWorker> = None;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));
        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(10));
        let is_playing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut current_track_id: Option<String> = None;

        loop {
            tokio::select! {
                _ = sync_interval.tick() => {
                    if let Some(ref sp) = spotify_opt {
                        let client = sp.client.clone();
                        let tx = self.tx.clone();
                        let media_tx = self.media_tx.clone();
                        let is_playing_clone = is_playing.clone();
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
                        });
                    }
                }
                _ = interval.tick() => {
                    if is_playing.load(std::sync::atomic::Ordering::SeqCst) {
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
                                                let mut tracks = std::collections::HashSet::new();
                                                let mut stream = client.current_user_saved_tracks(None);
                                                while let Some(item) = stream.next().await {
                                                    if let Ok(saved_track) = item {
                                                        if let Some(id) = saved_track.track.id {
                                                            tracks.insert(id.id().to_string());
                                                        }
                                                    }
                                                }
                                                
                                                if !tracks.is_empty() {
                                                    let mut cache = crate::config::AppConfig::load_cache();
                                                    cache.liked_tracks = tracks.clone();
                                                    let _ = crate::config::AppConfig::save_cache(&cache);
                                                    
                                                    let mut results = std::collections::HashMap::new();
                                                    for tid in tracks {
                                                        results.insert(tid, true);
                                                    }
                                                    let _ = tx.send(WorkerEvent::LikedStatusUpdate(results)).await;
                                                }
                                            });

                                            if let Ok(user) = sp.client.current_user().await {
                                                let _ = self.tx.send(WorkerEvent::UserIdentityLoaded(user.id.id().to_string())).await;
                                            }
                                        }

                                        audio::spawn_librespot_daemon(String::new(), "echo-rs".to_string(), self.tx.clone()).await;

                                        // Fetch playlists initially
                                        if let Some(ref mut sp) = spotify_opt {
                                            if let Ok(playlists) = sp.fetch_playlists().await {
                                                let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                            }
                                            if let Ok(albums) = sp.fetch_albums().await {
                                                let _ = self.tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
                                            }
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
                            AppEvent::LoadContextTracks(id, is_album, _, metadata) => {
                                if let Some(ref sp) = spotify_opt {
                                    if is_album {
                                        if let Ok((tracks, album_metadata)) = sp.fetch_album_tracks(&id).await {
                                            let _ = self.tx.send(WorkerEvent::TracksLoaded(tracks, album_metadata.or(metadata))).await;
                                        }
                                    } else {
                                        if let Ok(tracks) = sp.fetch_tracks(&id).await {
                                            let _ = self.tx.send(WorkerEvent::TracksLoaded(tracks, metadata)).await;
                                        }
                                    }
                                }
                            }
                            AppEvent::PlayTrack { context_id, track_id, is_album, title, artist, duration_ms, image_url, album_id } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    match sp.play_track(&context_id, &track_id, is_album).await {
                                        Ok(_) => {
                                            is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                            current_track_id = Some(track_id.clone());
                                            let item = PlaybackItem {
                                                id: track_id,
                                                title: title.clone(),
                                                artist: artist.clone(),
                                                duration_ms,
                                                image_url: image_url.clone(),
                                                album_id: album_id.clone(),
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
                                is_playing.store(playing, std::sync::atomic::Ordering::SeqCst);
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.toggle_playback(playing).await;
                                }
                            }
                            AppEvent::ToggleShuffle(is_shuffled) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.toggle_shuffle(is_shuffled).await;
                                }
                            }
                            AppEvent::SetRepeatMode(mode) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let state = match mode.as_str() {
                                        "Track" => rspotify::model::RepeatState::Track,
                                        "Context" => rspotify::model::RepeatState::Context,
                                        _ => rspotify::model::RepeatState::Off,
                                    };
                                    let _ = sp.set_repeat_mode(state).await;
                                }
                            }
                            AppEvent::SetVolume(vol) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.set_volume(vol).await;
                                }
                            }
                            AppEvent::NextTrack { current_track_id: ui_current_track_id } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.next_track().await;
                                    is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                    let previous_track_id = ui_current_track_id.or_else(|| current_track_id.clone());
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), previous_track_id, false);
                                }
                            }
                            AppEvent::PreviousTrack { current_track_id: ui_current_track_id } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.previous_track().await;
                                    is_playing.store(true, std::sync::atomic::Ordering::SeqCst);
                                    let previous_track_id = ui_current_track_id.or_else(|| current_track_id.clone());
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), previous_track_id, true);
                                }
                            }
                            AppEvent::LoadTrackMetadata(tid) => {
                                if let Some(ref mut sp) = spotify_opt
                                    && let Ok((title, artist, image_url)) = sp.get_track_metadata(&tid).await {
                                        let _ = self.tx.send(WorkerEvent::TrackMetadataLoaded { track_id: tid, title, artist, image_url }).await;
                                    }
                            }
                            AppEvent::GlobalSearch(query) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    match sp.search_catalog(&query).await {
                                        Ok(results) => {
                                            let _ = self.tx.send(WorkerEvent::SearchResultsLoaded(results)).await;
                                        }
                                        Err(e) => {
                                            let _ = std::fs::write("echo-debug-search.log", format!("Search error: {:?}", e));
                                        }
                                    }
                                }
                            }
                            AppEvent::AddToQueue(track_ids) => {
                                if let Some(ref sp) = spotify_opt {
                                    let count = track_ids.len();
                                    let _ = sp.add_to_queue(track_ids).await;
                                    let _ = self.tx.send(WorkerEvent::TracksQueued(count)).await;
                                }
                            }
                            AppEvent::AddTracksToPlaylist(playlist_id, track_ids) => {
                                if let Some(ref sp) = spotify_opt {
                                    use rspotify::prelude::OAuthClient;
                                    use rspotify::model::{PlaylistId, PlayableId, TrackId};
                                    
                                    if let Ok(pid) = PlaylistId::from_id(&playlist_id) {
                                        let mut items = Vec::new();
                                        for t_id in &track_ids {
                                            if let Ok(id) = TrackId::from_id(t_id) {
                                                items.push(PlayableId::Track(id));
                                            }
                                        }
                                        if !items.is_empty() {
                                            let res = sp.client.playlist_add_items(pid.clone(), items, None).await;
                                            if let Err(e) = res {
                                                let _ = std::fs::write("echo-debug-add.log", format!("Add error: {:?}", e));
                                            } else {
                                                // Trigger a refresh of the playlists to show the new tracks count
                                                if let Ok(playlists) = sp.fetch_playlists().await {
                                                    let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                                }
                                                let _ = self.tx.send(WorkerEvent::ForceContextRefresh).await;
                                            }
                                        }
                                    }
                                }
                            }
                            AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids) => {
                                if let Some(ref sp) = spotify_opt {
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
                                                if let Ok(playlists) = sp.fetch_playlists().await {
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
                                    // The user's Spotify user ID is needed to create a playlist
                                    // rspotify provides `current_user()` to get the profile, or we can use the ID if we have it
                                    if let Ok(me) = sp.client.current_user().await {
                                        let res = sp.client.user_playlist_create(
                                            me.id,
                                            &name,
                                            Some(false), // public = false
                                            Some(false), // collaborative = false
                                            Some("Created by echo-rs"),
                                        ).await;
                                        if res.is_ok()
                                            && let Ok(playlists) = sp.fetch_playlists().await {
                                                let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                            }
                                    }
                                }
                            }
                            AppEvent::RenamePlaylist(playlist_id, new_name) => {
                                if let Some(ref sp) = spotify_opt
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
                                                let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                            }
                                    }
                            }
                            AppEvent::DeletePlaylists(playlist_ids) => {
                                if let Some(ref sp) = spotify_opt {
                                    for id_str in &playlist_ids {
                                        if let Ok(pid) = rspotify::model::PlaylistId::from_id(id_str) {
                                            let _ = sp.client.library_remove([rspotify::model::LibraryId::Playlist(pid)]).await;
                                        }
                                    }
                                    if let Ok(playlists) = sp.fetch_playlists().await {
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
                                            let _ = self.tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
                                        }
                                    }
                                }
                            }
                            AppEvent::FetchQueue => {
                                if let Some(ref sp) = spotify_opt {
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
                            AppEvent::ToggleTrackLike(track_id, like) => {
                                if let Some(ref sp) = spotify_opt {
                                    use rspotify::model::{TrackId, LibraryId};
                                    if let Ok(tid) = TrackId::from_id(&track_id) {
                                        let lib_id = LibraryId::Track(tid);
                                        if like {
                                            let _ = sp.client.library_add([lib_id]).await;
                                        } else {
                                            let _ = sp.client.library_remove([lib_id]).await;
                                        }
                                    }
                                }
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
