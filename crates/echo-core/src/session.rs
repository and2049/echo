//! The playback session that survives a restart: the playing track, its context and position,
//! the manually queued tracks and the shuffle/repeat modes, kept in `session.json` under the
//! config directory.
//!
//! The reducer saves it whenever the session changes enough to matter ([`persist_if_changed`],
//! at most every ten seconds of progress) and the frontends save it once more on every way out
//! ([`persist`]). At launch [`restore`] puts the track back paused where it stopped and records a
//! [`PendingResume`]; play, next and previous then start it through the ordinary `PlayTrack`
//! path ([`resume_event`]), and [`complete_resume`] finishes the job once the worker confirms
//! the start: the position is kept instead of reset to zero, the saved seek is sent, and the
//! queue is re-added. Playing anything else, or a real playback report from another device,
//! drops the pending resume.

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{
    app::AppState,
    config::echo_config_root,
    events::{AppEvent, WorkerEvent},
    image_tasks,
    models::{PlaybackItem, PlaybackTarget, PlayingContext, Track, TrackSource},
};

const PROGRESS_SAVE_STEP_MS: u32 = 10_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaybackSession {
    pub item: PlaybackItem,
    #[serde(default)]
    pub context: Option<PlayingContext>,
    #[serde(default)]
    pub progress_ms: u32,
    #[serde(default)]
    pub is_shuffled: bool,
    #[serde(default)]
    pub repeat_mode: String,
    #[serde(default)]
    pub queue: Vec<Track>,
}

/// How a restored session starts again, kept on the playback state until it does.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingResume {
    pub track_id: String,
    pub target: PlaybackTarget,
    pub progress_ms: u32,
    pub queue: Vec<String>,
}

/// The parts of the session whose change is worth a save.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveMark {
    track_id: String,
    is_playing: bool,
    context: Option<PlayingContext>,
    queue_len: usize,
    progress_ms: u32,
}

impl SaveMark {
    fn of(state: &AppState) -> Option<Self> {
        Some(Self {
            track_id: state.playback.playing_track_id.clone()?,
            is_playing: state.playback.is_playing,
            context: state.playback.playing_context.clone(),
            queue_len: state.data.manual_queue.len(),
            progress_ms: state.playback.display_progress_ms(),
        })
    }

    fn needs_save(&self, last: Option<&Self>) -> bool {
        let Some(last) = last else { return true };
        self.track_id != last.track_id
            || self.is_playing != last.is_playing
            || self.context != last.context
            || self.queue_len != last.queue_len
            || self.progress_ms.abs_diff(last.progress_ms) >= PROGRESS_SAVE_STEP_MS
    }
}

pub fn path() -> PathBuf {
    echo_config_root().join("session.json")
}

pub fn load() -> Option<PlaybackSession> {
    load_from(&path())
}

pub fn load_from(path: &Path) -> Option<PlaybackSession> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_to(path: &Path, session: &PlaybackSession) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string(session)?)?;
    Ok(())
}

/// What the running state would save, or `None` while nothing has played yet.
pub fn snapshot(state: &AppState) -> Option<PlaybackSession> {
    let playback = &state.playback;
    let item = PlaybackItem {
        id: playback.playing_track_id.clone()?,
        source: playback.playing_track_source.unwrap_or_default(),
        local_path: playback.playing_track_local_path.clone(),
        title: playback.playing_track_title.clone(),
        artist: playback.playing_track_artist.clone(),
        duration_ms: playback.duration_ms,
        image_url: playback.playing_track_image_url.clone(),
        album_id: playback.playing_track_album_id.clone(),
        artist_id: playback.playing_track_artist_id.clone(),
    };
    let queue = state
        .data
        .queue
        .iter()
        .take(state.data.manual_queue.len())
        .cloned()
        .collect();
    Some(PlaybackSession {
        item,
        context: playback.playing_context.clone(),
        progress_ms: playback.display_progress_ms(),
        is_shuffled: playback.is_shuffled,
        repeat_mode: playback.repeat_mode.clone(),
        queue,
    })
}

pub fn persist(state: &AppState) {
    if let Some(session) = snapshot(state) {
        let _ = save_to(&path(), &session);
    }
}

pub fn persist_if_changed(state: &mut AppState) {
    let Some(mark) = SaveMark::of(state) else {
        return;
    };
    if !mark.needs_save(state.playback.session_mark.as_ref()) {
        return;
    }
    let Some(session) = snapshot(state) else {
        return;
    };
    if save_to(&path(), &session).is_ok() {
        state.playback.session_mark = Some(mark);
    }
}

pub fn resume_target(item: &PlaybackItem, context: Option<&PlayingContext>) -> PlaybackTarget {
    match (item.source, item.local_path.as_ref()) {
        (TrackSource::Local, Some(path)) => PlaybackTarget::LocalTrack {
            track_id: item.id.clone(),
            path: path.clone(),
        },
        _ => match context {
            Some(context) => PlaybackTarget::SpotifyContextJump {
                context_id: context.context_id.clone(),
                is_album: context.is_album,
            },
            None => PlaybackTarget::SpotifyTrack {
                track_id: item.id.clone(),
            },
        },
    }
}

/// Puts a saved session back into a fresh state, paused, and remembers how to start it.
pub fn restore(
    state: &mut AppState,
    session: PlaybackSession,
    worker_tx: &mpsc::Sender<WorkerEvent>,
) {
    let PlaybackSession {
        item,
        context,
        progress_ms,
        is_shuffled,
        repeat_mode,
        queue,
    } = session;
    let progress_ms = progress_ms.min(item.duration_ms);
    let queue_ids: Vec<String> = queue.iter().map(|track| track.id.clone()).collect();
    let playback = &mut state.playback;
    playback.is_playing = false;
    playback.playing_track_id = Some(item.id.clone());
    playback.playing_track_title = item.title.clone();
    playback.playing_track_artist = item.artist.clone();
    playback.playing_track_album_id = item.album_id.clone();
    playback.playing_track_artist_id = item.artist_id.clone();
    playback.playing_track_source = Some(item.source);
    playback.playing_track_local_path = item.local_path.clone();
    playback.playing_track_image_url = item.image_url.clone();
    playback.duration_ms = item.duration_ms;
    playback.progress_ms = progress_ms;
    playback.playing_context = context.clone();
    playback.is_shuffled = is_shuffled;
    playback.repeat_mode = repeat_mode;
    playback.playback_last_updated_at = None;
    playback.pending_resume = Some(PendingResume {
        track_id: item.id.clone(),
        target: resume_target(&item, context.as_ref()),
        progress_ms,
        queue: queue_ids.clone(),
    });
    state.data.manual_queue = queue_ids;
    state.data.queue = queue;
    if let Some(url) = item.image_url {
        image_tasks::spawn_track_image_processing(
            item.id,
            url,
            worker_tx.clone(),
            state.ui.library_config.cover_img_pixels,
        );
    }
}

/// The play event that starts the pending session, or `None` when nothing is pending.
pub fn resume_event(state: &AppState) -> Option<AppEvent> {
    let pending = state.playback.pending_resume.as_ref()?;
    let playback = &state.playback;
    Some(AppEvent::PlayTrack {
        target: pending.target.clone(),
        track_id: playback.playing_track_id.clone()?,
        title: playback.playing_track_title.clone(),
        artist: playback.playing_track_artist.clone(),
        duration_ms: playback.duration_ms,
        image_url: playback.playing_track_image_url.clone(),
        album_id: playback.playing_track_album_id.clone(),
    })
}

/// Called when the worker confirms a track started. Returns whether that start was the pending
/// resume, in which case the saved position is kept and the seek and queue are sent; any other
/// track starting drops the pending resume.
pub fn complete_resume(
    state: &mut AppState,
    app_tx: &mpsc::UnboundedSender<AppEvent>,
    started_id: &str,
) -> bool {
    let Some(pending) = state.playback.pending_resume.take() else {
        return false;
    };
    if pending.track_id != started_id {
        return false;
    }
    state.playback.progress_ms = pending.progress_ms;
    if pending.progress_ms > 0 {
        let _ = app_tx.send(AppEvent::SeekTo(pending.progress_ms));
    }
    if !pending.queue.is_empty() {
        state.data.manual_queue.clear();
        let _ = app_tx.send(AppEvent::AddToQueue(pending.queue));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> PlaybackItem {
        PlaybackItem {
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            title: format!("title {id}"),
            artist: "artist".to_string(),
            duration_ms: 240_000,
            image_url: Some("https://i.scdn.co/image/x".to_string()),
            album_id: Some("album".to_string()),
            artist_id: None,
        }
    }

    fn track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: "artist".to_string(),
            album: String::new(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    fn session() -> PlaybackSession {
        PlaybackSession {
            item: item("a"),
            context: Some(PlayingContext {
                context_id: "pl".to_string(),
                is_album: false,
            }),
            progress_ms: 65_000,
            is_shuffled: true,
            repeat_mode: "Context".to_string(),
            queue: vec![track("q1"), track("q2")],
        }
    }

    fn channels() -> (
        mpsc::UnboundedSender<AppEvent>,
        mpsc::UnboundedReceiver<AppEvent>,
        mpsc::Sender<WorkerEvent>,
    ) {
        let (app_tx, app_rx) = mpsc::unbounded_channel();
        let (worker_tx, _worker_rx) = mpsc::channel(8);
        (app_tx, app_rx, worker_tx)
    }

    #[test]
    fn a_session_round_trips_through_its_file() {
        let dir = std::env::temp_dir().join(format!("echo-session-{}", std::process::id()));
        let path = dir.join("nested").join("session.json");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_from(&path).is_none());
        save_to(&path, &session()).unwrap();
        assert_eq!(load_from(&path), Some(session()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn restore_pauses_the_track_where_it_stopped_and_shows_the_queue() {
        let (_app_tx, _app_rx, worker_tx) = channels();
        let mut state = AppState::new();
        restore(&mut state, session(), &worker_tx);

        assert!(!state.playback.is_playing);
        assert_eq!(state.playback.playing_track_id.as_deref(), Some("a"));
        assert_eq!(state.playback.progress_ms, 65_000);
        assert!(state.playback.is_shuffled);
        assert_eq!(state.data.manual_queue, vec!["q1", "q2"]);
        assert_eq!(state.data.queue.len(), 2);
        let pending = state.playback.pending_resume.as_ref().unwrap();
        assert_eq!(
            pending.target,
            PlaybackTarget::SpotifyContextJump {
                context_id: "pl".to_string(),
                is_album: false,
            }
        );
        assert_eq!(snapshot(&state), Some(session()));
    }

    #[test]
    fn a_local_track_resumes_from_its_file_and_a_bare_track_stands_alone() {
        let mut local = item("local:1");
        local.source = TrackSource::Local;
        local.local_path = Some(PathBuf::from("song.flac"));
        assert_eq!(
            resume_target(&local, None),
            PlaybackTarget::LocalTrack {
                track_id: "local:1".to_string(),
                path: PathBuf::from("song.flac"),
            }
        );
        assert_eq!(
            resume_target(&item("a"), None),
            PlaybackTarget::SpotifyTrack {
                track_id: "a".to_string()
            }
        );
    }

    #[tokio::test]
    async fn completing_the_resume_keeps_the_position_and_sends_seek_and_queue() {
        let (app_tx, mut app_rx, worker_tx) = channels();
        let mut state = AppState::new();
        restore(&mut state, session(), &worker_tx);
        let event = resume_event(&state).unwrap();
        assert!(matches!(event, AppEvent::PlayTrack { ref track_id, .. } if track_id == "a"));

        assert!(complete_resume(&mut state, &app_tx, "a"));
        assert!(state.playback.pending_resume.is_none());
        assert_eq!(state.playback.progress_ms, 65_000);
        assert!(state.data.manual_queue.is_empty());
        assert!(matches!(app_rx.try_recv(), Ok(AppEvent::SeekTo(65_000))));
        assert!(
            matches!(app_rx.try_recv(), Ok(AppEvent::AddToQueue(ids)) if ids == vec!["q1", "q2"])
        );
        assert!(resume_event(&state).is_none());
    }

    #[tokio::test]
    async fn starting_another_track_drops_the_pending_resume() {
        let (app_tx, mut app_rx, worker_tx) = channels();
        let mut state = AppState::new();
        restore(&mut state, session(), &worker_tx);
        assert!(!complete_resume(&mut state, &app_tx, "b"));
        assert!(state.playback.pending_resume.is_none());
        assert!(app_rx.try_recv().is_err());
    }

    #[test]
    fn the_save_mark_ignores_small_progress_but_not_state_changes() {
        let mut state = AppState::new();
        assert!(SaveMark::of(&state).is_none());
        state.playback.playing_track_id = Some("a".to_string());
        let first = SaveMark::of(&state).unwrap();
        assert!(first.needs_save(None));

        state.playback.progress_ms = 4_000;
        assert!(!SaveMark::of(&state).unwrap().needs_save(Some(&first)));
        state.playback.progress_ms = 12_000;
        assert!(SaveMark::of(&state).unwrap().needs_save(Some(&first)));

        state.playback.progress_ms = 0;
        state.data.manual_queue.push("q".to_string());
        assert!(SaveMark::of(&state).unwrap().needs_save(Some(&first)));
    }
}
