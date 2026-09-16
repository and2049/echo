//! The local play history: what echo itself played, which Spotify never records for a librespot
//! device. Kept newest first in `history.json` under the config directory, capped at
//! [`CAP`] records, and never uploaded.
//!
//! A play counts once the track has been heard for [`LISTEN_MS`] of wall-clock time, or half
//! its length when it is shorter than a minute; paused time and seeking add nothing
//! ([`observe`], run after every worker event, accrues at most a second per observation so a
//! stalled feed cannot inflate it). A track that starts over after its play was recorded is a
//! new play, so repeat-one and manual replays each get a row. The queue view's Recent tab shows
//! [`AppState::recent_plays`](crate::app::DataState::recent_plays): this list merged with
//! Spotify's recently-played feed by time ([`merged`]), the same play reported by both kept once.

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Result;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    config::echo_config_root,
    models::{PlayingContext, Track},
};

pub const CAP: usize = 500;
pub const LISTEN_MS: u32 = 30_000;
const MAX_STEP_MS: u64 = 1_000;
const REPLAY_WINDOW_MS: u32 = 5_000;
const SAME_PLAY_WINDOW_SECS: i64 = 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayRecord {
    pub track: Track,
    pub played_at: String,
    #[serde(default)]
    pub context: Option<PlayingContext>,
}

/// Listening time on the playing track, on the playback state between observations.
#[derive(Clone, Debug, PartialEq)]
pub struct ListenTimer {
    track_id: String,
    listened_ms: u64,
    recorded: bool,
    last_seen: Instant,
    last_progress: u32,
}

impl ListenTimer {
    fn start(track_id: &str, now: Instant, progress: u32) -> Self {
        Self {
            track_id: track_id.to_string(),
            listened_ms: 0,
            recorded: false,
            last_seen: now,
            last_progress: progress,
        }
    }
}

pub fn threshold_ms(duration_ms: u32) -> u32 {
    if duration_ms == 0 {
        LISTEN_MS
    } else {
        LISTEN_MS.min(duration_ms / 2).max(1)
    }
}

pub fn path() -> PathBuf {
    echo_config_root().join("history.json")
}

pub fn load() -> Vec<PlayRecord> {
    load_from(&path())
}

pub fn load_from(path: &Path) -> Vec<PlayRecord> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_to(path: &Path, records: &[PlayRecord]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string(records)?)?;
    Ok(())
}

pub fn now_stamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Accrues listening time on the playing track and records the play once it qualifies.
pub fn observe(state: &mut AppState, now: Instant) {
    let Some(track_id) = state.playback.playing_track_id.clone() else {
        state.playback.listen = None;
        return;
    };
    let progress = state.playback.display_progress_ms();
    let timer = match state.playback.listen.take() {
        Some(mut timer) if timer.track_id == track_id => {
            if state.playback.is_playing {
                let step = now.duration_since(timer.last_seen).as_millis() as u64;
                timer.listened_ms += step.min(MAX_STEP_MS);
            }
            let replayed = timer.recorded
                && progress < REPLAY_WINDOW_MS
                && timer.last_progress > progress + REPLAY_WINDOW_MS;
            if replayed {
                ListenTimer::start(&track_id, now, progress)
            } else {
                timer.last_seen = now;
                timer.last_progress = progress;
                timer
            }
        }
        _ => ListenTimer::start(&track_id, now, progress),
    };
    let qualifies =
        !timer.recorded && timer.listened_ms >= u64::from(threshold_ms(state.playback.duration_ms));
    state.playback.listen = Some(ListenTimer {
        recorded: timer.recorded || qualifies,
        ..timer
    });
    if qualifies && let Some(track) = playing_track(state) {
        record(
            state,
            PlayRecord {
                track,
                played_at: now_stamp(),
                context: state.playback.playing_context.clone(),
            },
        );
    }
}

fn playing_track(state: &AppState) -> Option<Track> {
    let playback = &state.playback;
    Some(Track {
        explicit: false,
        added_by: None,
        id: playback.playing_track_id.clone()?,
        source: playback.playing_track_source.unwrap_or_default(),
        local_path: playback.playing_track_local_path.clone(),
        name: playback.playing_track_title.clone(),
        artist: playback.playing_track_artist.clone(),
        album: String::new(),
        added_at: None,
        duration_ms: playback.duration_ms,
        image_url: playback.playing_track_image_url.clone(),
        album_id: playback.playing_track_album_id.clone(),
        artist_id: playback.playing_track_artist_id.clone(),
        artists: Vec::new(),
    })
}

pub fn record(state: &mut AppState, record: PlayRecord) {
    state.data.play_history.insert(0, record);
    state.data.play_history.truncate(CAP);
    let _ = save_to(&path(), &state.data.play_history);
    rebuild_recent(state);
}

pub fn clear(state: &mut AppState) {
    state.data.play_history.clear();
    let _ = std::fs::remove_file(path());
    rebuild_recent(state);
}

/// Spotify's recently-played feed as records; the parser keeps each play's time in `added_at`.
pub fn spotify_records(tracks: &[Track]) -> Vec<PlayRecord> {
    tracks
        .iter()
        .filter_map(|track| {
            let played_at = track.added_at.clone()?;
            Some(PlayRecord {
                track: Track {
                    added_at: None,
                    ..track.clone()
                },
                played_at,
                context: None,
            })
        })
        .collect()
}

fn stamp(record: &PlayRecord) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&record.played_at)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// Both lists by time, newest first; a play both sides report within a minute appears once.
pub fn merged(local: &[PlayRecord], spotify: &[PlayRecord]) -> Vec<PlayRecord> {
    let mut all: Vec<(DateTime<Utc>, &PlayRecord)> = local
        .iter()
        .chain(spotify)
        .filter_map(|record| Some((stamp(record)?, record)))
        .collect();
    all.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let mut out: Vec<(DateTime<Utc>, PlayRecord)> = Vec::with_capacity(all.len());
    for (at, record) in all {
        let duplicate = out
            .iter()
            .rev()
            .take_while(|(seen_at, _)| (*seen_at - at).num_seconds() <= SAME_PLAY_WINDOW_SECS)
            .any(|(_, seen)| seen.track.id == record.track.id);
        if duplicate {
            continue;
        }
        out.push((at, record.clone()));
    }
    out.into_iter().map(|(_, record)| record).collect()
}

pub fn rebuild_recent(state: &mut AppState) {
    let spotify = spotify_records(&state.data.recently_played);
    state.data.recent_plays = merged(&state.data.play_history, &spotify);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
            id: id.to_string(),
            source: crate::models::TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: "artist".to_string(),
            album: String::new(),
            added_at: None,
            duration_ms: 200_000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    fn record_at(id: &str, played_at: &str) -> PlayRecord {
        PlayRecord {
            track: track(id),
            played_at: played_at.to_string(),
            context: None,
        }
    }

    fn playing(id: &str, duration_ms: u32) -> AppState {
        let mut state = AppState::new();
        state.playback.playing_track_id = Some(id.to_string());
        state.playback.playing_track_title = id.to_string();
        state.playback.duration_ms = duration_ms;
        state.playback.is_playing = true;
        state
    }

    fn observe_for(state: &mut AppState, start: Instant, ticks: u32) -> Instant {
        let mut now = start;
        for _ in 0..ticks {
            now += Duration::from_millis(100);
            state.playback.progress_ms += 100;
            observe(state, now);
        }
        now
    }

    #[test]
    fn a_play_is_recorded_after_thirty_seconds_of_listening() {
        let mut state = playing("a", 200_000);
        let start = Instant::now();
        observe(&mut state, start);
        let now = observe_for(&mut state, start, 299);
        assert!(state.data.play_history.is_empty());
        observe_for(&mut state, now, 1);
        assert_eq!(state.data.play_history.len(), 1);
        assert_eq!(state.data.play_history[0].track.id, "a");
        assert_eq!(state.data.recent_plays.len(), 1);
        observe_for(&mut state, now, 100);
        assert_eq!(state.data.play_history.len(), 1);
    }

    #[test]
    fn short_tracks_count_at_half_their_length() {
        assert_eq!(threshold_ms(40_000), 20_000);
        assert_eq!(threshold_ms(0), LISTEN_MS);
        assert_eq!(threshold_ms(600_000), LISTEN_MS);
    }

    #[test]
    fn paused_time_and_stalled_feeds_do_not_count() {
        let mut state = playing("a", 200_000);
        let start = Instant::now();
        observe(&mut state, start);
        state.playback.is_playing = false;
        observe(&mut state, start + Duration::from_secs(120));
        state.playback.is_playing = true;
        observe(&mut state, start + Duration::from_secs(121));
        observe(&mut state, start + Duration::from_secs(600));
        assert!(state.data.play_history.is_empty());
        assert_eq!(state.playback.listen.as_ref().unwrap().listened_ms, 2_000);
    }

    #[test]
    fn a_track_starting_over_after_its_play_is_a_new_play() {
        let mut state = playing("a", 200_000);
        let start = Instant::now();
        observe(&mut state, start);
        let now = observe_for(&mut state, start, 300);
        assert_eq!(state.data.play_history.len(), 1);
        state.playback.progress_ms = 199_000;
        let now = observe_for(&mut state, now, 1);
        state.playback.progress_ms = 0;
        let now = observe_for(&mut state, now, 1);
        assert!(!state.playback.listen.as_ref().unwrap().recorded);
        observe_for(&mut state, now, 300);
        assert_eq!(state.data.play_history.len(), 2);
    }

    #[test]
    fn seeking_back_within_a_play_is_not_a_replay() {
        let mut state = playing("a", 200_000);
        let start = Instant::now();
        observe(&mut state, start);
        let now = observe_for(&mut state, start, 300);
        state.playback.progress_ms = 20_000;
        observe_for(&mut state, now, 1);
        assert!(state.playback.listen.as_ref().unwrap().recorded);
    }

    #[test]
    fn merged_orders_by_time_and_keeps_a_shared_play_once() {
        let local = vec![
            record_at("a", "2026-09-16T10:00:00.000Z"),
            record_at("b", "2026-09-16T09:00:00.000Z"),
        ];
        let spotify = vec![
            record_at("c", "2026-09-16T09:30:00Z"),
            record_at("a", "2026-09-16T10:00:20Z"),
            record_at("a", "2026-09-15T10:00:00Z"),
        ];
        let ids: Vec<_> = merged(&local, &spotify)
            .iter()
            .map(|record| record.track.id.clone())
            .collect();
        assert_eq!(ids, vec!["a", "c", "b", "a"]);
    }

    #[test]
    fn spotify_records_take_their_time_from_added_at_and_skip_undated_rows() {
        let mut dated = track("a");
        dated.added_at = Some("2026-09-16T10:00:00Z".to_string());
        let records = spotify_records(&[dated, track("b")]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].played_at, "2026-09-16T10:00:00Z");
        assert!(records[0].track.added_at.is_none());
    }

    #[test]
    fn the_file_round_trips_and_a_missing_file_is_empty() {
        let dir = std::env::temp_dir().join(format!("echo-history-{}", std::process::id()));
        let path = dir.join("history.json");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_from(&path).is_empty());
        let records = vec![record_at("a", "2026-09-16T10:00:00Z")];
        save_to(&path, &records).unwrap();
        assert_eq!(load_from(&path), records);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
