pub mod normal;
pub mod setup;

use crate::app::{AppMode, AppState};
use crate::events::AppEvent;

pub fn handle_event(state: &mut AppState, event: &AppEvent) -> Option<AppEvent> {
    match event {
        AppEvent::Quit => {
            state.is_running = false;
            None
        }
        AppEvent::Key(key_event) => {
            match state.mode {
                AppMode::Setup => setup::handle_key(state, key_event),
                AppMode::Normal => normal::handle_key(state, key_event),
                _ => None
            }
        }
        _ => None
    }
}
