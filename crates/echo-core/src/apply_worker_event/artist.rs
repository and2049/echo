use tokio::sync::mpsc;

use crate::{
    app::{self, AppState},
    events::WorkerEvent,
    image_tasks,
    models::{DiscographyFilter, DiscographyPage},
};

use super::misc::set_timed_status;

pub fn handle_page_opened(
    state: &mut AppState,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    artist_id: String,
    artist_name: String,
    artist_image_url: Option<String>,
) {
    if state.data.pending_artist_page_id.as_deref() != Some(artist_id.as_str()) {
        return;
    }
    if state
        .data
        .artist_page_data
        .as_ref()
        .is_none_or(|data| data.artist_id != artist_id)
    {
        state.data.artist_page_data = Some(crate::models::ArtistPageData {
            artist_id,
            artist_name,
            image_url: artist_image_url.clone(),
            albums: Vec::new(),
            top_tracks: Vec::new(),
            chips: Default::default(),
        });
    } else if let Some(data) = state.data.artist_page_data.as_mut()
        && data.image_url.is_none()
    {
        data.image_url = artist_image_url.clone();
    }
    state.ui.active_view = app::ActiveView::ArtistPage;
    state.ui.artist_page_album_index = 0;
    state.data.artist_page_loading = true;
    state.data.artist_albums_loading = true;
    state.data.artist_top_tracks_loading = true;
    if let Some(url) = artist_image_url.as_ref() {
        image_tasks::spawn_header_for_url(
            url,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
}

/// Stores a chip's rows; when it is the chip on screen they become the visible album rows.
/// The desktop cursor counts the Popular rows first, so it clamps against the combined length.
pub fn handle_albums_loaded(
    state: &mut AppState,
    artist_id: String,
    filter: DiscographyFilter,
    page: DiscographyPage,
) {
    let active = state.ui.artist_discography_filter;
    let Some(data) = state.data.artist_page_data.as_mut() else {
        return;
    };
    if data.artist_id != artist_id {
        return;
    }
    if filter == active {
        data.albums = page.albums.clone();
        state.data.artist_albums_loading = false;
        state.data.artist_page_loading = false;
    }
    data.chips.insert(filter, page);
    let rows = data.top_tracks.len() + data.album_row_count(active);
    state.ui.artist_page_album_index = state.ui.artist_page_album_index.min(rows.saturating_sub(1));
}

pub fn handle_albums_load_failed(state: &mut AppState, artist_id: String, message: String) {
    if state
        .data
        .artist_page_data
        .as_ref()
        .is_some_and(|data| data.artist_id == artist_id)
    {
        state.data.artist_albums_loading = false;
        state.data.artist_page_loading = false;
        let status = if message == "refresh already in progress" {
            "Artist albums refresh already in progress.".to_string()
        } else {
            format!("Artist albums failed: {message}")
        };
        set_timed_status(state, status, 5);
    }
}

pub fn handle_albums_rate_limited(state: &mut AppState, artist_id: String, retry_after_secs: u64) {
    if let Some(data) = state.data.artist_page_data.as_ref()
        && data.artist_id == artist_id
    {
        let has_cached_albums = !data.albums.is_empty();
        state.data.artist_albums_loading = false;
        state.data.artist_page_loading = false;
        let message = if has_cached_albums {
            format!(
                "Artist albums rate limited. Showing cached albums. Try again in {retry_after_secs}s."
            )
        } else {
            format!("Artist albums rate limited. Try again in {retry_after_secs}s.")
        };
        set_timed_status(state, message, 5);
    }
}

pub fn handle_top_tracks_loaded(
    state: &mut AppState,
    artist_id: String,
    tracks: Vec<crate::models::Track>,
) {
    if let Some(data) = state.data.artist_page_data.as_mut()
        && data.artist_id == artist_id
    {
        data.top_tracks = tracks;
        state.data.artist_top_tracks_loading = false;
    }
}

/// Log-only failure (already logged at the API layer): the Popular section simply
/// stays hidden while the rest of the page remains usable.
pub fn handle_top_tracks_load_failed(state: &mut AppState, artist_id: String, _message: String) {
    if state
        .data
        .artist_page_data
        .as_ref()
        .is_some_and(|data| data.artist_id == artist_id)
    {
        state.data.artist_top_tracks_loading = false;
    }
}

pub fn handle_image_resolved(
    state: &mut AppState,
    worker_tx: &mpsc::Sender<WorkerEvent>,
    artist_id: String,
    image_url: String,
) {
    if let Some(data) = state.data.artist_page_data.as_mut()
        && data.artist_id == artist_id
        && data.image_url.is_none()
    {
        data.image_url = Some(image_url.clone());
        image_tasks::spawn_header_for_url(
            &image_url,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn stale_artist_page_result_is_ignored() {
        let (_worker_tx, _) = mpsc::channel::<crate::events::WorkerEvent>(1);
        let mut state = AppState::new();
        state.begin_artist_page_load("current".to_string(), "Current".to_string(), None);

        handle_albums_loaded(
            &mut state,
            "stale".to_string(),
            DiscographyFilter::All,
            one_album_page(),
        );

        assert_eq!(
            state.data.pending_artist_page_id.as_deref(),
            Some("current")
        );
        assert_eq!(
            state
                .data
                .artist_page_data
                .as_ref()
                .map(|data| data.artist_name.as_str()),
            Some("Current")
        );
        assert_eq!(
            state
                .data
                .artist_page_data
                .as_ref()
                .map(|data| data.albums.len()),
            Some(0)
        );
    }

    fn one_album_page() -> DiscographyPage {
        DiscographyPage {
            albums: vec![crate::models::Album {
                id: "album".to_string(),
                name: "Album".to_string(),
                artists: "Artist".to_string(),
                image_url: None,
                thumb_url: None,
                release_year: "2024".to_string(),
                release_date: None,
                track_count: None,
                group: None,
            }],
            has_more: true,
            total: 2,
        }
    }

    #[tokio::test]
    async fn a_chip_off_screen_is_stored_without_replacing_the_visible_rows() {
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string(), None);

        handle_albums_loaded(
            &mut state,
            "artist".to_string(),
            DiscographyFilter::Singles,
            one_album_page(),
        );
        let data = state.data.artist_page_data.as_ref().unwrap();
        assert!(data.albums.is_empty());
        assert!(data.chips.contains_key(&DiscographyFilter::Singles));
        assert!(state.data.artist_albums_loading);

        handle_albums_loaded(
            &mut state,
            "artist".to_string(),
            DiscographyFilter::All,
            one_album_page(),
        );
        let data = state.data.artist_page_data.as_ref().unwrap();
        assert_eq!(data.albums.len(), 1);
        assert_eq!(data.album_row_count(DiscographyFilter::All), 2);
        assert!(!state.data.artist_albums_loading);
    }

    fn sample_track(id: &str) -> crate::models::Track {
        crate::models::Track {
            explicit: false,
            added_by: None,
            id: id.to_string(),
            source: crate::models::TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: String::new(),
            album: String::new(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    #[tokio::test]
    async fn stale_top_tracks_are_ignored() {
        let mut state = AppState::new();
        state.begin_artist_page_load("current".to_string(), "Current".to_string(), None);

        handle_top_tracks_loaded(&mut state, "stale".to_string(), vec![sample_track("t")]);

        assert!(
            state
                .data
                .artist_page_data
                .as_ref()
                .is_some_and(|data| data.top_tracks.is_empty())
        );
        assert!(state.data.artist_top_tracks_loading);
    }

    #[tokio::test]
    async fn artist_album_rate_limit_leaves_page_open() {
        let (_worker_tx, _) = mpsc::channel::<crate::events::WorkerEvent>(1);
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string(), None);

        handle_albums_rate_limited(&mut state, "artist".to_string(), 49);

        assert!(matches!(state.ui.active_view, app::ActiveView::ArtistPage));
        assert!(!state.data.artist_albums_loading);
        assert!(!state.data.artist_page_loading);

        handle_albums_loaded(
            &mut state,
            "artist".to_string(),
            DiscographyFilter::All,
            one_album_page(),
        );

        assert_eq!(
            state
                .data
                .artist_page_data
                .as_ref()
                .map(|data| data.albums.len()),
            Some(1)
        );
    }
}
