use crate::{
    app::AppState,
    models::{LocalLibrary, LocalScanReport},
};

use super::misc::set_timed_status;

pub fn handle_playlists_loaded(state: &mut AppState, playlists: Vec<crate::models::Playlist>) {
    state.data.playlists = playlists;
    state.compute_library_view();
}

pub fn handle_spotify_rootlist_loaded(
    state: &mut AppState,
    entries: Vec<crate::models::RootlistEntry>,
) {
    if state.data.spotify_rootlist == entries {
        return;
    }
    // Keep the cursor on the row it was on: folders arriving reshuffle the sidebar.
    let selected = state
        .data
        .library_view
        .get(state.ui.selected_playlist_index)
        .and_then(|node| match node {
            crate::models::LibraryNode::Playlist { playlist, .. } => Some(playlist.id.clone()),
            _ => None,
        });
    state.data.spotify_rootlist = entries;
    state.compute_library_view();
    if let Some(id) = selected
        && let Some(index) = state.data.library_view.iter().position(|node| {
            matches!(node, crate::models::LibraryNode::Playlist { playlist, .. } if playlist.id == id)
        })
    {
        state.ui.selected_playlist_index = index;
    }
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
        .data
        .active_tracklist_context
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
        let title = state
            .data
            .local_playlists
            .playlists
            .iter()
            .find(|p| p.id == context.id)
            .map(|p| p.name.clone())
            .unwrap_or(context.title);
        state.show_local_playlist(&context.id, title);
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
