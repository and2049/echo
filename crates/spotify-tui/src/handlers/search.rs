use crossterm::event::{KeyCode, KeyEvent};
use echo_core::app::{AppMode, AppState};
use echo_core::events::AppEvent;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Esc => {
            state.ui.mode = AppMode::Normal;
            state.ui.search_query.clear();
            state.ui.search_matches.clear();
        }
        KeyCode::Backspace => {
            state.ui.search_query.pop();
            echo_core::intent::update_search_matches(state);
        }
        KeyCode::Char(c) => {
            state.ui.search_query.push(c);
            echo_core::intent::update_search_matches(state);
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
