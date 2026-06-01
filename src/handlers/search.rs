use crate::app::{AppMode, AppState, ActiveView};
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Esc => {
            state.mode = AppMode::Normal;
            state.search_query.clear();
            state.search_matches.clear();
        }
        KeyCode::Backspace => {
            state.search_query.pop();
            update_search_matches(state);
        }
        KeyCode::Char(c) => {
            state.search_query.push(c);
            update_search_matches(state);
        }
        KeyCode::Enter => {
            state.mode = AppMode::Normal;
            if !state.search_matches.is_empty() {
                state.selected_track_index = state.search_matches[0];
            }
        }
        _ => {}
    }
    None
}

fn update_search_matches(state: &mut AppState) {
    state.search_matches.clear();
    if state.search_query.is_empty() {
        return;
    }

    let query = state.search_query.to_lowercase();
    
    // We only search tracks if we are in TrackList view
    if state.active_view == ActiveView::TrackList {
        for (i, track) in state.tracks.iter().enumerate() {
            if track.name.to_lowercase().contains(&query) 
                || track.artist.to_lowercase().contains(&query) {
                state.search_matches.push(i);
            }
        }

        // Implement incsearch logic: jump cursor to the first match immediately
        if !state.search_matches.is_empty() {
            state.selected_track_index = state.search_matches[0];
        }
    }
}
