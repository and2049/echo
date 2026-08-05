use echo_core::{app::AppState, events::AppEvent, intent};

pub fn play_selected(state: &mut AppState) -> Option<AppEvent> {
    intent::play_track_at(state, state.ui.selected_track_index)
}

pub fn mark_selected_for_delete(state: &mut AppState) {
    if let Some(track) = state.data.tracks.get(state.ui.selected_track_index)
        && let Some(context) = &state.data.active_tracklist_context
        && context.can_modify_playlist(state.data.user_id.as_ref())
    {
        if state.ui.pending_d_press {
            state.ui.track_delete_prompt = Some((context.id.clone(), vec![track.id.clone()]));
            state.ui.pending_d_press = false;
        } else {
            state.ui.pending_d_press = true;
        }
    }
}
