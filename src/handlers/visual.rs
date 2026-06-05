use crate::app::{ActiveView, AppMode, AppState};
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    if let Some((playlist_id, track_ids)) = state.track_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.track_delete_prompt = None;
            state.mode = AppMode::Normal;
            state.visual_selection_start = None;
            return Some(AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids));
        }
        state.track_delete_prompt = None;
        return None;
    }

    if let Some(playlist_ids) = state.playlist_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.playlist_delete_prompt = None;
            state.mode = AppMode::Normal;
            state.visual_selection_start = None;
            return Some(AppEvent::DeletePlaylists(playlist_ids));
        }
        state.playlist_delete_prompt = None;
        return None;
    }

    if let Some(album_ids) = state.album_mass_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.album_mass_delete_prompt = None;
            state.mode = AppMode::Normal;
            state.visual_selection_start = None;
            return Some(AppEvent::RemoveAlbums(album_ids));
        }
        state.album_mass_delete_prompt = None;
        return None;
    }

    if state.playlist_add_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.playlist_add_modal_open = false;
                state.selected_playlist_modal_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.selected_playlist_modal_index + 1
                    < crate::handlers::normal::playlist_modal_choices(state).len()
                {
                    state.selected_playlist_modal_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if state.selected_playlist_modal_index > 0 => {
                state.selected_playlist_modal_index -= 1;
            }
            KeyCode::Enter => {
                let playlists = crate::handlers::normal::playlist_modal_choices(state);
                if let Some(playlist) = playlists.get(state.selected_playlist_modal_index) {
                    let tracks = if let Some((start, end)) = state.get_visual_selection_range() {
                        match state.active_view {
                            ActiveView::TrackList => state.tracks[start..=end].to_vec(),
                            ActiveView::SearchResults => {
                                if state.active_search_tab == crate::app::SearchTab::Tracks {
                                    state.search_results.tracks[start..=end]
                                        .iter()
                                        .map(|t| crate::models::Track {
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
                                        .collect()
                                } else {
                                    vec![]
                                }
                            }
                            ActiveView::Queue => state.queue[start..=end].to_vec(),
                            _ => vec![],
                        }
                    } else {
                        vec![]
                    };
                    state.playlist_add_modal_open = false;
                    state.selected_playlist_modal_index = 0;
                    state.mode = AppMode::Normal;
                    state.visual_selection_start = None;
                    if !tracks.is_empty() {
                        return Some(AppEvent::AddTracksToPlaylist(playlist.id.clone(), tracks));
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
        KeyCode::Esc => {
            state.mode = AppMode::Normal;
            state.visual_selection_start = None;
            state.status_message = None;
        }
        KeyCode::Char('j') | KeyCode::Down => match state.active_view {
            ActiveView::TrackList => {
                if state.selected_track_index + 1 < state.tracks.len() {
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
                if state.selected_queue_index + 1 < state.queue.len() {
                    state.selected_queue_index += 1;
                }
            }
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
            ActiveView::Devices => {
                if state.selected_device_index + 1 < state.devices.len() {
                    state.selected_device_index += 1;
                }
            }
            ActiveView::ArtistList => {
                if state.selected_artist_index + 1 < state.followed_artists.len() {
                    state.selected_artist_index += 1;
                }
            }
            ActiveView::ArtistPage => {
                if let Some(ref data) = state.artist_page_data {
                    if state.artist_page_album_index + 1 < data.albums.len() {
                        state.artist_page_album_index += 1;
                    }
                }
            }
        },
        KeyCode::Char('k') | KeyCode::Up => match state.active_view {
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
            ActiveView::Library => {
                if state.selected_playlist_index > 0 {
                    state.selected_playlist_index -= 1;
                }
            }
            ActiveView::Devices => {
                if state.selected_device_index > 0 {
                    state.selected_device_index -= 1;
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
        },
        KeyCode::Char('q') => {
            if let Some((start, end)) = state.get_visual_selection_range() {
                let track_ids: Vec<String> = match state.active_view {
                    ActiveView::TrackList => state.tracks[start..=end]
                        .iter()
                        .map(|t| t.id.clone())
                        .collect(),
                    ActiveView::SearchResults => {
                        if state.active_search_tab == crate::app::SearchTab::Tracks {
                            state.search_results.tracks[start..=end]
                                .iter()
                                .map(|t| t.id.clone())
                                .collect()
                        } else {
                            vec![]
                        }
                    }
                    ActiveView::Queue => state.queue[start..=end]
                        .iter()
                        .map(|t| t.id.clone())
                        .collect(),
                    ActiveView::ArtistPage => vec![],
                    _ => vec![],
                };

                state.mode = AppMode::Normal;
                state.visual_selection_start = None;
                state.status_message = Some(format!("Added {} tracks to queue", track_ids.len()));

                if !track_ids.is_empty() {
                    return Some(AppEvent::AddToQueue(track_ids));
                }
            } else {
                state.mode = AppMode::Normal;
                state.visual_selection_start = None;
                state.status_message = None;
            }
        }
        KeyCode::Char('x') => {
            if state.active_view == ActiveView::Library
                && state.active_library_tab != crate::app::LibraryTab::Albums
                && let Some((start, end)) = state.get_visual_selection_range()
            {
                let selected_ids: Vec<String> = state.library_view[start..=end]
                    .iter()
                    .filter_map(|node| match node {
                        crate::models::LibraryNode::Playlist { playlist, .. } => {
                            if playlist.id != "LIKED_SONGS" && playlist.id != "local-library" {
                                Some(playlist.id.clone())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                    .collect();

                if !selected_ids.is_empty() {
                    state.operation_register = selected_ids;

                    for f in &mut state.library_config.folders {
                        f.playlists
                            .retain(|id| !state.operation_register.contains(id));
                    }
                    state.save_library_config();
                    state.compute_library_view();

                    state.mode = AppMode::Normal;
                    state.visual_selection_start = None;
                    state.status_message = Some("Cut playlists".to_string());
                }
            }
        }
        KeyCode::Char('d') => {
            if state.active_view == ActiveView::TrackList
                && let Some((start, end)) = state.get_visual_selection_range()
                && let Some(context) = &state.active_tracklist_context
                && context.can_modify_playlist(state.user_id.as_ref())
            {
                if state.pending_d_press {
                    let track_ids = state.tracks[start..=end]
                        .iter()
                        .map(|t| t.id.clone())
                        .collect();
                    state.track_delete_prompt = Some((context.id.clone(), track_ids));
                    state.pending_d_press = false;
                } else {
                    state.pending_d_press = true;
                }
            }
            if state.active_view == ActiveView::Library
                && state.active_library_tab != crate::app::LibraryTab::Albums
            {
                if let Some((start, end)) = state.get_visual_selection_range() {
                    if state.pending_d_press {
                        let selected_ids: Vec<String> = state.library_view[start..=end]
                            .iter()
                            .filter_map(|node| match node {
                                crate::models::LibraryNode::Playlist { playlist, .. } => {
                                    if playlist.id.starts_with("local-playlist:")
                                        || (playlist.id != "LIKED_SONGS"
                                            && playlist.id != "local-library"
                                            && Some(&playlist.owner_id) == state.user_id.as_ref())
                                    {
                                        Some(playlist.id.clone())
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            })
                            .collect();

                        if !selected_ids.is_empty() {
                            state.playlist_delete_prompt = Some(selected_ids);
                            state.pending_d_press = false;
                        }
                    } else {
                        state.pending_d_press = true;
                    }
                }
            } else if state.active_view == ActiveView::Library
                && let Some((start, end)) = state.get_visual_selection_range()
            {
                if state.pending_d_press {
                    if state.active_library_tab == crate::app::LibraryTab::Albums {
                        let album_ids = state.saved_albums[start..=end]
                            .iter()
                            .map(|a| a.id.clone())
                            .collect();
                        state.album_mass_delete_prompt = Some(album_ids);
                    } else {
                        let selected_ids: Vec<String> = state.library_view[start..=end]
                            .iter()
                            .filter_map(|node| match node {
                                crate::models::LibraryNode::Playlist { playlist, .. } => {
                                    if playlist.id.starts_with("local-playlist:")
                                        || (playlist.id != "LIKED_SONGS"
                                            && playlist.id != "local-library"
                                            && Some(&playlist.owner_id) == state.user_id.as_ref())
                                    {
                                        Some(playlist.id.clone())
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            })
                            .collect();

                        if !selected_ids.is_empty() {
                            state.playlist_delete_prompt = Some(selected_ids);
                        }
                    }
                    state.pending_d_press = false;
                } else {
                    state.pending_d_press = true;
                }
            }
        }
        KeyCode::Char('a') => {
            state.playlist_add_modal_open = true;
            state.selected_playlist_modal_index = 0;
        }
        _ => {}
    }
    None
}

fn search_results_len(state: &AppState) -> usize {
    match state.active_search_tab {
        crate::app::SearchTab::Tracks => state.search_results.tracks.len(),
        crate::app::SearchTab::Albums => state.search_results.albums.len(),
        crate::app::SearchTab::Artists => state.search_results.artists.len(),
    }
}
