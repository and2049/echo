use crate::app::AppState;
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle(state: &mut AppState, key: &KeyEvent) -> (bool, Option<AppEvent>) {
    if let Some(folder_name) = state.ui.folder_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state
                .ui
                .library_config
                .folders
                .retain(|fd| fd.name != folder_name);
            state.save_library_config();
            state.compute_library_view();

            if state.ui.selected_playlist_index >= state.data.library_view.len() {
                state.ui.selected_playlist_index = state.data.library_view.len().saturating_sub(1);
            }
        }
        state.ui.folder_delete_prompt = None;
        state.ui.playlist_delete_prompt = None;
        return (true, None);
    }

    if let Some(playlist_ids) = state.ui.playlist_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.ui.playlist_delete_prompt = None;
            return (true, Some(AppEvent::DeletePlaylists(playlist_ids)));
        }
        state.ui.playlist_delete_prompt = None;
        return (true, None);
    }

    if let Some(album_ids) = state.ui.album_mass_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.ui.album_mass_delete_prompt = None;
            return (true, Some(AppEvent::RemoveAlbums(album_ids)));
        }
        state.ui.album_mass_delete_prompt = None;
        return (true, None);
    }

    if let Some((playlist_id, track_ids)) = state.ui.track_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.ui.track_delete_prompt = None;
            return (
                true,
                Some(AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids)),
            );
        }
        state.ui.track_delete_prompt = None;
        return (true, None);
    }

    if let Some(track_id) = state.ui.liked_track_remove_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.ui.liked_track_remove_prompt = None;
            state.data.liked_tracks.remove(&track_id);

            let mut cache = crate::config::AppConfig::load_cache();
            cache.liked_tracks = state.data.liked_tracks.clone();
            let _ = crate::config::AppConfig::save_cache(&cache);

            return (true, Some(AppEvent::ToggleTrackLike(track_id, false)));
        }
        state.ui.liked_track_remove_prompt = None;
        return (true, None);
    }

    (false, None)
}
