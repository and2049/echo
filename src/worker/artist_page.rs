use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use crate::events::WorkerEvent;

use super::{
    api::client::{ArtistAlbumsCachePolicy, ArtistAlbumsResponse, EchoSpotifyClient},
    errors::artist_retry_after_secs,
};

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
        match api
            .artist_albums_with_policy(&artist_id, ArtistAlbumsCachePolicy::UseCache)
            .await
        {
            Ok(response) => {
                send_artist_albums_response(tx, artist_id, response).await;
            }
            Err(e) => {
                send_artist_albums_error(tx, artist_id, e).await;
            }
        }
    });
}

pub fn spawn_refresh_artist_albums(
    api_client: Option<&EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
) {
    let Some(api) = api_client.cloned() else {
        return;
    };

    tokio::spawn(async move {
        match api
            .artist_albums_with_policy(&artist_id, ArtistAlbumsCachePolicy::Refresh)
            .await
        {
            Ok(response) => {
                send_artist_albums_response(tx, artist_id, response).await;
            }
            Err(e) => {
                send_artist_albums_error(tx, artist_id, e).await;
            }
        }
    });
}

async fn send_artist_albums_response(
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    response: ArtistAlbumsResponse,
) {
    let mut sent_albums = false;
    if let Some(albums) = response.cached {
        sent_albums = true;
        let _ = tx
            .send(WorkerEvent::ArtistAlbumsLoaded {
                artist_id: artist_id.clone(),
                albums,
            })
            .await;
    }
    if let Some(albums) = response.refreshed {
        sent_albums = true;
        let _ = tx
            .send(WorkerEvent::ArtistAlbumsLoaded {
                artist_id: artist_id.clone(),
                albums,
            })
            .await;
    }
    if response.refresh_skipped && !sent_albums {
        let _ = tx
            .send(WorkerEvent::ArtistAlbumsLoadFailed {
                artist_id,
                message: "refresh already in progress".to_string(),
            })
            .await;
    }
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
