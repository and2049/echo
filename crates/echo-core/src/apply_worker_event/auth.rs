use crate::app::{self, AppState};
use crate::events::AppEvent;
use tokio::sync::mpsc;

pub fn handle(state: &mut AppState) {
    state.ui.mode = app::AppMode::Normal;
    state.ui.status_message = None;
    state.ui.status_message_expiry = None;
}

pub fn handle_reauthorization_required(
    state: &mut AppState,
    app_tx: &mpsc::UnboundedSender<AppEvent>,
) {
    if state.ui.mode != app::AppMode::Authenticating {
        state.ui.mode = app::AppMode::Authenticating;
        let _ = app_tx.send(AppEvent::StartAuth);
    }
}

pub fn handle_failure(state: &mut AppState, message: String) {
    state.ui.mode = app::AppMode::Normal;
    super::misc::set_timed_status(
        state,
        format!(
            "Spotify authentication failed: {message}. Local playback is still available; run :spotifylogin to retry."
        ),
        10,
    );
}

pub fn handle_user_identity(state: &mut AppState, user_id: String) {
    state.data.user_id = Some(user_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentication_failure_returns_to_local_capable_mode() {
        let mut state = AppState::new();
        state.ui.mode = app::AppMode::Authenticating;

        handle_failure(&mut state, "cancelled".to_string());

        assert!(state.ui.mode == app::AppMode::Normal);
        assert!(
            state
                .ui
                .status_message
                .as_deref()
                .is_some_and(|message| message.contains(":spotifylogin"))
        );
    }

    #[test]
    fn reauthorization_starts_once_while_authenticating() {
        let mut state = AppState::new();
        let (tx, mut rx) = mpsc::unbounded_channel();

        handle_reauthorization_required(&mut state, &tx);
        handle_reauthorization_required(&mut state, &tx);

        assert!(matches!(rx.try_recv(), Ok(AppEvent::StartAuth)));
        assert!(rx.try_recv().is_err());
    }
}
