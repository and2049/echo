use crate::{
    app::{ActiveView, AppState},
    events::AppEvent,
    models::{LibraryNode, Playlist, Track, TrackListContextKind, TrackSource},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistEditFocus {
    Name,
    Description,
}

#[derive(Clone, Debug)]
pub struct PlaylistEditDraft {
    pub id: String,
    pub name: String,
    pub description: String,
    pub public: bool,
    pub focus: PlaylistEditFocus,
}

#[derive(Clone, Debug)]
pub struct DuplicatePrompt {
    pub playlist_id: String,
    pub playlist_name: String,
    pub tracks: Vec<Track>,
    pub duplicates: Vec<String>,
    pub position: Option<usize>,
}

pub fn owns_playlist(state: &AppState, id: &str) -> bool {
    if id == "LIKED_SONGS" || id.starts_with("local-") {
        return false;
    }
    let Some(user) = state.data.user_id.as_ref() else {
        return false;
    };
    state
        .data
        .playlists
        .iter()
        .any(|p| p.id == id && &p.owner_id == user)
        || state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|c| {
                c.id == id
                    && c.kind == TrackListContextKind::Playlist
                    && c.owner_id.as_ref() == Some(user)
            })
}

pub fn open_playlist_edit(state: &mut AppState, id: &str) {
    if !owns_playlist(state, id) {
        return;
    }
    let playlist = state.data.playlists.iter().find(|p| p.id == id);
    let context = state
        .data
        .active_tracklist_context
        .as_ref()
        .filter(|c| c.id == id);
    let details = context.and(state.data.active_context_details.as_ref());
    state.ui.playlist_edit = Some(PlaylistEditDraft {
        id: id.into(),
        name: playlist
            .map(|p| p.name.clone())
            .or_else(|| context.map(|c| c.title.clone()))
            .unwrap_or_default(),
        description: details
            .and_then(|d| d.description.clone())
            .or_else(|| playlist.and_then(|p| p.description.clone()))
            .unwrap_or_default(),
        public: details
            .and_then(|d| d.public)
            .or_else(|| playlist.and_then(|p| p.public))
            .unwrap_or(false),
        focus: PlaylistEditFocus::Name,
    });
}

pub fn cancel_playlist_edit(state: &mut AppState) {
    state.ui.playlist_edit = None;
}

pub fn apply_playlist_details(
    state: &mut AppState,
    id: &str,
    name: &str,
    description: &str,
    public: bool,
) {
    if let Some(p) = state.data.playlists.iter_mut().find(|p| p.id == id) {
        p.name = name.into();
        p.description = Some(description.into());
        p.public = Some(public);
    }
    if let Some(c) = state
        .data
        .active_tracklist_context
        .as_mut()
        .filter(|c| c.id == id && c.kind == TrackListContextKind::Playlist)
    {
        c.title = name.into();
        let details = state.data.active_context_details.get_or_insert_default();
        details.description = Some(description.into());
        details.public = Some(public);
    }
    state.compute_library_view();
}

pub fn submit_playlist_edit(state: &mut AppState) -> Option<AppEvent> {
    let draft = state.ui.playlist_edit.as_ref()?;
    if draft.id.starts_with("local-playlist:")
        && !draft.name.trim().is_empty()
        && state
            .data
            .local_playlists
            .playlists
            .iter()
            .any(|p| p.id == draft.id)
    {
        let draft = state.ui.playlist_edit.take()?;
        return Some(AppEvent::RenamePlaylist(draft.id, draft.name.trim().into()));
    }
    if draft.name.trim().is_empty() || !owns_playlist(state, &draft.id) {
        return None;
    }
    let draft = state.ui.playlist_edit.take()?;
    Some(AppEvent::UpdatePlaylistDetails {
        id: draft.id,
        name: draft.name.trim().into(),
        description: draft.description,
        public: draft.public,
    })
}

pub fn toggle_follow_playlist(state: &AppState, id: &str) -> Option<AppEvent> {
    if id.is_empty() || id == "LIKED_SONGS" || id.starts_with("local-") || owns_playlist(state, id)
    {
        return None;
    }
    Some(if state.data.playlists.iter().any(|p| p.id == id) {
        AppEvent::UnfollowPlaylist(id.into())
    } else {
        AppEvent::FollowPlaylist(id.into())
    })
}

pub fn confirm_duplicate_skip(state: &mut AppState) -> Option<AppEvent> {
    let prompt = state.ui.duplicate_prompt.take()?;
    let tracks: Vec<_> = prompt
        .tracks
        .into_iter()
        .filter(|t| !prompt.duplicates.contains(&t.id))
        .collect();
    (!tracks.is_empty()).then(|| {
        crate::action_menu::playlist_add_event(prompt.playlist_id, tracks, prompt.position)
    })
}

/// Tracks dropped on `library_view[index]`: Liked Songs saves the ones not yet liked, an owned
/// or local playlist takes them through the duplicate check, anything else is refused.
pub fn drop_tracks_on_library_row(
    state: &mut AppState,
    index: usize,
    tracks: Vec<Track>,
) -> Vec<AppEvent> {
    let Some(LibraryNode::Playlist { playlist, .. }) = state.data.library_view.get(index).cloned()
    else {
        return Vec::new();
    };
    if playlist.id == "LIKED_SONGS" {
        let unliked: Vec<String> = tracks
            .into_iter()
            .filter(|track| {
                track.source == TrackSource::Spotify && !state.data.liked_tracks.contains(&track.id)
            })
            .map(|track| track.id)
            .collect();
        return unliked
            .into_iter()
            .filter_map(|id| super::toggle_like_track(state, id))
            .collect();
    }
    if !owns_playlist(state, &playlist.id) {
        return Vec::new();
    }
    crate::action_menu::stage_playlist_add(state, &playlist, tracks)
        .into_iter()
        .collect()
}

/// Tracks dropped between the rows of the open playlist. A single row dragged within the same
/// list is reordered; anything else is inserted as copies before `index`, or appended when
/// `index` is `None`, through the duplicate check. Needs the original order on screen so the
/// row positions match Spotify's.
pub fn drop_tracks_in_tracklist(
    state: &mut AppState,
    tracks: Vec<Track>,
    same_list_row: Option<usize>,
    index: Option<usize>,
) -> Option<AppEvent> {
    if state.ui.active_view != ActiveView::TrackList {
        return None;
    }
    let len = state.data.tracks.len();
    if let Some(from) = same_list_row {
        return super::move_track_in_playlist(state, from, index.unwrap_or(len.saturating_sub(1)));
    }
    let context = state.data.active_tracklist_context.clone()?;
    if !context.can_modify_playlist(state.data.user_id.as_ref()) {
        return None;
    }
    if state.ui.track_sort != crate::app::TrackSort::Original || !state.ui.track_sort_ascending {
        state.ui.status_message = Some(crate::i18n::t(
            "messages.reorder_requires_original",
            &state.ui.library_config.language,
        ));
        state.ui.status_message_expiry =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
        return None;
    }
    let playlist = Playlist {
        description: None,
        public: None,
        collaborative: false,
        track_count: None,
        snapshot_id: None,
        id: context.id.clone(),
        name: context.title.clone(),
        owner: context.subtitle.clone(),
        owner_id: context.owner_id.clone().unwrap_or_default(),
        image_url: context.image_url.clone(),
        thumb_url: None,
    };
    let position = index.unwrap_or(len).min(len);
    crate::action_menu::stage_playlist_add_at(state, &playlist, tracks, Some(position))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistPageAction {
    Edit,
    Rename,
    ToggleSaved,
    Queue,
    Refresh,
    CopyLink,
    Delete,
}

pub fn playlist_page_actions(state: &AppState) -> Vec<PlaylistPageAction> {
    use PlaylistPageAction::*;
    let Some(c) = &state.data.active_tracklist_context else {
        return Vec::new();
    };
    match c.kind {
        TrackListContextKind::Playlist if c.id != "LIKED_SONGS" => {
            if owns_playlist(state, &c.id) {
                vec![Edit, Queue, Refresh, CopyLink, Delete]
            } else {
                vec![ToggleSaved, Queue, Refresh, CopyLink]
            }
        }
        TrackListContextKind::Album => vec![ToggleSaved, Queue, Refresh, CopyLink],
        TrackListContextKind::LocalPlaylist => vec![Rename, Refresh, Delete],
        _ => Vec::new(),
    }
}

pub fn page_context_saved(state: &AppState) -> bool {
    state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|c| {
            if c.is_album() {
                state.data.saved_albums.iter().any(|a| a.id == c.id)
            } else {
                state.data.playlists.iter().any(|p| p.id == c.id)
            }
        })
}

pub fn page_context_link(state: &AppState) -> Option<String> {
    let c = state.data.active_tracklist_context.as_ref()?;
    let kind = match c.kind {
        TrackListContextKind::Playlist if c.id != "LIKED_SONGS" => "playlist",
        TrackListContextKind::Album => "album",
        _ => return None,
    };
    Some(format!("https://open.spotify.com/{kind}/{}", c.id))
}

pub fn run_playlist_page_action(
    state: &mut AppState,
    action: PlaylistPageAction,
) -> Option<AppEvent> {
    if !playlist_page_actions(state).contains(&action) {
        return None;
    }
    let context = state.data.active_tracklist_context.clone()?;
    match action {
        PlaylistPageAction::Edit => open_playlist_edit(state, &context.id),
        PlaylistPageAction::Delete => state.ui.playlist_delete_prompt = Some(vec![context.id]),
        PlaylistPageAction::Queue => {
            let ids: Vec<_> = state.data.tracks.iter().map(|t| t.id.clone()).collect();
            return (!ids.is_empty()).then_some(AppEvent::AddToQueue(ids));
        }
        PlaylistPageAction::Refresh => return super::refresh_view(state),
        PlaylistPageAction::ToggleSaved => {
            return if context.is_album() {
                Some(if page_context_saved(state) {
                    AppEvent::RemoveAlbums(vec![context.id])
                } else {
                    AppEvent::SaveAlbums(vec![context.id])
                })
            } else {
                toggle_follow_playlist(state, &context.id)
            };
        }
        PlaylistPageAction::Rename => {
            state.ui.playlist_edit = Some(PlaylistEditDraft {
                id: context.id,
                name: context.title,
                description: String::new(),
                public: false,
                focus: PlaylistEditFocus::Name,
            });
        }
        PlaylistPageAction::CopyLink => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContext;

    fn state() -> AppState {
        let mut state = AppState::new();
        state.data.user_id = Some("me".into());
        state.data.active_tracklist_context = Some(TrackListContext::playlist(
            "p".into(),
            "Name".into(),
            "Me".into(),
            "me".into(),
            None,
        ));
        state.data.active_context_details = Some(crate::context_details::ContextDetails {
            description: Some("Description".into()),
            public: Some(true),
            ..Default::default()
        });
        state
    }

    #[test]
    fn edit_is_owned_only_and_empty_name_keeps_the_draft() {
        let mut state = state();
        open_playlist_edit(&mut state, "p");
        let draft = state.ui.playlist_edit.as_mut().unwrap();
        assert_eq!(draft.description, "Description");
        assert!(draft.public);
        draft.name = "  ".into();
        assert!(submit_playlist_edit(&mut state).is_none());
        state.ui.playlist_edit.as_mut().unwrap().name = " New ".into();
        assert!(
            matches!(submit_playlist_edit(&mut state), Some(AppEvent::UpdatePlaylistDetails { name, public: true, .. }) if name == "New")
        );
        assert!(state.ui.playlist_edit.is_none());
        state.data.user_id = Some("other".into());
        open_playlist_edit(&mut state, "p");
        assert!(state.ui.playlist_edit.is_none());
        state.data.user_id = None;
        assert!(!owns_playlist(&state, "p"));
    }

    #[test]
    fn updating_details_changes_only_the_matching_context() {
        let mut state = state();
        apply_playlist_details(&mut state, "other", "Other", "Other", false);
        assert_eq!(
            state.data.active_tracklist_context.as_ref().unwrap().title,
            "Name"
        );
        apply_playlist_details(&mut state, "p", "New", "", false);
        assert_eq!(
            state.data.active_tracklist_context.as_ref().unwrap().title,
            "New"
        );
        let details = state.data.active_context_details.as_ref().unwrap();
        assert_eq!(details.description.as_deref(), Some(""));
        assert_eq!(details.public, Some(false));
    }

    #[test]
    fn local_page_rename_targets_the_context_even_outside_the_sidebar_view() {
        let mut state = state();
        state
            .data
            .local_playlists
            .playlists
            .push(crate::models::LocalPlaylist {
                id: "local-playlist:p".into(),
                name: "Old".into(),
                created_unix_secs: 0,
                updated_unix_secs: 0,
                entries: Vec::new(),
            });
        state.data.active_tracklist_context = Some(TrackListContext::local_playlist(
            "local-playlist:p".into(),
            "Old".into(),
        ));
        run_playlist_page_action(&mut state, PlaylistPageAction::Rename);
        state.ui.playlist_edit.as_mut().unwrap().name = "New".into();
        assert!(
            matches!(submit_playlist_edit(&mut state), Some(AppEvent::RenamePlaylist(id, name)) if id == "local-playlist:p" && name == "New")
        );
        let mut playlists = state.data.local_playlists.clone();
        playlists.playlists[0].name = "New".into();
        let (app_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (worker_tx, _) = tokio::sync::mpsc::channel(1);
        crate::apply_worker_event::apply_worker_event(
            crate::events::WorkerEvent::LocalPlaylistsLoaded(playlists),
            &mut state,
            &app_tx,
            &worker_tx,
        );
        assert_eq!(
            state.data.active_tracklist_context.as_ref().unwrap().title,
            "New"
        );
    }

    #[test]
    fn page_actions_distinguish_ownership_albums_and_local_playlists() {
        use PlaylistPageAction::*;
        let mut state = state();
        assert_eq!(
            playlist_page_actions(&state),
            [Edit, Queue, Refresh, CopyLink, Delete]
        );
        assert!(toggle_follow_playlist(&state, "p").is_none());
        state.data.user_id = Some("other".into());
        assert_eq!(
            playlist_page_actions(&state),
            [ToggleSaved, Queue, Refresh, CopyLink]
        );
        assert!(matches!(
            toggle_follow_playlist(&state, "p"),
            Some(AppEvent::FollowPlaylist(_))
        ));
        state.data.playlists.push(serde_json::from_value(serde_json::json!({"id":"p","name":"P","owner":"Me","owner_id":"me","image_url":null})).unwrap());
        assert!(matches!(
            toggle_follow_playlist(&state, "p"),
            Some(AppEvent::UnfollowPlaylist(_))
        ));
        assert_eq!(
            page_context_link(&state).as_deref(),
            Some("https://open.spotify.com/playlist/p")
        );
        state.data.active_tracklist_context = Some(TrackListContext::album(
            "a".into(),
            "A".into(),
            "Artist".into(),
            None,
        ));
        assert!(matches!(
            run_playlist_page_action(&mut state, ToggleSaved),
            Some(AppEvent::SaveAlbums(_))
        ));
        assert_eq!(
            page_context_link(&state).as_deref(),
            Some("https://open.spotify.com/album/a")
        );
        state.data.active_tracklist_context = Some(TrackListContext::local_playlist(
            "local-playlist:p".into(),
            "Local".into(),
        ));
        assert_eq!(playlist_page_actions(&state), [Rename, Refresh, Delete]);
        assert!(page_context_link(&state).is_none());
        assert!(toggle_follow_playlist(&state, "LIKED_SONGS").is_none());
        assert!(toggle_follow_playlist(&state, "local-playlist:p").is_none());
    }

    fn track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    fn playlist_state() -> AppState {
        let mut state = state();
        state.ui.active_view = ActiveView::TrackList;
        state.data.tracks = vec![track("a"), track("b"), track("c")];
        state.data.original_tracks = state.data.tracks.clone();
        state
    }

    #[test]
    fn drops_into_the_open_playlist_reorder_insert_or_prompt() {
        let mut state = playlist_state();
        assert!(matches!(
            drop_tracks_in_tracklist(&mut state, vec![track("a")], Some(0), Some(2)),
            Some(AppEvent::MoveTrack { from: 0, to: 2, .. })
        ));

        let mut state = playlist_state();
        assert!(matches!(
            drop_tracks_in_tracklist(&mut state, vec![track("x"), track("y")], None, Some(1)),
            Some(AppEvent::InsertTracksInPlaylist { ref playlist_id, ref tracks, position: 1 })
                if playlist_id == "p" && tracks.len() == 2
        ));
        assert!(matches!(
            drop_tracks_in_tracklist(&mut state, vec![track("x")], None, None),
            Some(AppEvent::InsertTracksInPlaylist { position: 3, .. })
        ));

        assert!(
            drop_tracks_in_tracklist(&mut state, vec![track("b"), track("z")], None, Some(0))
                .is_none()
        );
        let prompt = state.ui.duplicate_prompt.take().unwrap();
        assert_eq!(prompt.duplicates, vec!["b"]);
        assert_eq!(prompt.position, Some(0));
        state.ui.duplicate_prompt = Some(prompt);
        assert!(matches!(
            confirm_duplicate_skip(&mut state),
            Some(AppEvent::InsertTracksInPlaylist { ref tracks, position: 0, .. }) if tracks.len() == 1
        ));

        state.ui.track_sort_ascending = false;
        assert!(drop_tracks_in_tracklist(&mut state, vec![track("x")], None, Some(0)).is_none());
        assert!(state.ui.status_message.is_some());

        state.ui.track_sort_ascending = true;
        state.data.user_id = Some("other".into());
        assert!(drop_tracks_in_tracklist(&mut state, vec![track("x")], None, Some(0)).is_none());
    }

    #[test]
    fn drops_on_library_rows_like_or_add_only_where_allowed() {
        let mut state = state();
        let liked: Playlist = serde_json::from_value(serde_json::json!({"id":"LIKED_SONGS","name":"Liked","owner":"","owner_id":"me","image_url":null})).unwrap();
        let mine: Playlist = serde_json::from_value(serde_json::json!({"id":"mine","name":"Mine","owner":"Me","owner_id":"me","image_url":null})).unwrap();
        let theirs: Playlist = serde_json::from_value(serde_json::json!({"id":"theirs","name":"Theirs","owner":"Them","owner_id":"them","image_url":null})).unwrap();
        state.data.playlists = vec![mine.clone(), theirs.clone()];
        state.data.library_view = [liked, mine, theirs]
            .into_iter()
            .map(|playlist| LibraryNode::Playlist {
                playlist,
                indent: 0,
            })
            .collect();
        state.data.liked_tracks.insert("a".to_string());

        let events = drop_tracks_on_library_row(&mut state, 0, vec![track("a"), track("b")]);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], AppEvent::ToggleTrackLike(id, true) if id == "b"));
        assert!(state.data.liked_tracks.contains("b"));

        let events = drop_tracks_on_library_row(&mut state, 1, vec![track("a")]);
        assert!(
            matches!(&events[..], [AppEvent::AddTracksToPlaylist(id, tracks)] if id == "mine" && tracks.len() == 1)
        );

        assert!(drop_tracks_on_library_row(&mut state, 2, vec![track("a")]).is_empty());
        assert!(drop_tracks_on_library_row(&mut state, 9, vec![track("a")]).is_empty());
    }
}
