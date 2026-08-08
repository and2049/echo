use echo_core::{
    app::{ActiveView, AppState},
    events::AppEvent,
    models::BrowseNode,
};

pub fn load_event_if_needed(_state: &AppState) -> Option<AppEvent> {
    None
}

pub fn enter_active_node(state: &mut AppState) -> Option<AppEvent> {
    match state.ui.active_browse_node {
        BrowseNode::TopTracks => return echo_core::intent::open_top_tracks(state),
        BrowseNode::RecentlyPlayed => return echo_core::intent::open_recently_played(state),
        BrowseNode::FollowedArtists => {
            if state.data.followed_artists.is_empty() {
                return Some(AppEvent::FetchFollowedArtists);
            }
            state.push_view_history();
            state.ui.active_view = ActiveView::ArtistList;
            state.ui.selected_artist_index = 0;
        }
    }
    None
}

pub fn select_node_from_library_index(state: &mut AppState) {
    state.ui.active_browse_node = match state.ui.selected_playlist_index {
        0 => BrowseNode::TopTracks,
        1 => BrowseNode::RecentlyPlayed,
        _ => BrowseNode::FollowedArtists,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_tracks_selection_does_not_request_fetch() {
        let mut state = AppState::new();
        state.ui.active_browse_node = BrowseNode::TopTracks;

        assert!(load_event_if_needed(&state).is_none());
    }

    #[test]
    fn entering_empty_top_tracks_requests_fetch() {
        let mut state = AppState::new();
        state.ui.active_browse_node = BrowseNode::TopTracks;

        assert!(matches!(
            enter_active_node(&mut state),
            Some(AppEvent::FetchTopTracks)
        ));
    }

    #[test]
    fn clicking_top_track_loads_artist_page() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::Library;
        state.ui.selected_playlist_index = 0; // The "Browse" node
        state.data.top_tracks = vec![echo_core::models::Track {
            id: "track1".to_string(),
            source: echo_core::models::TrackSource::Spotify,
            local_path: None,
            name: "Test Track".to_string(),
            artist: "Test Artist".to_string(),
            album: String::new(),
            added_at: None,
            artist_id: Some("artist1".to_string()),
            album_id: None,
            duration_ms: 60000,
            image_url: None,
            artists: Vec::new(),
        }];

        assert!(enter_active_node(&mut state).is_none());
        assert!(
            !state
                .data
                .active_tracklist_context
                .as_ref()
                .unwrap()
                .requires_worker_load()
        );
    }

    #[test]
    fn generated_top_tracks_do_not_request_worker_load() {
        let mut state = AppState::new();
        state.ui.active_browse_node = BrowseNode::TopTracks;
        state.data.top_tracks = vec![echo_core::models::Track {
            id: "track".to_string(),
            source: echo_core::models::TrackSource::Spotify,
            local_path: None,
            name: "Track".to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artists: Vec::new(),
        }];

        assert!(enter_active_node(&mut state).is_none());
        assert!(
            !state
                .data
                .active_tracklist_context
                .as_ref()
                .unwrap()
                .requires_worker_load()
        );
    }
}
