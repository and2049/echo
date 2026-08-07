use echo_core::{app::AppState, events::AppEvent, intent};

pub fn play_selected(state: &mut AppState) -> Option<AppEvent> {
    intent::play_track_at(state, state.ui.selected_track_index)
}
