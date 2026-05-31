use crossterm::event::{KeyCode, KeyEvent};
use crate::app::{AppState, AppMode};
use crate::config::{AppConfig, SpotifyCredentials};
use crate::events::AppEvent;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Esc => {
            state.is_running = false;
        }
        KeyCode::Tab => {
            state.setup_focus_secret = !state.setup_focus_secret;
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
                return Some(AppEvent::StartAuth);
            }
        }
        KeyCode::Backspace => {
            if state.setup_focus_secret {
                state.setup_client_secret.pop();
            } else {
                state.setup_client_id.pop();
            }
        }
        KeyCode::Char(c) => {
            if state.setup_focus_secret {
                state.setup_client_secret.push(c);
            } else {
                state.setup_client_id.push(c);
            }
        }
        _ => {}
    }
    None
}
