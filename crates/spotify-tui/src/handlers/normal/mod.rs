use echo_core::app::{ActiveView, AppState};
use echo_core::events::AppEvent;
use crate::handlers::{browse, tracklist};
use crossterm::event::{KeyCode, KeyEvent};

mod modals;
mod prompts;

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

    let configured = crate::handlers::keymap::configured_action(state, key);
    if let Some(action) = configured.action {
        return crate::handlers::keymap::execute(state, action);
    }
    if configured.consumed {
        return None;
    }

    let navigation = crate::handlers::navigation::command_for_key(state, key);
    if let Some(command) = navigation.command {
        return crate::handlers::navigation::execute(state, command);
    }
    if navigation.consumed {
        return None;
    }

    if key.code != KeyCode::Char('d') {
        state.ui.pending_d_press = false;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => match state.ui.active_view {
            ActiveView::Library => {
                if state.ui.active_library_tab == echo_core::app::LibraryTab::Browse {
                    if state.ui.selected_playlist_index < 2 {
                        state.ui.selected_playlist_index += 1;
                    }
                    browse::select_node_from_library_index(state);
                    return browse::load_event_if_needed(state);
                } else {
                    let max_len = if state.ui.active_library_tab == echo_core::app::LibraryTab::Albums {
                        state.data.saved_albums.len()
                    } else {
                        state.data.library_view.len()
                    };
                    if max_len > 0 && state.ui.selected_playlist_index < max_len.saturating_sub(1) {
                        state.ui.selected_playlist_index += 1;
                    }
                }
            }
            ActiveView::TrackList => {
                if state.ui.selected_track_index < state.data.tracks.len().saturating_sub(1) {
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
                        && state.ui.artist_page_album_index < data.albums.len().saturating_sub(1)
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
                    && state.ui.selected_queue_index < state.data.queue.len().saturating_sub(1)
                {
                    state.ui.selected_queue_index += 1;
                }
            }
            ActiveView::Devices => {
                if !state.data.devices.is_empty()
                    && state.ui.selected_device_index < state.data.devices.len().saturating_sub(1)
                {
                    state.ui.selected_device_index += 1;
                }
            }
        },
        KeyCode::Char('k') | KeyCode::Up => match state.ui.active_view {
            ActiveView::Library => {
                if state.ui.active_library_tab == echo_core::app::LibraryTab::Browse {
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
        KeyCode::Char('l') | KeyCode::Char('L')
            if key
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
                && !key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::SHIFT) =>
        {
            state.ui.needs_terminal_clear = true;
        }
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
                        state.ui.status_message = Some(echo_core::i18n::t(
                            "messages.added_to_liked",
                            &state.ui.library_config.language,
                        ));
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
                        state.ui.status_message = Some(echo_core::i18n::t(
                            "messages.added_to_liked",
                            &state.ui.library_config.language,
                        ));
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            } else if state.ui.active_view == ActiveView::SearchResults
                && state.ui.active_search_tab == echo_core::app::SearchTab::Tracks
            {
                let i = state.ui.selected_search_index;
                if let Some(track) = state.data.search_results.tracks.get(i) {
                    let track_id = track.id.clone();
                    if state.data.liked_tracks.contains(&track_id) {
                        state.ui.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.data.liked_tracks.insert(track_id.clone());

                        let mut cache = echo_core::config::AppConfig::load_cache();
                        cache.liked_tracks = state.data.liked_tracks.clone();
                        let _ = echo_core::config::AppConfig::save_cache(&cache);

                        state.ui.status_message = Some(echo_core::i18n::t(
                            "messages.added_to_liked",
                            &state.ui.library_config.language,
                        ));
                        state.ui.status_message_expiry =
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
                echo_core::intent::toggle_condensed_lyrics(state);
            } else {
                state.ui.lyrics_modal_open = !state.ui.lyrics_modal_open;
            }
        }
        KeyCode::Enter | KeyCode::Char('z') => {
            if state.ui.active_view == ActiveView::Library {
                let index = state.ui.selected_playlist_index;
                if state.ui.active_library_tab == echo_core::app::LibraryTab::Albums {
                    return echo_core::intent::open_album(state, index);
                } else if state.ui.active_library_tab == echo_core::app::LibraryTab::Browse {
                    if let Some(event) = browse::enter_active_node(state) {
                        return Some(event);
                    }
                } else {
                    return echo_core::intent::open_library_entry(state, index);
                }
            } else if state.ui.active_view == ActiveView::TrackList {
                return tracklist::play_selected(state);
            } else if state.ui.active_view == ActiveView::Queue {
                return echo_core::intent::play_queue_track_at(
                    state,
                    state.ui.selected_queue_index,
                );
            } else if state.ui.active_view == ActiveView::SearchResults {
                return echo_core::intent::activate_search_result(
                    state,
                    state.ui.selected_search_index,
                );
            } else if state.ui.active_view == ActiveView::ArtistList {
                return echo_core::intent::open_followed_artist(
                    state,
                    state.ui.selected_artist_index,
                );
            } else if state.ui.active_view == ActiveView::ArtistPage {
                return echo_core::intent::open_artist_album(
                    state,
                    state.ui.artist_page_album_index,
                );
            }
        }
        KeyCode::Char(':') => {
            state.ui.mode = echo_core::app::AppMode::Command;
            state.ui.command_buffer.clear();
            state.ui.status_message = None;
        }
        KeyCode::Char('/') => {
            state.ui.mode = echo_core::app::AppMode::Search;
            state.ui.search_query.clear();
            state.ui.search_matches.clear();
            state.ui.status_message = None;
        }
        KeyCode::Char('f') => {
            state.ui.mode = echo_core::app::AppMode::Command;
            state.ui.command_buffer = "search ".to_string();
            state.ui.status_message = None;
        }
        KeyCode::Char('n') if !state.ui.search_matches.is_empty() => {
            echo_core::intent::next_search_match(state, true);
        }
        KeyCode::Char('N') if !state.ui.search_matches.is_empty() => {
            echo_core::intent::next_search_match(state, false);
        }
        KeyCode::Char('d') | KeyCode::Char('x') if state.ui.active_view == ActiveView::Library => {
            if state.ui.active_library_tab == echo_core::app::LibraryTab::Albums {
                if key.code == KeyCode::Char('d')
                    && state.ui.selected_playlist_index < state.data.saved_albums.len()
                {
                    let album = &state.data.saved_albums[state.ui.selected_playlist_index];
                    if state.ui.pending_d_press {
                        state.ui.album_mass_delete_prompt = Some(vec![album.id.clone()]);
                        state.ui.pending_d_press = false;
                    } else {
                        state.ui.pending_d_press = true;
                    }
                }
                return None;
            }
            if state.ui.selected_playlist_index < state.data.library_view.len() {
                match &state.data.library_view[state.ui.selected_playlist_index] {
                    echo_core::models::LibraryNode::Playlist { playlist, .. } => {
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
                            state.ui.operation_register = vec![playlist.id.clone()];

                            for f in &mut state.ui.library_config.folders {
                                f.playlists.retain(|id| id != &playlist.id);
                            }
                            state.save_library_config();
                            state.compute_library_view();
                        } else if key.code == KeyCode::Char('d')
                            && Some(&playlist.owner_id) == state.data.user_id.as_ref()
                        {
                            if state.ui.pending_d_press {
                                state.ui.playlist_delete_prompt = Some(vec![playlist.id.clone()]);
                                state.ui.pending_d_press = false;
                            } else {
                                state.ui.pending_d_press = true;
                            }
                        }
                    }
                    echo_core::models::LibraryNode::Folder(f) => {
                        if key.code == KeyCode::Char('d') {
                            if state.ui.pending_d_press {
                                state.ui.folder_delete_prompt = Some(f.name.clone());
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
                Some(echo_core::models::ActionMenuContext::from(t))
            } else if state.ui.active_view == ActiveView::Queue
                && state.ui.selected_queue_index < state.data.queue.len()
            {
                let t = &state.data.queue[state.ui.selected_queue_index];
                Some(echo_core::models::ActionMenuContext::from(t))
            } else if state.ui.active_view == ActiveView::SearchResults
                && state.ui.active_search_tab == echo_core::app::SearchTab::Tracks
                && state.ui.selected_search_index < state.data.search_results.tracks.len()
            {
                let t = &state.data.search_results.tracks[state.ui.selected_search_index];
                Some(echo_core::models::ActionMenuContext::from(t))
            } else if !state.playback.playing_track_id.is_none() {
                Some(echo_core::models::ActionMenuContext {
                    track_id: state.playback.playing_track_id.clone().unwrap_or_default(),
                    source: state
                        .playback
                        .playing_track_source
                        .unwrap_or(echo_core::models::TrackSource::Spotify),
                    track_name: state.playback.playing_track_title.clone(),
                    local_path: state.playback.playing_track_local_path.clone(),
                    album_id: state.playback.playing_track_album_id.clone(),
                    album_name: state
                        .data
                        .local_library
                        .tracks
                        .iter()
                        .find(|track| Some(track.id.as_str()) == state.playback.playing_track_id.as_deref())
                        .map(|track| track.album.clone())
                        .unwrap_or_default(),
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
                && state.ui.active_search_tab == echo_core::app::SearchTab::Albums
            {
                if state.ui.selected_search_index < state.data.search_results.albums.len() {
                    let album = &state.data.search_results.albums[state.ui.selected_search_index];
                    state.ui.status_message = Some(
                        echo_core::i18n::t(
                            "messages.saved_to_library",
                            &state.ui.library_config.language,
                        )
                        .replace("{}", &album.name),
                    );
                    state.ui.status_message_expiry =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
                && state.ui.selected_playlist_index < state.data.library_view.len() =>
        {
            let node = &state.data.library_view[state.ui.selected_playlist_index];
            match node {
                echo_core::models::LibraryNode::Folder(f) => {
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
                echo_core::models::LibraryNode::Playlist { .. } => {}
            }
            state.ui.operation_register.clear();
            state.save_library_config();
            state.compute_library_view();
        }
        KeyCode::Char('m') if state.ui.active_view == ActiveView::Library => {
            echo_core::intent::toggle_pin_selected(state);
        }
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Backspace => {
            if state.pop_view_history() {
                state.clear_pending_artist_page();
                if state.data.tracklist_image_url.is_some() {
                    return Some(AppEvent::ReloadHeaderImage);
                }
            } else if state.ui.active_view == ActiveView::TrackList {
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
                return Some(echo_core::intent::back_to_artist_list(state));
            } else if state.ui.active_view == ActiveView::SearchResults {
                state.ui.active_view = ActiveView::Library;
                state.data.search_results = echo_core::models::SearchResults::default();
                state.ui.search_context_query.clear();
                state.ui.status_message = None;
                if state.data.tracklist_image_url.is_some() {
                    return Some(AppEvent::ReloadHeaderImage);
                }
            }
        }
        KeyCode::Char('q') => {
            return echo_core::intent::queue_selected_track(state);
        }
        KeyCode::Char('Q') => {
            return Some(echo_core::intent::open_queue(state));
        }
        KeyCode::Char('D') => {
            return Some(echo_core::intent::open_device_picker(state));
        }
        KeyCode::Char(' ') => {
            state.playback.is_playing = !state.playback.is_playing;
            state.playback.playback_last_updated_at = Some(std::time::Instant::now());
            return Some(AppEvent::TogglePlayback(state.playback.is_playing));
        }
        KeyCode::Char('c') if state.ui.active_view == ActiveView::Library => {
            state.ui.mode = echo_core::app::AppMode::Command;
            state.ui.command_buffer = "newplaylist ".to_string();
        }
        KeyCode::Char('e') => {
            if state.ui.active_view == ActiveView::Library
                && let Some(node) = state
                    .data
                    .library_view
                    .get(state.ui.selected_playlist_index)
            {
                match node {
                    echo_core::models::LibraryNode::Playlist { playlist, .. } => {
                        state.ui.mode = echo_core::app::AppMode::Command;
                        state.ui.command_buffer = format!("rename {}", playlist.name);
                    }
                    echo_core::models::LibraryNode::Folder(f) => {
                        state.ui.mode = echo_core::app::AppMode::Command;
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
            state.ui.mode = echo_core::app::AppMode::Visual;
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
            state.ui.status_message = Some(echo_core::i18n::t(
                "messages.visual_block",
                &state.ui.library_config.language,
            ));
        }
        KeyCode::Char(',') => return seek_by(state, -5),
        KeyCode::Char('.') => return seek_by(state, 5),
        KeyCode::Char('0') => return seek_to(state, 0),
        KeyCode::Char('M') => {
            let volume = state.playback.toggle_mute_target();
            state.playback.volume = volume;
            state.save_volume();
            return Some(AppEvent::SetVolume(volume as u8));
        }
        KeyCode::Char('T') => {
            let enabled = !state.ui.library_config.library_thumbnails;
            state.set_library_thumbnails(enabled);
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
            return echo_core::intent::refresh_view(state);
        }
        KeyCode::Char('r') => {
            return Some(echo_core::intent::cycle_repeat(state));
        }
        KeyCode::Char('=') => {
            return Some(echo_core::intent::adjust_volume(state, 1));
        }
        KeyCode::Char('-') => {
            return Some(echo_core::intent::adjust_volume(state, -1));
        }
        KeyCode::Char('+') => {
            return Some(echo_core::intent::adjust_volume(state, 5));
        }
        KeyCode::Char('_') => {
            return Some(echo_core::intent::adjust_volume(state, -5));
        }
        KeyCode::Tab => {
            if state.ui.active_view == ActiveView::Library {
                state.ui.active_library_tab = match state.ui.active_library_tab {
                    echo_core::app::LibraryTab::Playlists => echo_core::app::LibraryTab::Albums,
                    echo_core::app::LibraryTab::Albums => echo_core::app::LibraryTab::Browse,
                    // Artists is desktop-only; fold it into the cycle defensively.
                    echo_core::app::LibraryTab::Browse | echo_core::app::LibraryTab::Artists => {
                        echo_core::app::LibraryTab::Playlists
                    }
                };
                state.ui.selected_playlist_index = 0;
                if state.ui.active_library_tab == echo_core::app::LibraryTab::Browse {
                    state.ui.active_browse_node = echo_core::models::BrowseNode::TopTracks;
                    return browse::load_event_if_needed(state);
                }
            } else if state.ui.active_view == ActiveView::SearchResults {
                state.ui.active_search_tab = match state.ui.active_search_tab {
                    echo_core::app::SearchTab::Tracks => echo_core::app::SearchTab::Albums,
                    echo_core::app::SearchTab::Albums => echo_core::app::SearchTab::Artists,
                    echo_core::app::SearchTab::Artists => echo_core::app::SearchTab::Tracks,
                };
                state.ui.selected_search_index = 0;
            }
        }
        _ => {}
    }
    None
}

fn seek_by(state: &mut AppState, seconds: i64) -> Option<AppEvent> {
    let target = state.playback.seek_target(seconds);
    seek_to(state, target)
}

fn seek_to(state: &mut AppState, progress_ms: u32) -> Option<AppEvent> {
    if state.playback.playing_track_id.is_none() || state.playback.duration_ms == 0 {
        state.ui.status_message = Some("Nothing is currently seekable".to_string());
        state.ui.status_message_expiry =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
        return None;
    }
    state.playback.set_optimistic_progress(progress_ms);
    Some(AppEvent::SeekTo(state.playback.progress_ms))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn search_results_len(state: &AppState) -> usize {
    match state.ui.active_search_tab {
        echo_core::app::SearchTab::Tracks => state.data.search_results.tracks.len(),
        echo_core::app::SearchTab::Albums => state.data.search_results.albums.len(),
        echo_core::app::SearchTab::Artists => state.data.search_results.artists.len(),
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

    #[test]
    fn seek_keys_emit_absolute_targets() {
        let mut state = AppState::new();
        state.playback.playing_track_id = Some("track".to_string());
        state.playback.duration_ms = 60_000;
        state.playback.progress_ms = 10_000;

        let event = handle_key(
            &mut state,
            &KeyEvent::new(KeyCode::Char('.'), crossterm::event::KeyModifiers::NONE),
        );
        assert!(matches!(event, Some(AppEvent::SeekTo(15_000))));
    }

    #[test]
    fn mute_key_preserves_volume_for_restore() {
        let mut state = AppState::new();
        state.playback.volume = 42;

        let event = handle_key(
            &mut state,
            &KeyEvent::new(KeyCode::Char('M'), crossterm::event::KeyModifiers::NONE),
        );
        assert!(matches!(event, Some(AppEvent::SetVolume(0))));
        assert_eq!(state.playback.previous_volume, Some(42));
    }

    #[test]
    fn ctrl_l_requests_redraw_without_triggering_navigation() {
        let mut state = AppState::new();

        let event = handle_key(
            &mut state,
            &KeyEvent::new(
                KeyCode::Char('l'),
                crossterm::event::KeyModifiers::CONTROL,
            ),
        );

        assert!(event.is_none());
        assert!(state.ui.needs_terminal_clear);
    }
}
