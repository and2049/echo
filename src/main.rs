mod app;
mod config;
mod events;
mod models;
mod tui;
mod handlers;
mod worker;

use std::panic;
use anyhow::Result;
use tokio::sync::mpsc;
use crossterm::event::{self, Event, KeyEventKind};
use std::time::Duration;

use app::AppState;
use events::{AppEvent, WorkerEvent};
use tui::Tui;
use worker::Worker;

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
    let mut state = AppState::default();
    
    // Initialize image graphics picker (Guesses Sixel, Kitty, or Halfblocks based on terminal)
    if let Ok(mut picker) = ratatui_image::picker::Picker::from_query_stdio() {
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
                }
                WorkerEvent::TracksLoaded(tracks) => {
                    state.tracks = tracks;
                    state.active_view = app::ActiveView::TrackList;
                    state.selected_track_index = 0;
                }
                WorkerEvent::PlaybackStarted(duration) => {
                    state.playback.is_playing = true;
                    state.playback.duration_ms = duration;
                    state.playback.progress_ms = 0;
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
                WorkerEvent::SyncPlaybackState { is_playing, is_shuffled, progress_ms, duration_ms, track_id } => {
                    state.playback.is_playing = is_playing;
                    state.playback.is_shuffled = is_shuffled;
                    state.playback.progress_ms = progress_ms;
                    if duration_ms > 0 {
                        state.playback.duration_ms = duration_ms;
                    }
                    if state.playback.playing_track_id != track_id {
                        state.playback.playing_track_id = track_id.clone();
                        // Clear the old image immediately
                        state.playback.playing_track_image = None;
                        
                        if let Some(tid) = track_id {
                            let _ = app_tx.send(AppEvent::LoadTrackMetadata(tid)).await;
                        }
                    }
                }
                WorkerEvent::TrackMetadataLoaded { title, artist, image_url } => {
                    state.playback.playing_track_title = title;
                    state.playback.playing_track_artist = artist;
                    
                    if let Some(url) = image_url {
                        if let Some(ref mut picker) = state.image_picker {
                            let mut picker_clone = picker.clone();
                            let tx = worker_tx_clone.clone();
                            
                            tokio::spawn(async move {
                                if let Ok(resp) = reqwest::get(&url).await {
                                    if let Ok(bytes) = resp.bytes().await {
                                        if let Ok(image_handle) = tokio::task::spawn_blocking(move || {
                                            if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                                                // Create a fixed size protocol
                                                if let Ok(protocol) = picker_clone.new_protocol(dyn_img, ratatui::layout::Size::new(14, 5), ratatui_image::Resize::Fit(None)) {
                                                    return Some(protocol);
                                                }
                                            }
                                            None
                                        }).await {
                                            if let Some(protocol) = image_handle {
                                                let _ = tx.send(WorkerEvent::TrackImageProcessed(protocol)).await;
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
                WorkerEvent::TrackImageProcessed(protocol) => {
                    state.playback.playing_track_image = Some(protocol);
                }
                WorkerEvent::ForceRedraw => {
                    let _ = tui.terminal.clear();
                }
                _ => {}
            }
        }
    }

    tui.exit()?;
    Ok(())
}
