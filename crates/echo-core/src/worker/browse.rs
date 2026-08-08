use tokio::sync::mpsc;

use crate::events::WorkerEvent;
use crate::models::Album;

use super::{
    api::client::{ArtistAlbumsCachePolicy, EchoSpotifyClient},
    errors::api_request_error_message,
};

/// How far back the What's New feed reaches, as an ISO date cutoff.
const WHATS_NEW_WINDOW_DAYS: i64 = 90;
const WHATS_NEW_MAX_ARTISTS: usize = 50;
const WHATS_NEW_MAX_ALBUMS: usize = 50;

pub fn spawn_top_tracks(
    api_client: Option<EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    range: crate::models::TopItemsRange,
) {
    let Some(api) = api_client else {
        return;
    };
    tokio::spawn(async move {
        match api.top_tracks(range).await {
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

pub fn spawn_top_artists(
    api_client: Option<EchoSpotifyClient>,
    tx: mpsc::Sender<WorkerEvent>,
    range: crate::models::TopItemsRange,
) {
    let Some(api) = api_client else {
        return;
    };
    tokio::spawn(async move {
        match api.top_artists(range).await {
            Ok(Some(artists)) => {
                let _ = tx.send(WorkerEvent::TopArtistsLoaded(artists)).await;
            }
            Ok(None) => {}
            Err(e) => {
                send_api_error(tx, "Top artists", "top_artists", e).await;
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

/// Scans followed artists' discographies for recent releases. Serves the 6h persistent
/// cache when fresh; otherwise walks artists sequentially through the shared artist-albums
/// cache (warming artist pages as a side effect), emitting cumulative snapshots as it goes.
pub fn spawn_whats_new(api_client: Option<EchoSpotifyClient>, tx: mpsc::Sender<WorkerEvent>) {
    let Some(api) = api_client else {
        return;
    };
    tokio::spawn(async move {
        if let Some(albums) = crate::config::AppConfig::load_cache().get_whats_new() {
            let _ = tx
                .send(WorkerEvent::WhatsNewLoaded {
                    albums,
                    done: 1,
                    total: 1,
                })
                .await;
            return;
        }

        let artists = match api.followed_artists().await {
            Ok(Some(artists)) => artists,
            Ok(None) => return,
            Err(e) => {
                send_api_error(tx, "What's New", "whats_new", e).await;
                return;
            }
        };

        let cutoff = whats_new_cutoff();
        let total = artists.len().min(WHATS_NEW_MAX_ARTISTS);
        let mut merged: Vec<Album> = Vec::new();
        for (index, artist) in artists.into_iter().take(WHATS_NEW_MAX_ARTISTS).enumerate() {
            let done = index + 1;
            let mut from_network = false;
            match api
                .artist_albums_with_policy(&artist.id, ArtistAlbumsCachePolicy::UseCache)
                .await
            {
                Ok(response) => {
                    from_network = response.refreshed.is_some();
                    if let Some(albums) = response.refreshed.or(response.cached) {
                        merged.extend(albums);
                    }
                }
                Err(_) => {}
            }
            if done % 3 == 0 && done < total {
                let _ = tx
                    .send(WorkerEvent::WhatsNewLoaded {
                        albums: recent_releases(&merged, &cutoff),
                        done,
                        total,
                    })
                    .await;
            }
            if from_network && done < total {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }

        let albums = recent_releases(&merged, &cutoff);
        let mut cache = crate::config::AppConfig::load_cache();
        cache.set_whats_new(albums.clone());
        let _ = crate::config::AppConfig::save_cache(&cache);
        let _ = tx
            .send(WorkerEvent::WhatsNewLoaded {
                albums,
                done: total,
                total,
            })
            .await;
    });
}

fn whats_new_cutoff() -> String {
    (chrono::Utc::now() - chrono::Duration::days(WHATS_NEW_WINDOW_DAYS))
        .format("%Y-%m-%d")
        .to_string()
}

/// Filters to releases dated on or after `cutoff` (ISO string compare, so year-only
/// precision entries drop out), dedupes by id, newest first, capped.
fn recent_releases(albums: &[Album], cutoff: &str) -> Vec<Album> {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<Album> = albums
        .iter()
        .filter(|album| album.release_date.as_deref().is_some_and(|date| date >= cutoff))
        .filter(|album| seen.insert(album.id.clone()))
        .cloned()
        .collect();
    out.sort_by(|a, b| b.release_date.cmp(&a.release_date));
    out.truncate(WHATS_NEW_MAX_ALBUMS);
    out
}

async fn send_api_error(
    tx: mpsc::Sender<WorkerEvent>,
    label: &str,
    log_name: &str,
    err: anyhow::Error,
) {
    let message = api_request_error_message(&err);
    let _ = std::fs::write(crate::config::debug_log_path("echo-debug-api.log"), format!("{log_name} err: {err:?}\n"));
    let _ = tx
        .send(WorkerEvent::ApiRequestFailed {
            label: label.to_string(),
            message,
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(id: &str, release_date: Option<&str>) -> Album {
        Album {
            id: id.to_string(),
            name: id.to_string(),
            artists: String::new(),
            image_url: None,
            thumb_url: None,
            release_year: release_date
                .map(|d| d.chars().take(4).collect())
                .unwrap_or_default(),
            release_date: release_date.map(str::to_string),
            track_count: None,
        }
    }

    #[test]
    fn recent_releases_filters_dedupes_and_sorts_newest_first() {
        let albums = vec![
            album("old", Some("2025-01-01")),
            album("new", Some("2026-08-01")),
            album("newer", Some("2026-08-05")),
            album("new", Some("2026-08-01")),
            album("year-only", Some("2026")),
            album("undated", None),
        ];
        let out = recent_releases(&albums, "2026-05-10");
        let ids: Vec<&str> = out.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["newer", "new"]);
    }

    #[test]
    fn recent_releases_month_precision_survives_the_cutoff() {
        let albums = vec![album("month", Some("2026-08"))];
        assert_eq!(recent_releases(&albums, "2026-05-10").len(), 1);
        assert_eq!(recent_releases(&albums, "2026-09-01").len(), 0);
    }
}
