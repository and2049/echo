use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::WorkerEvent,
    i18n, image_tasks,
    models::{SearchResults, Track, TrackListContext},
};

use super::misc::set_timed_status;

pub fn handle_home_feed_finished(
    state: &mut AppState,
    feed: crate::home::HomeFeed,
    range: Option<crate::models::TopItemsRange>,
    success: bool,
) {
    let fetch = state.data.home_fetches.entry(feed).or_default();
    fetch.in_flight = false;
    if success {
        fetch.fetched_at = Some(std::time::Instant::now());
        fetch.range = range;
    }
}

pub fn handle_context_details_loaded(
    state: &mut AppState,
    context_id: &str,
    details: crate::context_details::ContextDetails,
) {
    if state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|context| context.id == context_id)
    {
        state.data.active_context_details = Some(details);
    }
}

pub fn handle_tracks_loaded(
    state: &mut AppState,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    tracks: Vec<Track>,
    context: TrackListContext,
) {
    if state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|active| active.id != context.id || active.kind != context.kind)
    {
        return;
    }
    let preserve_track_selection = state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|active| active.id == context.id && active.kind == context.kind);
    let selected_track_index = if preserve_track_selection && !tracks.is_empty() {
        state
            .data
            .tracks
            .get(state.ui.selected_track_index)
            .and_then(|selected| tracks.iter().position(|track| track.id == selected.id))
            .unwrap_or_else(|| {
                state
                    .ui
                    .selected_track_index
                    .min(tracks.len().saturating_sub(1))
            })
    } else {
        0
    };
    state.data.original_tracks = tracks.clone();
    state.data.tracks = tracks;
    if !preserve_track_selection {
        state.data.active_context_details = None;
        state.ui.track_sort = crate::app::TrackSort::Original;
        state.ui.track_sort_ascending = true;
    }
    state.data.tracklist_image_url = context.image_url.clone();
    if let Some(url) = context.image_url.as_ref() {
        image_tasks::spawn_header_for_url(
            url,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
    state.data.active_tracklist_context = Some(context);
    if !preserve_track_selection {
        state.ui.active_view = app::ActiveView::TrackList;
    }
    state.ui.selected_track_index = selected_track_index;
    state.sort_tracks(state.ui.track_sort);
}

/// Refreshes Liked Songs in place when it is the open list; otherwise the stored copy is
/// what the next open shows.
pub fn handle_liked_songs_updated(
    state: &mut AppState,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    tracks: Vec<Track>,
    total: Option<u32>,
) {
    let Some(context) = state
        .data
        .active_tracklist_context
        .clone()
        .filter(|context| context.id == "LIKED_SONGS")
    else {
        return;
    };
    let details = crate::context_details::liked_songs(&context, &tracks, total);
    handle_tracks_loaded(state, worker_tx, tracks, context);
    state.data.active_context_details = Some(details);
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
    reconcile_manual_queue(&mut state.data.manual_queue, &tracks);
    if state.ui.queue_tab == crate::app::QueueTab::Queue {
        state.ui.selected_queue_index = state
            .ui
            .selected_queue_index
            .min(tracks.len().saturating_sub(1));
    }
    state.data.queue = tracks;
}

pub fn handle_devices_loaded(state: &mut AppState, devices: Vec<crate::models::Device>) {
    state.data.devices = devices;
    if state.ui.selected_device_index >= state.data.devices.len() {
        state.ui.selected_device_index = state.data.devices.len().saturating_sub(1);
    }
}

/// Drops the leading ids that no longer head the fetched queue, so what survives is exactly the
/// manually queued tracks still upcoming. `zip` tolerates the Web API's 20-item cap.
pub fn reconcile_manual_queue(manual: &mut Vec<String>, queue: &[Track]) {
    let keep_from = (0..=manual.len())
        .find(|&skip| {
            manual[skip..]
                .iter()
                .zip(queue)
                .all(|(id, track)| *id == track.id)
        })
        .unwrap_or(manual.len());
    manual.drain(..keep_from);
}

pub fn handle_tracks_queued(state: &mut AppState, track_ids: Vec<String>) {
    let count = track_ids.len();
    state.data.manual_queue.extend(track_ids);
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
    crate::history::rebuild_recent(state);
    if take_pending(state, crate::models::BrowseNode::RecentlyPlayed) {
        let _ = crate::intent::open_recently_played(state);
    } else {
        refresh_open_generated_list(
            state,
            "RECENTLY_PLAYED",
            &state.data.recently_played.clone(),
        );
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
    state.ui.selected_track_index = state
        .ui
        .selected_track_index
        .min(tracks.len().saturating_sub(1));
    state.sort_tracks(state.ui.track_sort);
}

#[cfg(test)]
mod tests {
    #[test]
    fn home_requests_finish_without_marking_failures_fresh() {
        use crate::home::{HomeFeed, HomeFetch};
        let mut state = AppState::new();
        state.data.home_fetches.insert(
            HomeFeed::TopTracks,
            HomeFetch {
                in_flight: true,
                ..Default::default()
            },
        );
        handle_home_feed_finished(&mut state, HomeFeed::TopTracks, None, false);
        let fetch = &state.data.home_fetches[&HomeFeed::TopTracks];
        assert!(!fetch.in_flight);
        assert!(fetch.fetched_at.is_none());
        handle_home_feed_finished(
            &mut state,
            HomeFeed::TopTracks,
            Some(crate::models::TopItemsRange::Short),
            true,
        );
        let at = state.data.home_fetches[&HomeFeed::TopTracks].fetched_at;
        handle_home_feed_finished(&mut state, HomeFeed::TopTracks, None, false);
        let fetch = &state.data.home_fetches[&HomeFeed::TopTracks];
        assert_eq!(fetch.fetched_at, at);
        assert_eq!(fetch.range, Some(crate::models::TopItemsRange::Short));
    }

    #[test]
    fn details_ignore_stale_context_and_survive_history() {
        use crate::context_details::ContextDetails;
        let mut state = AppState::new();
        let context = TrackListContext::album("a".into(), "A".into(), "Artist".into(), None);
        state.begin_tracklist_load(context);
        let details = ContextDetails {
            track_count: Some(12),
            ..Default::default()
        };
        handle_context_details_loaded(&mut state, "old", details.clone());
        assert!(state.data.active_context_details.is_none());
        handle_context_details_loaded(&mut state, "a", details.clone());
        assert_eq!(state.data.active_context_details, Some(details.clone()));
        state.begin_tracklist_load(TrackListContext::album(
            "b".into(),
            "B".into(),
            "Artist".into(),
            None,
        ));
        assert!(state.data.active_context_details.is_none());
        state.pop_view_history();
        assert_eq!(state.data.active_context_details, Some(details));
    }

    #[test]
    fn track_refresh_preserves_sort_and_ignores_an_old_page() {
        use crate::app::TrackSort;
        let mut state = AppState::new();
        let context = TrackListContext::album("a".into(), "A".into(), "Artist".into(), None);
        state.begin_tracklist_load(context.clone());
        state.data.tracks = vec![sample_track("a"), sample_track("b")];
        state.data.original_tracks = state.data.tracks.clone();
        crate::intent::sort_by_column(&mut state, TrackSort::Title);
        crate::intent::sort_by_column(&mut state, TrackSort::Title);
        let (tx, _) = mpsc::channel(1);
        handle_tracks_loaded(
            &mut state,
            &tx,
            vec![sample_track("a"), sample_track("b")],
            context,
        );
        assert_eq!(state.data.tracks[0].id, "b");
        assert_eq!(state.data.tracks[state.ui.selected_track_index].id, "a");
        handle_tracks_loaded(
            &mut state,
            &tx,
            vec![],
            TrackListContext::album("old".into(), "Old".into(), "Artist".into(), None),
        );
        assert_eq!(state.data.tracks.len(), 2);
    }

    #[test]
    fn liked_songs_updates_refresh_only_the_open_liked_list() {
        let (tx, _) = mpsc::channel(1);
        let mut state = AppState::new();
        let album = TrackListContext::album("a".into(), "A".into(), "Artist".into(), None);
        state.begin_tracklist_load(album);
        handle_liked_songs_updated(&mut state, &tx, vec![sample_track("x")], Some(1));
        assert!(state.data.tracks.is_empty());

        let liked = TrackListContext::playlist(
            "LIKED_SONGS".into(),
            "Liked Songs".into(),
            String::new(),
            "spotify".into(),
            None,
        );
        state.begin_tracklist_load(liked);
        handle_liked_songs_updated(
            &mut state,
            &tx,
            vec![sample_track("x"), sample_track("y")],
            Some(3),
        );
        state.ui.selected_track_index = 1;
        handle_liked_songs_updated(
            &mut state,
            &tx,
            vec![sample_track("new"), sample_track("x"), sample_track("y")],
            Some(4),
        );
        assert_eq!(state.data.tracks.len(), 3);
        assert_eq!(state.data.tracks[state.ui.selected_track_index].id, "y");
        let details = state.data.active_context_details.as_ref().unwrap();
        assert_eq!(details.track_count, Some(4));
        assert_eq!(details.duration_ms, 3000);
    }

    use super::*;
    use crate::models::{BrowseNode, TrackSource};

    fn sample_track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
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

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn manual_queue_keeps_the_ids_still_heading_the_fetched_queue() {
        let mut manual = ids(&["a", "b", "c"]);
        reconcile_manual_queue(
            &mut manual,
            &[sample_track("b"), sample_track("c"), sample_track("ctx")],
        );
        assert_eq!(manual, ids(&["b", "c"]));
    }

    #[test]
    fn manual_queue_clears_when_nothing_heads_the_fetched_queue() {
        let mut manual = ids(&["a", "b"]);
        reconcile_manual_queue(&mut manual, &[sample_track("ctx")]);
        assert!(manual.is_empty());
    }

    #[test]
    fn manual_queue_survives_the_fetch_cap() {
        let all: Vec<String> = (0..25).map(|i| i.to_string()).collect();
        let mut manual = all.clone();
        let fetched: Vec<Track> = all.iter().take(20).map(|id| sample_track(id)).collect();
        reconcile_manual_queue(&mut manual, &fetched);
        assert_eq!(manual, all);
    }

    #[test]
    fn queue_load_reconciles_what_tracks_queued_recorded() {
        crate::i18n::init();
        let mut state = AppState::new();
        handle_tracks_queued(&mut state, ids(&["a", "b"]));
        assert_eq!(state.ui.recent_queue_count, 2);
        handle_queue_loaded(&mut state, vec![sample_track("b"), sample_track("ctx")]);
        assert_eq!(state.data.manual_queue, ids(&["b"]));
        assert_eq!(state.data.queue.len(), 2);
    }

    #[test]
    fn queue_reload_keeps_the_cursor_in_range_instead_of_resetting_it() {
        let mut state = AppState::new();
        state.ui.selected_queue_index = 5;
        handle_queue_loaded(&mut state, vec![sample_track("a"), sample_track("b")]);
        assert_eq!(state.ui.selected_queue_index, 1);
        handle_queue_loaded(
            &mut state,
            vec![sample_track("a"), sample_track("b"), sample_track("c")],
        );
        assert_eq!(state.ui.selected_queue_index, 1);
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

        handle_top_tracks_loaded(
            &mut state,
            vec![sample_track("new-a"), sample_track("new-b")],
        );

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
