use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::{AppEvent, WorkerEvent},
    tui::Tui,
};

mod auth;
mod artist;
mod data;
mod images;
mod library;
mod misc;
mod playback;


pub async fn apply_worker_event(
    worker_event: WorkerEvent,
    state: &mut AppState,
    app_tx: &mpsc::Sender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    tui: &mut Tui,
) {
    match worker_event {
        WorkerEvent::AuthenticationComplete => auth::handle(state),
        WorkerEvent::UserIdentityLoaded(user_id) => auth::handle_user_identity(state, user_id),
        WorkerEvent::ForceContextRefresh => misc::handle_force_context_refresh(state, app_tx).await,
        WorkerEvent::ApiRequestFailed { label, message } => {
            misc::handle_api_request_failed(state, label, message)
        }
        WorkerEvent::ForceRedraw => misc::handle_force_redraw(tui),
        WorkerEvent::PlaylistsLoaded(playlists) => library::handle_playlists_loaded(state, playlists),
        WorkerEvent::AlbumsLoaded(albums) => library::handle_albums_loaded(state, albums),
        WorkerEvent::LocalLibraryLoaded { library, report } => {
            library::handle_local_library_loaded(state, library, report)
        }
        WorkerEvent::LocalPlaylistsLoaded(playlists) => {
            library::handle_local_playlists_loaded(state, playlists)
        }
        WorkerEvent::LikedStatusUpdate(results) => library::handle_liked_status_update(state, results),
        WorkerEvent::Tick => playback::handle_tick(state, app_tx),
        WorkerEvent::PlaybackStarted { item } => {
            playback::handle_playback_started(state, app_tx, worker_tx, item).await
        }
        WorkerEvent::SyncPlaybackState { is_playing, is_shuffled, repeat_mode, volume, device_name, progress_ms, item } => {
            playback::handle_sync_playback_state(state, app_tx, worker_tx, is_playing, is_shuffled, repeat_mode, volume, device_name, progress_ms, item).await
        }
        WorkerEvent::PlaybackControlState { is_playing } => {
            playback::handle_playback_control_state(state, is_playing)
        }
        WorkerEvent::TrackMetadataLoaded { track_id, title, artist, image_url } => {
            playback::handle_track_metadata_loaded(state, worker_tx, track_id, title, artist, image_url)
        }
        WorkerEvent::TrackImageProcessed { track_id, protocol } => {
            playback::handle_track_image_processed(state, track_id, protocol)
        }
        WorkerEvent::LyricsLoaded(lyrics) => playback::handle_lyrics_loaded(state, lyrics),
        WorkerEvent::AudioVisualizationReady(bands, flag) => {
            playback::handle_audio_visualization_ready(state, bands, flag)
        }
        WorkerEvent::HeaderImageProcessed(protocol) => images::handle(state, protocol),
        WorkerEvent::TracksLoaded(tracks, context) => {
            data::handle_tracks_loaded(state, worker_tx, tracks, context)
        }
        WorkerEvent::TracksLoadFailed { context_id: _, message } => {
            data::handle_tracks_load_failed(state, message)
        }
        WorkerEvent::SearchResultsLoaded(results) => data::handle_search_results_loaded(state, results),
        WorkerEvent::QueueLoaded(tracks) => data::handle_queue_loaded(state, tracks),
        WorkerEvent::DevicesLoaded(devices) => data::handle_devices_loaded(state, devices),
        WorkerEvent::TracksQueued(count) => data::handle_tracks_queued(state, count),
        WorkerEvent::TopTracksLoaded(tracks) => data::handle_top_tracks_loaded(state, tracks),
        WorkerEvent::RecentlyPlayedLoaded(tracks) => data::handle_recently_played_loaded(state, tracks),
        WorkerEvent::FollowedArtistsLoaded(artists) => data::handle_followed_artists_loaded(state, artists),
        WorkerEvent::ArtistPageOpened { artist_id, artist_name, artist_image_url } => {
            artist::handle_page_opened(state, worker_tx, artist_id, artist_name, artist_image_url)
        }
        WorkerEvent::ArtistAlbumsLoaded { artist_id, albums } => {
            artist::handle_albums_loaded(state, artist_id, albums)
        }
        WorkerEvent::ArtistAlbumsLoadFailed { artist_id, message } => {
            artist::handle_albums_load_failed(state, artist_id, message)
        }
        WorkerEvent::ArtistAlbumsRateLimited { artist_id, retry_after_secs } => {
            artist::handle_albums_rate_limited(state, artist_id, retry_after_secs)
        }
        WorkerEvent::ArtistImageResolved { artist_id, image_url } => {
            artist::handle_image_resolved(state, worker_tx, artist_id, image_url)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContext;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn force_context_refresh_reuses_stored_context() {
        let (app_tx, mut app_rx) = mpsc::channel(1);
        let (worker_tx, _) = mpsc::channel(1);
        let mut tui = Tui::test();
        let mut state = AppState::new();
        let context = TrackListContext::playlist(
            "playlist".to_string(),
            "Playlist".to_string(),
            "Owner".to_string(),
            "owner".to_string(),
            None,
        );
        state.ui.active_view = app::ActiveView::TrackList;
        state.data.active_tracklist_context = Some(context.clone());

        apply_worker_event(
            WorkerEvent::ForceContextRefresh,
            &mut state,
            &app_tx,
            &worker_tx,
            &mut tui,
        )
        .await;

        let AppEvent::RefreshContextTracks(sent) = app_rx.try_recv().unwrap() else {
            panic!("expected RefreshContextTracks");
        };
        assert_eq!(sent, context);
    }
}
