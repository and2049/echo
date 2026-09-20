use tokio::sync::mpsc;

use crate::{
    app::AppState,
    events::{AppEvent, WorkerEvent},
    image_tasks,
    models::PlaybackItem,
};

pub fn handle_tick(state: &mut AppState, app_tx: &mpsc::UnboundedSender<AppEvent>) {
    if state.playback.is_playing {
        state.playback.progress_ms += 100;
        state.playback.playback_last_updated_at = Some(std::time::Instant::now());
        if state.playback.duration_ms > 0
            && state.playback.progress_ms >= state.playback.duration_ms
        {
            state.playback.is_playing = false;
            let _ = app_tx.send(AppEvent::ForcePlaybackSync);
        }
    }
}

pub fn handle_playback_started(
    state: &mut AppState,
    app_tx: &mpsc::UnboundedSender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    item: PlaybackItem,
) {
    state.playback.is_playing = true;
    state.playback.playing_track_id = Some(item.id.clone());
    state.playback.playing_track_title = item.title.clone();
    state.playback.playing_track_artist = item.artist.clone();
    state.playback.playing_track_album_id = item.album_id.clone();
    state.playback.playing_track_artist_id = item.artist_id.clone();
    state.playback.playing_track_source = Some(item.source);
    state.playback.playing_track_local_path = item.local_path.clone();
    state.playback.playing_track_image_url = item.image_url.clone();
    state.playback.previous_track_image = state.playback.playing_track_image.take();
    state.playback.duration_ms = item.duration_ms;
    if !crate::session::complete_resume(state, app_tx, &item.id) {
        state.playback.progress_ms = 0;
    }
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());

    if state.playback.current_lyric_track_id.as_deref() != Some(item.id.as_str()) {
        state.playback.current_lyric_track_id = Some(item.id.clone());
        state.playback.is_fetching_lyrics = true;
        state.playback.current_lyrics = None;
        let _ = app_tx.send(AppEvent::FetchLyrics(
            item.id.clone(),
            item.title.clone(),
            item.artist.clone(),
            item.duration_ms,
        ));
    }

    if let Some(url) = item.image_url {
        image_tasks::spawn_track_image_processing(
            item.id,
            url,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    } else {
        let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn handle_sync_playback_state(
    state: &mut AppState,
    app_tx: &mpsc::UnboundedSender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    is_playing: bool,
    is_shuffled: bool,
    repeat_mode: String,
    _volume: Option<u32>,
    device_name: String,
    progress_ms: u32,
    item: Option<PlaybackItem>,
    context: Option<crate::models::PlayingContext>,
) {
    let keep_restored = item.is_none() && state.playback.pending_resume.is_some();
    state.playback.is_shuffled = is_shuffled;
    state.playback.repeat_mode = repeat_mode;
    state.playback.device_name = device_name;
    if keep_restored {
        return;
    }
    state.playback.is_playing = is_playing;
    state.playback.progress_ms = progress_ms;
    state.playback.playing_context = context;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());

    if let Some(item) = item {
        if state.playback.playing_track_id.as_deref() != Some(item.id.as_str()) {
            state.playback.pending_resume = None;
        }
        apply_synced_playback_item(item, state, app_tx, worker_tx);
    }
}

pub fn handle_playback_control_state(state: &mut AppState, is_playing: bool) {
    state.playback.is_playing = is_playing;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());
}

pub fn handle_track_metadata_loaded(
    state: &mut AppState,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    track_id: String,
    title: String,
    artist: String,
    image_url: Option<String>,
) {
    if state.playback.playing_track_id.as_deref() != Some(track_id.as_str()) {
        return;
    }

    state.playback.playing_track_title = title;
    state.playback.playing_track_artist = artist;
    state.playback.playing_track_image_url = image_url.clone();

    if let Some(url) = image_url {
        image_tasks::spawn_track_image_processing(
            track_id,
            url,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
}

pub fn handle_track_image_processed(
    state: &mut AppState,
    track_id: String,
    artwork: crate::artwork::SharedArtwork,
) {
    if state.playback.playing_track_id.as_deref() == Some(track_id.as_str()) {
        state.playback.playing_track_image = Some(artwork);
        state.playback.previous_track_image = None;
        if state.playback.fetching_track_id.as_deref() == Some(track_id.as_str()) {
            state.playback.fetching_track_id = None;
        }
    }
}

pub fn handle_lyrics_loaded(state: &mut AppState, lyrics: Option<crate::models::Lyrics>) {
    state.playback.current_lyrics = lyrics;
    state.playback.is_fetching_lyrics = false;
}

pub fn handle_audio_visualization_ready(
    state: &mut AppState,
    shared_bands: std::sync::Arc<parking_lot::Mutex<[f32; 32]>>,
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    flag.store(
        state.ui.library_config.enable_visualizer,
        std::sync::atomic::Ordering::Relaxed,
    );
    state.playback.audio_visualization = Some(shared_bands);
    state.playback.enable_visualizer = Some(flag);
}

fn apply_synced_playback_item(
    item: PlaybackItem,
    state: &mut AppState,
    app_tx: &mpsc::UnboundedSender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
) {
    let track_changed = state.playback.playing_track_id.as_deref() != Some(item.id.as_str());

    state.playback.playing_track_id = Some(item.id.clone());
    state.playback.playing_track_title = item.title.clone();
    state.playback.playing_track_artist = item.artist.clone();
    state.playback.playing_track_album_id = item.album_id.clone();
    state.playback.playing_track_artist_id = item.artist_id.clone();
    state.playback.playing_track_source = Some(item.source);
    state.playback.playing_track_local_path = item.local_path.clone();
    state.playback.playing_track_image_url = item.image_url.clone();
    state.playback.duration_ms = item.duration_ms;

    if track_changed {
        state.playback.previous_track_image = state.playback.playing_track_image.take();
        if state.queue_visible() {
            let _ = app_tx.send(AppEvent::FetchQueue);
        }

        if state.playback.current_lyric_track_id.as_deref() != Some(item.id.as_str()) {
            state.playback.current_lyric_track_id = Some(item.id.clone());
            state.playback.is_fetching_lyrics = true;
            state.playback.current_lyrics = None;
            let _ = app_tx.send(AppEvent::FetchLyrics(
                item.id.clone(),
                item.title.clone(),
                item.artist.clone(),
                item.duration_ms,
            ));
        }
    }

    if let Some(url) = item.image_url {
        let should_process_image = track_changed
            || (state.playback.playing_track_image.is_none()
                && state.playback.fetching_track_id.as_deref() != Some(item.id.as_str()));

        if should_process_image {
            state.playback.fetching_track_id = Some(item.id.clone());
            image_tasks::spawn_track_image_processing(
                item.id.clone(),
                url,
                worker_tx.clone(),
                state.ui.library_config.cover_img_pixels,
            );
        }
    } else if track_changed || state.playback.playing_track_artist.is_empty() {
        let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackSource;

    fn item(id: &str) -> PlaybackItem {
        PlaybackItem {
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            title: id.to_string(),
            artist: "artist".to_string(),
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
        }
    }

    #[test]
    fn a_track_change_refetches_the_queue_only_while_that_view_is_open() {
        let mut state = AppState::new();
        let (app_tx, mut app_rx) = mpsc::unbounded_channel();
        let (worker_tx, _worker_rx) = mpsc::channel(8);

        apply_synced_playback_item(item("a"), &mut state, &app_tx, &worker_tx);
        state.ui.active_view = crate::app::ActiveView::Queue;
        apply_synced_playback_item(item("a"), &mut state, &app_tx, &worker_tx);
        apply_synced_playback_item(item("b"), &mut state, &app_tx, &worker_tx);

        let fetches = std::iter::from_fn(|| app_rx.try_recv().ok())
            .filter(|event| matches!(event, AppEvent::FetchQueue))
            .count();
        assert_eq!(fetches, 1);
    }

    fn restored_state(worker_tx: &mpsc::Sender<WorkerEvent>) -> AppState {
        let mut state = AppState::new();
        crate::session::restore(
            &mut state,
            crate::session::PlaybackSession {
                item: item("a"),
                context: Some(crate::models::PlayingContext {
                    context_id: "pl".to_string(),
                    is_album: false,
                }),
                progress_ms: 500,
                is_shuffled: false,
                repeat_mode: "Off".to_string(),
                queue: Vec::new(),
            },
            worker_tx,
        );
        state
    }

    #[tokio::test]
    async fn an_empty_playback_report_leaves_the_restored_session_alone() {
        let (app_tx, _app_rx) = mpsc::unbounded_channel();
        let (worker_tx, _worker_rx) = mpsc::channel(8);
        let mut state = restored_state(&worker_tx);

        handle_sync_playback_state(
            &mut state,
            &app_tx,
            &worker_tx,
            false,
            true,
            "Context".to_string(),
            None,
            "echo-rs".to_string(),
            0,
            None,
            None,
        );

        assert_eq!(state.playback.progress_ms, 500);
        assert!(state.playback.playing_context.is_some());
        assert!(state.playback.pending_resume.is_some());
        assert!(state.playback.is_shuffled);
    }

    #[tokio::test]
    async fn a_track_reported_by_another_device_replaces_the_restored_session() {
        let (app_tx, _app_rx) = mpsc::unbounded_channel();
        let (worker_tx, _worker_rx) = mpsc::channel(8);
        let mut state = restored_state(&worker_tx);

        handle_sync_playback_state(
            &mut state,
            &app_tx,
            &worker_tx,
            true,
            false,
            "Off".to_string(),
            None,
            "phone".to_string(),
            7_000,
            Some(item("b")),
            None,
        );

        assert_eq!(state.playback.playing_track_id.as_deref(), Some("b"));
        assert_eq!(state.playback.progress_ms, 7_000);
        assert!(state.playback.pending_resume.is_none());
    }

    #[tokio::test]
    async fn the_resumed_start_keeps_its_position_while_any_other_start_resets_it() {
        let (app_tx, _app_rx) = mpsc::unbounded_channel();
        let (worker_tx, _worker_rx) = mpsc::channel(8);
        let mut state = restored_state(&worker_tx);
        handle_playback_started(&mut state, &app_tx, &worker_tx, item("a"));
        assert_eq!(state.playback.progress_ms, 500);
        assert!(state.playback.is_playing);

        let mut state = restored_state(&worker_tx);
        handle_playback_started(&mut state, &app_tx, &worker_tx, item("b"));
        assert_eq!(state.playback.progress_ms, 0);
        assert!(state.playback.pending_resume.is_none());
    }
}
