use tokio::sync::mpsc;

use crate::{
    app::AppState,
    events::{AppEvent, WorkerEvent},
    image_tasks,
    models::PlaybackItem,
};

pub fn handle_tick(state: &mut AppState, app_tx: &mpsc::Sender<AppEvent>) {
    if state.playback.is_playing {
        state.playback.progress_ms += 100;
        state.playback.playback_last_updated_at = Some(std::time::Instant::now());
        if state.playback.duration_ms > 0
            && state.playback.progress_ms >= state.playback.duration_ms
        {
            state.playback.is_playing = false;
            let _ = app_tx.try_send(AppEvent::ForcePlaybackSync);
        }
    }
}

pub async fn handle_playback_started(
    state: &mut AppState,
    app_tx: &mpsc::Sender<AppEvent>,
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
    state.playback.previous_track_image = state.playback.playing_track_image.take();
    state.playback.duration_ms = item.duration_ms;
    state.playback.progress_ms = 0;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());

    if state.playback.current_lyric_track_id.as_deref() != Some(item.id.as_str()) {
        state.playback.current_lyric_track_id = Some(item.id.clone());
        state.playback.is_fetching_lyrics = true;
        state.playback.current_lyrics = None;
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
        if let Some(ref picker) = state.ui.image_picker {
            image_tasks::spawn_track_image_processing(
                item.id,
                url,
                picker,
                worker_tx.clone(),
                state.ui.library_config.cover_img_pixels,
            );
        }
    } else {
        let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id)).await;
    }
}

pub async fn handle_sync_playback_state(
    state: &mut AppState,
    app_tx: &mpsc::Sender<AppEvent>,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    is_playing: bool,
    is_shuffled: bool,
    repeat_mode: String,
    volume: Option<u32>,
    device_name: String,
    progress_ms: u32,
    item: Option<PlaybackItem>,
) {
    state.playback.is_playing = is_playing;
    state.playback.is_shuffled = is_shuffled;
    state.playback.repeat_mode = repeat_mode;
    if let Some(volume) = volume {
        state.playback.volume = volume;
    }
    state.playback.device_name = device_name;
    state.playback.progress_ms = progress_ms;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());

    if let Some(item) = item {
        apply_synced_playback_item(item, state, app_tx, worker_tx).await;
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

    if let Some(url) = image_url
        && let Some(ref picker) = state.ui.image_picker
    {
        image_tasks::spawn_track_image_processing(
            track_id,
            url,
            picker,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
}

pub fn handle_track_image_processed(
    state: &mut AppState,
    track_id: String,
    protocol: ratatui_image::protocol::StatefulProtocol,
) {
    if state.playback.playing_track_id.as_deref() == Some(track_id.as_str()) {
        state.playback.playing_track_image = Some(protocol);
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

async fn apply_synced_playback_item(
    item: PlaybackItem,
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
    state.playback.playing_track_source = Some(item.source);
    state.playback.duration_ms = item.duration_ms;

    if track_changed {
        state.playback.previous_track_image = state.playback.playing_track_image.take();

        if state.playback.current_lyric_track_id.as_deref() != Some(item.id.as_str()) {
            state.playback.current_lyric_track_id = Some(item.id.clone());
            state.playback.is_fetching_lyrics = true;
            state.playback.current_lyrics = None;
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
        if let Some(ref picker) = state.ui.image_picker {
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
                    state.ui.library_config.cover_img_pixels,
                );
            }
        }
    } else if track_changed || state.playback.playing_track_artist.is_empty() {
        let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id)).await;
    }
}
