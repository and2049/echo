//! Track action-menu behavior shared by both frontends: executing a menu entry, labelling it,
//! and the add-to-playlist picker it can open. The TUI presents these as the `A` popup and the
//! desktop as a right-click context menu; both resolve an [`ActionMenuAction`] and call [`run`].

use crate::app::{ActiveView, AppState, SearchTab};
use crate::events::AppEvent;
use crate::models::{
    ActionMenuAction, ActionMenuContext, Playlist, Track, TrackListContext, TrackSource,
};

/// Execute one action-menu entry, mutating state and returning the event to send, if any.
///
/// Consumes `ctx` — build an owned context (e.g. `ActionMenuContext::from(&track)`) before
/// taking the mutable state borrow.
pub fn run(state: &mut AppState, ctx: ActionMenuContext, action: ActionMenuAction) -> Option<AppEvent> {
    match action {
        ActionMenuAction::GoToAlbum => {
            if ctx.source == TrackSource::Local {
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
                return Some(AppEvent::LoadContextTracks(TrackListContext::album(
                    album_id,
                    String::new(),
                    String::new(),
                    None,
                )));
            }
        }
        ActionMenuAction::GoToArtist => {
            if ctx.source == TrackSource::Local && !ctx.artist_name.is_empty() {
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

/// Display label for one menu entry, in the configured language and reflecting current
/// liked/saved state.
pub fn label(state: &AppState, ctx: &ActionMenuContext, action: ActionMenuAction) -> String {
    let lang = &state.ui.library_config.language;
    match action {
        ActionMenuAction::GoToAlbum => crate::i18n::t("actions.go_to_album", lang),
        ActionMenuAction::GoToArtist => crate::i18n::t("actions.go_to_artist", lang),
        ActionMenuAction::AddToPlaylist => crate::i18n::t("actions.add_to_playlist", lang),
        ActionMenuAction::AddToQueue => crate::i18n::t("actions.add_to_queue", lang),
        ActionMenuAction::ToggleLike => {
            if state.data.liked_tracks.contains(&ctx.track_id) {
                crate::i18n::t("actions.unlike_track", lang)
            } else {
                crate::i18n::t("actions.like_track", lang)
            }
        }
        ActionMenuAction::ToggleSavedAlbum => {
            let saved = ctx
                .album_id
                .as_ref()
                .is_some_and(|id| state.data.saved_albums.iter().any(|album| &album.id == id));
            if saved { "Remove album from library" } else { "Save album to library" }.to_string()
        }
        ActionMenuAction::CopyLink => "Copy Spotify link".to_string(),
        ActionMenuAction::CopyPath => "Copy file path".to_string(),
        ActionMenuAction::OpenFolder => "Show in file manager".to_string(),
    }
}

fn set_action_status(state: &mut AppState, message: &str) {
    state.ui.status_message = Some(message.to_string());
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
}

/// Playlists the user can add tracks to: their own Spotify playlists plus local playlists.
pub fn playlist_add_choices(state: &AppState) -> Vec<Playlist> {
    let mut playlists: Vec<Playlist> = state
        .data
        .playlists
        .iter()
        .filter(|p| Some(&p.owner_id) == state.data.user_id.as_ref())
        .cloned()
        .collect();
    playlists.extend(
        state
            .data
            .local_playlists
            .to_library_playlists(&state.data.local_library),
    );
    playlists
}

/// Confirm the add-to-playlist picker at `choice_index`. Tracks come from the operation
/// register when set (the action-menu path), else from the current selection. Out-of-range
/// index is a no-op and leaves the modal open, matching the TUI's behavior.
pub fn commit_playlist_add(state: &mut AppState, choice_index: usize) -> Option<AppEvent> {
    let playlists = playlist_add_choices(state);
    let playlist = playlists.get(choice_index)?;
    let tracks = if !state.ui.operation_register.is_empty() {
        let ids: Vec<_> = state.ui.operation_register.drain(..).collect();
        resolve_tracks_by_ids(state, &ids)
    } else {
        selected_tracks_for_playlist(state)
    };
    let playlist_id = playlist.id.clone();
    state.ui.playlist_add_modal_open = false;
    state.ui.selected_playlist_modal_index = 0;
    if tracks.is_empty() {
        return None;
    }
    Some(AppEvent::AddTracksToPlaylist(playlist_id, tracks))
}

/// Close the add-to-playlist picker without adding. Clears the operation register so a later
/// open can't silently add the previously staged track.
pub fn cancel_playlist_add(state: &mut AppState) {
    state.ui.playlist_add_modal_open = false;
    state.ui.selected_playlist_modal_index = 0;
    state.ui.operation_register.clear();
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
        ActiveView::SearchResults if state.ui.active_search_tab == SearchTab::Tracks => state
            .data
            .search_results
            .tracks
            .get(state.ui.selected_search_index)
            .map(Track::from)
            .into_iter()
            .collect(),
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
                .map(Track::from)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackSource;

    fn spotify_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: format!("track {id}"),
            artist: "artist".to_string(),
            album: "album".to_string(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: Some("album-1".to_string()),
            artist_id: Some("artist-1".to_string()),
        }
    }

    fn owned_playlist(id: &str, owner_id: &str) -> Playlist {
        Playlist {
            id: id.to_string(),
            name: format!("playlist {id}"),
            owner: "owner".to_string(),
            owner_id: owner_id.to_string(),
            image_url: None,
            thumb_url: None,
        }
    }

    #[test]
    fn toggle_like_flips_state_and_emits_event() {
        let mut state = AppState::new();
        let ctx = ActionMenuContext::from(&spotify_track("t1"));

        let event = run(&mut state, ctx.clone(), ActionMenuAction::ToggleLike);
        assert!(state.data.liked_tracks.contains("t1"));
        assert!(matches!(event, Some(AppEvent::ToggleTrackLike(id, true)) if id == "t1"));

        let event = run(&mut state, ctx, ActionMenuAction::ToggleLike);
        assert!(!state.data.liked_tracks.contains("t1"));
        assert!(matches!(event, Some(AppEvent::ToggleTrackLike(id, false)) if id == "t1"));
    }

    #[test]
    fn add_to_playlist_stages_register_and_opens_modal() {
        let mut state = AppState::new();
        let ctx = ActionMenuContext::from(&spotify_track("t1"));

        let event = run(&mut state, ctx, ActionMenuAction::AddToPlaylist);
        assert!(event.is_none());
        assert!(state.ui.playlist_add_modal_open);
        assert_eq!(state.ui.operation_register, vec!["t1".to_string()]);
        assert_eq!(state.ui.selected_playlist_modal_index, 0);
    }

    #[test]
    fn playlist_choices_filter_by_owner() {
        let mut state = AppState::new();
        state.data.user_id = Some("me".to_string());
        state.data.playlists = vec![owned_playlist("p1", "me"), owned_playlist("p2", "them")];

        let choices = playlist_add_choices(&state);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].id, "p1");
    }

    #[test]
    fn commit_drains_register_and_emits_add() {
        let mut state = AppState::new();
        state.data.user_id = Some("me".to_string());
        state.data.playlists = vec![owned_playlist("p1", "me")];
        state.data.tracks = vec![spotify_track("t1")];
        state.ui.operation_register = vec!["t1".to_string()];
        state.ui.playlist_add_modal_open = true;

        let event = commit_playlist_add(&mut state, 0);
        assert!(state.ui.operation_register.is_empty());
        assert!(!state.ui.playlist_add_modal_open);
        match event {
            Some(AppEvent::AddTracksToPlaylist(playlist_id, tracks)) => {
                assert_eq!(playlist_id, "p1");
                assert_eq!(tracks.len(), 1);
                assert_eq!(tracks[0].id, "t1");
            }
            _ => panic!("expected AddTracksToPlaylist"),
        }
    }

    #[test]
    fn commit_out_of_range_leaves_modal_open() {
        let mut state = AppState::new();
        state.ui.playlist_add_modal_open = true;

        assert!(commit_playlist_add(&mut state, 5).is_none());
        assert!(state.ui.playlist_add_modal_open);
    }

    #[test]
    fn cancel_clears_register() {
        let mut state = AppState::new();
        state.ui.playlist_add_modal_open = true;
        state.ui.operation_register = vec!["stale".to_string()];

        cancel_playlist_add(&mut state);
        assert!(!state.ui.playlist_add_modal_open);
        assert!(state.ui.operation_register.is_empty());
    }
}
