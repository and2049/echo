use tokio::sync::mpsc;

use crate::{
    config::{AppConfig, CacheData},
    events::WorkerEvent,
    models::{TrackListContext, TrackListContextKind},
};

use super::api::SpotifyWorker;

pub async fn load_context_tracks(
    spotify: Option<&SpotifyWorker>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
) {
    load_context_tracks_with_policy(spotify, context, tx, ContextTrackCachePolicy::UseCache).await;
}

pub async fn refresh_context_tracks(
    spotify: Option<&SpotifyWorker>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
) {
    load_context_tracks_with_policy(spotify, context, tx, ContextTrackCachePolicy::Refresh).await;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextTrackCachePolicy {
    UseCache,
    Refresh,
}

async fn load_context_tracks_with_policy(
    spotify: Option<&SpotifyWorker>,
    context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
    policy: ContextTrackCachePolicy,
) {
    let Some(sp) = spotify else {
        return;
    };

    if context.is_album() {
        load_album_tracks(sp, context, tx, policy).await;
    } else {
        load_playlist_tracks(sp, context, tx, policy).await;
    }
}

pub(crate) fn route_for_context(context: &TrackListContext) -> TrackListContextKind {
    context.kind
}

async fn load_album_tracks(
    sp: &SpotifyWorker,
    mut context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
    policy: ContextTrackCachePolicy,
) {
    if policy == ContextTrackCachePolicy::UseCache
        && let Some(entry) = AppConfig::load_cache().get_context_tracks_entry(&context)
    {
        let needs_refresh = CacheData::context_tracks_need_refresh(&entry);
        let cached = entry.value;
        let _ = tx
            .send(WorkerEvent::TracksLoaded(
                cached.tracks.clone(),
                cached.context.clone(),
            ))
            .await;
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
            let _ = tx.send(WorkerEvent::TracksLoaded(tracks, context)).await;
        }
        Err(e) => {
            let _ = std::fs::write(
                "echo-debug-tracks.log",
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
    mut context: TrackListContext,
    tx: &mpsc::Sender<WorkerEvent>,
    policy: ContextTrackCachePolicy,
) {
    if policy == ContextTrackCachePolicy::UseCache
        && let Some(entry) = AppConfig::load_cache().get_context_tracks_entry(&context)
    {
        let needs_refresh = CacheData::context_tracks_need_refresh(&entry);
        let cached = entry.value;
        let _ = tx
            .send(WorkerEvent::TracksLoaded(
                cached.tracks.clone(),
                cached.context.clone(),
            ))
            .await;
        if !needs_refresh {
            return;
        }
        context = cached.context;
    }

    let id = context.id.clone();
    match sp.fetch_tracks(&id).await {
        Ok(tracks) => {
            update_context_cache(context.clone(), tracks.clone());
            let _ = tx.send(WorkerEvent::TracksLoaded(tracks, context)).await;
        }
        Err(e) => {
            let _ = std::fs::write(
                "echo-debug-tracks.log",
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
    let mut cache = AppConfig::load_cache();
    cache.set_context_tracks(context, tracks);
    let _ = AppConfig::save_cache(&cache);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContextKind;

    #[test]
    fn context_routes_stay_typed() {
        let playlist = TrackListContext::playlist(
            "playlist".to_string(),
            "Playlist".to_string(),
            "Owner".to_string(),
            "owner".to_string(),
            None,
        );
        let album = TrackListContext::album(
            "album".to_string(),
            "Album".to_string(),
            "Artist".to_string(),
            None,
        );
        let generated = TrackListContext::generated("TOP_TRACKS", "Top Tracks");

        assert_eq!(route_for_context(&playlist), TrackListContextKind::Playlist);
        assert_eq!(route_for_context(&album), TrackListContextKind::Album);
        assert_eq!(
            route_for_context(&generated),
            TrackListContextKind::Generated
        );
    }
}
