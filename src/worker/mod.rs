pub mod audio;
pub mod spotify;

use crate::config::AppConfig;
use crate::events::{AppEvent, WorkerEvent};
use crate::models::PlaybackItem;
use spotify::SpotifyWorker;
use tokio::sync::mpsc;

pub struct Worker {
    rx: mpsc::Receiver<AppEvent>,
    tx: mpsc::Sender<WorkerEvent>,
}

impl Worker {
    pub fn new(rx: mpsc::Receiver<AppEvent>, tx: mpsc::Sender<WorkerEvent>) -> Self {
        Self { rx, tx }
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
                    Ok(Some((is_playing, is_shuffled, progress_ms, item))) => {
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
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut sync_interval = tokio::time::interval(std::time::Duration::from_secs(10));
        let mut is_playing = false;
        let mut current_track_id: Option<String> = None;

        loop {
            tokio::select! {
                _ = sync_interval.tick() => {
                    if let Some(ref mut sp) = spotify_opt {
                        if let Ok(Some((playing, shuffled, progress_ms, item))) = sp.sync_playback_state().await {
                            is_playing = playing;
                            if let Some(item) = item.as_ref() {
                                current_track_id = Some(item.id.clone());
                            }
                            let _ = self.tx.send(WorkerEvent::SyncPlaybackState { is_playing: playing, is_shuffled: shuffled, progress_ms, item }).await;
                        }
                    }
                }
                _ = interval.tick() => {
                    if is_playing {
                        let _ = self.tx.send(WorkerEvent::Tick).await;
                    }
                }
                event_opt = self.rx.recv() => {
                    if let Some(event) = event_opt {
                        match event {
                            AppEvent::Quit => break,
                            AppEvent::StartAuth => {
                                let config = AppConfig::load();
                                if config.spotify_credentials.is_some() {
                                    if let Ok(client) = SpotifyWorker::new(&config).await {
                                        spotify_opt = Some(client);
                                        let _ = self.tx.send(WorkerEvent::AuthenticationComplete).await;

                                        audio::spawn_librespot_daemon(String::new(), "Echo TUI".to_string(), self.tx.clone()).await;

                                        // Fetch playlists initially
                                        if let Some(ref mut sp) = spotify_opt {
                                            if let Ok(playlists) = sp.fetch_playlists().await {
                                                let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
                                            }
                                            if let Ok(albums) = sp.fetch_albums().await {
                                                let _ = self.tx.send(WorkerEvent::AlbumsLoaded(albums)).await;
                                            }
                                            // Initial State Sync (Seamless Handoff)
                                            if let Ok(Some((is_playing, is_shuffled, progress_ms, item))) = sp.sync_playback_state().await {
                                                if let Some(item) = item.as_ref() {
                                                    current_track_id = Some(item.id.clone());
                                                }
                                                let _ = self.tx.send(WorkerEvent::SyncPlaybackState { is_playing, is_shuffled, progress_ms, item }).await;
                                            }
                                        }
                                    }
                                }
                            }
                            AppEvent::LoadContextTracks(id, is_album) => {
                                if let Some(ref sp) = spotify_opt {
                                    let tracks = if is_album {
                                        sp.fetch_album_tracks(&id).await.unwrap_or_default()
                                    } else {
                                        sp.fetch_tracks(&id).await.unwrap_or_default()
                                    };
                                    let _ = self.tx.send(WorkerEvent::TracksLoaded(tracks)).await;
                                }
                            }
                            AppEvent::PlayTrack { context_id, track_id, is_album, title, artist, duration_ms, image_url } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    match sp.play_track(&context_id, &track_id, is_album).await {
                                        Ok(_) => {
                                            is_playing = true;
                                            current_track_id = Some(track_id.clone());
                                            let item = PlaybackItem {
                                                id: track_id,
                                                title,
                                                artist,
                                                duration_ms,
                                                image_url,
                                            };
                                            let _ = self.tx.send(WorkerEvent::PlaybackStarted {
                                                item,
                                            }).await;
                                        }
                                        Err(e) => {
                                            let _ = std::fs::write("echo-debug-worker.log", format!("Worker PlayTrack failed: {:?}", e));
                                        }
                                    }
                                }
                            }
                            AppEvent::TogglePlayback(playing) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.toggle_playback(playing).await;
                                }
                            }
                            AppEvent::ToggleShuffle(shuffled) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.toggle_shuffle(shuffled).await;
                                }
                            }
                            AppEvent::NextTrack { current_track_id: ui_current_track_id } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.next_track().await;
                                    is_playing = true;
                                    let previous_track_id = ui_current_track_id.or_else(|| current_track_id.clone());
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), previous_track_id, false);
                                }
                            }
                            AppEvent::PreviousTrack { current_track_id: ui_current_track_id } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.previous_track().await;
                                    is_playing = true;
                                    let previous_track_id = ui_current_track_id.or_else(|| current_track_id.clone());
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone(), previous_track_id, true);
                                }
                            }
                            AppEvent::LoadTrackMetadata(tid) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    if let Ok((title, artist, image_url)) = sp.get_track_metadata(&tid).await {
                                        let _ = self.tx.send(WorkerEvent::TrackMetadataLoaded { track_id: tid, title, artist, image_url }).await;
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
