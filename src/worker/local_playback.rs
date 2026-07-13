use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::mpsc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use rodio::{Decoder, OutputStream, OutputStreamBuilder, Sink};

use crate::models::{PlaybackItem, Track, TrackSource};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepeatMode {
    Off,
    Track,
    Context,
}

impl Default for RepeatMode {
    fn default() -> Self {
        Self::Off
    }
}

impl RepeatMode {
    pub fn from_label(label: &str) -> Self {
        match label {
            "Track" => Self::Track,
            "Context" => Self::Context,
            _ => Self::Off,
        }
    }

    pub fn as_label(self) -> String {
        match self {
            Self::Off => "Off",
            Self::Track => "Track",
            Self::Context => "Context",
        }
        .to_string()
    }
}

#[derive(Clone, Debug)]
pub struct LocalPlaybackSnapshot {
    pub item: Option<PlaybackItem>,
    pub queue: Vec<Track>,
    pub is_playing: bool,
    pub is_shuffled: bool,
    pub repeat_mode: String,
    pub volume: u32,
    pub progress_ms: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum LocalPlaybackError {
    #[error("{0}")]
    OutputUnavailable(String),
    #[error(transparent)]
    Track(#[from] anyhow::Error),
}

pub type LocalPlaybackResult<T> = std::result::Result<T, LocalPlaybackError>;

#[derive(Clone, Debug)]
struct OutputStreamFailure {
    generation: u64,
    message: String,
}

#[derive(Clone, Debug)]
pub enum LocalPlaybackTick {
    Snapshot(LocalPlaybackSnapshot),
    OutputUnavailable {
        snapshot: LocalPlaybackSnapshot,
        message: String,
    },
}

pub struct LocalPlaybackEngine {
    queue: LocalQueue,
    output_stream: Option<OutputStream>,
    sink: Option<Sink>,
    volume: u32,
    playing: bool,
    resume_position_ms: u32,
    output_unavailable: bool,
    stream_generation: u64,
    output_error_tx: mpsc::Sender<OutputStreamFailure>,
    output_error_rx: mpsc::Receiver<OutputStreamFailure>,
}

// LocalPlaybackEngine is only ever used from a single tokio task.
// The audio backend (rodio/cpal) reports non-Send on macOS due to
// CoreAudio callback types, but is safe to move between threads when accessed exclusively from one task.
unsafe impl Send for LocalPlaybackEngine {}

impl Default for LocalPlaybackEngine {
    fn default() -> Self {
        let (output_error_tx, output_error_rx) = mpsc::channel();
        Self {
            queue: LocalQueue::default(),
            output_stream: None,
            sink: None,
            volume: 100,
            playing: false,
            resume_position_ms: 0,
            output_unavailable: false,
            stream_generation: 0,
            output_error_tx,
            output_error_rx,
        }
    }
}

impl LocalPlaybackEngine {
    pub fn play_context(
        &mut self,
        tracks: Vec<Track>,
        selected_index: usize,
    ) -> LocalPlaybackResult<LocalPlaybackSnapshot> {
        self.queue = LocalQueue::new(tracks, selected_index);
        self.resume_position_ms = 0;
        self.output_unavailable = false;
        self.start_current()?;
        Ok(self.snapshot())
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.playing = false;
        self.output_stream = None;
        self.resume_position_ms = 0;
        self.output_unavailable = false;
    }

    pub fn toggle_playback(&mut self, playing: bool) -> LocalPlaybackResult<LocalPlaybackSnapshot> {
        if playing && (self.output_unavailable || self.sink.is_none()) {
            self.resume_current()?;
        } else if let Some(sink) = self.sink.as_ref() {
            if playing { sink.play() } else { sink.pause() }
        }
        self.playing = playing;
        Ok(self.snapshot())
    }

    pub fn next(&mut self) -> LocalPlaybackResult<LocalPlaybackSnapshot> {
        if self.queue.advance() {
            self.resume_position_ms = 0;
            self.start_current()?;
        } else {
            self.stop();
        }
        Ok(self.snapshot())
    }

    pub fn previous(&mut self) -> LocalPlaybackResult<LocalPlaybackSnapshot> {
        if self.queue.retreat() {
            self.resume_position_ms = 0;
            self.start_current()?;
        } else if self
            .sink
            .as_ref()
            .is_some_and(|sink| sink.get_pos() > Duration::from_secs(3))
        {
            self.start_current()?;
        }
        Ok(self.snapshot())
    }

    pub fn set_shuffle(&mut self, shuffled: bool) -> LocalPlaybackSnapshot {
        self.queue.set_shuffle(shuffled);
        self.snapshot()
    }

    pub fn set_repeat_mode(&mut self, mode: RepeatMode) -> LocalPlaybackSnapshot {
        self.queue.repeat_mode = mode;
        self.snapshot()
    }

    pub fn set_volume(&mut self, volume: u32) -> LocalPlaybackSnapshot {
        self.volume = volume.min(100);
        if let Some(sink) = self.sink.as_ref() {
            sink.set_volume(self.volume as f32 / 100.0);
        }
        self.snapshot()
    }

    pub fn seek_to(&mut self, progress_ms: u32) -> LocalPlaybackResult<LocalPlaybackSnapshot> {
        let sink = self.sink.as_ref().context("local playback is not active")?;
        sink.try_seek(Duration::from_millis(u64::from(progress_ms)))
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(self.snapshot())
    }

    pub fn add_to_queue(&mut self, tracks: Vec<Track>) -> LocalPlaybackSnapshot {
        self.queue.append_tracks(tracks);
        self.snapshot()
    }

    pub fn tick(&mut self) -> LocalPlaybackResult<Option<LocalPlaybackTick>> {
        if let Some(message) = self.take_output_error() {
            self.disconnect_output();
            return Ok(Some(LocalPlaybackTick::OutputUnavailable {
                snapshot: self.snapshot(),
                message,
            }));
        }
        if !self.playing {
            return Ok(None);
        }

        if self.sink.as_ref().is_some_and(Sink::empty) {
            if self.queue.advance_after_end() {
                self.start_current()?;
            } else {
                self.stop();
            }
            return Ok(Some(LocalPlaybackTick::Snapshot(self.snapshot())));
        }

        Ok(Some(LocalPlaybackTick::Snapshot(self.snapshot())))
    }

    pub fn snapshot(&self) -> LocalPlaybackSnapshot {
        LocalPlaybackSnapshot {
            item: self.queue.current().map(playback_item_from_track),
            queue: self.queue.remaining_tracks(),
            is_playing: self.playing,
            is_shuffled: self.queue.shuffled,
            repeat_mode: self.queue.repeat_mode.as_label(),
            volume: self.volume,
            progress_ms: self
                .sink
                .as_ref()
                .map(|sink| sink.get_pos().as_millis().try_into().unwrap_or(u32::MAX))
                .unwrap_or(self.resume_position_ms),
        }
    }

    fn start_current(&mut self) -> LocalPlaybackResult<()> {
        let attempts = self.queue.len().max(1);
        let mut last_error = None;
        for _ in 0..attempts {
            let Some(path) = self.queue.current_local_path() else {
                last_error = Some(anyhow::anyhow!("local track is missing a file path"));
                if !self.queue.advance() {
                    break;
                }
                continue;
            };
            match build_decoder(&path) {
                Ok(source) => {
                    self.start_source(source, 0)?;
                    return Ok(());
                }
                Err(error) => {
                    last_error = Some(error);
                    if !self.queue.advance() {
                        break;
                    }
                }
            }
        }

        Err(LocalPlaybackError::Track(last_error.unwrap_or_else(|| {
            anyhow::anyhow!("local queue has no playable tracks")
        })))
    }

    fn resume_current(&mut self) -> LocalPlaybackResult<()> {
        let path = self
            .queue
            .current_local_path()
            .ok_or_else(|| anyhow::anyhow!("local track is missing a file path"))?;
        let source = build_decoder(&path)?;
        self.start_source(source, self.resume_position_ms)
    }

    fn start_source(
        &mut self,
        source: rodio::decoder::Decoder<File>,
        resume_position_ms: u32,
    ) -> LocalPlaybackResult<()> {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        if self.output_stream.is_none() {
            self.stream_generation = self.stream_generation.wrapping_add(1);
            let generation = self.stream_generation;
            let output_error_tx = self.output_error_tx.clone();
            let builder = match OutputStreamBuilder::from_default_device() {
                Ok(builder) => builder,
                Err(error) => {
                    self.output_unavailable = true;
                    self.playing = false;
                    return Err(output_error(error));
                }
            }
            .with_error_callback(move |error| {
                let _ = output_error_tx.send(OutputStreamFailure {
                    generation,
                    message: error.to_string(),
                });
            });
            let mut stream = match builder.open_stream_or_fallback() {
                Ok(stream) => stream,
                Err(error) => {
                    self.output_unavailable = true;
                    self.playing = false;
                    return Err(output_error(error));
                }
            };
            stream.log_on_drop(false);
            self.output_stream = Some(stream);
        }

        let stream = self
            .output_stream
            .as_ref()
            .context("audio output stream unavailable")?;
        let sink = Sink::connect_new(stream.mixer());
        sink.set_volume(self.volume as f32 / 100.0);
        sink.append(source);
        if resume_position_ms > 0 {
            if let Err(error) = sink.try_seek(Duration::from_millis(u64::from(resume_position_ms)))
            {
                self.output_stream = None;
                self.output_unavailable = true;
                self.playing = false;
                return Err(output_error(error));
            }
        }
        self.sink = Some(sink);
        self.playing = true;
        self.resume_position_ms = resume_position_ms;
        self.output_unavailable = false;
        Ok(())
    }

    fn take_output_error(&mut self) -> Option<String> {
        let mut active_error = None;
        while let Ok(error) = self.output_error_rx.try_recv() {
            if error.generation == self.stream_generation {
                active_error = Some(error.message);
            }
        }
        active_error
    }

    fn disconnect_output(&mut self) {
        self.resume_position_ms = self
            .sink
            .as_ref()
            .map(|sink| sink.get_pos().as_millis().try_into().unwrap_or(u32::MAX))
            .unwrap_or(self.resume_position_ms);
        if let Some(sink) = self.sink.take() {
            sink.pause();
        }
        self.output_stream = None;
        self.playing = false;
        self.output_unavailable = true;
    }
}

fn output_error(error: impl std::fmt::Display) -> LocalPlaybackError {
    LocalPlaybackError::OutputUnavailable(error.to_string())
}

fn build_decoder(path: &Path) -> anyhow::Result<rodio::decoder::Decoder<File>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let byte_len = file.metadata().ok().map(|metadata| metadata.len());
    let mut decoder = Decoder::builder().with_data(file);
    if let Some(byte_len) = byte_len {
        decoder = decoder.with_byte_len(byte_len).with_seekable(true);
    }
    if let Some(ext) = path.extension().and_then(|ext| ext.to_str()) {
        decoder = decoder.with_hint(ext);
    }
    decoder
        .build()
        .with_context(|| format!("failed to decode {}", path.display()))
}

#[derive(Clone, Debug, Default)]
pub struct LocalQueue {
    original_tracks: Vec<Track>,
    play_order: Vec<usize>,
    position: usize,
    shuffled: bool,
    repeat_mode: RepeatMode,
}

impl LocalQueue {
    pub fn new(tracks: Vec<Track>, selected_index: usize) -> Self {
        let position = selected_index.min(tracks.len().saturating_sub(1));
        Self {
            play_order: (0..tracks.len()).collect(),
            original_tracks: tracks,
            position,
            shuffled: false,
            repeat_mode: RepeatMode::Off,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.play_order
            .get(self.position)
            .and_then(|track_index| self.original_tracks.get(*track_index))
    }

    pub fn current_local_path(&self) -> Option<PathBuf> {
        self.current()?.local_path.clone()
    }

    pub fn len(&self) -> usize {
        self.play_order.len()
    }

    pub fn advance(&mut self) -> bool {
        if self.play_order.is_empty() {
            return false;
        }
        if self.position + 1 < self.play_order.len() {
            self.position += 1;
            true
        } else if self.repeat_mode == RepeatMode::Context {
            self.position = 0;
            true
        } else {
            false
        }
    }

    pub fn advance_after_end(&mut self) -> bool {
        if self.repeat_mode == RepeatMode::Track {
            return !self.play_order.is_empty();
        }
        self.advance()
    }

    pub fn retreat(&mut self) -> bool {
        if self.play_order.is_empty() {
            return false;
        }
        if self.position > 0 {
            self.position -= 1;
            true
        } else if self.repeat_mode == RepeatMode::Context {
            self.position = self.play_order.len() - 1;
            true
        } else {
            false
        }
    }

    pub fn set_shuffle(&mut self, shuffled: bool) {
        if self.shuffled == shuffled {
            return;
        }

        let current_track_id = self.current().map(|track| track.id.clone());
        self.shuffled = shuffled;
        if shuffled {
            let current_index = self.play_order.get(self.position).copied();
            let mut rest: Vec<usize> = (0..self.original_tracks.len())
                .filter(|idx| Some(*idx) != current_index)
                .collect();
            shuffle_indices(&mut rest);
            self.play_order = current_index.into_iter().chain(rest).collect();
            self.position = 0;
        } else {
            self.play_order = (0..self.original_tracks.len()).collect();
            if let Some(current_track_id) = current_track_id {
                self.position = self
                    .play_order
                    .iter()
                    .position(|idx| self.original_tracks[*idx].id == current_track_id)
                    .unwrap_or(0);
            }
        }
    }

    pub fn remaining_tracks(&self) -> Vec<Track> {
        self.play_order
            .iter()
            .skip(self.position.saturating_add(1))
            .filter_map(|idx| self.original_tracks.get(*idx).cloned())
            .collect()
    }

    pub fn append_tracks(&mut self, tracks: Vec<Track>) {
        for track in tracks {
            let idx = self.original_tracks.len();
            self.original_tracks.push(track);
            self.play_order.push(idx);
        }
    }
}

fn playback_item_from_track(track: &Track) -> PlaybackItem {
    PlaybackItem {
        id: track.id.clone(),
        source: TrackSource::Local,
        local_path: track.local_path.clone(),
        title: track.name.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
        artist_id: track.artist_id.clone(),
    }
}

fn shuffle_indices(indices: &mut [usize]) {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let mut state = seed ^ ((indices.len() as u64) << 32) ^ 0x9e37_79b9_7f4a_7c15;
    for i in (1..indices.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        indices.swap(i, (state as usize) % (i + 1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Local,
            local_path: Some(PathBuf::from(format!("/music/{id}.wav"))),
            name: id.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            duration_ms: 1_000,
            image_url: None,
            album_id: None,
            artist_id: None,
        }
    }

    fn queue() -> LocalQueue {
        LocalQueue::new(vec![track("a"), track("b"), track("c"), track("d")], 1)
    }

    #[test]
    fn next_advances_within_local_order() {
        let mut queue = queue();

        assert_eq!(queue.current().unwrap().id, "b");
        assert!(queue.advance());
        assert_eq!(queue.current().unwrap().id, "c");
    }

    #[test]
    fn previous_retreats_until_the_beginning() {
        let mut queue = queue();

        assert!(queue.retreat());
        assert_eq!(queue.current().unwrap().id, "a");
        assert!(!queue.retreat());
        assert_eq!(queue.current().unwrap().id, "a");
    }

    #[test]
    fn repeat_track_replays_same_track_after_end() {
        let mut queue = queue();
        queue.repeat_mode = RepeatMode::Track;

        assert!(queue.advance_after_end());
        assert_eq!(queue.current().unwrap().id, "b");
    }

    #[test]
    fn repeat_context_wraps_at_boundaries() {
        let mut queue = LocalQueue::new(vec![track("a"), track("b")], 1);
        queue.repeat_mode = RepeatMode::Context;

        assert!(queue.advance());
        assert_eq!(queue.current().unwrap().id, "a");
        assert!(queue.retreat());
        assert_eq!(queue.current().unwrap().id, "b");
    }

    #[test]
    fn shuffle_keeps_current_track_and_preserves_all_tracks_once() {
        let mut queue = queue();
        let current = queue.current().unwrap().id.clone();

        queue.set_shuffle(true);

        assert_eq!(queue.current().unwrap().id, current);
        let mut ids: Vec<_> = queue
            .play_order
            .iter()
            .map(|idx| queue.original_tracks[*idx].id.as_str())
            .collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn disabling_shuffle_restores_original_order_and_current_track() {
        let mut queue = queue();
        queue.set_shuffle(true);
        assert!(queue.advance());
        let current = queue.current().unwrap().id.clone();

        queue.set_shuffle(false);

        assert_eq!(queue.current().unwrap().id, current);
        let ids: Vec<_> = queue
            .play_order
            .iter()
            .map(|idx| queue.original_tracks[*idx].id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn appended_tracks_are_added_after_existing_remaining_queue() {
        let mut queue = queue();

        queue.append_tracks(vec![track("e"), track("f")]);

        let ids: Vec<_> = queue
            .remaining_tracks()
            .into_iter()
            .map(|track| track.id)
            .collect();
        assert_eq!(ids, vec!["c", "d", "e", "f"]);
    }

    #[test]
    fn output_errors_ignore_stale_stream_generations() {
        let mut engine = LocalPlaybackEngine::default();
        engine.stream_generation = 2;
        engine
            .output_error_tx
            .send(OutputStreamFailure {
                generation: 1,
                message: "old stream".to_string(),
            })
            .unwrap();
        engine
            .output_error_tx
            .send(OutputStreamFailure {
                generation: 2,
                message: "device removed".to_string(),
            })
            .unwrap();

        assert_eq!(
            engine.take_output_error().as_deref(),
            Some("device removed")
        );
    }

    #[test]
    fn disconnect_preserves_queue_and_saved_position() {
        let mut engine = LocalPlaybackEngine::default();
        engine.queue = queue();
        engine.resume_position_ms = 4_200;
        engine.playing = true;

        engine.disconnect_output();

        assert_eq!(engine.queue.current().unwrap().id, "b");
        assert_eq!(engine.snapshot().progress_ms, 4_200);
        assert!(!engine.snapshot().is_playing);
        assert!(engine.output_unavailable);
    }

    #[test]
    fn missing_track_file_is_not_an_output_device_error() {
        let mut engine = LocalPlaybackEngine::default();
        let error = engine.play_context(vec![track("missing")], 0).unwrap_err();

        assert!(matches!(error, LocalPlaybackError::Track(_)));
    }
}
