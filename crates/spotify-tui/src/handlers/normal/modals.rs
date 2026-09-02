use crossterm::event::{KeyCode, KeyEvent};
use echo_core::action_menu;
use echo_core::app::AppState;
use echo_core::events::AppEvent;

pub fn handle(state: &mut AppState, key: &KeyEvent) -> (bool, Option<AppEvent>) {
    if state.ui.playlist_add_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                action_menu::cancel_playlist_add(state);
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.ui.selected_playlist_modal_index + 1
                    < action_menu::playlist_add_choices(state).len()
                {
                    state.ui.selected_playlist_modal_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if state.ui.selected_playlist_modal_index > 0 => {
                state.ui.selected_playlist_modal_index -= 1;
            }
            KeyCode::Enter => {
                let index = state.ui.selected_playlist_modal_index;
                if let Some(event) = action_menu::commit_playlist_add(state, index) {
                    return (true, Some(event));
                }
            }
            _ => {}
        }
        return (true, None);
    }

    if state.ui.device_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.ui.device_modal_open = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.ui.selected_device_index + 1 < state.data.devices.len() {
                    state.ui.selected_device_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.ui.selected_device_index > 0 {
                    state.ui.selected_device_index -= 1;
                }
            }
            KeyCode::Enter => {
                let index = state.ui.selected_device_index;
                if let Some(event) = echo_core::intent::transfer_to_device(state, index) {
                    return (true, Some(event));
                }
            }
            _ => {}
        }
        return (true, None);
    }

    if state.ui.lyrics_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('L') => {
                state.ui.lyrics_modal_open = false;
            }
            _ => {}
        }
        return (true, None);
    }

    if state.ui.action_menu_open {
        let action_count = state
            .ui
            .action_menu_context
            .as_ref()
            .map_or(0, |ctx| ctx.actions().len());
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.ui.action_menu_open = false;
                state.ui.action_menu_context = None;
                state.ui.selected_action_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.ui.selected_action_index + 1 < action_count {
                    state.ui.selected_action_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.ui.selected_action_index > 0 {
                    state.ui.selected_action_index -= 1;
                }
            }
            KeyCode::Enter => {
                if let Some(ctx) = state.ui.action_menu_context.clone() {
                    let action = ctx.actions().get(state.ui.selected_action_index).copied();
                    state.ui.action_menu_open = false;
                    state.ui.action_menu_context = None;
                    state.ui.selected_action_index = 0;
                    if let Some(action) = action {
                        return (true, action_menu::run(state, ctx, action));
                    }
                }
            }
            _ => {}
        }
        return (true, None);
    }

    (false, None)
}
