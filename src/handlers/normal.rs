use crossterm::event::{KeyCode, KeyEvent};
use crate::app::{AppState, ActiveView};
use crate::events::AppEvent;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Char('q') => {
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
            } else if state.active_view == ActiveView::TrackList {
                if state.selected_track_index < state.tracks.len() {
                    let track = &state.tracks[state.selected_track_index];
                    let playlist_id = state.playlists[state.selected_playlist_index].id.clone();
                    return Some(AppEvent::PlayTrack { 
                        playlist_id,
                        track_id: track.id.clone(), 
                        duration_ms: track.duration_ms 
                    });
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Backspace => {
            if state.active_view == ActiveView::TrackList {
                state.active_view = ActiveView::Library;
            }
        }
        KeyCode::Char('p') | KeyCode::Char(' ') => {
            state.playback.is_playing = !state.playback.is_playing;
            return Some(AppEvent::TogglePlayback(state.playback.is_playing));
        }
        KeyCode::Char('s') => {
            state.playback.is_shuffled = !state.playback.is_shuffled;
            return Some(AppEvent::ToggleShuffle(state.playback.is_shuffled));
        }
        KeyCode::Char(']') | KeyCode::Char('>') => {
            state.playback.progress_ms = 0;
            return Some(AppEvent::NextTrack);
        }
        KeyCode::Char('[') | KeyCode::Char('<') => {
            state.playback.progress_ms = 0;
            return Some(AppEvent::PreviousTrack);
        }
        _ => {}
    }
    None
}
