use super::api::SpotifyWorker;
use crate::events::{AppEvent, WorkerEvent};
use anyhow::Result;
use rspotify::{
    model::{PlayableId, PlaylistId, TrackId},
    prelude::*,
};
use tokio::sync::mpsc;

pub fn spawn(sp: SpotifyWorker, tx: mpsc::Sender<WorkerEvent>, event: AppEvent) {
    tokio::spawn(async move {
        if let Err(error) = update(&sp, &tx, event).await {
            let _ = tx
                .send(WorkerEvent::ApiRequestFailed {
                    label: "Playlist".into(),
                    message: super::errors::api_request_error_message(&error),
                })
                .await;
        }
    });
}

#[allow(deprecated)]
async fn update(sp: &SpotifyWorker, tx: &mpsc::Sender<WorkerEvent>, event: AppEvent) -> Result<()> {
    let name = match &event {
        AppEvent::CreatePlaylistWithTracks { name, .. }
        | AppEvent::UpdatePlaylistDetails { name, .. } => name.clone(),
        _ => String::new(),
    };
    let message = match event {
        AppEvent::UpdatePlaylistDetails {
            id,
            name,
            description,
            public,
        } => {
            sp.client
                .playlist_change_detail(
                    PlaylistId::from_id(&id)?,
                    Some(&name),
                    Some(public),
                    Some(&description),
                    None,
                )
                .await?;
            let _ = tx
                .send(WorkerEvent::PlaylistDetailsUpdated {
                    id,
                    name,
                    description,
                    public,
                })
                .await;
            "messages.playlist_updated"
        }
        AppEvent::FollowPlaylist(id) => {
            sp.client
                .playlist_follow(PlaylistId::from_id(&id)?, Some(false))
                .await?;
            "messages.followed_playlist"
        }
        AppEvent::UnfollowPlaylist(id) => {
            sp.client
                .playlist_unfollow(PlaylistId::from_id(&id)?)
                .await?;
            "messages.unfollowed_playlist"
        }
        AppEvent::CreatePlaylistWithTracks { name, tracks } => {
            anyhow::ensure!(
                tracks
                    .iter()
                    .all(|t| t.source == crate::models::TrackSource::Spotify),
                "Local tracks cannot be added to a Spotify playlist"
            );
            let ids: Vec<_> = tracks
                .iter()
                .filter(|t| t.source == crate::models::TrackSource::Spotify)
                .map(|t| TrackId::from_id(&t.id).map(PlayableId::Track))
                .collect::<std::result::Result<_, _>>()?;
            anyhow::ensure!(!ids.is_empty(), "No Spotify tracks to add");
            let user = sp.client.current_user().await?;
            let playlist = sp
                .client
                .user_playlist_create(user.id, &name, Some(false), None, None)
                .await?;
            for batch in ids.chunks(100) {
                sp.client
                    .playlist_add_items(playlist.id.clone(), batch.iter().cloned(), None)
                    .await?;
            }
            "messages.created_playlist"
        }
        _ => return Ok(()),
    };
    let playlists = sp.fetch_playlists().await?;
    super::save_playlists_cache(playlists.clone());
    let _ = tx.send(WorkerEvent::PlaylistsLoaded(playlists)).await;
    let _ = tx
        .send(WorkerEvent::PlaylistOperationCompleted { key: message, name })
        .await;
    Ok(())
}
