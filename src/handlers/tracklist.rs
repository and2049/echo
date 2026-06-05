use crate::{
    app::AppState,
    events::AppEvent,
    models::{Track, TrackListContext},
};

pub fn play_selected(state: &AppState) -> Option<AppEvent> {
    let track = state.tracks.get(state.selected_track_index)?;
    play_event(track, state.active_tracklist_context.as_ref()?)
}

pub fn play_event(track: &Track, context: &TrackListContext) -> Option<AppEvent> {
    let target = context.playback_target_for_track(track)?;
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
    if let Some(track) = state.tracks.get(state.selected_track_index)
        && let Some(context) = &state.active_tracklist_context
        && context.can_modify_playlist(state.user_id.as_ref())
    {
        if state.pending_d_press {
            state.track_delete_prompt = Some((context.id.clone(), vec![track.id.clone()]));
            state.pending_d_press = false;
        } else {
            state.pending_d_press = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContextKind;

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

        let Some(AppEvent::PlayTrack { target, .. }) = play_event(&track, &context)
        else {
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
}
