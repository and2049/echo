use crate::app::{AppMode, AppState};
use crate::config::{AppConfig, SpotifyCredentials};
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Tab => {
            state.setup_focus_secret = !state.setup_focus_secret;
            None
        }
        KeyCode::Enter => {
            if !state.setup_client_id.is_empty() && !state.setup_client_secret.is_empty() {
                let mut config = AppConfig::load();
                config.spotify_credentials = Some(SpotifyCredentials {
                    client_id: state.setup_client_id.clone(),
                    client_secret: state.setup_client_secret.clone(),
                });
                let _ = config.save();

                state.mode = AppMode::Authenticating;
                Some(AppEvent::StartAuth)
            } else {
                None
            }
        }
        KeyCode::Backspace => {
            if state.setup_focus_secret {
                state.setup_client_secret.pop();
            } else {
                state.setup_client_id.pop();
            }
            None
        }
        KeyCode::Char(c) => {
            if state.setup_focus_secret {
                state.setup_client_secret.push(c);
            } else {
                state.setup_client_id.push(c);
            }
            None
        }
        KeyCode::Esc => Some(AppEvent::Quit),
        _ => None,
    }
}
