use crate::app::AppState;
use crate::thumbnails::ThumbState;

pub fn handle(
    state: &mut AppState,
    protocol: ratatui_image::protocol::StatefulProtocol,
) {
    state.ui.active_library_header_image = Some(protocol);
    state.ui.header_image_dirty = true;
}

pub fn handle_thumbnail(
    state: &mut AppState,
    url: String,
    protocol: Option<ratatui_image::protocol::StatefulProtocol>,
) {
    let entry = match protocol {
        Some(protocol) => ThumbState::Ready {
            protocol: Box::new(protocol),
            buffer: None,
        },
        None => ThumbState::Failed,
    };
    state.ui.thumbnails.entries.insert(url, entry);
}
