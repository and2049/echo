use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::mpsc;

use crate::{events::WorkerEvent, models::DiscographyFilter};

use super::{
    api::client::{ArtistAlbumsCachePolicy, EchoSpotifyClient},
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
        // The Option survives for the top-tracks search below: querying the literal
        // "Unknown Artist" placeholder would return garbage.
        let real_artist_name = artist_name.clone();
        let known_artist_name = artist_name.unwrap_or_else(|| "Unknown Artist".to_string());

        // If we already know the image URL, send it immediately; otherwise
        // spawn a background fetch so the header can appear once resolved.
        let resolved_image_url = if artist_image_url.is_some() {
            artist_image_url.clone()
        } else {
            // Kick off a parallel fetch — we'll send ArtistImageResolved when done.
            let api_clone = api.clone();
            let tx_clone = tx.clone();
            let id_clone = artist_id.clone();
            tokio::spawn(async move {
                if let Some(url) = api_clone.fetch_artist_image_url(&id_clone).await {
                    let _ = tx_clone
                        .send(WorkerEvent::ArtistImageResolved {
                            artist_id: id_clone,
                            image_url: url,
                        })
                        .await;
                }
            });
            None
        };

        let _ = tx
            .send(WorkerEvent::ArtistPageOpened {
                artist_id: artist_id.clone(),
                artist_name: known_artist_name.clone(),
                artist_image_url: resolved_image_url,
            })
            .await;

        // Top tracks load in parallel with the albums below; stale responses are
        // discarded by the reducers' pending-artist-id guard.
        {
            let api_clone = api.clone();
            let tx_clone = tx.clone();
            let id_clone = artist_id.clone();
            tokio::spawn(async move {
                match api_clone
                    .artist_top_tracks(&id_clone, real_artist_name.as_deref())
                    .await
                {
                    Ok(Some(tracks)) => {
                        let _ = tx_clone
                            .send(WorkerEvent::ArtistTopTracksLoaded {
                                artist_id: id_clone,
                                tracks,
                            })
                            .await;
                    }
                    // An identical fetch is already in flight; that task will emit.
                    Ok(None) => {}
                    Err(e) => {
                        let _ = tx_clone
                            .send(WorkerEvent::ArtistTopTracksLoadFailed {
                                artist_id: id_clone,
                                message: e.to_string(),
                            })
                            .await;
                    }
                }
            });
        }
        load_discography(
            api,
            tx,
            artist_id,
            DiscographyFilter::All,
            ArtistAlbumsCachePolicy::UseCache,
            false,
        )
        .await;
    });
}

pub fn spawn_load_artist_albums(
    api_client: Option<&EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    filter: DiscographyFilter,
    more: bool,
) {
    let Some(api) = api_client.cloned() else {
        return;
    };
    tokio::spawn(load_discography(
        api,
        tx,
        artist_id,
        filter,
        ArtistAlbumsCachePolicy::UseCache,
        more,
    ));
}

pub fn spawn_refresh_artist_albums(
    api_client: Option<&EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    filter: DiscographyFilter,
) {
    let Some(api) = api_client.cloned() else {
        return;
    };
    tokio::spawn(load_discography(
        api,
        tx,
        artist_id,
        filter,
        ArtistAlbumsCachePolicy::Refresh,
        false,
    ));
}

async fn load_discography(
    api: EchoSpotifyClient,
    tx: mpsc::Sender<WorkerEvent>,
    artist_id: String,
    filter: DiscographyFilter,
    policy: ArtistAlbumsCachePolicy,
    more: bool,
) {
    let event = match api.discography_page(&artist_id, filter, policy, more).await {
        Ok(Some(page)) => WorkerEvent::ArtistAlbumsLoaded {
            artist_id,
            filter,
            page,
        },
        Ok(None) => WorkerEvent::ArtistAlbumsLoadFailed {
            artist_id,
            message: "refresh already in progress".to_string(),
        },
        Err(err) => {
            // An empty chip still shows the cached releases under the failure message.
            if !more && let Some(page) = api.stale_discography(&artist_id, filter).await {
                let _ = tx
                    .send(WorkerEvent::ArtistAlbumsLoaded {
                        artist_id: artist_id.clone(),
                        filter,
                        page,
                    })
                    .await;
            }
            match artist_retry_after_secs(&err) {
                Some(retry_after_secs) => WorkerEvent::ArtistAlbumsRateLimited {
                    artist_id,
                    retry_after_secs,
                },
                None => WorkerEvent::ArtistAlbumsLoadFailed {
                    artist_id,
                    message: err.to_string(),
                },
            }
        }
    };
    let _ = tx.send(event).await;
}
