use tokio::sync::mpsc;

use crate::{
    config::{AppConfig, CacheData},
    events::WorkerEvent,
    models::TrackListContext,
};

use super::api::SpotifyWorker;

pub async fn load_context_tracks(
    spotify: Option<&SpotifyWorker>,
    api: Option<&super::api::client::EchoSpotifyClient>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
) {
    load_context_tracks_with_policy(spotify, api, context, tx, ContextTrackCachePolicy::UseCache)
        .await;
}

pub async fn refresh_context_tracks(
    spotify: Option<&SpotifyWorker>,
    api: Option<&super::api::client::EchoSpotifyClient>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
) {
    load_context_tracks_with_policy(spotify, api, context, tx, ContextTrackCachePolicy::Refresh)
        .await;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextTrackCachePolicy {
    UseCache,
    Refresh,
}

async fn load_context_tracks_with_policy(
    spotify: Option<&SpotifyWorker>,
    api: Option<&super::api::client::EchoSpotifyClient>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
    policy: ContextTrackCachePolicy,
) {
    let Some(sp) = spotify else {
        return;
    };

    if context.is_album() {
        load_album_tracks(sp, api, context, tx, policy).await;
    } else {
        load_playlist_tracks(sp, api, context, tx, policy).await;
    }
}

async fn load_album_tracks(
    sp: &SpotifyWorker,
    api: Option<&super::api::client::EchoSpotifyClient>,
    mut context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
    policy: ContextTrackCachePolicy,
) {
    if policy == ContextTrackCachePolicy::UseCache
        && let Some(entry) = AppConfig::load_cache().get_context_tracks_entry(&context)
    {
        let needs_refresh = CacheData::context_tracks_need_refresh(&entry);
        let cached = entry.value;
        send_loaded(api, cached.tracks.clone(), cached.context.clone(), tx).await;
        if !needs_refresh {
            return;
        }
        context = cached.context;
    }

    let id = context.id.clone();
    match sp.fetch_album_tracks(&id).await {
        Ok((tracks, album_metadata)) => {
            if let Some((album_id, title, artists, image_url)) = album_metadata {
                context.id = album_id;
                context.title = title;
                context.subtitle = artists;
                if !image_url.is_empty() {
                    context.image_url = Some(image_url);
                }
            }
            update_context_cache(context.clone(), tracks.clone());
            send_loaded(api, tracks, context, tx).await;
        }
        Err(e) => {
            let _ = std::fs::write(
                crate::config::debug_log_path("echo-debug-tracks.log"),
                format!("load album tracks err id={id}: {e:?}\n"),
            );
            let _ = tx
                .send(WorkerEvent::TracksLoadFailed {
                    context_id: id,
                    message: e.to_string(),
                })
                .await;
        }
    }
}

async fn load_playlist_tracks(
    sp: &SpotifyWorker,
    api: Option<&super::api::client::EchoSpotifyClient>,
    mut context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
    policy: ContextTrackCachePolicy,
) {
    if policy == ContextTrackCachePolicy::UseCache
        && context.id == "LIKED_SONGS"
        && let Some(entry) = AppConfig::load_cache().get_context_tracks_entry(&context)
    {
        let needs_refresh = CacheData::context_tracks_need_refresh(&entry);
        let cached = entry.value;
        send_loaded(api, cached.tracks.clone(), cached.context.clone(), tx).await;
        if !needs_refresh {
            return;
        }
        context = cached.context;
    }

    let id = context.id.clone();
    match sp.fetch_tracks(&id).await {
        Ok(tracks) => {
            update_context_cache(context.clone(), tracks.clone());
            send_loaded(api, tracks, context, tx).await;
        }
        Err(e) => {
            let _ = std::fs::write(
                crate::config::debug_log_path("echo-debug-tracks.log"),
                format!("load playlist tracks err id={id}: {e:?}\n"),
            );
            let _ = tx
                .send(WorkerEvent::TracksLoadFailed {
                    context_id: id,
                    message: e.to_string(),
                })
                .await;
        }
    }
}

fn update_context_cache(context: TrackListContext, tracks: Vec<crate::models::Track>) {
    AppConfig::update_cache(|cache| cache.set_context_tracks(context, tracks));
}

async fn send_loaded(
    api: Option<&super::api::client::EchoSpotifyClient>,
    mut tracks: Vec<crate::models::Track>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
) {
    let details = if let Some(api) = api {
        match api.context_details(&context, &mut tracks).await {
            Ok(details) => Some(details),
            Err(error) => {
                let _ = tx
                    .send(WorkerEvent::ApiRequestFailed {
                        label: context.title.clone(),
                        message: error.to_string(),
                    })
                    .await;
                None
            }
        }
    } else {
        None
    };
    let context_id = context.id.clone();
    let _ = tx.send(WorkerEvent::TracksLoaded(tracks, context)).await;
    if let Some(details) = details {
        let _ = tx
            .send(WorkerEvent::ContextDetailsLoaded {
                context_id,
                details,
            })
            .await;
    }
}
