use crate::{
    app::{ActiveView, AppState},
    events::AppEvent,
    models::{BrowseNode, TrackListContext},
};

pub fn load_event_if_needed(state: &AppState) -> Option<AppEvent> {
    match state.active_browse_node {
        BrowseNode::TopTracks if state.top_tracks.is_empty() => Some(AppEvent::FetchTopTracks),
        BrowseNode::RecentlyPlayed if state.recently_played.is_empty() => {
            Some(AppEvent::FetchRecentlyPlayed)
        }
        BrowseNode::FollowedArtists if state.followed_artists.is_empty() => {
            Some(AppEvent::FetchFollowedArtists)
        }
        _ => None,
    }
}

pub fn enter_active_node(state: &mut AppState) -> Option<AppEvent> {
    match state.active_browse_node {
        BrowseNode::TopTracks => {
            if state.top_tracks.is_empty() {
                return Some(AppEvent::FetchTopTracks);
            }
            state.show_generated_tracks(
                state.top_tracks.clone(),
                TrackListContext::generated("TOP_TRACKS", "Top Tracks"),
            );
        }
        BrowseNode::RecentlyPlayed => {
            if state.recently_played.is_empty() {
                return Some(AppEvent::FetchRecentlyPlayed);
            }
            state.show_generated_tracks(
                state.recently_played.clone(),
                TrackListContext::generated("RECENTLY_PLAYED", "Recently Played"),
            );
        }
        BrowseNode::FollowedArtists => {
            if state.followed_artists.is_empty() {
                return Some(AppEvent::FetchFollowedArtists);
            }
            state.active_view = ActiveView::ArtistList;
            state.selected_artist_index = 0;
        }
    }
    None
}

pub fn select_node_from_library_index(state: &mut AppState) {
    state.active_browse_node = match state.selected_playlist_index {
        0 => BrowseNode::TopTracks,
        1 => BrowseNode::RecentlyPlayed,
        _ => BrowseNode::FollowedArtists,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_tracks_empty_requests_fetch() {
        let mut state = AppState::new();
        state.active_browse_node = BrowseNode::TopTracks;

        assert!(matches!(
            load_event_if_needed(&state),
            Some(AppEvent::FetchTopTracks)
        ));
    }

    #[test]
    fn generated_top_tracks_do_not_request_worker_load() {
        let mut state = AppState::new();
        state.active_browse_node = BrowseNode::TopTracks;
        state.top_tracks = vec![crate::models::Track {
            id: "track".to_string(),
            name: "Track".to_string(),
            artist: "Artist".to_string(),
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        }];

        assert!(enter_active_node(&mut state).is_none());
        assert!(
            !state
                .active_tracklist_context
                .as_ref()
                .unwrap()
                .requires_worker_load()
        );
    }
}
