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
pub fn run(
    state: &mut AppState,
    ctx: ActionMenuContext,
    action: ActionMenuAction,
) -> Option<AppEvent> {
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
                let context =
                    TrackListContext::album(album_id, ctx.album_name, ctx.artist_name, None);
                state.begin_tracklist_load(context.clone());
                return Some(AppEvent::LoadContextTracks(context));
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
            state.ui.playlist_add_filter.clear();
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
                let saved = state
                    .data
                    .saved_albums
                    .iter()
                    .any(|album| album.id == album_id);
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
            if saved {
                "Remove album from library"
            } else {
                "Save album to library"
            }
            .to_string()
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
    let playlist = playlist.clone();
    state.ui.playlist_add_modal_open = false;
    state.ui.selected_playlist_modal_index = 0;
    state.ui.playlist_add_filter.clear();
    if tracks.is_empty() {
        return None;
    }
    stage_playlist_add(state, &playlist, tracks)
}

pub fn find_duplicates(candidates: &[Track], existing: &[Track]) -> Vec<String> {
    let existing: std::collections::HashSet<_> = existing.iter().map(|t| t.id.as_str()).collect();
    let mut seen = std::collections::HashSet::new();
    candidates
        .iter()
        .filter(|t| existing.contains(t.id.as_str()) && seen.insert(t.id.as_str()))
        .map(|t| t.id.clone())
        .collect()
}

/// The worker event that adds `tracks` to a playlist, at the end or before `position`.
pub fn playlist_add_event(
    playlist_id: String,
    tracks: Vec<Track>,
    position: Option<usize>,
) -> AppEvent {
    match position {
        Some(position) => AppEvent::InsertTracksInPlaylist {
            playlist_id,
            tracks,
            position,
        },
        None => AppEvent::AddTracksToPlaylist(playlist_id, tracks),
    }
}

pub fn stage_playlist_add(
    state: &mut AppState,
    playlist: &Playlist,
    tracks: Vec<Track>,
) -> Option<AppEvent> {
    stage_playlist_add_at(state, playlist, tracks, None)
}

/// Adds `tracks` to `playlist` unless some are already in it, in which case the duplicate
/// prompt is staged instead and the eventual add keeps `position`.
pub fn stage_playlist_add_at(
    state: &mut AppState,
    playlist: &Playlist,
    tracks: Vec<Track>,
    position: Option<usize>,
) -> Option<AppEvent> {
    if tracks.is_empty() {
        return None;
    }
    let cache = crate::config::AppConfig::load_cache();
    let existing = if state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|c| c.id == playlist.id)
    {
        state.data.tracks.clone()
    } else if playlist.id.starts_with("local-playlist:") {
        state
            .data
            .local_playlists
            .tracks_for_playlist(&playlist.id, &state.data.local_library)
    } else {
        cache
            .context_tracks
            .values()
            .find(|entry| entry.value.context.id == playlist.id)
            .map(|entry| entry.value.tracks.clone())
            .unwrap_or_default()
    };
    let duplicates = find_duplicates(&tracks, &existing);
    if !duplicates.is_empty() {
        state.ui.duplicate_prompt = Some(crate::intent::DuplicatePrompt {
            playlist_id: playlist.id.clone(),
            playlist_name: playlist.name.clone(),
            tracks,
            duplicates,
            position,
        });
        return None;
    }
    Some(playlist_add_event(playlist.id.clone(), tracks, position))
}

pub fn filtered_playlist_add_choices(state: &AppState) -> Vec<(usize, Playlist)> {
    let query = state.ui.playlist_add_filter.to_lowercase();
    playlist_add_choices(state)
        .into_iter()
        .enumerate()
        .filter(|(_, p)| p.name.to_lowercase().contains(&query))
        .collect()
}

pub fn commit_filtered_playlist_add(state: &mut AppState, index: usize) -> Option<AppEvent> {
    if index == 0 {
        let tracks = if state.ui.operation_register.is_empty() {
            selected_tracks_for_playlist(state)
        } else {
            resolve_tracks_by_ids(state, &state.ui.operation_register)
        };
        if tracks.is_empty() {
            return None;
        }
        let name = if tracks.len() == 1 {
            tracks[0].name.clone()
        } else {
            crate::i18n::t("desktop.new_playlist", &state.ui.library_config.language)
        };
        cancel_playlist_add(state);
        return Some(AppEvent::CreatePlaylistWithTracks { name, tracks });
    }
    let (original, _) = filtered_playlist_add_choices(state).get(index - 1)?.clone();
    commit_playlist_add(state, original)
}

/// Close the add-to-playlist picker without adding. Clears the operation register so a later
/// open can't silently add the previously staged track.
pub fn cancel_playlist_add(state: &mut AppState) {
    state.ui.playlist_add_filter.clear();
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
        .chain(state.data.top_tracks.iter())
        .chain(state.data.recently_played.iter())
        .chain(
            state
                .data
                .artist_page_data
                .iter()
                .flat_map(|page| page.top_tracks.iter()),
        )
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
    #[test]
    fn duplicates_use_ids_preserve_candidate_order_and_are_unique() {
        let mut a = spotify_track("a");
        let mut b = spotify_track("b");
        a.name = "Same".into();
        b.name = "Same".into();
        assert_eq!(
            find_duplicates(&[b.clone(), a.clone(), b.clone()], &[a.clone(), b]),
            ["b", "a"]
        );
        assert!(find_duplicates(&[a], &[]).is_empty());
        assert!(find_duplicates(&[spotify_track("a")], &[spotify_track("b")]).is_empty());
    }

    #[test]
    fn filtered_picker_maps_choices_and_keeps_new_playlist_first() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::TrackList;
        state.data.user_id = Some("me".into());
        state.data.playlists = vec![
            owned_playlist("first", "me"),
            owned_playlist("other", "them"),
            owned_playlist("SECOND", "me"),
        ];
        state.ui.playlist_add_filter = "second".into();
        let choices = filtered_playlist_add_choices(&state);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].0, 1);
        state.data.tracks = vec![spotify_track("track")];
        assert!(
            matches!(commit_filtered_playlist_add(&mut state, 1), Some(AppEvent::AddTracksToPlaylist(id, _)) if id == "SECOND")
        );
        assert!(state.ui.playlist_add_filter.is_empty());
        state.ui.playlist_add_filter = "no matches".into();
        assert!(
            matches!(commit_filtered_playlist_add(&mut state, 0), Some(AppEvent::CreatePlaylistWithTracks { name, tracks }) if name == "track track" && tracks.len() == 1)
        );
    }

    #[test]
    fn duplicate_confirmation_preserves_all_or_skips_existing_tracks() {
        let mut state = AppState::new();
        let playlist = owned_playlist("p", "me");
        state.data.active_tracklist_context = Some(TrackListContext::playlist(
            "p".into(),
            "P".into(),
            "Me".into(),
            "me".into(),
            None,
        ));
        state.data.tracks = vec![spotify_track("existing")];
        let tracks = vec![spotify_track("existing"), spotify_track("new")];
        assert!(stage_playlist_add(&mut state, &playlist, tracks.clone()).is_none());
        assert!(crate::intent::prompt_active(&state));
        assert!(
            matches!(crate::intent::confirm_duplicate_skip(&mut state), Some(AppEvent::AddTracksToPlaylist(_, tracks)) if tracks.len() == 1 && tracks[0].id == "new")
        );
        stage_playlist_add(&mut state, &playlist, tracks.clone());
        assert!(
            matches!(crate::intent::confirm_prompt(&mut state), Some(AppEvent::AddTracksToPlaylist(_, tracks)) if tracks.len() == 2)
        );
        stage_playlist_add(&mut state, &playlist, vec![spotify_track("existing")]);
        assert!(crate::intent::confirm_duplicate_skip(&mut state).is_none());
        stage_playlist_add(&mut state, &playlist, tracks);
        crate::intent::cancel_prompt(&mut state);
        assert!(!crate::intent::prompt_active(&state));
    }

    #[test]
    fn visual_selection_flows_through_duplicate_detection() {
        let mut state = AppState::new();
        state.data.user_id = Some("me".into());
        state.data.playlists = vec![owned_playlist("p", "me")];
        state.data.active_tracklist_context = Some(TrackListContext::playlist(
            "p".into(),
            "P".into(),
            "Me".into(),
            "me".into(),
            None,
        ));
        state.ui.active_view = ActiveView::TrackList;
        state.data.tracks = vec![spotify_track("a"), spotify_track("b")];
        crate::intent::enter_visual(&mut state);
        state.ui.selected_track_index = 1;
        crate::intent::add_visual_selection_to_playlist(&mut state);
        assert!(commit_playlist_add(&mut state, 0).is_none());
        let prompt = state.ui.duplicate_prompt.as_ref().unwrap();
        assert_eq!(prompt.tracks.len(), 2);
        assert_eq!(prompt.duplicates, ["a", "b"]);
    }

    #[test]
    fn opening_an_album_clears_the_previous_header_details() {
        let mut state = AppState::new();
        state.data.active_context_details = Some(crate::context_details::ContextDetails::default());
        let mut track = spotify_track("track");
        track.album_id = Some("album".into());
        track.album = "Album".into();
        assert!(matches!(
            run(
                &mut state,
                ActionMenuContext::from(&track),
                ActionMenuAction::GoToAlbum
            ),
            Some(AppEvent::LoadContextTracks(_))
        ));
        assert!(state.data.active_context_details.is_none());
        assert_eq!(
            state.data.active_tracklist_context.as_ref().unwrap().id,
            "album"
        );
        assert_eq!(state.ui.active_view, ActiveView::TrackList);
    }

    use super::*;
    use crate::models::TrackSource;

    fn spotify_track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
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
            artists: Vec::new(),
        }
    }

    fn owned_playlist(id: &str, owner_id: &str) -> Playlist {
        Playlist {
            description: None,
            public: None,
            collaborative: false,
            track_count: None,
            snapshot_id: None,
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
