use echo_core::{app::AppState, events::AppEvent, models::BrowseNode};

pub fn load_event_if_needed(_state: &AppState) -> Option<AppEvent> {
    None
}

pub fn enter_active_node(state: &mut AppState) -> Option<AppEvent> {
    match state.ui.active_browse_node {
        BrowseNode::TopTracks => echo_core::intent::open_top_tracks(state),
        BrowseNode::RecentlyPlayed => echo_core::intent::open_recently_played(state),
        BrowseNode::FollowedArtists => echo_core::intent::open_artist_list(state),
        BrowseNode::TopArtists => echo_core::intent::open_top_artists(state),
        BrowseNode::WhatsNew => echo_core::intent::open_whats_new(state),
    }
}

pub fn select_node_from_library_index(state: &mut AppState) {
    state.ui.active_browse_node = match state.ui.selected_playlist_index {
        0 => BrowseNode::TopTracks,
        1 => BrowseNode::RecentlyPlayed,
        2 => BrowseNode::FollowedArtists,
        3 => BrowseNode::TopArtists,
        _ => BrowseNode::WhatsNew,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use echo_core::app::ActiveView;

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
            Some(AppEvent::FetchTopTracks { .. })
        ));
    }

    #[test]
    fn browse_index_three_maps_to_top_artists() {
        let mut state = AppState::new();
        state.ui.selected_playlist_index = 3;

        select_node_from_library_index(&mut state);

        assert_eq!(state.ui.active_browse_node, BrowseNode::TopArtists);
    }

    #[test]
    fn entering_empty_top_artists_opens_list_and_requests_fetch() {
        let mut state = AppState::new();
        state.ui.active_browse_node = BrowseNode::TopArtists;

        let event = enter_active_node(&mut state);

        assert!(matches!(event, Some(AppEvent::FetchTopArtists { .. })));
        assert_eq!(state.ui.active_view, ActiveView::ArtistList);
        assert_eq!(
            state.ui.artist_list_source,
            echo_core::app::ArtistListSource::Top
        );
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
