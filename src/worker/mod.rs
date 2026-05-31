pub mod spotify;

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

    pub async fn run(mut self) {
        let mut spotify_opt: Option<SpotifyWorker> = None;

        while let Some(event) = self.rx.recv().await {
            match event {
                AppEvent::Quit => break,
                AppEvent::StartAuth => {
                    let config = AppConfig::load();
                    if config.spotify_credentials.is_some() {
                        if let Ok(client) = SpotifyWorker::new(&config).await {
                            spotify_opt = Some(client);
                            let _ = self.tx.send(WorkerEvent::AuthenticationComplete).await;
                            
                            // Fetch playlists initially
                            if let Some(ref sp) = spotify_opt {
                                if let Ok(playlists) = sp.fetch_playlists().await {
                                    let _ = self.tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
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
                _ => {}
            }
        }
    }
}
