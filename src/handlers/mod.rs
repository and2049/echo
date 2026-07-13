pub mod artist_page;
pub mod browse;
pub mod command;
pub mod normal;
pub mod navigation;
pub mod keymap;
pub mod search;
pub mod setup;
pub mod tracklist;
pub mod visual;

use crate::app::{AppMode, AppState};
use crate::events::AppEvent;

pub fn handle_event(state: &mut AppState, event: &AppEvent) -> Option<AppEvent> {
    match event {
        AppEvent::Quit => {
            state.ui.is_running = false;
            None
        }
        AppEvent::Key(key_event) => match state.ui.mode {
            AppMode::Setup => setup::handle_key(state, key_event),
            AppMode::Normal => normal::handle_key(state, key_event),
            AppMode::Command => command::handle_key(state, key_event),
            AppMode::Search => search::handle_key(state, key_event),
            AppMode::Visual => visual::handle_key(state, key_event),
            _ => None,
        },
        _ => None,
    }
}
