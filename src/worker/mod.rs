pub mod spotify;
pub mod audio;

use rspotify::prelude::*;
use tokio::sync::mpsc;
use crate::events::{AppEvent, WorkerEvent};
use crate::config::AppConfig;
use spotify::SpotifyWorker;

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
    ) {
        tokio::spawn(async move {
            let mut log = String::from("=== spawn_playback_sync started ===\n");

            for attempt in 0..5u32 {
                let wait_secs = attempt + 1;
                log.push_str(&format!("Attempt {}: waiting {}s...\n", attempt, wait_secs));
                tokio::time::sleep(std::time::Duration::from_secs(wait_secs as u64)).await;

                let result = client.current_playback(None, None::<Vec<_>>).await;

                match &result {
                    Ok(Some(playback)) => {
                        let is_playing = playback.is_playing;
                        let is_shuffled = playback.shuffle_state;
                        let progress_ms = playback
                            .progress
                            .unwrap_or_default()
                            .num_milliseconds() as u32;

                        let mut duration_ms = 0u32;
                        let mut track_id: Option<String> = None;

                        match &playback.item {
                            Some(rspotify::model::PlayableItem::Track(t)) => {
                                duration_ms = t.duration.num_milliseconds() as u32;
                                track_id = t.id.as_ref().map(|i| i.id().to_string());
                                log.push_str(&format!(
                                    "  → Track: '{}', duration_ms={}, progress_ms={}, is_playing={}\n",
                                    t.name, duration_ms, progress_ms, is_playing
                                ));
                            }
                            Some(rspotify::model::PlayableItem::Episode(e)) => {
                                log.push_str(&format!("  → Item is an Episode: '{}' (not a track, skipping)\n", e.name));
                            }
                            None => {
                                log.push_str("  → playback.item is None (Spotify API transitioning)\n");
                            }
                            _ => {
                                log.push_str("  → playback.item is unknown variant\n");
                            }
                        }

                        if duration_ms > 0 {
                            log.push_str(&format!("  → Sending SyncPlaybackState (duration_ms={})\n", duration_ms));
                            let _ = std::fs::write("echo-debug-sync.log", &log);
                            let _ = tx.send(WorkerEvent::SyncPlaybackState {
                                is_playing,
                                is_shuffled,
                                progress_ms,
                                duration_ms,
                                track_id,
                            }).await;
                            return;
                        }
                        // duration_ms == 0 — item was None or episode, retry
                    }
                    Ok(None) => {
                        log.push_str("  → current_playback returned Ok(None) — no active playback\n");
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

        loop {
            tokio::select! {
                _ = sync_interval.tick() => {
                    if let Some(ref mut sp) = spotify_opt {
                        if let Ok(Some((playing, shuffled, progress_ms, duration_ms, track_id))) = sp.sync_playback_state().await {
                            is_playing = playing;
                            let _ = self.tx.send(WorkerEvent::SyncPlaybackState { is_playing: playing, is_shuffled: shuffled, progress_ms, duration_ms, track_id }).await;
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
                                            // Initial State Sync (Seamless Handoff)
                                            if let Ok(Some((is_playing, is_shuffled, progress_ms, duration_ms, track_id))) = sp.sync_playback_state().await {
                                                let _ = self.tx.send(WorkerEvent::SyncPlaybackState { is_playing, is_shuffled, progress_ms, duration_ms, track_id }).await;
                                            }
                                        }
                                    }
                                }
                            }
                            AppEvent::LoadPlaylistTracks(id) => {
                                if let Some(ref sp) = spotify_opt {
                                    match sp.fetch_tracks(&id).await {
                                        Ok(tracks) => {
                                            let _ = self.tx.send(WorkerEvent::TracksLoaded(tracks)).await;
                                        }
                                        Err(e) => {
                                            let _ = std::fs::write("debug.log", format!("Failed to fetch tracks for {}: {:?}", id, e));
                                        }
                                    }
                                }
                            }
                            AppEvent::PlayTrack { playlist_id, track_id, duration_ms } => {
                                if let Some(ref mut sp) = spotify_opt {
                                    match sp.play_track(&playlist_id, &track_id).await {
                                        Ok(_) => {
                                            is_playing = true;
                                            let _ = self.tx.send(WorkerEvent::PlaybackStarted(duration_ms)).await;
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
                            AppEvent::NextTrack => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.next_track().await;
                                    is_playing = true;
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone());
                                }
                            }
                            AppEvent::PreviousTrack => {
                                if let Some(ref mut sp) = spotify_opt {
                                    let _ = sp.previous_track().await;
                                    is_playing = true;
                                    Self::spawn_playback_sync(sp.client.clone(), self.tx.clone());
                                }
                            }
                            AppEvent::LoadTrackMetadata(tid) => {
                                if let Some(ref mut sp) = spotify_opt {
                                    if let Ok((title, artist, image_url)) = sp.get_track_metadata(&tid).await {
                                        let _ = self.tx.send(WorkerEvent::TrackMetadataLoaded { title, artist, image_url }).await;
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
