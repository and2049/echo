use tokio::sync::mpsc;

use crate::events::WorkerEvent;

use super::{api::client::EchoSpotifyClient, errors::api_request_error_message};

pub fn spawn_top_tracks(api_client: Option<EchoSpotifyClient>, tx: mpsc::Sender<WorkerEvent>) {
    let Some(api) = api_client else {
        return;
    };
    tokio::spawn(async move {
        match api.top_tracks().await {
            Ok(Some(tracks)) => {
                let _ = tx.send(WorkerEvent::TopTracksLoaded(tracks)).await;
            }
            Ok(None) => {}
            Err(e) => {
                send_api_error(tx, "Top tracks", "top_tracks", e).await;
            }
        }
    });
}

pub fn spawn_recently_played(api_client: Option<EchoSpotifyClient>, tx: mpsc::Sender<WorkerEvent>) {
    let Some(api) = api_client else {
        return;
    };
    tokio::spawn(async move {
        match api.recently_played().await {
            Ok(Some(tracks)) => {
                let _ = tx.send(WorkerEvent::RecentlyPlayedLoaded(tracks)).await;
            }
            Ok(None) => {}
            Err(e) => {
                send_api_error(tx, "Recently played", "recently_played", e).await;
            }
        }
    });
}

pub fn spawn_followed_artists(
    api_client: Option<EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
) {
    let Some(api) = api_client else {
        return;
    };
    tokio::spawn(async move {
        match api.followed_artists().await {
            Ok(Some(artists)) => {
                let _ = tx.send(WorkerEvent::FollowedArtistsLoaded(artists)).await;
            }
            Ok(None) => {}
            Err(e) => {
                send_api_error(tx, "Followed artists", "followed_artists", e).await;
            }
        }
    });
}

async fn send_api_error(
    tx: mpsc::Sender<WorkerEvent>,
    label: &str,
    log_name: &str,
    err: anyhow::Error,
) {
    let message = api_request_error_message(&err);
    let _ = std::fs::write("echo-debug-api.log", format!("{log_name} err: {err:?}\n"));
    let _ = tx
        .send(WorkerEvent::ApiRequestFailed {
            label: label.to_string(),
            message,
        })
        .await;
}
