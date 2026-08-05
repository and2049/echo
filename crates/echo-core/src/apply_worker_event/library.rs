use crate::{
    app::AppState,
    models::{LocalLibrary, LocalScanReport},
};

use super::misc::set_timed_status;

pub fn handle_playlists_loaded(state: &mut AppState, playlists: Vec<crate::models::Playlist>) {
    state.data.playlists = playlists;
    state.compute_library_view();
}

pub fn handle_albums_loaded(state: &mut AppState, albums: Vec<crate::models::Album>) {
    state.data.saved_albums = albums;
}

pub fn handle_local_library_loaded(
    state: &mut AppState,
    library: LocalLibrary,
    report: LocalScanReport,
) {
    state.data.local_library = library;
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
        .data.active_tracklist_context
        .as_ref()
        .is_some_and(|context| context.kind == crate::models::TrackListContextKind::LocalLibrary)
    {
        state.show_local_library();
    }
}

pub fn handle_local_playlists_loaded(
    state: &mut AppState,
    playlists: crate::models::LocalPlaylists,
) {
    state.data.local_playlists = playlists;
    state.compute_library_view();
    if let Some(context) = state.data.active_tracklist_context.clone()
        && context.kind == crate::models::TrackListContextKind::LocalPlaylist
    {
        state.show_local_playlist(&context.id, context.title);
    }
}

pub fn handle_liked_status_update(
    state: &mut AppState,
    results: std::collections::HashMap<String, bool>,
) {
    for (id, liked) in results {
        if liked {
            state.data.liked_tracks.insert(id);
        } else {
            state.data.liked_tracks.remove(&id);
        }
    }
}
