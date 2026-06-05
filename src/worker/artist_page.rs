use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use crate::events::WorkerEvent;

use super::{api::client::EchoSpotifyClient, errors::artist_retry_after_secs};

pub fn cancel_pending_artist_page(generation: &AtomicU64) {
    generation.fetch_add(1, Ordering::SeqCst);
}

pub fn spawn_load_artist_page(
    api_client: Option<&EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    artist_name: Option<String>,
    artist_image_url: Option<String>,
) {
    let Some(api) = api_client.cloned() else {
        return;
    };

    tokio::spawn(async move {
        let known_artist_name = artist_name.unwrap_or_else(|| "Unknown Artist".to_string());
        let _ = tx
            .send(WorkerEvent::ArtistPageOpened {
                artist_id: artist_id.clone(),
                artist_name: known_artist_name.clone(),
                artist_image_url,
            })
            .await;
        match api.artist_albums(&artist_id).await {
            Ok(Some(albums)) => {
                let _ = tx
                    .send(WorkerEvent::ArtistAlbumsLoaded { artist_id, albums })
                    .await;
            }
            Ok(None) => {}
            Err(e) => {
                send_artist_albums_error(tx, artist_id, e).await;
            }
        }
    });
}

async fn send_artist_albums_error(
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    err: anyhow::Error,
) {
    if let Some(retry_after_secs) = artist_retry_after_secs(&err) {
        let _ = tx
            .send(WorkerEvent::ArtistAlbumsRateLimited {
                artist_id,
                retry_after_secs,
            })
            .await;
    } else {
        let _ = tx
            .send(WorkerEvent::ArtistAlbumsLoadFailed {
                artist_id,
                message: err.to_string(),
            })
            .await;
    }
}
