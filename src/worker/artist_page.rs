use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::mpsc;

use crate::events::WorkerEvent;

use super::{api::client::EchoSpotifyClient, errors::artist_retry_after_secs};

const MAX_ARTIST_PAGE_AUTO_RETRY_SECS: u64 = 5;

pub fn cancel_pending_artist_page(generation: &AtomicU64) {
    generation.fetch_add(1, Ordering::SeqCst);
}

pub fn spawn_load_artist_page(
    api_client: Option<&EchoSpotifyClient>,
    generation: Arc<AtomicU64>,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    artist_name: Option<String>,
) {
    let Some(api) = api_client.cloned() else {
        return;
    };

    let request_generation = generation.fetch_add(1, Ordering::SeqCst).saturating_add(1);
    tokio::spawn(async move {
        let aid = artist_id;
        let known_artist_name = artist_name;
        let mut attempts = 0usize;
        let result = loop {
            match api.artist_page(&aid, known_artist_name.clone()).await {
                Ok(result) => break Ok(result),
                Err(e) => {
                    if let Some(retry_after_secs) = artist_retry_after_secs(&e)
                        && retry_after_secs <= MAX_ARTIST_PAGE_AUTO_RETRY_SECS
                        && attempts < 5
                    {
                        if generation.load(Ordering::SeqCst) != request_generation {
                            break Ok(None);
                        }
                        attempts += 1;
                        let _ = tx
                            .send(WorkerEvent::ArtistPageRateLimited {
                                artist_id: aid.clone(),
                                retry_after_secs,
                            })
                            .await;
                        tokio::time::sleep(std::time::Duration::from_secs(
                            retry_after_secs.saturating_add(1),
                        ))
                        .await;
                        if generation.load(Ordering::SeqCst) != request_generation {
                            break Ok(None);
                        }
                        continue;
                    }

                    break Err(e);
                }
            }
        };

        match result {
            Ok(Some((artist_name, top_tracks, albums))) => {
                let _ = tx
                    .send(WorkerEvent::ArtistPageLoaded {
                        artist_id: aid.clone(),
                        artist_name,
                        top_tracks,
                        albums,
                    })
                    .await;
            }
            Ok(None) => {}
            Err(e) => {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("echo-debug-artist.log")
                {
                    use std::io::Write;
                    let _ = writeln!(file, "fetch_artist_page err: {:?}", e);
                }
                let _ = tx
                    .send(WorkerEvent::ArtistPageLoadFailed {
                        artist_id: aid.clone(),
                        message: e.to_string(),
                    })
                    .await;
            }
        }
    });
}

pub fn spawn_load_artist_albums(
    api_client: Option<&EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
) {
    let Some(api) = api_client.cloned() else {
        return;
    };

    tokio::spawn(async move {
        match api.artist_albums(&artist_id).await {
            Ok(Some(albums)) => {
                let _ = tx
                    .send(WorkerEvent::ArtistAlbumsLoaded { artist_id, albums })
                    .await;
            }
            Ok(None) => {}
            Err(e) => {
                let _ = tx
                    .send(WorkerEvent::ArtistAlbumsLoadFailed {
                        artist_id,
                        message: e.to_string(),
                    })
                    .await;
            }
        }
    });
}
