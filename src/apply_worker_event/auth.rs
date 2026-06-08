use crate::app::{self, AppState};

pub fn handle(state: &mut AppState) {
    state.ui.mode = app::AppMode::Normal;
}

pub fn handle_user_identity(state: &mut AppState, user_id: String) {
    state.data.user_id = Some(user_id);
}
