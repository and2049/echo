mod app;
mod config;
mod events;
mod handlers;
mod models;
mod tui;
mod worker;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use std::panic;
use std::time::Duration;
use tokio::sync::mpsc;

use app::AppState;
use events::{AppEvent, WorkerEvent};
use tui::Tui;
use worker::Worker;

fn spawn_track_image_processing(
    track_id: String,
    url: String,
    picker: &ratatui_image::picker::Picker,
    tx: mpsc::Sender<WorkerEvent>,
) {
    let picker_clone = picker.clone();

    tokio::spawn(async move {
        if let Ok(resp) = reqwest::get(&url).await {
            if let Ok(bytes) = resp.bytes().await {
                if let Ok(image_handle) = tokio::task::spawn_blocking(move || {
                    if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                        if let Ok(protocol) = picker_clone.new_protocol(
                            dyn_img,
                            ratatui::layout::Size::new(10, 5),
                            ratatui_image::Resize::Fit(None),
                        ) {
                            return Some(protocol);
                        }
                    }
                    None
                })
                .await
                {
                    if let Some(protocol) = image_handle {
                        let _ = tx
                            .send(WorkerEvent::TrackImageProcessed { track_id, protocol })
                            .await;
                    }
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(panic_info);
    }));

    let (app_tx, worker_rx) = mpsc::channel::<AppEvent>(32);
    let (worker_tx, mut app_rx) = mpsc::channel::<WorkerEvent>(32);
    let worker_tx_clone = worker_tx.clone();

    let worker = Worker::new(worker_rx, worker_tx);
    tokio::spawn(async move {
        worker.run().await;
    });

    let config = config::AppConfig::load();
    let mut state = AppState::new();
    state.library_config = config.library.clone();

    // Initialize image graphics picker (Guesses Sixel, Kitty, or Halfblocks based on terminal)
    if let Ok(picker) = ratatui_image::picker::Picker::from_query_stdio() {
        state.image_picker = Some(picker);
    }

    if config.spotify_credentials.is_some() {
        state.mode = app::AppMode::Authenticating;
        let _ = app_tx.send(AppEvent::StartAuth).await;
    } else {
        state.mode = app::AppMode::Setup;
    }

    let mut tui = Tui::new()?;
    tui.enter()?;

    while state.is_running {
        let force_clear = state.needs_terminal_clear;
        tui.apply_background(state.active_theme.background, force_clear)?;
        state.needs_terminal_clear = false;
        tui.terminal.draw(|f| {
            tui::render::render_app(f, &state);
        })?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let event = AppEvent::Key(key);
                    let mut outgoing_event = None;
                    if let Some(cmd) = handlers::handle_event(&mut state, &event) {
                        outgoing_event = Some(cmd);
                    }

                    if !state.is_running {
                        let _ = app_tx.send(AppEvent::Quit).await;
                    } else {
                        let _ = app_tx.send(event).await;

                        if let Some(ev) = outgoing_event {
                            let _ = app_tx.send(ev).await;
                        }
                    }
                }
            }
        }

        while let Ok(worker_event) = app_rx.try_recv() {
            match worker_event {
                WorkerEvent::AuthenticationComplete => {
                    state.mode = app::AppMode::Normal;
                }
                WorkerEvent::PlaylistsLoaded(playlists) => {
                    state.playlists = playlists;
                    state.compute_library_view();
                }
                WorkerEvent::AlbumsLoaded(albums) => {
                    state.saved_albums = albums;
                }
                WorkerEvent::TracksLoaded(tracks) => {
                    state.tracks = tracks;
                    state.active_view = app::ActiveView::TrackList;
                    state.selected_track_index = 0;
                }
                WorkerEvent::PlaybackStarted { item } => {
                    state.playback.is_playing = true;
                    state.playback.playing_track_id = Some(item.id.clone());
                    state.playback.playing_track_title = item.title.clone();
                    state.playback.playing_track_artist = item.artist.clone();
                    state.playback.playing_track_image = None;
                    state.playback.duration_ms = item.duration_ms;
                    state.playback.progress_ms = 0;

                    if let Some(url) = item.image_url {
                        if let Some(ref picker) = state.image_picker {
                            spawn_track_image_processing(
                                item.id,
                                url,
                                picker,
                                worker_tx_clone.clone(),
                            );
                        }
                    } else {
                        let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id)).await;
                    }
                }
                WorkerEvent::Tick => {
                    if state.playback.is_playing {
                        state.playback.progress_ms += 1000;
                        // Only auto-stop if duration_ms is known (> 0); when it's 0 we are
                        // in the transitional window after a skip, waiting for SyncPlaybackState.
                        if state.playback.duration_ms > 0
                            && state.playback.progress_ms >= state.playback.duration_ms
                        {
                            state.playback.is_playing = false; // song ended
                        }
                    }
                }
                WorkerEvent::SyncPlaybackState {
                    is_playing,
                    is_shuffled,
                    repeat_mode,
                    volume,
                    device_name,
                    progress_ms,
                    item,
                } => {
                    state.playback.is_playing = is_playing;
                    state.playback.is_shuffled = is_shuffled;
                    state.playback.repeat_mode = repeat_mode;
                    if let Some(volume) = volume {
                        state.playback.volume = volume;
                    }
                    state.playback.device_name = device_name;
                    state.playback.progress_ms = progress_ms;

                    if let Some(item) = item {
                        let track_changed =
                            state.playback.playing_track_id.as_deref() != Some(item.id.as_str());

                        state.playback.playing_track_id = Some(item.id.clone());
                        state.playback.playing_track_title = item.title.clone();
                        state.playback.playing_track_artist = item.artist.clone();
                        state.playback.duration_ms = item.duration_ms;

                        if track_changed {
                            state.playback.playing_track_image = None;
                        }

                        if let Some(url) = item.image_url {
                            if let Some(ref picker) = state.image_picker {
                                let should_process_image =
                                    track_changed || state.playback.playing_track_image.is_none();
                                if should_process_image {
                                    spawn_track_image_processing(
                                        item.id,
                                        url,
                                        picker,
                                        worker_tx_clone.clone(),
                                    );
                                }
                            }
                        } else if track_changed || state.playback.playing_track_artist.is_empty() {
                            let _ = app_tx.send(AppEvent::LoadTrackMetadata(item.id)).await;
                        }
                    }
                }
                WorkerEvent::TrackMetadataLoaded {
                    track_id,
                    title,
                    artist,
                    image_url,
                } => {
                    if state.playback.playing_track_id.as_deref() != Some(track_id.as_str()) {
                        continue;
                    }

                    state.playback.playing_track_title = title;
                    state.playback.playing_track_artist = artist;

                    if let Some(url) = image_url {
                        if let Some(ref picker) = state.image_picker {
                            spawn_track_image_processing(
                                track_id,
                                url,
                                picker,
                                worker_tx_clone.clone(),
                            );
                        }
                    }
                }
                WorkerEvent::TrackImageProcessed { track_id, protocol } => {
                    if state.playback.playing_track_id.as_deref() == Some(track_id.as_str()) {
                        state.playback.playing_track_image = Some(protocol);
                    }
                }
                WorkerEvent::ForceRedraw => {
                    let _ = tui.terminal.clear();
                }
                WorkerEvent::SearchResultsLoaded(results) => {
                    state.search_results = results;
                    state.selected_search_index = 0;
                    state.active_view = app::ActiveView::SearchResults;
                    state.status_message = Some(format!("Search: {}", state.search_context_query));
                }
            }
        }
    }

    tui.exit()?;
    Ok(())
}
