use crate::app::{ActiveView, AppMode, AppState};
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Esc => {
            state.ui.mode = AppMode::Normal;
            state.ui.search_query.clear();
            state.ui.search_matches.clear();
        }
        KeyCode::Backspace => {
            state.ui.search_query.pop();
            update_search_matches(state);
        }
        KeyCode::Char(c) => {
            state.ui.search_query.push(c);
            update_search_matches(state);
        }
        KeyCode::Enter => {
            state.ui.mode = AppMode::Normal;
            if !state.ui.search_matches.is_empty() {
                state.ui.selected_track_index = state.ui.search_matches[0];
            }
        }
        _ => {}
    }
    None
}

fn update_search_matches(state: &mut AppState) {
    state.ui.search_matches.clear();
    if state.ui.search_query.is_empty() {
        return;
    }

    let query = state.ui.search_query.to_lowercase();

    // We only search tracks if we are in TrackList view
    if state.ui.active_view == ActiveView::TrackList {
        for (i, track) in state.data.tracks.iter().enumerate() {
            if track.name.to_lowercase().contains(&query)
                || track.artist.to_lowercase().contains(&query)
            {
                state.ui.search_matches.push(i);
            }
        }

        // Implement incsearch logic: jump cursor to the first match immediately
        if !state.ui.search_matches.is_empty() {
            state.ui.selected_track_index = state.ui.search_matches[0];
        }
    }
}
