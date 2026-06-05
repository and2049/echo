use crate::app::{ActiveView, AppState};
use crate::events::AppEvent;
use crate::handlers::{artist_page, browse, tracklist};
use crate::models::{Playlist, SearchTrack, Track, TrackListContext};
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    if let Some(folder_name) = state.folder_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state
                .library_config
                .folders
                .retain(|fd| fd.name != folder_name);
            state.save_library_config();
            state.compute_library_view();

            // Adjust selection index if it goes out of bounds
            if state.selected_playlist_index >= state.library_view.len() {
                state.selected_playlist_index = state.library_view.len().saturating_sub(1);
            }
        }
        state.folder_delete_prompt = None;
        state.playlist_delete_prompt = None;
        return None;
    }

    if let Some(playlist_ids) = state.playlist_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.playlist_delete_prompt = None;
            return Some(AppEvent::DeletePlaylists(playlist_ids));
        }
        state.playlist_delete_prompt = None;
        return None;
    }

    if let Some(album_ids) = state.album_mass_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.album_mass_delete_prompt = None;
            return Some(AppEvent::RemoveAlbums(album_ids));
        }
        state.album_mass_delete_prompt = None;
        return None;
    }

    if let Some((playlist_id, track_ids)) = state.track_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.track_delete_prompt = None;
            return Some(AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids));
        }
        state.track_delete_prompt = None;
        return None;
    }

    if let Some(track_id) = state.liked_track_remove_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.liked_track_remove_prompt = None;
            state.liked_tracks.remove(&track_id);

            let mut cache = crate::config::AppConfig::load_cache();
            cache.liked_tracks = state.liked_tracks.clone();
            let _ = crate::config::AppConfig::save_cache(&cache);

            return Some(AppEvent::ToggleTrackLike(track_id, false));
        }
        state.liked_track_remove_prompt = None;
        return None;
    }

    if state.playlist_add_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.playlist_add_modal_open = false;
                state.selected_playlist_modal_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.selected_playlist_modal_index + 1 < playlist_modal_choices(state).len() {
                    state.selected_playlist_modal_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if state.selected_playlist_modal_index > 0 => {
                state.selected_playlist_modal_index -= 1;
            }
            KeyCode::Enter => {
                let playlists = playlist_modal_choices(state);
                if let Some(playlist) = playlists.get(state.selected_playlist_modal_index) {
                    // If operation_register was populated (e.g. from action menu), use it directly.
                    let tracks = if !state.operation_register.is_empty() {
                        let ids: Vec<_> = state.operation_register.drain(..).collect();
                        resolve_tracks_by_ids(state, &ids)
                    } else {
                        selected_tracks_for_playlist(state)
                    };
                    state.playlist_add_modal_open = false;
                    state.selected_playlist_modal_index = 0;
                    if !tracks.is_empty() {
                        return Some(AppEvent::AddTracksToPlaylist(playlist.id.clone(), tracks));
                    }
                }
            }
            _ => {}
        }
        return None;
    }

    if state.device_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.device_modal_open = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.selected_device_index + 1 < state.devices.len() {
                    state.selected_device_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.selected_device_index > 0 {
                    state.selected_device_index -= 1;
                }
            }
            KeyCode::Enter => {
                if let Some(device) = state.devices.get(state.selected_device_index) {
                    state.device_modal_open = false;
                    let id = device.id.clone();
                    if !id.is_empty() {
                        return Some(AppEvent::TransferPlayback(id));
                    }
                }
            }
            _ => {}
        }
        return None;
    }

    if state.lyrics_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('L') => {
                state.lyrics_modal_open = false;
            }
            _ => {}
        }
        return None;
    }

    // Action menu intercepts all input while open
    if state.action_menu_open {
        const ACTION_COUNT: usize = 5;
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.action_menu_open = false;
                state.action_menu_context = None;
                state.selected_action_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.selected_action_index + 1 < ACTION_COUNT {
                    state.selected_action_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.selected_action_index > 0 {
                    state.selected_action_index -= 1;
                }
            }
            KeyCode::Enter => {
                if let Some(ctx) = state.action_menu_context.clone() {
                    let action_idx = state.selected_action_index;
                    state.action_menu_open = false;
                    state.action_menu_context = None;
                    state.selected_action_index = 0;
                    match action_idx {
                        0 => {
                            // Go to Album
                            if ctx.source == crate::models::TrackSource::Local {
                                if let Some(album) = state
                                    .local_library
                                    .tracks
                                    .iter()
                                    .find(|track| track.id == ctx.track_id)
                                    .map(|track| track.album.clone())
                                    .filter(|album| !album.is_empty())
                                {
                                    let tracks: Vec<_> = state
                                        .local_library
                                        .to_tracks()
                                        .into_iter()
                                        .filter(|track| {
                                            state
                                                .local_library
                                                .tracks
                                                .iter()
                                                .find(|local| local.id == track.id)
                                                .is_some_and(|local| local.album == album)
                                        })
                                        .collect();
                                    if !tracks.is_empty() {
                                        state.show_generated_tracks(
                                            tracks,
                                            TrackListContext::generated(
                                                format!("local-album:{album}"),
                                                album,
                                            ),
                                        );
                                    }
                                }
                            } else if let Some(album_id) = ctx.album_id {
                                return Some(AppEvent::LoadContextTracks(
                                    crate::models::TrackListContext::album(
                                        album_id,
                                        String::new(),
                                        String::new(),
                                        None,
                                    ),
                                ));
                            }
                        }
                        1 => {
                            // Go to Artist
                            if ctx.source == crate::models::TrackSource::Local
                                && !ctx.artist_name.is_empty()
                            {
                                let artist = ctx.artist_name.clone();
                                let tracks: Vec<_> = state
                                    .local_library
                                    .to_tracks()
                                    .into_iter()
                                    .filter(|track| track.artist == artist)
                                    .collect();
                                if !tracks.is_empty() {
                                    state.show_generated_tracks(
                                        tracks,
                                        TrackListContext::generated(
                                            format!("local-artist:{artist}"),
                                            artist,
                                        ),
                                    );
                                }
                            } else if let Some(artist_id) = ctx.artist_id {
                                // Must call begin_artist_page_load first — ArtistPageOpened
                                // is gated on pending_artist_page_id matching the artist id.
                                state.begin_artist_page_load(
                                    artist_id.clone(),
                                    ctx.artist_name.clone(),
                                    None,
                                );
                                return Some(AppEvent::LoadArtistPage {
                                    artist_id,
                                    artist_name: Some(ctx.artist_name),
                                    artist_image_url: None,
                                });
                            }
                        }
                        2 => {
                            // Add to Playlist
                            state.action_menu_context = None;
                            // Store track id so the playlist modal can find it
                            state.operation_register = vec![ctx.track_id];
                            state.playlist_add_modal_open = true;
                            state.selected_playlist_modal_index = 0;
                        }
                        3 => {
                            // Add to Queue
                            return Some(AppEvent::AddToQueue(vec![ctx.track_id]));
                        }
                        4 => {
                            // Like / Unlike Track
                            let is_liked = state.liked_tracks.contains(&ctx.track_id);
                            if is_liked {
                                state.liked_tracks.remove(&ctx.track_id);
                            } else {
                                state.liked_tracks.insert(ctx.track_id.clone());
                            }
                            return Some(AppEvent::ToggleTrackLike(ctx.track_id, !is_liked));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        return None;
    }

    if key.code != KeyCode::Char('d') {
        state.pending_d_press = false;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => match state.active_view {
            ActiveView::Library => {
                if state.active_library_tab == crate::app::LibraryTab::Browse {
                    if state.selected_playlist_index < 2 {
                        state.selected_playlist_index += 1;
                    }
                    browse::select_node_from_library_index(state);
                    return browse::load_event_if_needed(state);
                } else {
                    let max_len = if state.active_library_tab == crate::app::LibraryTab::Albums {
                        state.saved_albums.len()
                    } else {
                        state.library_view.len()
                    };
                    if max_len > 0 && state.selected_playlist_index < max_len.saturating_sub(1) {
                        state.selected_playlist_index += 1;
                    }
                }
            }
            ActiveView::TrackList => {
                if state.selected_track_index < state.tracks.len().saturating_sub(1) {
                    state.selected_track_index += 1;
                }
            }
            ActiveView::ArtistList => {
                if !state.followed_artists.is_empty()
                    && state.selected_artist_index < state.followed_artists.len().saturating_sub(1)
                {
                    state.selected_artist_index += 1;
                }
            }
            ActiveView::ArtistPage => {
                if let Some(ref data) = state.artist_page_data {
                    if !data.albums.is_empty()
                        && state.artist_page_album_index < data.albums.len().saturating_sub(1)
                    {
                        state.artist_page_album_index += 1;
                    }
                }
            }
            ActiveView::SearchResults => {
                let max = search_results_len(state);
                if max > 0 && state.selected_search_index < max.saturating_sub(1) {
                    state.selected_search_index += 1;
                }
            }
            ActiveView::Queue => {
                if !state.queue.is_empty()
                    && state.selected_queue_index < state.queue.len().saturating_sub(1)
                {
                    state.selected_queue_index += 1;
                }
            }
            ActiveView::Devices => {
                if !state.devices.is_empty()
                    && state.selected_device_index < state.devices.len().saturating_sub(1)
                {
                    state.selected_device_index += 1;
                }
            }
        },
        KeyCode::Char('k') | KeyCode::Up => match state.active_view {
            ActiveView::Library => {
                if state.active_library_tab == crate::app::LibraryTab::Browse {
                    if state.selected_playlist_index > 0 {
                        state.selected_playlist_index -= 1;
                    }
                    browse::select_node_from_library_index(state);
                    return browse::load_event_if_needed(state);
                } else {
                    if state.selected_playlist_index > 0 {
                        state.selected_playlist_index -= 1;
                    }
                }
            }
            ActiveView::TrackList => {
                if state.selected_track_index > 0 {
                    state.selected_track_index -= 1;
                }
            }
            ActiveView::ArtistList => {
                if state.selected_artist_index > 0 {
                    state.selected_artist_index -= 1;
                }
            }
            ActiveView::ArtistPage => {
                if state.artist_page_album_index > 0 {
                    state.artist_page_album_index -= 1;
                }
            }
            ActiveView::SearchResults => {
                if state.selected_search_index > 0 {
                    state.selected_search_index -= 1;
                }
            }
            ActiveView::Queue => {
                if state.selected_queue_index > 0 {
                    state.selected_queue_index -= 1;
                }
            }
            ActiveView::Devices => {
                if state.selected_device_index > 0 {
                    state.selected_device_index -= 1;
                }
            }
        },
        KeyCode::Char('l') => {
            if state.active_view == ActiveView::Library {
                return crate::handlers::normal::handle_key(
                    state,
                    &KeyEvent::new(KeyCode::Enter, key.modifiers),
                );
            } else if state.active_view == ActiveView::TrackList {
                if state.selected_track_index < state.tracks.len() {
                    let track = &state.tracks[state.selected_track_index];
                    let track_id = track.id.clone();
                    if state.liked_tracks.contains(&track_id) {
                        state.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.liked_tracks.insert(track_id.clone());
                        state.status_message = Some(crate::i18n::t(
                            "messages.added_to_liked",
                            &state.library_config.language,
                        ));
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            } else if state.active_view == ActiveView::Queue {
                if state.selected_queue_index < state.queue.len() {
                    let track = &state.queue[state.selected_queue_index];
                    let track_id = track.id.clone();
                    if state.liked_tracks.contains(&track_id) {
                        state.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.liked_tracks.insert(track_id.clone());
                        state.status_message = Some(crate::i18n::t(
                            "messages.added_to_liked",
                            &state.library_config.language,
                        ));
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            } else if state.active_view == ActiveView::SearchResults
                && state.active_search_tab == crate::app::SearchTab::Tracks
            {
                let i = state.selected_search_index;
                if let Some(track) = state.search_results.tracks.get(i) {
                    let track_id = track.id.clone();
                    if state.liked_tracks.contains(&track_id) {
                        state.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.liked_tracks.insert(track_id.clone());

                        let mut cache = crate::config::AppConfig::load_cache();
                        cache.liked_tracks = state.liked_tracks.clone();
                        let _ = crate::config::AppConfig::save_cache(&cache);

                        state.status_message = Some(crate::i18n::t(
                            "messages.added_to_liked",
                            &state.library_config.language,
                        ));
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
                state.condensed_lyrics_enabled = !state.condensed_lyrics_enabled;
                let mut app_config = crate::config::AppConfig::load();
                app_config.library.condensed_lyrics_enabled = state.condensed_lyrics_enabled;
                let _ = app_config.save();
            } else {
                state.lyrics_modal_open = !state.lyrics_modal_open;
            }
        }
        KeyCode::Enter | KeyCode::Char('z') => {
            if state.active_view == ActiveView::Library {
                let context = if state.active_library_tab == crate::app::LibraryTab::Albums {
                    if state.selected_playlist_index < state.saved_albums.len() {
                        let album = &state.saved_albums[state.selected_playlist_index];
                        Some(TrackListContext::album(
                            album.id.clone(),
                            album.name.clone(),
                            album.artists.clone(),
                            album.image_url.clone(),
                        ))
                    } else {
                        None
                    }
                } else if state.active_library_tab == crate::app::LibraryTab::Browse {
                    if let Some(event) = browse::enter_active_node(state) {
                        return Some(event);
                    }
                    None
                } else {
                    if let Some(node) = state
                        .library_view
                        .get(state.selected_playlist_index)
                        .cloned()
                    {
                        match node {
                            crate::models::LibraryNode::Playlist { playlist, .. } => {
                                if playlist.id == "local-library" {
                                    state.show_local_library();
                                    None
                                } else if playlist.id.starts_with("local-playlist:") {
                                    state.show_local_playlist(&playlist.id, playlist.name.clone());
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
            } else if state.active_view == ActiveView::TrackList {
                return tracklist::play_selected(state);
            } else if state.active_view == ActiveView::SearchResults {
                let i = state.selected_search_index;
                match state.active_search_tab {
                    crate::app::SearchTab::Tracks => {
                        if let Some(t) = state.search_results.tracks.get(i) {
                            // Play track directly (no context — use URI playback)
                            let track_id = t.id.clone();
                            return Some(AppEvent::PlayTrack {
                                target: crate::models::PlaybackTarget::SpotifyTrack {
                                    track_id: track_id.clone(),
                                },
                                track_id,
                                title: t.name.clone(),
                                artist: t.artist.clone(),
                                duration_ms: t.duration_ms,
                                image_url: t.image_url.clone(),
                                album_id: t.album_id.clone(),
                            });
                        }
                    }
                    crate::app::SearchTab::Albums => {
                        if let Some(album) = state.search_results.albums.get(i) {
                            let context = TrackListContext::album(
                                album.id.clone(),
                                album.name.clone(),
                                album.artist.clone(),
                                album.image_url.clone(),
                            );
                            state.prev_view = None; // album loaded from search, Backspace returns to Search
                            state.begin_tracklist_load(context.clone());
                            return Some(AppEvent::LoadContextTracks(context));
                        }
                    }
                    crate::app::SearchTab::Artists => {
                        return artist_page::enter_search_artist(state);
                    }
                }
            } else if state.active_view == ActiveView::ArtistList {
                return artist_page::enter_followed_artist(state);
            } else if state.active_view == ActiveView::ArtistPage {
                return artist_page::enter_artist_page_selection(state);
            }
        }
        KeyCode::Char(':') => {
            state.mode = crate::app::AppMode::Command;
            state.command_buffer.clear();
            state.status_message = None;
        }
        KeyCode::Char('/') => {
            state.mode = crate::app::AppMode::Search;
            state.search_query.clear();
            state.search_matches.clear();
            state.status_message = None;
        }
        KeyCode::Char('f') => {
            state.mode = crate::app::AppMode::Command;
            state.command_buffer = "search ".to_string();
            state.status_message = None;
        }
        KeyCode::Char('n') if !state.search_matches.is_empty() => {
            // Find the first match index that is greater than the current selected_track_index
            if let Some(&next_idx) = state
                .search_matches
                .iter()
                .find(|&&i| i > state.selected_track_index)
            {
                state.selected_track_index = next_idx;
            } else {
                // Wrap around to the first match
                state.selected_track_index = state.search_matches[0];
            }
        }
        KeyCode::Char('N') if !state.search_matches.is_empty() => {
            // Find the last match index that is less than the current selected_track_index
            if let Some(&prev_idx) = state
                .search_matches
                .iter()
                .rev()
                .find(|&&i| i < state.selected_track_index)
            {
                state.selected_track_index = prev_idx;
            } else {
                // Wrap around to the last match
                state.selected_track_index = *state.search_matches.last().unwrap();
            }
        }
        KeyCode::Char('d') | KeyCode::Char('x') if state.active_view == ActiveView::Library => {
            if state.active_library_tab == crate::app::LibraryTab::Albums {
                if key.code == KeyCode::Char('d')
                    && state.selected_playlist_index < state.saved_albums.len()
                {
                    let album = &state.saved_albums[state.selected_playlist_index];
                    if state.pending_d_press {
                        state.album_mass_delete_prompt = Some(vec![album.id.clone()]);
                        state.pending_d_press = false;
                    } else {
                        state.pending_d_press = true;
                    }
                }
                return None;
            }
            if state.selected_playlist_index < state.library_view.len() {
                match &state.library_view[state.selected_playlist_index] {
                    crate::models::LibraryNode::Playlist { playlist, .. } => {
                        if playlist.id == "LIKED_SONGS" {
                            return None;
                        }
                        if playlist.id == "local-library" {
                            return None;
                        }
                        if playlist.id.starts_with("local-playlist:") {
                            if key.code == KeyCode::Char('d') {
                                if state.pending_d_press {
                                    state.playlist_delete_prompt = Some(vec![playlist.id.clone()]);
                                    state.pending_d_press = false;
                                } else {
                                    state.pending_d_press = true;
                                }
                            }
                            return None;
                        }

                        if key.code == KeyCode::Char('x') {
                            // Put in cut register
                            state.operation_register = vec![playlist.id.clone()];

                            // Remove from any folders
                            for f in &mut state.library_config.folders {
                                f.playlists.retain(|id| id != &playlist.id);
                            }
                            state.save_library_config();
                            state.compute_library_view();
                        } else if key.code == KeyCode::Char('d')
                            && Some(&playlist.owner_id) == state.user_id.as_ref()
                        {
                            if state.pending_d_press {
                                state.playlist_delete_prompt = Some(vec![playlist.id.clone()]);
                                state.pending_d_press = false;
                            } else {
                                state.pending_d_press = true;
                            }
                        }
                    }
                    crate::models::LibraryNode::Folder(f) => {
                        if key.code == KeyCode::Char('d') {
                            if state.pending_d_press {
                                state.folder_delete_prompt = Some(f.name.clone());
                                state.pending_d_press = false;
                            } else {
                                state.pending_d_press = true;
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('d') => {
            if state.active_view == ActiveView::TrackList {
                tracklist::mark_selected_for_delete(state);
            }
        }
        KeyCode::Char('A') => {
            // Build context from the hovered track (if in a track-list view)
            // or from the currently playing track (if in library/other views).
            let ctx = if state.active_view == ActiveView::TrackList
                && state.selected_track_index < state.tracks.len()
            {
                let t = &state.tracks[state.selected_track_index];
                Some(crate::models::ActionMenuContext {
                    track_id: t.id.clone(),
                    source: t.source,
                    track_name: t.name.clone(),
                    album_id: t.album_id.clone(),
                    artist_id: t.artist_id.clone(),
                    artist_name: t.artist.clone(),
                })
            } else if state.active_view == ActiveView::Queue
                && state.selected_queue_index < state.queue.len()
            {
                let t = &state.queue[state.selected_queue_index];
                Some(crate::models::ActionMenuContext {
                    track_id: t.id.clone(),
                    source: t.source,
                    track_name: t.name.clone(),
                    album_id: t.album_id.clone(),
                    artist_id: t.artist_id.clone(),
                    artist_name: t.artist.clone(),
                })
            } else if state.active_view == ActiveView::SearchResults
                && state.active_search_tab == crate::app::SearchTab::Tracks
                && state.selected_search_index < state.search_results.tracks.len()
            {
                let t = &state.search_results.tracks[state.selected_search_index];
                Some(crate::models::ActionMenuContext {
                    track_id: t.id.clone(),
                    source: t.source,
                    track_name: t.name.clone(),
                    album_id: t.album_id.clone(),
                    artist_id: t.artist_id.clone(),
                    artist_name: t.artist.clone(),
                })
            } else if !state.playback.playing_track_id.is_none() {
                // Library / other views — use the currently playing track
                Some(crate::models::ActionMenuContext {
                    track_id: state.playback.playing_track_id.clone().unwrap_or_default(),
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
                state.action_menu_context = Some(ctx);
                state.action_menu_open = true;
                state.selected_action_index = 0;
            }
        }
        KeyCode::Char('a') => {
            if state.active_view == ActiveView::SearchResults
                && state.active_search_tab == crate::app::SearchTab::Albums
            {
                if state.selected_search_index < state.search_results.albums.len() {
                    let album = &state.search_results.albums[state.selected_search_index];
                    state.status_message = Some(
                        crate::i18n::t("messages.saved_to_library", &state.library_config.language)
                            .replace("{}", &album.name),
                    );
                    state.status_message_expiry =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    return Some(AppEvent::SaveAlbums(vec![album.id.clone()]));
                }
            } else {
                state.playlist_add_modal_open = true;
                state.selected_playlist_modal_index = 0;
            }
        }
        KeyCode::Char('p')
            if state.active_view == ActiveView::Library
                && !state.operation_register.is_empty()
                && state.selected_playlist_index < state.library_view.len() =>
        {
            let node = &state.library_view[state.selected_playlist_index];
            match node {
                crate::models::LibraryNode::Folder(f) => {
                    let folder_name = f.name.clone();
                    if let Some(folder) = state
                        .library_config
                        .folders
                        .iter_mut()
                        .find(|fd| fd.name == folder_name)
                    {
                        for id in &state.operation_register {
                            if !folder.playlists.contains(id) {
                                folder.playlists.push(id.clone());
                            }
                        }
                    }
                    // Unpin anything moved into a folder to avoid duplicates
                    for id in &state.operation_register {
                        state.library_config.pinned.retain(|p| p != id);
                    }
                }
                crate::models::LibraryNode::Playlist { .. } => {
                    // If focused on a standard playlist, pasting it here keeps it at the root level.
                    // Since we already removed it from folders during "cut", it is implicitly at root.
                }
            }
            state.operation_register.clear();
            state.save_library_config();
            state.compute_library_view();
        }
        KeyCode::Char('m') if state.active_view == ActiveView::Library => {
            if state.active_library_tab == crate::app::LibraryTab::Albums {
                return None;
            }
            if state.selected_playlist_index < state.library_view.len()
                && let crate::models::LibraryNode::Playlist { playlist, .. } =
                    &state.library_view[state.selected_playlist_index]
            {
                let id = &playlist.id;
                if id == "LIKED_SONGS" || id == "local-library" {
                    return None;
                }
                if state.library_config.pinned.contains(id) {
                    state.library_config.pinned.retain(|p| p != id);
                } else {
                    state.library_config.pinned.push(id.clone());
                }
                state.save_library_config();
                state.compute_library_view();
            }
        }
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Backspace => {
            if state.active_view == ActiveView::TrackList {
                if search_has_results(state) {
                    // Came from a search drill-down — go back to search results
                    state.active_view = ActiveView::SearchResults;
                } else {
                    state.active_view = ActiveView::Library;
                }
            } else if state.active_view == ActiveView::Queue
                || state.active_view == ActiveView::ArtistList
            {
                state.active_view = ActiveView::Library;
                // If we're returning from ArtistList (where the header was cleared by ArtistPage),
                // we need to reload the tracklist cover image if one exists.
                if state.tracklist_image_url.is_some() {
                    return Some(AppEvent::ReloadHeaderImage);
                }
            } else if state.active_view == ActiveView::ArtistPage {
                if search_has_results(state) {
                    state.active_view = ActiveView::SearchResults;
                    state.clear_pending_artist_page();
                    return Some(AppEvent::CancelArtistPageLoad);
                }
                return Some(artist_page::back_to_artist_list(state));
            } else if state.active_view == ActiveView::SearchResults {
                // Clear search and return to Library
                state.active_view = ActiveView::Library;
                state.search_results = crate::models::SearchResults::default();
                state.search_context_query.clear();
                state.status_message = None;
                if state.tracklist_image_url.is_some() {
                    return Some(AppEvent::ReloadHeaderImage);
                }
            }
        }
        KeyCode::Char('q') => {
            // Add currently hovered track to queue
            let track_id = if state.active_view == ActiveView::TrackList {
                state
                    .tracks
                    .get(state.selected_track_index)
                    .map(|t| t.id.clone())
            } else if state.active_view == ActiveView::SearchResults {
                if state.active_search_tab == crate::app::SearchTab::Tracks {
                    state
                        .search_results
                        .tracks
                        .get(state.selected_search_index)
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
            // Open queue view
            state.active_view = ActiveView::Queue;
            state.selected_queue_index = 0;
            return Some(AppEvent::FetchQueue);
        }
        KeyCode::Char('D') => {
            state.device_modal_open = true;
            state.selected_device_index = 0;
            return Some(AppEvent::FetchDevices);
        }
        KeyCode::Char(' ') => {
            state.playback.is_playing = !state.playback.is_playing;
            return Some(AppEvent::TogglePlayback(state.playback.is_playing));
        }
        KeyCode::Char('c') if state.active_view == ActiveView::Library => {
            state.mode = crate::app::AppMode::Command;
            state.command_buffer = "newplaylist ".to_string();
        }
        KeyCode::Char('e') => {
            if state.active_view == ActiveView::Library
                && let Some(node) = state.library_view.get(state.selected_playlist_index)
            {
                match node {
                    crate::models::LibraryNode::Playlist { playlist, .. } => {
                        state.mode = crate::app::AppMode::Command;
                        state.command_buffer = format!("rename {}", playlist.name);
                    }
                    crate::models::LibraryNode::Folder(f) => {
                        state.mode = crate::app::AppMode::Command;
                        state.command_buffer = format!("rename {}", f.name);
                    }
                }
            }
        }
        KeyCode::Char('s') => {
            state.playback.is_shuffled = !state.playback.is_shuffled;
            return Some(AppEvent::ToggleShuffle(state.playback.is_shuffled));
        }
        KeyCode::Char('v') => {
            state.mode = crate::app::AppMode::Visual;
            let current_idx = match state.active_view {
                ActiveView::TrackList => state.selected_track_index,
                ActiveView::SearchResults => state.selected_search_index,
                ActiveView::Queue => state.selected_queue_index,
                ActiveView::Library => state.selected_playlist_index,
                ActiveView::Devices => state.selected_device_index,
                ActiveView::ArtistList => state.selected_artist_index,
                ActiveView::ArtistPage => state.artist_page_album_index,
            };
            state.visual_selection_start = Some(current_idx);
            state.status_message = Some(crate::i18n::t(
                "messages.visual_block",
                &state.library_config.language,
            ));
        }
        KeyCode::Char(']') | KeyCode::Char('>') => {
            state.playback.progress_ms = 0;
            state.playback.duration_ms = 0;
            return Some(AppEvent::NextTrack {
                current_track_id: state.playback.playing_track_id.clone(),
            });
        }
        KeyCode::Char('[') | KeyCode::Char('<') => {
            state.playback.progress_ms = 0;
            state.playback.duration_ms = 0;
            return Some(AppEvent::PreviousTrack {
                current_track_id: state.playback.playing_track_id.clone(),
            });
        }
        KeyCode::Char('R') => {
            if state.active_view == ActiveView::ArtistPage
                && let Some(data) = state.artist_page_data.as_ref()
            {
                if state.artist_albums_loading {
                    state.status_message =
                        Some("Artist albums refresh already in progress.".to_string());
                    state.status_message_expiry =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    return None;
                }
                state.artist_albums_loading = true;
                state.status_message = Some("Refreshing artist albums...".to_string());
                return Some(AppEvent::RefreshArtistAlbums {
                    artist_id: data.artist_id.clone(),
                });
            } else if state.active_view == ActiveView::Library {
                state.status_message = Some("Refreshing library...".to_string());
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
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Char('-') => {
            let next_vol = state.playback.volume.saturating_sub(1);
            state.playback.volume = next_vol;
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Char('+') => {
            let next_vol = (state.playback.volume + 5).min(100);
            state.playback.volume = next_vol;
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Char('_') => {
            let next_vol = state.playback.volume.saturating_sub(5);
            state.playback.volume = next_vol;
            return Some(AppEvent::SetVolume(next_vol as u8));
        }
        KeyCode::Tab => {
            if state.active_view == ActiveView::Library {
                state.active_library_tab = match state.active_library_tab {
                    crate::app::LibraryTab::Playlists => crate::app::LibraryTab::Albums,
                    crate::app::LibraryTab::Albums => crate::app::LibraryTab::Browse,
                    crate::app::LibraryTab::Browse => crate::app::LibraryTab::Playlists,
                };
                state.selected_playlist_index = 0;
                if state.active_library_tab == crate::app::LibraryTab::Browse {
                    state.active_browse_node = crate::models::BrowseNode::TopTracks;
                    return browse::load_event_if_needed(state);
                }
            } else if state.active_view == ActiveView::SearchResults {
                state.active_search_tab = match state.active_search_tab {
                    crate::app::SearchTab::Tracks => crate::app::SearchTab::Albums,
                    crate::app::SearchTab::Albums => crate::app::SearchTab::Artists,
                    crate::app::SearchTab::Artists => crate::app::SearchTab::Tracks,
                };
                state.selected_search_index = 0;
            }
        }
        _ => {}
    }
    None
}

pub fn playlist_modal_choices(state: &AppState) -> Vec<Playlist> {
    let mut playlists: Vec<Playlist> = state
        .playlists
        .iter()
        .filter(|p| Some(&p.owner_id) == state.user_id.as_ref())
        .cloned()
        .collect();
    playlists.extend(state.local_playlists.to_library_playlists());
    playlists
}

fn selected_tracks_for_playlist(state: &AppState) -> Vec<Track> {
    match state.active_view {
        ActiveView::TrackList => state
            .tracks
            .get(state.selected_track_index)
            .cloned()
            .into_iter()
            .collect(),
        ActiveView::SearchResults if state.active_search_tab == crate::app::SearchTab::Tracks => {
            state
                .search_results
                .tracks
                .get(state.selected_search_index)
                .map(track_from_search_track)
                .into_iter()
                .collect()
        }
        ActiveView::Queue => state
            .queue
            .get(state.selected_queue_index)
            .cloned()
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn resolve_tracks_by_ids(state: &AppState, ids: &[String]) -> Vec<Track> {
    ids.iter()
        .filter_map(|id| find_track_by_id(state, id))
        .collect()
}

fn find_track_by_id(state: &AppState, id: &str) -> Option<Track> {
    state
        .tracks
        .iter()
        .chain(state.queue.iter())
        .find(|track| track.id == id)
        .cloned()
        .or_else(|| {
            state
                .search_results
                .tracks
                .iter()
                .find(|track| track.id == id)
                .map(track_from_search_track)
        })
        .or_else(|| {
            state
                .local_library
                .to_tracks()
                .into_iter()
                .find(|track| track.id == id)
        })
}

fn track_from_search_track(track: &SearchTrack) -> Track {
    Track {
        id: track.id.clone(),
        source: track.source,
        local_path: track.local_path.clone(),
        name: track.name.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
        artist_id: track.artist_id.clone(),
    }
}

fn search_results_len(state: &AppState) -> usize {
    match state.active_search_tab {
        crate::app::SearchTab::Tracks => state.search_results.tracks.len(),
        crate::app::SearchTab::Albums => state.search_results.albums.len(),
        crate::app::SearchTab::Artists => state.search_results.artists.len(),
    }
}

fn search_has_results(state: &AppState) -> bool {
    !state.search_results.tracks.is_empty()
        || !state.search_results.albums.is_empty()
        || !state.search_results.artists.is_empty()
}
