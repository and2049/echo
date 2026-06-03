use crate::app::{ActiveView, AppState};
use crate::events::AppEvent;
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
                let user_playlists: Vec<_> = state.playlists.iter().filter(|p| Some(&p.owner_id) == state.user_id.as_ref()).collect();
                if state.selected_playlist_modal_index + 1 < user_playlists.len() {
                    state.selected_playlist_modal_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up
                if state.selected_playlist_modal_index > 0 => {
                    state.selected_playlist_modal_index -= 1;
                }
            KeyCode::Enter => {
                let user_playlists: Vec<_> = state.playlists.iter().filter(|p| Some(&p.owner_id) == state.user_id.as_ref()).collect();
                if let Some(playlist) = user_playlists.get(state.selected_playlist_modal_index) {
                    let track_ids = match state.active_view {
                        ActiveView::TrackList => state.tracks.get(state.selected_track_index).map(|t| vec![t.id.clone()]).unwrap_or_default(),
                        ActiveView::SearchResults => if state.active_search_tab == crate::app::SearchTab::Tracks {
                            state.search_results.tracks.get(state.selected_search_index).map(|t| vec![t.id.clone()]).unwrap_or_default()
                        } else { vec![] },
                        ActiveView::Queue => state.queue.get(state.selected_queue_index).map(|t| vec![t.id.clone()]).unwrap_or_default(),
                        _ => vec![],
                    };
                    state.playlist_add_modal_open = false;
                    state.selected_playlist_modal_index = 0;
                    if !track_ids.is_empty() {
                        return Some(AppEvent::AddTracksToPlaylist(playlist.id.clone(), track_ids));
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
        KeyCode::Char('j') => match state.active_view {
            ActiveView::Library => {
                let max_len = if state.active_library_tab == crate::app::LibraryTab::Albums {
                    state.saved_albums.len()
                } else {
                    state.library_view.len()
                };
                if max_len > 0 && state.selected_playlist_index < max_len.saturating_sub(1) {
                    state.selected_playlist_index += 1;
                }
            }
            ActiveView::TrackList => {
                if state.selected_track_index < state.tracks.len().saturating_sub(1) {
                    state.selected_track_index += 1;
                }
            }
            ActiveView::SearchResults => {
                let max = search_results_len(state);
                if max > 0 && state.selected_search_index < max.saturating_sub(1) {
                    state.selected_search_index += 1;
                }
            }
            ActiveView::Queue => {
                if !state.queue.is_empty() && state.selected_queue_index < state.queue.len().saturating_sub(1) {
                    state.selected_queue_index += 1;
                }
            }
        },
        KeyCode::Char('k') => match state.active_view {
            ActiveView::Library => {
                if state.selected_playlist_index > 0 {
                    state.selected_playlist_index -= 1;
                }
            }
            ActiveView::TrackList => {
                if state.selected_track_index > 0 {
                    state.selected_track_index -= 1;
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
        },
        KeyCode::Char('l') => {
            if state.active_view == ActiveView::Library {
                return crate::handlers::normal::handle_key(state, &KeyEvent::new(KeyCode::Enter, key.modifiers));
            } else if state.active_view == ActiveView::TrackList {
                if state.selected_track_index < state.tracks.len() {
                    let track = &state.tracks[state.selected_track_index];
                    let track_id = track.id.clone();
                    if state.liked_tracks.contains(&track_id) {
                        state.liked_track_remove_prompt = Some(track_id);
                    } else {
                        state.liked_tracks.insert(track_id.clone());
                        state.status_message = Some(crate::i18n::t("messages.added_to_liked", &state.library_config.language));
                        state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
                        state.status_message = Some(crate::i18n::t("messages.added_to_liked", &state.library_config.language));
                        state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            } else if state.active_view == ActiveView::SearchResults && state.active_search_tab == crate::app::SearchTab::Tracks {
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
                        
                        state.status_message = Some(crate::i18n::t("messages.added_to_liked", &state.library_config.language));
                        state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        return Some(AppEvent::ToggleTrackLike(track_id, true));
                    }
                }
            }
        }
        KeyCode::Enter | KeyCode::Char('z') => {
            if state.active_view == ActiveView::Library {
                let (context_id, image_url, metadata) = if state.active_library_tab == crate::app::LibraryTab::Albums {
                    if state.selected_playlist_index < state.saved_albums.len() {
                        let album = &state.saved_albums[state.selected_playlist_index];
                        (album.id.clone(), album.image_url.clone(), Some((album.id.clone(), album.name.clone(), album.artists.clone(), String::new())))
                    } else {
                        (String::new(), None, None)
                    }
                } else {
                    if state.selected_playlist_index < state.library_view.len() {
                        match &state.library_view[state.selected_playlist_index] {
                            crate::models::LibraryNode::Playlist { playlist, .. } => {
                                (playlist.id.clone(), playlist.image_url.clone(), Some((playlist.id.clone(), playlist.name.clone(), playlist.owner.clone(), playlist.owner_id.clone())))
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
                                (String::new(), None, None)
                            }
                        }
                    } else {
                        (String::new(), None, None)
                    }
                };

                if !context_id.is_empty() {
                    let is_album = state.active_library_tab == crate::app::LibraryTab::Albums;
                    state.active_view = ActiveView::TrackList;
                    state.tracks.clear();
                    state.selected_track_index = 0;
                    state.active_library_header_image = None;
                    state.header_image_cache = None;
                    state.header_image_dirty = false;
                    return Some(AppEvent::LoadContextTracks(context_id, is_album, image_url, metadata));
                }
            } else if state.active_view == ActiveView::TrackList {
                if state.selected_track_index < state.tracks.len() {
                    let track = &state.tracks[state.selected_track_index];
                    let context_id = if state.active_library_tab == crate::app::LibraryTab::Albums {
                        if state.selected_playlist_index < state.saved_albums.len() {
                            state.saved_albums[state.selected_playlist_index].id.clone()
                        } else {
                            String::new()
                        }
                    } else {
                        if state.selected_playlist_index < state.library_view.len() {
                            match &state.library_view[state.selected_playlist_index] {
                                crate::models::LibraryNode::Playlist { playlist, .. } => {
                                    playlist.id.clone()
                                }
                                _ => String::new(),
                            }
                        } else {
                            String::new()
                        }
                    };

                    if !context_id.is_empty() {
                        return Some(AppEvent::PlayTrack {
                            context_id,
                            track_id: track.id.clone(),
                            is_album: state.active_library_tab == crate::app::LibraryTab::Albums,
                            title: track.name.clone(),
                            artist: track.artist.clone(),
                            duration_ms: track.duration_ms,
                            image_url: track.image_url.clone(),
                            album_id: track.album_id.clone(),
                        });
                    }
                }
            } else if state.active_view == ActiveView::SearchResults {
                let i = state.selected_search_index;
                match state.active_search_tab {
                    crate::app::SearchTab::Tracks => {
                        if let Some(t) = state.search_results.tracks.get(i) {
                            // Play track directly (no context — use URI playback)
                            let track_id = t.id.clone();
                            return Some(AppEvent::PlayTrack {
                                context_id: "LIKED_SONGS".to_string(), // URI-only play
                                track_id,
                                is_album: false,
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
                            let album_id = album.id.clone();
                            state.active_view = ActiveView::TrackList;
                            state.tracks.clear();
                            state.selected_track_index = 0;
                            // Stash SearchResults as previous view context
                            state.prev_view = None; // album loaded from search, Backspace returns to Search
                            let metadata = Some((album_id.clone(), album.name.clone(), album.artist.clone(), String::new()));
                            return Some(AppEvent::LoadContextTracks(album_id, true, album.image_url.clone(), metadata));
                        }
                    }
                }
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
        KeyCode::Char('n')
            if !state.search_matches.is_empty() => {
                // Find the first match index that is greater than the current selected_track_index
                if let Some(&next_idx) = state.search_matches.iter().find(|&&i| i > state.selected_track_index) {
                    state.selected_track_index = next_idx;
                } else {
                    // Wrap around to the first match
                    state.selected_track_index = state.search_matches[0];
                }
            }
        KeyCode::Char('N')
            if !state.search_matches.is_empty() => {
                // Find the last match index that is less than the current selected_track_index
                if let Some(&prev_idx) = state.search_matches.iter().rev().find(|&&i| i < state.selected_track_index) {
                    state.selected_track_index = prev_idx;
                } else {
                    // Wrap around to the last match
                    state.selected_track_index = *state.search_matches.last().unwrap();
                }
            }
        KeyCode::Char('d') | KeyCode::Char('x')
            if state.active_view == ActiveView::Library => {
                if state.active_library_tab == crate::app::LibraryTab::Albums {
                    if key.code == KeyCode::Char('d')
                        && state.selected_playlist_index < state.saved_albums.len() {
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
                                && Some(&playlist.owner_id) == state.user_id.as_ref() {
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
            if state.active_view == ActiveView::TrackList
                && let Some(track) = state.tracks.get(state.selected_track_index)
                    && let Some((playlist_id, _, _, playlist_owner_id)) = &state.tracklist_context_metadata
                        && Some(playlist_owner_id) == state.user_id.as_ref() {
                            if state.pending_d_press {
                                state.track_delete_prompt = Some((playlist_id.clone(), vec![track.id.clone()]));
                                state.pending_d_press = false;
                            } else {
                                state.pending_d_press = true;
                            }
                        }
        }
        KeyCode::Char('A') => {
            let mut album_id_opt = None;
            if state.active_view == ActiveView::TrackList {
                if state.selected_track_index < state.tracks.len() {
                    album_id_opt = state.tracks[state.selected_track_index].album_id.clone();
                }
            } else if state.active_view == ActiveView::Queue {
                if state.selected_track_index < state.queue.len() {
                    album_id_opt = state.queue[state.selected_track_index].album_id.clone();
                }
            } else if state.active_view == ActiveView::SearchResults && state.active_search_tab == crate::app::SearchTab::Tracks
                && state.selected_search_index < state.search_results.tracks.len() {
                    album_id_opt = state.search_results.tracks[state.selected_search_index].album_id.clone();
                }

            if let Some(album_id) = album_id_opt {
                state.active_view = ActiveView::TrackList;
                state.tracks.clear();
                state.selected_track_index = 0;
                state.active_library_header_image = None;
                state.header_image_cache = None;
                state.header_image_dirty = false;
                return Some(AppEvent::LoadContextTracks(album_id, true, None, None));
            }
        }
        KeyCode::Char('a') => {
            if state.active_view == ActiveView::SearchResults && state.active_search_tab == crate::app::SearchTab::Albums {
                if state.selected_search_index < state.search_results.albums.len() {
                    let album = &state.search_results.albums[state.selected_search_index];
                    state.status_message = Some(crate::i18n::t("messages.saved_to_library", &state.library_config.language).replace("{}", &album.name));
                    state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    return Some(AppEvent::SaveAlbums(vec![album.id.clone()]));
                }
            } else {
                state.playlist_add_modal_open = true;
                state.selected_playlist_modal_index = 0;
            }
        }
        KeyCode::Char('p')
            if state.active_view == ActiveView::Library && !state.operation_register.is_empty()
                && state.selected_playlist_index < state.library_view.len() => {
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
        KeyCode::Char('m')
            if state.active_view == ActiveView::Library => {
                if state.active_library_tab == crate::app::LibraryTab::Albums {
                    return None;
                }
                if state.selected_playlist_index < state.library_view.len()
                    && let crate::models::LibraryNode::Playlist { playlist, .. } =
                        &state.library_view[state.selected_playlist_index]
                    {
                        let id = &playlist.id;
                        if id == "LIKED_SONGS" {
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
                if !state.search_results.tracks.is_empty()
                    || !state.search_results.albums.is_empty()
                {
                    // Came from a search drill-down — go back to search results
                    state.active_view = ActiveView::SearchResults;
                } else {
                    state.active_view = ActiveView::Library;
                }
            } else if state.active_view == ActiveView::Queue {
                state.active_view = ActiveView::Library;
            } else if state.active_view == ActiveView::SearchResults {
                // Clear search and return to Library
                state.active_view = ActiveView::Library;
                state.search_results = crate::models::SearchResults::default();
                state.search_context_query.clear();
                state.status_message = None;
            }
        }
        KeyCode::Char('q') => {
            // Add currently hovered track to queue
            let track_id = if state.active_view == ActiveView::TrackList {
                state.tracks.get(state.selected_track_index).map(|t| t.id.clone())
            } else if state.active_view == ActiveView::SearchResults {
                if state.active_search_tab == crate::app::SearchTab::Tracks {
                    state.search_results.tracks.get(state.selected_search_index).map(|t| t.id.clone())
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
        KeyCode::Char(' ') => {
            state.playback.is_playing = !state.playback.is_playing;
            return Some(AppEvent::TogglePlayback(state.playback.is_playing));
        }
        KeyCode::Char('c')
            if state.active_view == ActiveView::Library => {
                state.mode = crate::app::AppMode::Command;
                state.command_buffer = "newplaylist ".to_string();
            }
        KeyCode::Char('e') => {
            if state.active_view == ActiveView::Library
                && let Some(node) = state.library_view.get(state.selected_playlist_index) {
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
            };
            state.visual_selection_start = Some(current_idx);
            state.status_message = Some(crate::i18n::t("messages.visual_block", &state.library_config.language));
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
                    crate::app::LibraryTab::Albums => crate::app::LibraryTab::Playlists,
                };
                state.selected_playlist_index = 0;
            } else if state.active_view == ActiveView::SearchResults {
                state.active_search_tab = match state.active_search_tab {
                    crate::app::SearchTab::Tracks => crate::app::SearchTab::Albums,
                    crate::app::SearchTab::Albums => crate::app::SearchTab::Tracks,
                };
                state.selected_search_index = 0;
            }
        }
        _ => {}
    }
    None
}

fn search_results_len(state: &AppState) -> usize {
    match state.active_search_tab {
        crate::app::SearchTab::Tracks => state.search_results.tracks.len(),
        crate::app::SearchTab::Albums => state.search_results.albums.len(),
    }
}
