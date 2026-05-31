pub mod normal;

use crate::app::{AppMode, AppState};
use crate::events::AppEvent;

pub fn handle_event(state: &mut AppState, event: &AppEvent) {
    match event {
        AppEvent::Quit => state.is_running = false,
        AppEvent::Key(key_event) => {
            match state.mode {
                AppMode::Normal => normal::handle_key(state, key_event),
                _ => {} // Visual and Command mode handlers will be added later
            }
        }
    }
}
