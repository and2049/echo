//! y/n handling for the destructive-action prompts. The prompts themselves live in
//! `echo_core::intent` (`prompt_active`/`confirm_prompt`/`cancel_prompt`), shared with the
//! desktop app's confirm modal.

use crossterm::event::{KeyCode, KeyEvent};
use echo_core::app::AppState;
use echo_core::events::AppEvent;

pub fn handle(state: &mut AppState, key: &KeyEvent) -> (bool, Option<AppEvent>) {
    if !echo_core::intent::prompt_active(state) {
        return (false, None);
    }
    if key.code == KeyCode::Char('y') {
        (true, echo_core::intent::confirm_prompt(state))
    } else {
        echo_core::intent::cancel_prompt(state);
        (true, None)
    }
}
