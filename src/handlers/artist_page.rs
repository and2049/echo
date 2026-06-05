use crate::{
    app::{ActiveView, AppState},
    events::AppEvent,
    models::TrackListContext,
};

pub fn enter_followed_artist(state: &mut AppState) -> Option<AppEvent> {
    let artist = state.followed_artists.get(state.selected_artist_index)?;
    enter_artist(state, artist.clone())
}

pub fn enter_search_artist(state: &mut AppState) -> Option<AppEvent> {
    let artist = state
        .search_results
        .artists
        .get(state.selected_search_index)?;
    enter_artist(state, artist.clone())
}

fn enter_artist(state: &mut AppState, artist: crate::models::Artist) -> Option<AppEvent> {
    let artist_id = artist.id.clone();
    let artist_name = artist.name.clone();
    let artist_image_url = artist.image_url.clone();
    state.begin_artist_page_load(
        artist_id.clone(),
        artist_name.clone(),
        artist_image_url.clone(),
    );
    Some(AppEvent::LoadArtistPage {
        artist_id,
        artist_name: Some(artist_name),
        artist_image_url,
    })
}

pub fn enter_artist_page_selection(state: &mut AppState) -> Option<AppEvent> {
    let data = state.artist_page_data.clone()?;
    let album = data.albums.get(state.artist_page_album_index)?;
    let context = TrackListContext::album(
        album.id.clone(),
        album.name.clone(),
        album.artists.clone(),
        album.image_url.clone(),
    );
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

pub fn back_to_artist_list(state: &mut AppState) -> AppEvent {
    state.active_view = ActiveView::ArtistList;
    state.clear_pending_artist_page();
    AppEvent::CancelArtistPageLoad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backing_out_cancels_pending_artist_without_clearing_page_data() {
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string(), None);

        let event = back_to_artist_list(&mut state);

        assert!(matches!(event, AppEvent::CancelArtistPageLoad));
        assert!(state.active_view == ActiveView::ArtistList);
        assert!(state.pending_artist_page_id.is_none());
    }

    #[test]
    fn selecting_followed_artist_opens_partial_shell_immediately() {
        let mut state = AppState::new();
        state.followed_artists.push(crate::models::Artist {
            id: "artist".to_string(),
            name: "Artist".to_string(),
            followers: 0,
            image_url: Some("image".to_string()),
        });

        let event = enter_followed_artist(&mut state);

        assert!(matches!(event, Some(AppEvent::LoadArtistPage { .. })));
        assert!(matches!(state.active_view, ActiveView::ArtistPage));
        assert_eq!(state.pending_artist_page_id.as_deref(), Some("artist"));
        let page = state.artist_page_data.as_ref().expect("artist shell");
        assert_eq!(page.artist_name, "Artist");
        assert_eq!(page.image_url.as_deref(), Some("image"));
        assert!(page.albums.is_empty());
        assert!(state.artist_albums_loading);
    }

    #[test]
    fn selecting_search_artist_opens_partial_shell_with_image() {
        let mut state = AppState::new();
        state.search_results.artists.push(crate::models::Artist {
            id: "artist".to_string(),
            name: "Search Artist".to_string(),
            followers: 0,
            image_url: Some("search-image".to_string()),
        });

        let event = enter_search_artist(&mut state);

        assert!(matches!(event, Some(AppEvent::LoadArtistPage { .. })));
        assert!(matches!(state.active_view, ActiveView::ArtistPage));
        let page = state.artist_page_data.as_ref().expect("artist shell");
        assert_eq!(page.artist_name, "Search Artist");
        assert_eq!(page.image_url.as_deref(), Some("search-image"));
    }
}
