use crate::app::AppState;
use crate::artwork::SharedArtwork;
use crate::thumbnails::{ThumbState, ThumbTier};

pub fn handle(state: &mut AppState, artwork: SharedArtwork) {
    state.ui.active_library_header_image = Some(artwork);
}

pub fn handle_thumbnail(
    state: &mut AppState,
    url: String,
    tier: ThumbTier,
    artwork: Option<SharedArtwork>,
) {
    let entry = match artwork {
        Some(artwork) => ThumbState::Ready { artwork },
        None => ThumbState::Failed,
    };
    state.ui.thumbnails.entries.insert((url, tier), entry);
}
