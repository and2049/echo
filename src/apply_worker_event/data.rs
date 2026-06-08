use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::WorkerEvent,
    image_tasks,
    i18n,
    models::{SearchResults, Track, TrackListContext},
};

use super::misc::set_timed_status;

pub fn handle_tracks_loaded(
    state: &mut AppState,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    tracks: Vec<Track>,
    context: TrackListContext,
) {
    let preserve_track_selection = state
        .data.active_tracklist_context
        .as_ref()
        .is_some_and(|active| active.id == context.id && active.kind == context.kind);
    let selected_track_index = if preserve_track_selection && !tracks.is_empty() {
        state
            .ui.selected_track_index
            .min(tracks.len().saturating_sub(1))
    } else {
        0
    };
    state.data.tracks = tracks;
    state.data.tracklist_image_url = context.image_url.clone();
    if let Some(url) = context.image_url.as_ref() {
        image_tasks::spawn_header_for_url(
            url,
            state.ui.image_picker.as_ref(),
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
    state.data.active_tracklist_context = Some(context);
    state.ui.active_view = app::ActiveView::TrackList;
    state.ui.selected_track_index = selected_track_index;
}

pub fn handle_tracks_load_failed(state: &mut AppState, message: String) {
    set_timed_status(state, format!("Unable to load tracks: {message}"), 5);
}

pub fn handle_search_results_loaded(state: &mut AppState, results: SearchResults) {
    state.data.search_results = results;
    state.ui.selected_search_index = 0;
    state.ui.active_view = app::ActiveView::SearchResults;
    state.ui.status_message = Some(format!("Search: {}", state.ui.search_context_query));
}

pub fn handle_queue_loaded(state: &mut AppState, tracks: Vec<Track>) {
    state.data.queue = tracks;
    state.ui.selected_queue_index = 0;
}

pub fn handle_devices_loaded(state: &mut AppState, devices: Vec<crate::models::Device>) {
    state.data.devices = devices;
    if state.ui.selected_device_index >= state.data.devices.len() {
        state.ui.selected_device_index = state.data.devices.len().saturating_sub(1);
    }
}

pub fn handle_tracks_queued(state: &mut AppState, count: usize) {
    state.ui.recent_queue_count += count;
    set_timed_status(
        state,
        i18n::t("messages.added_to_queue", &state.ui.library_config.language)
            .replace("{}", &count.to_string()),
        3,
    );
}

pub fn handle_top_tracks_loaded(state: &mut AppState, tracks: Vec<Track>) {
    state.data.top_tracks = tracks;
}

pub fn handle_recently_played_loaded(state: &mut AppState, tracks: Vec<Track>) {
    state.data.recently_played = tracks;
}

pub fn handle_followed_artists_loaded(state: &mut AppState, artists: Vec<crate::models::Artist>) {
    state.data.followed_artists = artists;
}
