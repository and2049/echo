use crossterm::event::{KeyCode, KeyEvent};
use echo_core::app::AppState;
use echo_core::events::AppEvent;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Tab => {
            state.ui.setup_focus_secret = !state.ui.setup_focus_secret;
            None
        }
        KeyCode::Enter => echo_core::intent::submit_setup_credentials(state),
        KeyCode::Backspace => {
            if state.ui.setup_focus_secret {
                state.ui.setup_client_secret.pop();
            } else {
                state.ui.setup_client_id.pop();
            }
            None
        }
        KeyCode::Char(c) => {
            if state.ui.setup_focus_secret {
                state.ui.setup_client_secret.push(c);
            } else {
                state.ui.setup_client_id.push(c);
            }
            None
        }
        KeyCode::Esc => Some(AppEvent::Quit),
        _ => None,
    }
}
