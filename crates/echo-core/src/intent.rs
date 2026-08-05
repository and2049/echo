//! Frontend-neutral user intents.
//!
//! "The user activated library row N" or "asked to play track N" means the same thing whether it
//! arrived as an Enter keypress in the TUI or a click in the desktop app: mutate selection state
//! and return the event the worker should receive. The frontends translate their input idioms
//! into these calls and send whatever comes back over `app_tx`.

use crate::app::AppState;
use crate::events::AppEvent;
use crate::models::{LibraryNode, PlaybackTarget, Track, TrackListContext, TrackSource};

/// Activates row `index` of the playlists sidebar (the `library_view` tree): opens playlists,
/// shows local collections, toggles folders.
pub fn open_library_entry(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let node = state.data.library_view.get(index).cloned()?;
    state.ui.selected_playlist_index = index;
    let context = match node {
        LibraryNode::Playlist { playlist, .. } => {
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
        LibraryNode::Folder(f) => {
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
    };

    let context = context?;
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Activates row `index` of the saved-albums list.
pub fn open_album(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let album = state.data.saved_albums.get(index)?;
    state.ui.selected_playlist_index = index;
    let context = TrackListContext::album(
        album.id.clone(),
        album.name.clone(),
        album.artists.clone(),
        album.image_url.clone(),
    );
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Plays track `index` of the current track list, selecting it first.
pub fn play_track_at(state: &mut AppState, index: usize) -> Option<AppEvent> {
    if index >= state.data.tracks.len() {
        return None;
    }
    state.ui.selected_track_index = index;
    let track = state.data.tracks.get(index)?;
    let context = state.data.active_tracklist_context.as_ref()?;
    let target = if track.source == TrackSource::Local {
        let tracks: Vec<_> = state
            .data
            .tracks
            .iter()
            .filter(|track| track.source == TrackSource::Local)
            .cloned()
            .collect();
        let selected_index = tracks
            .iter()
            .position(|local_track| local_track.id == track.id)
            .unwrap_or(0);
        PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        }
    } else {
        context.playback_target_for_track(track)?
    };
    play_event_with_target(track, target)
}

/// The play event for `track` inside `context`, with no selection side effects.
pub fn play_event(track: &Track, context: &TrackListContext) -> Option<AppEvent> {
    let target = context.playback_target_for_track(track)?;
    play_event_with_target(track, target)
}

fn play_event_with_target(track: &Track, target: PlaybackTarget) -> Option<AppEvent> {
    Some(AppEvent::PlayTrack {
        target,
        track_id: track.id.clone(),
        title: track.name.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContextKind;
    use std::path::PathBuf;

    #[test]
    fn generated_context_playback_never_masquerades_as_playlist() {
        let track = spotify_track("track");
        let context = TrackListContext::generated("TOP_TRACKS", "Top Tracks");

        let Some(AppEvent::PlayTrack { target, .. }) = play_event(&track, &context) else {
            panic!("expected play event");
        };

        assert_eq!(context.kind, TrackListContextKind::Generated);
        assert_eq!(
            target,
            PlaybackTarget::SpotifyTrack {
                track_id: "track".to_string()
            }
        );
    }

    #[test]
    fn playing_a_local_track_uses_ordered_local_context_target() {
        let mut state = AppState::new();
        state.data.active_tracklist_context = Some(TrackListContext::local_library());
        state.data.tracks = vec![
            local_track("local:a", "/music/a.wav"),
            local_track("local:b", "/music/b.wav"),
            local_track("local:c", "/music/c.wav"),
        ];

        let Some(AppEvent::PlayTrack {
            target, track_id, ..
        }) = play_track_at(&mut state, 1)
        else {
            panic!("expected local play event");
        };

        assert_eq!(track_id, "local:b");
        assert_eq!(state.ui.selected_track_index, 1);
        let PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        } = target
        else {
            panic!("expected local context target");
        };
        assert_eq!(selected_index, 1);
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[1].id, "local:b");
    }

    #[test]
    fn playing_a_local_track_excludes_spotify_entries_from_the_context() {
        let mut state = AppState::new();
        state.data.active_tracklist_context = Some(TrackListContext::local_playlist(
            "local-playlist:a".to_string(),
            "Mixed".to_string(),
        ));
        state.data.tracks = vec![
            local_track("local:a", "/music/a.wav"),
            spotify_track("spotify:a"),
            local_track("local:b", "/music/b.wav"),
        ];

        let Some(AppEvent::PlayTrack { target, .. }) = play_track_at(&mut state, 2) else {
            panic!("expected local play event");
        };

        let PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        } = target
        else {
            panic!("expected local context target");
        };
        assert_eq!(selected_index, 1);
        assert_eq!(tracks.len(), 2);
        assert!(
            tracks
                .iter()
                .all(|track| track.source == TrackSource::Local)
        );
    }

    #[test]
    fn out_of_range_indices_are_ignored() {
        let mut state = AppState::new();
        assert!(play_track_at(&mut state, 0).is_none());
        assert!(open_album(&mut state, 0).is_none());
        assert!(open_library_entry(&mut state, 0).is_none());
    }

    fn local_track(id: &str, path: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Local,
            local_path: Some(PathBuf::from(path)),
            name: id.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        }
    }

    fn spotify_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        }
    }
}
