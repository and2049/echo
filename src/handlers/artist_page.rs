use crate::{
    app::{ActiveView, AppState, ArtistPageTab},
    events::AppEvent,
    models::TrackListContext,
};

pub fn enter_followed_artist(state: &mut AppState) -> Option<AppEvent> {
    let artist = state.followed_artists.get(state.selected_artist_index)?;
    let artist_id = artist.id.clone();
    let artist_name = artist.name.clone();
    state.begin_artist_page_load(artist_id.clone(), artist_name.clone());
    Some(AppEvent::LoadArtistPage {
        artist_id,
        artist_name: Some(artist_name),
    })
}

pub fn enter_artist_page_selection(state: &mut AppState) -> Option<AppEvent> {
    if state.artist_page_tab == ArtistPageTab::TopTracks {
        let data = state.artist_page_data.clone()?;
        let track = data.top_tracks.get(state.artist_page_track_index)?;
        return Some(AppEvent::PlayTrack {
            context_id: "LIKED_SONGS".to_string(),
            track_id: track.id.clone(),
            is_album: false,
            title: track.name.clone(),
            artist: track.artist.clone(),
            duration_ms: track.duration_ms,
            image_url: track.image_url.clone(),
            album_id: track.album_id.clone(),
        });
    }

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

pub fn toggle_tab(state: &mut AppState) -> Option<AppEvent> {
    state.artist_page_tab = match state.artist_page_tab {
        ArtistPageTab::TopTracks => ArtistPageTab::Albums,
        ArtistPageTab::Albums => ArtistPageTab::TopTracks,
    };
    state.artist_page_track_index = 0;
    state.artist_page_album_index = 0;

    if state.artist_page_tab == ArtistPageTab::Albums
        && let Some(data) = state.artist_page_data.as_ref()
        && data.albums.is_empty()
    {
        state.artist_page_loading = true;
        return Some(AppEvent::LoadArtistAlbums {
            artist_id: data.artist_id.clone(),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backing_out_cancels_pending_artist_without_clearing_page_data() {
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string());

        let event = back_to_artist_list(&mut state);

        assert!(matches!(event, AppEvent::CancelArtistPageLoad));
        assert!(state.active_view == ActiveView::ArtistList);
        assert!(state.pending_artist_page_id.is_none());
    }
}
