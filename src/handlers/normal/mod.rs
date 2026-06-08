use crate::app::{ActiveView, AppState};
use crate::events::AppEvent;
use crate::handlers::{artist_page, browse, tracklist};
use crate::models::{SearchTrack, Track, TrackListContext};
use crossterm::event::{KeyCode, KeyEvent};

mod modals;
mod prompts;

pub use modals::playlist_modal_choices;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    let (prompt_active, prompt_event) = prompts::handle(state, key);
    if let Some(event) = prompt_event {
        return Some(event);
    }
    if prompt_active {
        return None;
    }

    let (modal_active, modal_event) = modals::handle(state, key);
    if let Some(event) = modal_event {
        return Some(event);
    }
    if modal_active {
        return None;
    }

    if key.code != KeyCode::Char('d') {
        state.ui.pending_d_press = false;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => match state.ui.active_view {
            ActiveView::Library => {
                if state.ui.active_library_tab == crate::app::LibraryTab::Browse {
                    if state.ui.selected_playlist_index < 2 {
                        state.ui.selected_playlist_index += 1;
                    }
                    browse::select_node_from_library_index(state);
                    return browse::load_event_if_needed(state);
                } else {
                    let max_len = if state.ui.active_library_tab
                        == crate::app::LibraryTab::Albums
                    {
                        state.data.saved_albums.len()
                    } else {
                        state.data.library_view.len()
                    };
                    if max_len > 0
                        && state.ui.selected_playlist_index < max_len.saturating_sub(1)
                    {
                        state.ui.selected_playlist_index += 1;
                    }
                }
            }
            ActiveView::TrackList => {
                if state.ui.selected_track_index < state.data.tracks.len().saturating_sub(1)
                {
                    state.ui.selected_track_index += 1;
                }
            }
            ActiveView::ArtistList => {
                if !state.data.followed_artists.is_empty()
                    && state.ui.selected_artist_index
                        < state.data.followed_artists.len().saturating_sub(1)
                {
                    state.ui.selected_artist_index += 1;
                }
            }
            ActiveView::ArtistPage => {
                if let Some(ref data) = state.data.artist_page_data {
                    if !data.albums.is_empty()
                        && state.ui.artist_page_album_index
                            < data.albums.len().saturating_sub(1)
                    {
                        state.ui.artist_page_album_index += 1;
                    }
                }
            }
            ActiveView::SearchResults => {
                let max = search_results_len(state);
                if max > 0 && state.ui.selected_search_index < max.saturating_sub(1) {
                    state.ui.selected_search_index += 1;
                }
            }
            ActiveView::Queue => {
                if !state.data.queue.is_empty()
                    && state.ui.selected_queue_index
                        < state.data.queue.len().saturating_sub(1)
                {
                    state.ui.selected_queue_index += 1;
                }
            }
            ActiveView::Devices => {
                if !state.data.devices.is_empty()
                    && state.ui.selected_device_index
                        < state.data.devices.len().saturating_sub(1)
                {
                    state.ui.selected_device_index += 1;
                }
            }
        },
        KeyCode::Char('k') | KeyCode::Up => match state.ui.active_view {
            ActiveView::Library => {
                if state.ui.active_library_tab == crate::app::LibraryTab::Browse {
                    if state.ui.selected_playlist_index > 0 {
                        state.ui.selected_playlist_index -= 1;
                    }
                    browse::select_node_from_library_index(state);
                    return browse::load_event_if_needed(state);
                } else if state.ui.selected_playlist_index > 0 {
                    state.ui.selected_playlist_index -= 1;
                }
            }
            ActiveView::TrackList => {
                if state.ui.selected_track_index > 0 {
                    state.ui.selected_track_index -= 1;
                }
            }
            ActiveView::ArtistList => {
                if state.ui.selected_artist_index > 0 {
                    state.ui.selected_artist_index -= 1;
                }
            }
            ActiveView::ArtistPage => {
                if state.ui.artist_page_album_index > 0 {
                    state.ui.artist_page_album_index -= 1;
                }
            }
            ActiveView::SearchResults => {
                if state.ui.selected_search_index > 0 {
                    state.ui.selected_search_index -= 1;
                }
            }
            ActiveView::Queue => {
                if state.ui.selected_queue_index > 0 {
                    state.ui.selected_queue_index -= 1;
                }
            }
            ActiveView::Devices => {
                if state.ui.selected_device_index > 0 {
                    state.ui.selected_device_index -= 1;
                }
            }
        },
        KeyCode::Char('l') => {
            if state.ui.active_view == ActiveView::Library {
                return crate::handlers::normal::handle_key(
                    state,
                    &KeyEvent::new(KeyCode::Enter, key.modifiers),
                );
            } else if state.ui.active_view == ActiveView::TrackList {
                if state.ui.selected_track_index < state.data.tracks.len() {
                    let track = &state.data.tracks[state.ui.selected_track_index];
                    let track_id = track.id.clone();
                    if state.data.liked_tracks.contains(&track_id) {
                        state.ui.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.data.liked_tracks.insert(track_id.clone());
                        state.ui.status_message = Some(crate::i18n::t(
                            "messages.added_to_liked",
                            &state.ui.library_config.language,
                        ));
                        state.ui.status_message_expiry = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_secs(3),
                        );
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            } else if state.ui.active_view == ActiveView::Queue {
                if state.ui.selected_queue_index < state.data.queue.len() {
                    let track = &state.data.queue[state.ui.selected_queue_index];
                    let track_id = track.id.clone();
                    if state.data.liked_tracks.contains(&track_id) {
                        state.ui.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.data.liked_tracks.insert(track_id.clone());
                        state.ui.status_message = Some(crate::i18n::t(
                            "messages.added_to_liked",
                            &state.ui.library_config.language,
                        ));
                        state.ui.status_message_expiry = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_secs(3),
                        );
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            } else if state.ui.active_view == ActiveView::SearchResults
                && state.ui.active_search_tab == crate::app::SearchTab::Tracks
            {
                let i = state.ui.selected_search_index;
                if let Some(track) = state.data.search_results.tracks.get(i) {
                    let track_id = track.id.clone();
                    if state.data.liked_tracks.contains(&track_id) {
                        state.ui.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.data.liked_tracks.insert(track_id.clone());

                        let mut cache = crate::config::AppConfig::load_cache();
                        cache.liked_tracks = state.data.liked_tracks.clone();
                        let _ = crate::config::AppConfig::save_cache(&cache);

                        state.ui.status_message = Some(crate::i18n::t(
                            "messages.added_to_liked",
                            &state.ui.library_config.language,
                        ));
                        state.ui.status_message_expiry = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_secs(3),
                        );
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            }
        }
        KeyCode::Char('L') => {
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
            {
                state.ui.condensed_lyrics_enabled = !state.ui.condensed_lyrics_enabled;
                let mut app_config = crate::config::AppConfig::load();
                app_config.library.condensed_lyrics_enabled =
                    state.ui.condensed_lyrics_enabled;
                let _ = app_config.save();
            } else {
                state.ui.lyrics_modal_open = !state.ui.lyrics_modal_open;
            }
        }
        KeyCode::Enter | KeyCode::Char('z') => {
            if state.ui.active_view == ActiveView::Library {
                let context = if state.ui.active_library_tab
                    == crate::app::LibraryTab::Albums
                {
                    if state.ui.selected_playlist_index < state.data.saved_albums.len() {
                        let album =
                            &state.data.saved_albums[state.ui.selected_playlist_index];
                        Some(TrackListContext::album(
                            album.id.clone(),
                            album.name.clone(),
                            album.artists.clone(),
                            album.image_url.clone(),
                        ))
                    } else {
                        None
                    }
                } else if state.ui.active_library_tab == crate::app::LibraryTab::Browse
                {
                    if let Some(event) = browse::enter_active_node(state) {
                        return Some(event);
                    }
                    None
                } else {
                    if let Some(node) = state
                        .data
                        .library_view
                        .get(state.ui.selected_playlist_index)
                        .cloned()
                    {
                        match node {
                            crate::models::LibraryNode::Playlist { playlist, .. } => {
                                if playlist.id == "local-library" {
                                    state.show_local_library();
                                    None
                                } else if playlist.id.starts_with("local-playlist:")
                                {
                                    state.show_local_playlist(
                                        &playlist.id,
                                        playlist.name.clone(),
                                    );
                                    None
                                } else {
                                    Some(TrackListContext::playlist(
                                        playlist.id.clone(),
                                        playlist.name.clone(),
                                        playlist.owner.clone(),
                                        playlist.owner_id.clone(),
                                        playlist.image_url.clone(),
                                    ))
                                }
                            }
                            crate::models::LibraryNode::Folder(f) => {
                                let folder_name = f.name.clone();
                                if let Some(folder) = state
                                    .ui
                                    .library_config
                                    .folders
                                    .iter_mut()
                                    .find(|fd| fd.name == folder_name)
                                {
                                    folder.is_open = !folder.is_open;
                                }
                                state.save_library_config();
                                state.compute_library_view();
                                None
                            }
                        }
                    } else {
                        None
                    }
                };

                if let Some(context) = context {
                    state.begin_tracklist_load(context.clone());
                    return Some(AppEvent::LoadContextTracks(context));
                }
            } else if state.ui.active_view == ActiveView::TrackList {
                return tracklist::play_selected(state);
            } else if state.ui.active_view == ActiveView::SearchResults {
                let i = state.ui.selected_search_index;
                match state.ui.active_search_tab {
                    crate::app::SearchTab::Tracks => {
                        if let Some(t) = state.data.search_results.tracks.get(i) {
                            return search_track_play_event(state, t);
                        }
                    }
                    crate::app::SearchTab::Albums => {
                        if let Some(album) = state.data.search_results.albums.get(i) {
                            if album.id.starts_with("local-album:") {
                                let album_name = album.name.clone();
                                let tracks: Vec<_> = state
                                    .data
                                    .local_library
                                    .to_tracks()
                                    .into_iter()
                                    .filter(|track| {
                                        state
                                            .data
                                            .local_library
                                            .tracks
                                            .iter()
                                            .find(|local| local.id == track.id)
                                            .is_some_and(|local| {
                                                local.album == album_name
                                            })
                                    })
                                    .collect();
                                if !tracks.is_empty() {
                                    state.show_generated_tracks(
                                        tracks,
                                        TrackListContext::generated(
                                            album.id.clone(),
                                            album.name.clone(),
                                        ),
                                    );
                                }
                                return None;
                            }
                            let context = TrackListContext::album(
                                album.id.clone(),
                                album.name.clone(),
                                album.artist.clone(),
                                album.image_url.clone(),
                            );
                            state.ui.prev_view = None;
                            state.begin_tracklist_load(context.clone());
                            return Some(AppEvent::LoadContextTracks(context));
                        }
                    }
                    crate::app::SearchTab::Artists => {
                        if let Some(artist) =
                            state.data.search_results.artists.get(i)
                            && artist.id.starts_with("local-artist:")
                        {
                            let artist_name = artist.name.clone();
                            let tracks: Vec<_> = state
                                .data
                                .local_library
                                .to_tracks()
                                .into_iter()
                                .filter(|track| track.artist == artist_name)
                                .collect();
                            if !tracks.is_empty() {
                                state.show_generated_tracks(
                                    tracks,
                                    TrackListContext::generated(
                                        artist.id.clone(),
                                        artist.name.clone(),
                                    ),
                                );
                            }
                            return None;
                        }
                        return artist_page::enter_search_artist(state);
                    }
                }
            } else if state.ui.active_view == ActiveView::ArtistList {
                return artist_page::enter_followed_artist(state);
            } else if state.ui.active_view == ActiveView::ArtistPage {
                return artist_page::enter_artist_page_selection(state);
            }
        }
        KeyCode::Char(':') => {
            state.ui.mode = crate::app::AppMode::Command;
            state.ui.command_buffer.clear();
            state.ui.status_message = None;
        }
        KeyCode::Char('/') => {
            state.ui.mode = crate::app::AppMode::Search;
            state.ui.search_query.clear();
            state.ui.search_matches.clear();
            state.ui.status_message = None;
        }
        KeyCode::Char('f') => {
            state.ui.mode = crate::app::AppMode::Command;
            state.ui.command_buffer = "search ".to_string();
            state.ui.status_message = None;
        }
        KeyCode::Char('n') if !state.ui.search_matches.is_empty() => {
            if let Some(&next_idx) = state
                .ui
                .search_matches
                .iter()
                .find(|&&i| i > state.ui.selected_track_index)
            {
                state.ui.selected_track_index = next_idx;
            } else {
                state.ui.selected_track_index = state.ui.search_matches[0];
            }
        }
        KeyCode::Char('N') if !state.ui.search_matches.is_empty() => {
            if let Some(&prev_idx) = state
                .ui
                .search_matches
                .iter()
                .rev()
                .find(|&&i| i < state.ui.selected_track_index)
            {
                state.ui.selected_track_index = prev_idx;
            } else {
                state.ui.selected_track_index =
                    *state.ui.search_matches.last().unwrap();
            }
        }
        KeyCode::Char('d') | KeyCode::Char('x')
            if state.ui.active_view == ActiveView::Library =>
        {
            if state.ui.active_library_tab == crate::app::LibraryTab::Albums {
                if key.code == KeyCode::Char('d')
                    && state.ui.selected_playlist_index
                        < state.data.saved_albums.len()
                {
                    let album =
                        &state.data.saved_albums[state.ui.selected_playlist_index];
                    if state.ui.pending_d_press {
                        state.ui.album_mass_delete_prompt =
                            Some(vec![album.id.clone()]);
                        state.ui.pending_d_press = false;
                    } else {
                        state.ui.pending_d_press = true;
                    }
                }
                return None;
            }
            if state.ui.selected_playlist_index < state.data.library_view.len() {
                match &state.data.library_view[state.ui.selected_playlist_index] {
                    crate::models::LibraryNode::Playlist { playlist, .. } => {
                        if playlist.id == "LIKED_SONGS" {
                            return None;
                        }
                        if playlist.id == "local-library" {
                            return None;
                        }
                        if playlist.id.starts_with("local-playlist:") {
                            if key.code == KeyCode::Char('d') {
                                if state.ui.pending_d_press {
                                    state.ui.playlist_delete_prompt =
                                        Some(vec![playlist.id.clone()]);
                                    state.ui.pending_d_press = false;
                                } else {
                                    state.ui.pending_d_press = true;
                                }
                            }
                            return None;
                        }

                        if key.code == KeyCode::Char('x') {
                            state.ui.operation_register =
                                vec![playlist.id.clone()];

                            for f in &mut state.ui.library_config.folders {
                                f.playlists.retain(|id| id != &playlist.id);
                            }
                            state.save_library_config();
                            state.compute_library_view();
                        } else if key.code == KeyCode::Char('d')
                            && Some(&playlist.owner_id)
                                == state.data.user_id.as_ref()
                        {
                            if state.ui.pending_d_press {
                                state.ui.playlist_delete_prompt =
                                    Some(vec![playlist.id.clone()]);
                                state.ui.pending_d_press = false;
                            } else {
                                state.ui.pending_d_press = true;
                            }
                        }
                    }
                    crate::models::LibraryNode::Folder(f) => {
                        if key.code == KeyCode::Char('d') {
                            if state.ui.pending_d_press {
                                state.ui.folder_delete_prompt =
                                    Some(f.name.clone());
                                state.ui.pending_d_press = false;
                            } else {
                                state.ui.pending_d_press = true;
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('d') => {
            if state.ui.active_view == ActiveView::TrackList {
                tracklist::mark_selected_for_delete(state);
            }
        }
        KeyCode::Char('A') => {
            let ctx = if state.ui.active_view == ActiveView::TrackList
                && state.ui.selected_track_index < state.data.tracks.len()
            {
                let t = &state.data.tracks[state.ui.selected_track_index];
                Some(crate::models::ActionMenuContext {
                    track_id: t.id.clone(),
                    source: t.source,
                    track_name: t.name.clone(),
                    album_id: t.album_id.clone(),
                    artist_id: t.artist_id.clone(),
                    artist_name: t.artist.clone(),
                })
            } else if state.ui.active_view == ActiveView::Queue
                && state.ui.selected_queue_index < state.data.queue.len()
            {
                let t = &state.data.queue[state.ui.selected_queue_index];
                Some(crate::models::ActionMenuContext {
                    track_id: t.id.clone(),
                    source: t.source,
                    track_name: t.name.clone(),
                    album_id: t.album_id.clone(),
                    artist_id: t.artist_id.clone(),
                    artist_name: t.artist.clone(),
                })
            } else if state.ui.active_view == ActiveView::SearchResults
                && state.ui.active_search_tab == crate::app::SearchTab::Tracks
                && state.ui.selected_search_index
                    < state.data.search_results.tracks.len()
            {
                let t = &state.data.search_results.tracks
                    [state.ui.selected_search_index];
                Some(crate::models::ActionMenuContext {
                    track_id: t.id.clone(),
                    source: t.source,
                    track_name: t.name.clone(),
                    album_id: t.album_id.clone(),
                    artist_id: t.artist_id.clone(),
                    artist_name: t.artist.clone(),
                })
            } else if !state.playback.playing_track_id.is_none() {
                Some(crate::models::ActionMenuContext {
                    track_id: state
                        .playback
                        .playing_track_id
                        .clone()
                        .unwrap_or_default(),
                    source: crate::models::TrackSource::Spotify,
                    track_name: state.playback.playing_track_title.clone(),
                    album_id: state.playback.playing_track_album_id.clone(),
                    artist_id: state.playback.playing_track_artist_id.clone(),
                    artist_name: state.playback.playing_track_artist.clone(),
                })
            } else {
                None
            };

            if let Some(ctx) = ctx {
                state.ui.action_menu_context = Some(ctx);
                state.ui.action_menu_open = true;
                state.ui.selected_action_index = 0;
            }
        }
        KeyCode::Char('a') => {
            if state.ui.active_view == ActiveView::SearchResults
                && state.ui.active_search_tab == crate::app::SearchTab::Albums
            {
                if state.ui.selected_search_index
                    < state.data.search_results.albums.len()
                {
                    let album = &state.data.search_results.albums
                        [state.ui.selected_search_index];
                    state.ui.status_message = Some(
                        crate::i18n::t(
                            "messages.saved_to_library",
                            &state.ui.library_config.language,
                        )
                        .replace("{}", &album.name),
                    );
                    state.ui.status_message_expiry = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_secs(3),
                    );
                    return Some(AppEvent::SaveAlbums(vec![album.id.clone()]));
                }
            } else {
                state.ui.playlist_add_modal_open = true;
                state.ui.selected_playlist_modal_index = 0;
            }
        }
        KeyCode::Char('p')
            if state.ui.active_view == ActiveView::Library
                && !state.ui.operation_register.is_empty()
                && state.ui.selected_playlist_index
                    < state.data.library_view.len() =>
        {
            let node =
                &state.data.library_view[state.ui.selected_playlist_index];
            match node {
                crate::models::LibraryNode::Folder(f) => {
                    let folder_name = f.name.clone();
                    if let Some(folder) = state
                        .ui
                        .library_config
                        .folders
                        .iter_mut()
                        .find(|fd| fd.name == folder_name)
                    {
                        for id in &state.ui.operation_register {
                            if !folder.playlists.contains(id) {
                                folder.playlists.push(id.clone());
                            }
                        }
                    }
                    for id in &state.ui.operation_register {
                        state.ui.library_config.pinned.retain(|p| p != id);
                    }
                }
                crate::models::LibraryNode::Playlist { .. } => {}
            }
            state.ui.operation_register.clear();
            state.save_library_config();
            state.compute_library_view();
        }
        KeyCode::Char('m') if state.ui.active_view == ActiveView::Library => {
            if state.ui.active_library_tab == crate::app::LibraryTab::Albums {
                return None;
            }
            if state.ui.selected_playlist_index < state.data.library_view.len()
                && let crate::models::LibraryNode::Playlist { playlist, .. } =
                    &state.data.library_view[state.ui.selected_playlist_index]
            {
                let id = &playlist.id;
                if id == "LIKED_SONGS" || id == "local-library" {
                    return None;
                }
                if state.ui.library_config.pinned.contains(id) {
                    state.ui.library_config.pinned.retain(|p| p != id);
                } else {
                    state.ui.library_config.pinned.push(id.clone());
                }
                state.save_library_config();
                state.compute_library_view();
            }
        }
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Backspace => {
            if state.ui.active_view == ActiveView::TrackList {
                if search_has_results(state) {
                    state.ui.active_view = ActiveView::SearchResults;
                } else {
                    state.ui.active_view = ActiveView::Library;
                }
            } else if state.ui.active_view == ActiveView::Queue
                || state.ui.active_view == ActiveView::ArtistList
            {
                state.ui.active_view = ActiveView::Library;
                if state.data.tracklist_image_url.is_some() {
                    return Some(AppEvent::ReloadHeaderImage);
                }
            } else if state.ui.active_view == ActiveView::ArtistPage {
                if search_has_results(state) {
                    state.ui.active_view = ActiveView::SearchResults;
                    state.clear_pending_artist_page();
                    return Some(AppEvent::CancelArtistPageLoad);
                }
                return Some(artist_page::back_to_artist_list(state));
            } else if state.ui.active_view == ActiveView::SearchResults {
                state.ui.active_view = ActiveView::Library;
                state.data.search_results = crate::models::SearchResults::default();
                state.ui.search_context_query.clear();
                state.ui.status_message = None;
                if state.data.tracklist_image_url.is_some() {
                    return Some(AppEvent::ReloadHeaderImage);
                }
            }
        }
        KeyCode::Char('q') => {
            let track_id = if state.ui.active_view == ActiveView::TrackList {
                state
                    .data
                    .tracks
                    .get(state.ui.selected_track_index)
                    .map(|t| t.id.clone())
            } else if state.ui.active_view == ActiveView::SearchResults {
                if state.ui.active_search_tab == crate::app::SearchTab::Tracks {
                    state
                        .data
                        .search_results
                        .tracks
                        .get(state.ui.selected_search_index)
                        .map(|t| t.id.clone())
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(id) = track_id {
                return Some(AppEvent::AddToQueue(vec![id]));
            }
        }
        KeyCode::Char('Q') => {
            state.ui.active_view = ActiveView::Queue;
            state.ui.selected_queue_index = 0;
            return Some(AppEvent::FetchQueue);
        }
        KeyCode::Char('D') => {
            state.ui.device_modal_open = true;
            state.ui.selected_device_index = 0;
            return Some(AppEvent::FetchDevices);
        }
        KeyCode::Char(' ') => {
            state.playback.is_playing = !state.playback.is_playing;
            state.playback.playback_last_updated_at = Some(std::time::Instant::now());
            return Some(AppEvent::TogglePlayback(state.playback.is_playing));
        }
        KeyCode::Char('c') if state.ui.active_view == ActiveView::Library => {
            state.ui.mode = crate::app::AppMode::Command;
            state.ui.command_buffer = "newplaylist ".to_string();
        }
        KeyCode::Char('e') => {
            if state.ui.active_view == ActiveView::Library
                && let Some(node) =
                    state.data.library_view.get(state.ui.selected_playlist_index)
            {
                match node {
                    crate::models::LibraryNode::Playlist { playlist, .. } => {
                        state.ui.mode = crate::app::AppMode::Command;
                        state.ui.command_buffer =
                            format!("rename {}", playlist.name);
                    }
                    crate::models::LibraryNode::Folder(f) => {
                        state.ui.mode = crate::app::AppMode::Command;
                        state.ui.command_buffer = format!("rename {}", f.name);
                    }
                }
            }
        }
        KeyCode::Char('s') => {
            state.playback.is_shuffled = !state.playback.is_shuffled;
            return Some(AppEvent::ToggleShuffle(state.playback.is_shuffled));
        }
        KeyCode::Char('v') => {
            state.ui.mode = crate::app::AppMode::Visual;
            let current_idx = match state.ui.active_view {
                ActiveView::TrackList => state.ui.selected_track_index,
                ActiveView::SearchResults => state.ui.selected_search_index,
                ActiveView::Queue => state.ui.selected_queue_index,
                ActiveView::Library => state.ui.selected_playlist_index,
                ActiveView::Devices => state.ui.selected_device_index,
                ActiveView::ArtistList => state.ui.selected_artist_index,
                ActiveView::ArtistPage => state.ui.artist_page_album_index,
            };
            state.ui.visual_selection_start = Some(current_idx);
            state.ui.status_message = Some(crate::i18n::t(
                "messages.visual_block",
                &state.ui.library_config.language,
            ));
        }
        KeyCode::Char(']') | KeyCode::Char('>') => {
            state.playback.progress_ms = 0;
            state.playback.duration_ms = 0;
            state.playback.playback_last_updated_at = Some(std::time::Instant::now());
            return Some(AppEvent::NextTrack {
                current_track_id: state.playback.playing_track_id.clone(),
            });
        }
        KeyCode::Char('[') | KeyCode::Char('<') => {
            state.playback.progress_ms = 0;
            state.playback.duration_ms = 0;
            state.playback.playback_last_updated_at = Some(std::time::Instant::now());
            return Some(AppEvent::PreviousTrack {
                current_track_id: state.playback.playing_track_id.clone(),
            });
        }
        KeyCode::Char('R') => {
            if state.ui.active_view == ActiveView::ArtistPage
                && let Some(data) = state.data.artist_page_data.as_ref()
            {
                if state.data.artist_albums_loading {
                    state.ui.status_message =
                        Some("Artist albums refresh already in progress.".to_string());
                    state.ui.status_message_expiry = Some(
                        std::time::Instant::now()
                            + std::time::Duration::from_secs(3),
                    );
                    return None;
                }
                state.data.artist_albums_loading = true;
                state.ui.status_message =
                    Some("Refreshing artist albums...".to_string());
                return Some(AppEvent::RefreshArtistAlbums {
                    artist_id: data.artist_id.clone(),
                });
            } else if state.ui.active_view == ActiveView::Library {
                state.ui.status_message =
                    Some("Refreshing library...".to_string());
                return Some(AppEvent::RefreshLibraryLists);
            }
        }
        KeyCode::Char('r') => {
            let next_mode = match state.playback.repeat_mode.as_str() {
                "Off" => "Track",
                "Track" => "Context",
                _ => "Off",
            };
            state.playback.repeat_mode = next_mode.to_string();
            return Some(AppEvent::SetRepeatMode(next_mode.to_string()));
        }
        KeyCode::Char('=') => {
            let next_vol = (state.playback.volume + 1).min(100);
            state.playback.volume = next_vol;
            state.save_volume();
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Char('-') => {
            let next_vol = state.playback.volume.saturating_sub(1);
            state.playback.volume = next_vol;
            state.save_volume();
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Char('+') => {
            let next_vol = (state.playback.volume + 5).min(100);
            state.playback.volume = next_vol;
            state.save_volume();
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Char('_') => {
            let next_vol = state.playback.volume.saturating_sub(5);
            state.playback.volume = next_vol;
            state.save_volume();
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Tab => {
            if state.ui.active_view == ActiveView::Library {
                state.ui.active_library_tab = match state.ui.active_library_tab {
                    crate::app::LibraryTab::Playlists => {
                        crate::app::LibraryTab::Albums
                    }
                    crate::app::LibraryTab::Albums => {
                        crate::app::LibraryTab::Browse
                    }
                    crate::app::LibraryTab::Browse => {
                        crate::app::LibraryTab::Playlists
                    }
                };
                state.ui.selected_playlist_index = 0;
                if state.ui.active_library_tab == crate::app::LibraryTab::Browse {
                    state.ui.active_browse_node =
                        crate::models::BrowseNode::TopTracks;
                    return browse::load_event_if_needed(state);
                }
            } else if state.ui.active_view == ActiveView::SearchResults {
                state.ui.active_search_tab = match state.ui.active_search_tab {
                    crate::app::SearchTab::Tracks => {
                        crate::app::SearchTab::Albums
                    }
                    crate::app::SearchTab::Albums => {
                        crate::app::SearchTab::Artists
                    }
                    crate::app::SearchTab::Artists => {
                        crate::app::SearchTab::Tracks
                    }
                };
                state.ui.selected_search_index = 0;
            }
        }
        _ => {}
    }
    None
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn search_track_play_event(
    state: &AppState,
    track: &SearchTrack,
) -> Option<AppEvent> {
    let playback_track = Track {
        id: track.id.clone(),
        source: track.source,
        local_path: track.local_path.clone(),
        name: track.name.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
        artist_id: track.artist_id.clone(),
    };
    let target = if track.source == crate::models::TrackSource::Local {
        let tracks: Vec<_> = state
            .data
            .search_results
            .tracks
            .iter()
            .filter(|result| result.source == crate::models::TrackSource::Local)
            .map(|t| Track {
                id: t.id.clone(),
                source: t.source,
                local_path: t.local_path.clone(),
                name: t.name.clone(),
                artist: t.artist.clone(),
                duration_ms: t.duration_ms,
                image_url: t.image_url.clone(),
                album_id: t.album_id.clone(),
                artist_id: t.artist_id.clone(),
            })
            .collect();
        let selected_index = tracks
            .iter()
            .position(|candidate| candidate.id == track.id)
            .unwrap_or(0);
        crate::models::PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        }
    } else {
        crate::models::PlaybackTarget::SpotifyTrack {
            track_id: track.id.clone(),
        }
    };

    Some(AppEvent::PlayTrack {
        target,
        track_id: playback_track.id,
        title: playback_track.name,
        artist: playback_track.artist,
        duration_ms: playback_track.duration_ms,
        image_url: playback_track.image_url,
        album_id: playback_track.album_id,
    })
}

fn search_results_len(state: &AppState) -> usize {
    match state.ui.active_search_tab {
        crate::app::SearchTab::Tracks => state.data.search_results.tracks.len(),
        crate::app::SearchTab::Albums => state.data.search_results.albums.len(),
        crate::app::SearchTab::Artists => state.data.search_results.artists.len(),
    }
}

fn search_has_results(state: &AppState) -> bool {
    !state.data.search_results.tracks.is_empty()
        || !state.data.search_results.albums.is_empty()
        || !state.data.search_results.artists.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{PlaybackTarget, TrackSource};
    use std::path::PathBuf;

    #[test]
    fn local_search_track_playback_uses_local_context() {
        let mut state = AppState::new();
        state.data.search_results.tracks = vec![
            SearchTrack {
                id: "spotify".to_string(),
                source: TrackSource::Spotify,
                local_path: None,
                name: "Spotify".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                duration_ms: 1,
                image_url: None,
                album_id: None,
                artist_id: None,
            },
            SearchTrack {
                id: "local:a".to_string(),
                source: TrackSource::Local,
                local_path: Some(PathBuf::from("/music/a.wav")),
                name: "Local A".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                duration_ms: 1,
                image_url: None,
                album_id: None,
                artist_id: None,
            },
            SearchTrack {
                id: "local:b".to_string(),
                source: TrackSource::Local,
                local_path: Some(PathBuf::from("/music/b.wav")),
                name: "Local B".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                duration_ms: 1,
                image_url: None,
                album_id: None,
                artist_id: None,
            },
        ];

        let Some(AppEvent::PlayTrack {
            target,
            track_id,
            ..
        }) = search_track_play_event(
            &state,
            &state.data.search_results.tracks[2],
        )
        else {
            panic!("expected play event");
        };

        assert_eq!(track_id, "local:b");
        let PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        } = target
        else {
            panic!("expected local context");
        };
        assert_eq!(tracks.len(), 2);
        assert_eq!(selected_index, 1);
    }
}
