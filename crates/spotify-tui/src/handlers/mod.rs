pub mod browse;
pub mod command;
pub mod keymap;
pub mod navigation;
pub mod normal;
pub mod search;
pub mod setup;
pub mod tracklist;
pub mod visual;

use crossterm::event::KeyEvent;
use echo_core::app::{AppMode, AppState};
use echo_core::events::AppEvent;

pub fn handle_event(state: &mut AppState, key_event: &KeyEvent) -> Option<AppEvent> {
    match state.ui.mode {
        AppMode::Setup => setup::handle_key(state, key_event),
        AppMode::Normal => normal::handle_key(state, key_event),
        AppMode::Command => command::handle_key(state, key_event),
        AppMode::Search => search::handle_key(state, key_event),
        AppMode::Visual => visual::handle_key(state, key_event),
        _ => None,
    }
}
