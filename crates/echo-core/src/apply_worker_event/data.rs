use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::WorkerEvent,
    i18n, image_tasks,
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
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|active| active.id == context.id && active.kind == context.kind);
    let selected_track_index = if preserve_track_selection && !tracks.is_empty() {
        state
            .ui
            .selected_track_index
            .min(tracks.len().saturating_sub(1))
    } else {
        0
    };
    state.data.original_tracks = tracks.clone();
    state.data.tracks = tracks;
    state.ui.track_sort = crate::app::TrackSort::Original;
    state.data.tracklist_image_url = context.image_url.clone();
    if let Some(url) = context.image_url.as_ref() {
        image_tasks::spawn_header_for_url(
            url,
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
    if state.ui.active_view != app::ActiveView::SearchResults {
        state.push_view_history();
    }
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
    if take_pending(state, crate::models::BrowseNode::TopTracks) {
        // The user opened Top Tracks while it was empty: finish the navigation now.
        let _ = crate::intent::open_top_tracks(state);
    } else {
        refresh_open_generated_list(state, "TOP_TRACKS", &state.data.top_tracks.clone());
    }
}

pub fn handle_recently_played_loaded(state: &mut AppState, tracks: Vec<Track>) {
    state.data.recently_played = tracks;
    if take_pending(state, crate::models::BrowseNode::RecentlyPlayed) {
        let _ = crate::intent::open_recently_played(state);
    } else {
        refresh_open_generated_list(state, "RECENTLY_PLAYED", &state.data.recently_played.clone());
    }
}

pub fn handle_followed_artists_loaded(state: &mut AppState, artists: Vec<crate::models::Artist>) {
    state.data.followed_artists = artists;
}

pub fn handle_whats_new_loaded(
    state: &mut AppState,
    albums: Vec<crate::models::Album>,
    done: usize,
    total: usize,
) {
    // The What's New view renders live from state (like the artist list), so no
    // navigation; each event carries the full merged list so far.
    state.data.whats_new = albums;
    state.data.whats_new_progress = (done < total).then_some((done, total));
    if state.ui.active_view == app::ActiveView::WhatsNew {
        state.ui.selected_whats_new_index = state
            .ui
            .selected_whats_new_index
            .min(state.data.whats_new.len().saturating_sub(1));
    }
}

pub fn handle_top_artists_loaded(state: &mut AppState, artists: Vec<crate::models::Artist>) {
    // The artist-list view renders live from state, so no navigation is needed here;
    // clamping keeps the cursor valid when a range switch shrank the list.
    state.data.top_artists = artists;
    if state.ui.active_view == app::ActiveView::ArtistList {
        state.ui.selected_artist_index = state
            .ui
            .selected_artist_index
            .min(state.artist_list().len().saturating_sub(1));
    }
}

fn take_pending(state: &mut AppState, node: crate::models::BrowseNode) -> bool {
    if state.ui.pending_browse_open == Some(node) {
        state.ui.pending_browse_open = None;
        true
    } else {
        false
    }
}

/// A generated tracklist snapshots its rows at open time, so when its source list is
/// refetched (e.g. a time-range switch) while on screen, swap the rows in place —
/// without re-navigating or touching view history.
fn refresh_open_generated_list(state: &mut AppState, context_id: &str, tracks: &[Track]) {
    let is_open = state.ui.active_view == app::ActiveView::TrackList
        && state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.id == context_id);
    if !is_open {
        return;
    }
    state.data.original_tracks = tracks.to_vec();
    state.data.tracks = tracks.to_vec();
    state.ui.track_sort = crate::app::TrackSort::Original;
    state.ui.selected_track_index = state
        .ui
        .selected_track_index
        .min(tracks.len().saturating_sub(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BrowseNode, TrackSource};

    fn sample_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: String::new(),
            album: String::new(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    #[test]
    fn top_tracks_load_consumes_pending_and_navigates() {
        let mut state = AppState::new();
        // A cold open: the intent fired the fetch and left a pending marker.
        assert!(crate::intent::open_top_tracks(&mut state).is_some());
        assert_eq!(state.ui.pending_browse_open, Some(BrowseNode::TopTracks));

        handle_top_tracks_loaded(&mut state, vec![sample_track("t")]);

        assert_eq!(state.ui.pending_browse_open, None);
        assert_eq!(state.ui.active_view, app::ActiveView::TrackList);
        assert_eq!(state.data.tracks.len(), 1);
    }

    #[test]
    fn background_top_tracks_load_does_not_navigate() {
        let mut state = AppState::new();
        let view_before = state.ui.active_view;

        handle_top_tracks_loaded(&mut state, vec![sample_track("t")]);

        assert_eq!(state.ui.active_view, view_before);
        assert!(state.data.tracks.is_empty());
    }

    #[test]
    fn top_tracks_load_refreshes_an_open_generated_list_in_place() {
        let mut state = AppState::new();
        state.data.top_tracks = vec![sample_track("old")];
        crate::intent::open_top_tracks(&mut state);
        let history_depth = state.ui.view_history.len();

        handle_top_tracks_loaded(&mut state, vec![sample_track("new-a"), sample_track("new-b")]);

        assert_eq!(state.ui.active_view, app::ActiveView::TrackList);
        assert_eq!(state.data.tracks.len(), 2);
        assert_eq!(state.data.tracks[0].id, "new-a");
        assert_eq!(state.ui.view_history.len(), history_depth);
    }

    #[test]
    fn top_artists_load_fills_the_live_list_without_navigating() {
        let mut state = AppState::new();
        let view_before = state.ui.active_view;

        handle_top_artists_loaded(
            &mut state,
            vec![crate::models::Artist {
                id: "a".to_string(),
                name: "A".to_string(),
                followers: 0,
                image_url: None,
            }],
        );

        assert_eq!(state.data.top_artists.len(), 1);
        assert_eq!(state.ui.active_view, view_before);
    }

    #[test]
    fn failed_api_request_clears_a_pending_browse_open() {
        let mut state = AppState::new();
        assert!(crate::intent::open_top_tracks(&mut state).is_some());

        super::super::misc::handle_api_request_failed(
            &mut state,
            "Top tracks".to_string(),
            "boom".to_string(),
        );

        assert_eq!(state.ui.pending_browse_open, None);
    }
}
