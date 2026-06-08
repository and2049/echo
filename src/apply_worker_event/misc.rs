use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::AppEvent,
    tui::Tui,
};

pub fn set_timed_status(state: &mut AppState, message: String, seconds: u64) {
    state.ui.status_message = Some(message);
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(seconds));
}

pub fn handle_force_redraw(tui: &mut Tui) {
    let _ = tui.terminal.clear();
}

pub async fn handle_force_context_refresh(
    state: &AppState,
    app_tx: &mpsc::Sender<AppEvent>,
) {
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
