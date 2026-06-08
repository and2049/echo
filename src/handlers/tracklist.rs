use crate::{
    app::AppState,
    events::AppEvent,
    models::{PlaybackTarget, Track, TrackListContext, TrackSource},
};

pub fn play_selected(state: &AppState) -> Option<AppEvent> {
    let track = state.data.tracks.get(state.ui.selected_track_index)?;
    let context = state.data.active_tracklist_context.as_ref()?;
    let target = if track.source == TrackSource::Local {
        let tracks: Vec<_> = state.data
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

pub fn mark_selected_for_delete(state: &mut AppState) {
    if let Some(track) = state.data.tracks.get(state.ui.selected_track_index)
        && let Some(context) = &state.data.active_tracklist_context
        && context.can_modify_playlist(state.data.user_id.as_ref())
    {
        if state.ui.pending_d_press {
            state.ui.track_delete_prompt = Some((context.id.clone(), vec![track.id.clone()]));
            state.ui.pending_d_press = false;
        } else {
            state.ui.pending_d_press = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TrackListContextKind, TrackSource};
    use std::path::PathBuf;

    #[test]
    fn generated_context_playback_never_masquerades_as_playlist() {
        let track = Track {
            id: "track".to_string(),
            source: crate::models::TrackSource::Spotify,
            local_path: None,
            name: "Track".to_string(),
            artist: "Artist".to_string(),
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        };
        let context = TrackListContext::generated("TOP_TRACKS", "Top Tracks");

        let Some(AppEvent::PlayTrack { target, .. }) = play_event(&track, &context) else {
            panic!("expected play event");
        };

        assert_eq!(context.kind, TrackListContextKind::Generated);
        assert_eq!(
            target,
            crate::models::PlaybackTarget::SpotifyTrack {
                track_id: "track".to_string()
            }
        );
    }

    #[test]
    fn selected_local_track_uses_ordered_local_context_target() {
        let mut state = AppState::new();
        state.data.active_tracklist_context = Some(TrackListContext::local_library());
        state.data.tracks = vec![
            local_track("local:a", "/music/a.wav"),
            local_track("local:b", "/music/b.wav"),
            local_track("local:c", "/music/c.wav"),
        ];
        state.ui.selected_track_index = 1;

        let Some(AppEvent::PlayTrack {
            target, track_id, ..
        }) = play_selected(&state)
        else {
            panic!("expected local play event");
        };

        assert_eq!(track_id, "local:b");
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
    fn selected_local_track_context_excludes_spotify_entries() {
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
        state.ui.selected_track_index = 2;

        let Some(AppEvent::PlayTrack { target, .. }) = play_selected(&state) else {
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

    fn local_track(id: &str, path: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Local,
            local_path: Some(PathBuf::from(path)),
            name: id.to_string(),
            artist: "Artist".to_string(),
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
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        }
    }
}
