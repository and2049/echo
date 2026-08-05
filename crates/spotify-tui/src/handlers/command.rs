//! Key handling for command mode. What the commands do lives in [`echo_core::commands`],
//! shared with the desktop app's command bar.

use echo_core::app::{AppMode, AppState};
use echo_core::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Tab => {
            echo_core::commands::cycle_suggestion(state, true);
            None
        }
        KeyCode::BackTab => {
            echo_core::commands::cycle_suggestion(state, false);
            None
        }
        KeyCode::Esc => {
            echo_core::commands::clear_suggestions(state);
            state.ui.mode = AppMode::Normal;
            state.ui.command_buffer.clear();
            state.ui.needs_terminal_clear = true;
            None
        }
        KeyCode::Backspace => {
            echo_core::commands::clear_suggestions(state);
            state.ui.command_buffer.pop();
            None
        }
        KeyCode::Char(c) => {
            echo_core::commands::clear_suggestions(state);
            state.ui.command_buffer.push(c);
            None
        }
        KeyCode::Enter => echo_core::commands::submit(state),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_submits_the_buffer_through_the_shared_registry() {
        let mut state = AppState::new();
        state.ui.mode = AppMode::Command;
        state.ui.command_buffer = "newlocalplaylist Road Mix".to_string();
        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);

        let Some(AppEvent::CreateLocalPlaylist(name)) = handle_key(&mut state, &key) else {
            panic!("expected CreateLocalPlaylist");
        };

        assert_eq!(name, "Road Mix");
        assert_eq!(state.ui.mode, AppMode::Normal);
    }
}
