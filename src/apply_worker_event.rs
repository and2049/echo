use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::{AppEvent, WorkerEvent},
    image_tasks,
    tui::Tui,
};

pub async fn apply_worker_event(
    worker_event: WorkerEvent,
    state: &mut AppState,
    app_tx: &mpsc::Sender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    tui: &mut Tui,
) {
    match worker_event {
        WorkerEvent::AuthenticationComplete => {
            state.mode = app::AppMode::Normal;
        }
        WorkerEvent::ForceContextRefresh => {
            if state.active_view == app::ActiveView::TrackList
                && let Some(context) = state.active_tracklist_context.clone()
                && context.requires_worker_load()
            {
                let _ = app_tx.send(AppEvent::RefreshContextTracks(context)).await;
            }
        }
        WorkerEvent::UserIdentityLoaded(user_id) => {
            state.user_id = Some(user_id);
        }
        WorkerEvent::PlaylistsLoaded(playlists) => {
            state.playlists = playlists;
            state.compute_library_view();
        }
        WorkerEvent::AlbumsLoaded(albums) => {
            state.saved_albums = albums;
        }
        WorkerEvent::LocalLibraryLoaded { library, report } => {
            state.local_library = library;
            state.compute_library_view();
            set_timed_status(
                state,
                format!(
                    "Local scan: {} files, {} added, {} updated, {} removed, {} skipped.",
                    report.files_found,
                    report.tracks_added,
                    report.tracks_updated,
                    report.tracks_removed,
                    report.skipped
                ),
                5,
            );
            if state
                .active_tracklist_context
                .as_ref()
                .is_some_and(|context| {
                    context.kind == crate::models::TrackListContextKind::LocalLibrary
                })
            {
                state.show_local_library();
            }
        }
        WorkerEvent::TracksLoaded(tracks, context) => {
            let preserve_track_selection = state
                .active_tracklist_context
                .as_ref()
                .is_some_and(|active| active.id == context.id && active.kind == context.kind);
            let selected_track_index = if preserve_track_selection && !tracks.is_empty() {
                state
                    .selected_track_index
                    .min(tracks.len().saturating_sub(1))
            } else {
                0
            };
            state.tracks = tracks;
            state.tracklist_image_url = context.image_url.clone();
            if let Some(url) = context.image_url.as_ref() {
                image_tasks::spawn_header_for_url(
                    url,
                    state.image_picker.as_ref(),
                    worker_tx.clone(),
                    state.library_config.cover_img_pixels,
                );
            }
            state.active_tracklist_context = Some(context);
            state.active_view = app::ActiveView::TrackList;
            state.selected_track_index = selected_track_index;
        }
        WorkerEvent::TracksLoadFailed {
            context_id: _,
            message,
        } => {
            set_timed_status(state, format!("Unable to load tracks: {message}"), 5);
        }
        WorkerEvent::ApiRequestFailed { label, message } => {
            let text = if message.starts_with("rate limited") {
                format!("{label} {message}")
            } else {
                format!("{label} failed: {message}")
            };
            set_timed_status(state, text, 5);
        }
        WorkerEvent::PlaybackStarted { item } => {
            state.playback.is_playing = true;
            state.playback.playing_track_id = Some(item.id.clone());
            state.playback.playing_track_title = item.title.clone();
            state.playback.playing_track_artist = item.artist.clone();
            state.playback.playing_track_album_id = item.album_id.clone();
            state.playback.playing_track_artist_id = item.artist_id.clone();
            state.playback.previous_track_image = state.playback.playing_track_image.take();
            state.playback.duration_ms = item.duration_ms;
            state.playback.progress_ms = 0;

            if state.current_lyric_track_id.as_deref() != Some(item.id.as_str()) {
                state.current_lyric_track_id = Some(item.id.clone());
                state.is_fetching_lyrics = true;
                state.current_lyrics = None;
                let _ = app_tx
                    .send(AppEvent::FetchLyrics(
                        item.id.clone(),
                        item.title.clone(),
                        item.artist.clone(),
                        item.duration_ms,
                    ))
                    .await;
            }

            if let Some(url) = item.image_url {
                if let Some(ref picker) = state.image_picker {
                    image_tasks::spawn_track_image_processing(
                        item.id,
                        url,
                        picker,
                        worker_tx.clone(),
                        state.library_config.cover_img_pixels,
                    );
                }
            } else {
                let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id)).await;
            }
        }
        WorkerEvent::Tick => {
            if state.playback.is_playing {
                state.playback.progress_ms += 100;
                if state.playback.duration_ms > 0
                    && state.playback.progress_ms >= state.playback.duration_ms
                {
                    state.playback.is_playing = false;
                    let _ = app_tx.try_send(AppEvent::ForcePlaybackSync);
                }
            }
        }
        WorkerEvent::SyncPlaybackState {
            is_playing,
            is_shuffled,
            repeat_mode,
            volume,
            device_name,
            progress_ms,
            item,
        } => {
            state.playback.is_playing = is_playing;
            state.playback.is_shuffled = is_shuffled;
            state.playback.repeat_mode = repeat_mode;
            if let Some(volume) = volume {
                state.playback.volume = volume;
            }
            state.playback.device_name = device_name;
            state.playback.progress_ms = progress_ms;

            if let Some(item) = item {
                apply_synced_playback_item(item, state, app_tx, worker_tx).await;
            }
        }
        WorkerEvent::TrackMetadataLoaded {
            track_id,
            title,
            artist,
            image_url,
        } => {
            if state.playback.playing_track_id.as_deref() != Some(track_id.as_str()) {
                return;
            }

            state.playback.playing_track_title = title;
            state.playback.playing_track_artist = artist;

            if let Some(url) = image_url
                && let Some(ref picker) = state.image_picker
            {
                image_tasks::spawn_track_image_processing(
                    track_id,
                    url,
                    picker,
                    worker_tx.clone(),
                    state.library_config.cover_img_pixels,
                );
            }
        }
        WorkerEvent::TrackImageProcessed { track_id, protocol } => {
            if state.playback.playing_track_id.as_deref() == Some(track_id.as_str()) {
                state.playback.playing_track_image = Some(protocol);
                state.playback.previous_track_image = None;
                if state.playback.fetching_track_id.as_deref() == Some(track_id.as_str()) {
                    state.playback.fetching_track_id = None;
                }
            }
        }
        WorkerEvent::HeaderImageProcessed(protocol) => {
            state.active_library_header_image = Some(protocol);
            state.header_image_dirty = true;
        }
        WorkerEvent::ForceRedraw => {
            let _ = tui.terminal.clear();
        }
        WorkerEvent::AudioVisualizationReady(shared_bands, flag) => {
            flag.store(
                state.library_config.enable_visualizer,
                std::sync::atomic::Ordering::Relaxed,
            );
            state.playback.audio_visualization = Some(shared_bands);
            state.playback.enable_visualizer = Some(flag);
        }
        WorkerEvent::SearchResultsLoaded(results) => {
            state.search_results = results;
            state.selected_search_index = 0;
            state.active_view = app::ActiveView::SearchResults;
            state.status_message = Some(format!("Search: {}", state.search_context_query));
        }
        WorkerEvent::QueueLoaded(tracks) => {
            state.queue = tracks;
            state.selected_queue_index = 0;
        }
        WorkerEvent::DevicesLoaded(devices) => {
            state.devices = devices;
            if state.selected_device_index >= state.devices.len() {
                state.selected_device_index = state.devices.len().saturating_sub(1);
            }
        }
        WorkerEvent::LyricsLoaded(lyrics) => {
            state.current_lyrics = lyrics;
            state.is_fetching_lyrics = false;
        }
        WorkerEvent::TracksQueued(count) => {
            state.recent_queue_count += count;
            set_timed_status(
                state,
                crate::i18n::t("messages.added_to_queue", &state.library_config.language)
                    .replace("{}", &count.to_string()),
                3,
            );
        }
        WorkerEvent::LikedStatusUpdate(results) => {
            for (id, liked) in results {
                if liked {
                    state.liked_tracks.insert(id);
                } else {
                    state.liked_tracks.remove(&id);
                }
            }
        }
        WorkerEvent::TopTracksLoaded(tracks) => {
            state.top_tracks = tracks;
        }
        WorkerEvent::RecentlyPlayedLoaded(tracks) => {
            state.recently_played = tracks;
        }
        WorkerEvent::FollowedArtistsLoaded(artists) => {
            state.followed_artists = artists;
        }
        WorkerEvent::ArtistPageOpened {
            artist_id,
            artist_name,
            artist_image_url,
        } => {
            if state.pending_artist_page_id.as_deref() != Some(artist_id.as_str()) {
                return;
            }
            if !state
                .artist_page_data
                .as_ref()
                .is_some_and(|data| data.artist_id == artist_id)
            {
                state.artist_page_data = Some(crate::models::ArtistPageData {
                    artist_id,
                    artist_name,
                    image_url: artist_image_url.clone(),
                    albums: Vec::new(),
                });
            } else if let Some(data) = state.artist_page_data.as_mut()
                && data.image_url.is_none()
            {
                data.image_url = artist_image_url.clone();
            }
            state.active_view = app::ActiveView::ArtistPage;
            state.artist_page_album_index = 0;
            state.artist_page_loading = true;
            state.artist_albums_loading = true;
            if let Some(url) = artist_image_url.as_ref() {
                image_tasks::spawn_header_for_url(
                    url,
                    state.image_picker.as_ref(),
                    worker_tx.clone(),
                    state.library_config.cover_img_pixels,
                );
            }
        }
        WorkerEvent::ArtistImageResolved {
            artist_id,
            image_url,
        } => {
            if let Some(data) = state.artist_page_data.as_mut()
                && data.artist_id == artist_id
                && data.image_url.is_none()
            {
                data.image_url = Some(image_url.clone());
                image_tasks::spawn_header_for_url(
                    &image_url,
                    state.image_picker.as_ref(),
                    worker_tx.clone(),
                    state.library_config.cover_img_pixels,
                );
            }
        }
        WorkerEvent::ArtistAlbumsLoaded { artist_id, albums } => {
            if let Some(data) = state.artist_page_data.as_mut()
                && data.artist_id == artist_id
            {
                let selected_album_index = if !albums.is_empty() {
                    state
                        .artist_page_album_index
                        .min(albums.len().saturating_sub(1))
                } else {
                    0
                };
                data.albums = albums;
                state.artist_page_album_index = selected_album_index;
                state.artist_albums_loading = false;
                state.artist_page_loading = false;
            }
        }
        WorkerEvent::ArtistAlbumsLoadFailed { artist_id, message } => {
            if state
                .artist_page_data
                .as_ref()
                .is_some_and(|data| data.artist_id == artist_id)
            {
                state.artist_albums_loading = false;
                state.artist_page_loading = false;
                let status = if message == "refresh already in progress" {
                    "Artist albums refresh already in progress.".to_string()
                } else {
                    format!("Artist albums failed: {message}")
                };
                set_timed_status(state, status, 5);
            }
        }
        WorkerEvent::ArtistAlbumsRateLimited {
            artist_id,
            retry_after_secs,
        } => {
            if let Some(data) = state.artist_page_data.as_ref()
                && data.artist_id == artist_id
            {
                let has_cached_albums = !data.albums.is_empty();
                state.artist_albums_loading = false;
                state.artist_page_loading = false;
                let message = if has_cached_albums {
                    format!(
                        "Artist albums rate limited. Showing cached albums. Try again in {retry_after_secs}s."
                    )
                } else {
                    format!("Artist albums rate limited. Try again in {retry_after_secs}s.")
                };
                set_timed_status(state, message, 5);
            }
        }
    }
}

async fn apply_synced_playback_item(
    item: crate::models::PlaybackItem,
    state: &mut AppState,
    app_tx: &mpsc::Sender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
) {
    let track_changed = state.playback.playing_track_id.as_deref() != Some(item.id.as_str());

    state.playback.playing_track_id = Some(item.id.clone());
    state.playback.playing_track_title = item.title.clone();
    state.playback.playing_track_artist = item.artist.clone();
    state.playback.playing_track_album_id = item.album_id.clone();
    state.playback.playing_track_artist_id = item.artist_id.clone();
    state.playback.duration_ms = item.duration_ms;

    if track_changed {
        state.playback.previous_track_image = state.playback.playing_track_image.take();

        if state.current_lyric_track_id.as_deref() != Some(item.id.as_str()) {
            state.current_lyric_track_id = Some(item.id.clone());
            state.is_fetching_lyrics = true;
            state.current_lyrics = None;
            let _ = app_tx
                .send(AppEvent::FetchLyrics(
                    item.id.clone(),
                    item.title.clone(),
                    item.artist.clone(),
                    item.duration_ms,
                ))
                .await;
        }
    }

    if let Some(url) = item.image_url {
        if let Some(ref picker) = state.image_picker {
            let should_process_image = track_changed
                || (state.playback.playing_track_image.is_none()
                    && state.playback.fetching_track_id.as_deref() != Some(item.id.as_str()));

            if should_process_image {
                state.playback.fetching_track_id = Some(item.id.clone());
                image_tasks::spawn_track_image_processing(
                    item.id.clone(),
                    url,
                    picker,
                    worker_tx.clone(),
                    state.library_config.cover_img_pixels,
                );
            }
        }
    } else if track_changed || state.playback.playing_track_artist.is_empty() {
        let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id)).await;
    }
}

fn set_timed_status(state: &mut AppState, message: String, seconds: u64) {
    state.status_message = Some(message);
    state.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(seconds));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn force_context_refresh_reuses_stored_context() {
        let (app_tx, mut app_rx) = mpsc::channel(1);
        let (worker_tx, _) = mpsc::channel(1);
        let mut tui = Tui::new().unwrap();
        let mut state = AppState::new();
        let context = crate::models::TrackListContext::playlist(
            "playlist".to_string(),
            "Playlist".to_string(),
            "Owner".to_string(),
            "owner".to_string(),
            None,
        );
        state.active_view = app::ActiveView::TrackList;
        state.active_tracklist_context = Some(context.clone());

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

    #[tokio::test]
    async fn stale_artist_page_result_is_ignored() {
        let (app_tx, _) = mpsc::channel(1);
        let (worker_tx, _) = mpsc::channel(1);
        let mut tui = Tui::new().unwrap();
        let mut state = AppState::new();
        state.begin_artist_page_load("current".to_string(), "Current".to_string(), None);

        apply_worker_event(
            WorkerEvent::ArtistAlbumsLoaded {
                artist_id: "stale".to_string(),
                albums: vec![crate::models::Album {
                    id: "album".to_string(),
                    name: "Album".to_string(),
                    artists: "Artist".to_string(),
                    image_url: None,
                    release_year: "2024".to_string(),
                    track_count: None,
                }],
            },
            &mut state,
            &app_tx,
            &worker_tx,
            &mut tui,
        )
        .await;

        assert_eq!(state.pending_artist_page_id.as_deref(), Some("current"));
        assert_eq!(
            state
                .artist_page_data
                .as_ref()
                .map(|data| data.artist_name.as_str()),
            Some("Current")
        );
        assert_eq!(
            state
                .artist_page_data
                .as_ref()
                .map(|data| data.albums.len()),
            Some(0)
        );
    }

    #[tokio::test]
    async fn artist_album_rate_limit_leaves_page_open() {
        let (app_tx, _) = mpsc::channel(1);
        let (worker_tx, _) = mpsc::channel(1);
        let mut tui = Tui::new().unwrap();
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string(), None);

        apply_worker_event(
            WorkerEvent::ArtistAlbumsRateLimited {
                artist_id: "artist".to_string(),
                retry_after_secs: 49,
            },
            &mut state,
            &app_tx,
            &worker_tx,
            &mut tui,
        )
        .await;

        assert!(matches!(state.active_view, app::ActiveView::ArtistPage));
        assert!(!state.artist_albums_loading);
        assert!(!state.artist_page_loading);

        apply_worker_event(
            WorkerEvent::ArtistAlbumsLoaded {
                artist_id: "artist".to_string(),
                albums: vec![crate::models::Album {
                    id: "album".to_string(),
                    name: "Album".to_string(),
                    artists: "Artist".to_string(),
                    image_url: None,
                    release_year: "2024".to_string(),
                    track_count: None,
                }],
            },
            &mut state,
            &app_tx,
            &worker_tx,
            &mut tui,
        )
        .await;

        assert_eq!(
            state
                .artist_page_data
                .as_ref()
                .map(|data| data.albums.len()),
            Some(1)
        );
    }
}
