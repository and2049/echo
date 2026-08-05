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
use crossterm::event::KeyEvent;

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
