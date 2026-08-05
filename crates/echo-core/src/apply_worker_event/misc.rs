use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::AppEvent,
};

pub fn set_timed_status(state: &mut AppState, message: String, seconds: u64) {
    state.ui.status_message = Some(message);
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(seconds));
}

pub fn handle_force_redraw(state: &mut AppState) {
    state.ui.needs_terminal_clear = true;
}

pub async fn handle_force_context_refresh(state: &AppState, app_tx: &mpsc::Sender<AppEvent>) {
    if state.ui.active_view == app::ActiveView::TrackList
        && let Some(context) = state.data.active_tracklist_context.clone()
        && context.requires_worker_load()
    {
        let _ = app_tx.send(AppEvent::RefreshContextTracks(context)).await;
    }
}

pub fn handle_api_request_failed(state: &mut AppState, label: String, message: String) {
    let text = if message.starts_with("rate limited") {
        format!("{label} {message}")
    } else {
        format!("{label} failed: {message}")
    };
    set_timed_status(state, text, 5);
}

pub fn handle_audio_output_unavailable(state: &mut AppState, message: String) {
    state.playback.progress_ms = state.playback.display_progress_ms();
    state.playback.is_playing = false;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());
    let message = message.trim_end_matches(['.', ' ']);
    state.ui.audio_output_error = Some(format!(
        "Audio output disconnected: {message}. Reconnect a device and press Space to resume."
    ));
    state.ui.needs_terminal_clear = true;
}

pub fn handle_audio_output_recovered(state: &mut AppState) {
    state.ui.audio_output_error = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_output_error_is_persistent_and_forces_redraw() {
        let mut state = AppState::new();
        state.playback.is_playing = true;
        state.playback.progress_ms = 1_000;
        state.playback.duration_ms = 10_000;
        state.playback.playback_last_updated_at =
            Some(std::time::Instant::now() - std::time::Duration::from_secs(1));

        handle_audio_output_unavailable(&mut state, "device is not available".to_string());

        assert!(!state.playback.is_playing);
        assert!(state.playback.progress_ms >= 2_000);
        assert_eq!(
            state.playback.display_progress_ms(),
            state.playback.progress_ms
        );
        assert!(state.ui.needs_terminal_clear);
        assert!(state.ui.status_message_expiry.is_none());
        assert!(
            state
                .ui
                .audio_output_error
                .as_deref()
                .unwrap()
                .contains("press Space to resume")
        );

        handle_audio_output_recovered(&mut state);
        assert!(state.ui.audio_output_error.is_none());
    }
}
