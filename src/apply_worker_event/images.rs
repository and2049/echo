use crate::app::AppState;

pub fn handle(
    state: &mut AppState,
    protocol: ratatui_image::protocol::StatefulProtocol,
) {
    state.ui.active_library_header_image = Some(protocol);
    state.ui.header_image_dirty = true;
}
