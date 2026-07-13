use crate::app::{ActiveView, AppState};
use crate::events::AppEvent;
use crate::models::{ActionMenuAction, Playlist, SearchTrack, Track, TrackListContext};
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle(state: &mut AppState, key: &KeyEvent) -> (bool, Option<AppEvent>) {
    if state.ui.playlist_add_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.ui.playlist_add_modal_open = false;
                state.ui.selected_playlist_modal_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.ui.selected_playlist_modal_index + 1 < playlist_modal_choices(state).len()
                {
                    state.ui.selected_playlist_modal_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up if state.ui.selected_playlist_modal_index > 0 => {
                state.ui.selected_playlist_modal_index -= 1;
            }
            KeyCode::Enter => {
                let playlists = playlist_modal_choices(state);
                if let Some(playlist) = playlists.get(state.ui.selected_playlist_modal_index) {
                    let tracks = if !state.ui.operation_register.is_empty() {
                        let ids: Vec<_> = state.ui.operation_register.drain(..).collect();
                        resolve_tracks_by_ids(state, &ids)
                    } else {
                        selected_tracks_for_playlist(state)
                    };
                    state.ui.playlist_add_modal_open = false;
                    state.ui.selected_playlist_modal_index = 0;
                    if !tracks.is_empty() {
                        return (
                            true,
                            Some(AppEvent::AddTracksToPlaylist(playlist.id.clone(), tracks)),
                        );
                    }
                }
            }
            _ => {}
        }
        return (true, None);
    }

    if state.ui.device_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.ui.device_modal_open = false;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.ui.selected_device_index + 1 < state.data.devices.len() {
                    state.ui.selected_device_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.ui.selected_device_index > 0 {
                    state.ui.selected_device_index -= 1;
                }
            }
            KeyCode::Enter => {
                if let Some(device) = state.data.devices.get(state.ui.selected_device_index) {
                    state.ui.device_modal_open = false;
                    let id = device.id.clone();
                    if !id.is_empty() {
                        return (true, Some(AppEvent::TransferPlayback(id)));
                    }
                }
            }
            _ => {}
        }
        return (true, None);
    }

    if state.ui.lyrics_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('L') => {
                state.ui.lyrics_modal_open = false;
            }
            _ => {}
        }
        return (true, None);
    }

    if state.ui.action_menu_open {
        let action_count = state
            .ui
            .action_menu_context
            .as_ref()
            .map_or(0, |ctx| ctx.actions().len());
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.ui.action_menu_open = false;
                state.ui.action_menu_context = None;
                state.ui.selected_action_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if state.ui.selected_action_index + 1 < action_count {
                    state.ui.selected_action_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.ui.selected_action_index > 0 {
                    state.ui.selected_action_index -= 1;
                }
            }
            KeyCode::Enter => {
                if let Some(ctx) = state.ui.action_menu_context.clone() {
                    let action_idx = state.ui.selected_action_index;
                    state.ui.action_menu_open = false;
                    state.ui.action_menu_context = None;
                    state.ui.selected_action_index = 0;
                    return (true, handle_action_menu_action(state, ctx, action_idx));
                }
            }
            _ => {}
        }
        return (true, None);
    }

    (false, None)
}

fn handle_action_menu_action(
    state: &mut AppState,
    ctx: crate::models::ActionMenuContext,
    action_idx: usize,
) -> Option<AppEvent> {
    let action = ctx.actions().get(action_idx).copied()?;
    match action {
        ActionMenuAction::GoToAlbum => {
            if ctx.source == crate::models::TrackSource::Local {
                if !ctx.album_name.is_empty() {
                    let album = ctx.album_name;
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
                                .is_some_and(|local| local.album == album)
                        })
                        .collect();
                    if !tracks.is_empty() {
                        state.show_generated_tracks(
                            tracks,
                            TrackListContext::generated(format!("local-album:{album}"), album),
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
        ActionMenuAction::GoToArtist => {
            if ctx.source == crate::models::TrackSource::Local && !ctx.artist_name.is_empty() {
                let artist = ctx.artist_name.clone();
                let tracks: Vec<_> = state
                    .data
                    .local_library
                    .to_tracks()
                    .into_iter()
                    .filter(|track| track.artist == artist)
                    .collect();
                if !tracks.is_empty() {
                    state.show_generated_tracks(
                        tracks,
                        TrackListContext::generated(format!("local-artist:{artist}"), artist),
                    );
                }
            } else if let Some(artist_id) = ctx.artist_id {
                state.begin_artist_page_load(artist_id.clone(), ctx.artist_name.clone(), None);
                return Some(AppEvent::LoadArtistPage {
                    artist_id,
                    artist_name: Some(ctx.artist_name),
                    artist_image_url: None,
                });
            }
        }
        ActionMenuAction::AddToPlaylist => {
            state.ui.action_menu_context = None;
            state.ui.operation_register = vec![ctx.track_id];
            state.ui.playlist_add_modal_open = true;
            state.ui.selected_playlist_modal_index = 0;
        }
        ActionMenuAction::AddToQueue => {
            return Some(AppEvent::AddToQueue(vec![ctx.track_id]));
        }
        ActionMenuAction::ToggleLike => {
            let is_liked = state.data.liked_tracks.contains(&ctx.track_id);
            if is_liked {
                state.data.liked_tracks.remove(&ctx.track_id);
            } else {
                state.data.liked_tracks.insert(ctx.track_id.clone());
            }
            return Some(AppEvent::ToggleTrackLike(ctx.track_id, !is_liked));
        }
        ActionMenuAction::ToggleSavedAlbum => {
            if let Some(album_id) = ctx.album_id {
                let saved = state.data.saved_albums.iter().any(|album| album.id == album_id);
                return Some(if saved {
                    AppEvent::RemoveAlbums(vec![album_id])
                } else {
                    AppEvent::SaveAlbums(vec![album_id])
                });
            }
        }
        ActionMenuAction::CopyLink => {
            match crate::platform::copy_to_clipboard(&format!(
                "https://open.spotify.com/track/{}",
                ctx.track_id
            )) {
                Ok(()) => set_action_status(state, "Spotify link copied"),
                Err(error) => set_action_status(state, &format!("Copy failed: {error}")),
            }
        }
        ActionMenuAction::CopyPath => {
            if let Some(path) = ctx.local_path {
                match crate::platform::copy_to_clipboard(&path.to_string_lossy()) {
                    Ok(()) => set_action_status(state, "File path copied"),
                    Err(error) => set_action_status(state, &format!("Copy failed: {error}")),
                }
            }
        }
        ActionMenuAction::OpenFolder => {
            if let Some(path) = ctx.local_path
                && let Err(error) = crate::platform::reveal_file(&path)
            {
                set_action_status(state, &format!("Unable to open file manager: {error}"));
            }
        }
    }
    None
}

fn set_action_status(state: &mut AppState, message: &str) {
    state.ui.status_message = Some(message.to_string());
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn playlist_modal_choices(state: &AppState) -> Vec<Playlist> {
    let mut playlists: Vec<Playlist> = state
        .data
        .playlists
        .iter()
        .filter(|p| Some(&p.owner_id) == state.data.user_id.as_ref())
        .cloned()
        .collect();
    playlists.extend(state.data.local_playlists.to_library_playlists());
    playlists
}

fn selected_tracks_for_playlist(state: &AppState) -> Vec<Track> {
    match state.ui.active_view {
        ActiveView::TrackList => state
            .data
            .tracks
            .get(state.ui.selected_track_index)
            .cloned()
            .into_iter()
            .collect(),
        ActiveView::SearchResults
            if state.ui.active_search_tab == crate::app::SearchTab::Tracks =>
        {
            state
                .data
                .search_results
                .tracks
                .get(state.ui.selected_search_index)
                .map(track_from_search_track)
                .into_iter()
                .collect()
        }
        ActiveView::Queue => state
            .data
            .queue
            .get(state.ui.selected_queue_index)
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
        .data
        .tracks
        .iter()
        .chain(state.data.queue.iter())
        .find(|track| track.id == id)
        .cloned()
        .or_else(|| {
            state
                .data
                .search_results
                .tracks
                .iter()
                .find(|track| track.id == id)
                .map(track_from_search_track)
        })
        .or_else(|| {
            state
                .data
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
        album: track.album.clone(),
        added_at: None,
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
        artist_id: track.artist_id.clone(),
    }
}
