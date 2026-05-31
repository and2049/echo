use crossterm::event::{KeyCode, KeyEvent};
use crate::app::{AppState, ActiveView};
use crate::events::AppEvent;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            state.is_running = false;
        }
        KeyCode::Char('j') => {
            match state.active_view {
                ActiveView::Library => {
                    if state.selected_playlist_index < state.playlists.len().saturating_sub(1) {
                        state.selected_playlist_index += 1;
                    }
                }
                ActiveView::TrackList => {
                    if state.selected_track_index < state.tracks.len().saturating_sub(1) {
                        state.selected_track_index += 1;
                    }
                }
            }
        }
        KeyCode::Char('k') => {
            match state.active_view {
                ActiveView::Library => {
                    if state.selected_playlist_index > 0 {
                        state.selected_playlist_index -= 1;
                    }
                }
                ActiveView::TrackList => {
                    if state.selected_track_index > 0 {
                        state.selected_track_index -= 1;
                    }
                }
            }
        }
        KeyCode::Enter | KeyCode::Char('l') => {
            if state.active_view == ActiveView::Library {
                if state.selected_playlist_index < state.playlists.len() {
                    let playlist_id = state.playlists[state.selected_playlist_index].id.clone();
                    
                    // Instantly swap view to provide UI feedback while the network request runs in background
                    state.active_view = ActiveView::TrackList;
                    state.tracks.clear();
                    state.selected_track_index = 0;
                    
                    return Some(AppEvent::LoadPlaylistTracks(playlist_id));
                }
            }
        }
        KeyCode::Backspace | KeyCode::Char('h') => {
            if state.active_view == ActiveView::TrackList {
                state.active_view = ActiveView::Library;
            }
        }
        _ => {}
    }
    None
}
