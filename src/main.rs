mod app;
mod config;
mod events;
mod handlers;
mod models;
mod tui;
mod worker;
mod i18n;

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
        if let Ok(resp) = reqwest::get(&url).await
            && let Ok(bytes) = resp.bytes().await
                && let Ok(image_handle) = tokio::task::spawn_blocking(move || {
                    if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                        let protocol = picker_clone.new_resize_protocol(dyn_img);
                        return Some(protocol);
                    }
                    None
                })
                .await
                    && let Some(protocol) = image_handle {
                        let _ = tx
                            .send(WorkerEvent::TrackImageProcessed { track_id, protocol })
                            .await;
                    }
    });
}

fn spawn_header_image_processing(
    url: String,
    picker: &ratatui_image::picker::Picker,
    tx: mpsc::Sender<WorkerEvent>,
) {
    let picker_clone = picker.clone();

    tokio::spawn(async move {
        if let Ok(resp) = reqwest::get(&url).await
            && let Ok(bytes) = resp.bytes().await
                && let Ok(image_handle) = tokio::task::spawn_blocking(move || {
                    if let Ok(dyn_img) = image::load_from_memory(&bytes) {
                        let protocol = picker_clone.new_resize_protocol(dyn_img);
                        return Some(protocol);
                    }
                    None
                })
                .await
                    && let Some(protocol) = image_handle {
                        let _ = tx
                            .send(WorkerEvent::HeaderImageProcessed(protocol))
                            .await;
                    }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    i18n::init();

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(panic_info);
    }));

    let (app_tx, worker_rx) = mpsc::channel::<AppEvent>(32);
    let (worker_tx, mut app_rx) = mpsc::channel::<WorkerEvent>(32);
    let worker_tx_clone = worker_tx.clone();
    
    let worker = Worker::new(worker_rx, worker_tx, app_tx.clone());
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

    let mut is_first_frame = true;

    while state.is_running {
        let mut needs_draw = is_first_frame;
        is_first_frame = false;

        if let Some(expiry) = state.status_message_expiry
            && std::time::Instant::now() >= expiry {
                state.status_message = None;
                state.status_message_expiry = None;
                state.recent_queue_count = 0;
                needs_draw = true;
            }

        if state.needs_terminal_clear {
            needs_draw = true;
        }

        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press {
                    needs_draw = true;
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
                            if let AppEvent::LoadContextTracks(_, _, ref img_url, _) = ev {
                                if let Some(url) = img_url
                                    && let Some(picker) = &state.image_picker {
                                        spawn_header_image_processing(url.clone(), picker, worker_tx_clone.clone());
                                }
                                let _ = app_tx.send(ev).await;
                            } else {
                                let _ = app_tx.send(ev).await;
                            }
                        }
                    }
                }

        while let Ok(worker_event) = app_rx.try_recv() {
            needs_draw = true;
            match worker_event {
                WorkerEvent::AuthenticationComplete => {
                    state.mode = app::AppMode::Normal;
                }
                WorkerEvent::ForceContextRefresh => {
                    if state.active_view == app::ActiveView::TrackList
                        && let Some((playlist_id, _, _, _)) = &state.tracklist_context_metadata {
                            let metadata = state.tracklist_context_metadata.clone();
                            let is_album = state.active_library_tab == app::LibraryTab::Albums;
                            let _ = app_tx.send(AppEvent::LoadContextTracks(playlist_id.clone(), is_album, None, metadata)).await;
                        }
                }
                WorkerEvent::UserIdentityLoaded(user_id) => {
                    state.user_id = Some(user_id);
                }
                WorkerEvent::PlaylistsLoaded(playlists) => {
                    state.playlists = playlists;
                    state.compute_library_view();
                }
                WorkerEvent::AlbumsLoaded(albums) => {
                    state.saved_albums = albums;
                }
                WorkerEvent::TracksLoaded(tracks, metadata) => {
                    state.tracks = tracks;
                    state.tracklist_context_metadata = metadata.clone();
                    state.active_view = app::ActiveView::TrackList;
                    state.selected_track_index = 0;
                    
                    if let Some(m) = metadata {
                        let url = m.3;
                        if !url.is_empty()
                            && let Some(picker) = &state.image_picker {
                                spawn_header_image_processing(url, picker, worker_tx_clone.clone());
                            }
                    }
                }
                WorkerEvent::PlaybackStarted { item } => {
                    state.playback.is_playing = true;
                    state.playback.playing_track_id = Some(item.id.clone());
                    state.playback.playing_track_title = item.title.clone();
                    state.playback.playing_track_artist = item.artist.clone();
                    state.playback.previous_track_image = state.playback.playing_track_image.take();
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
                        state.playback.progress_ms += 100;
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
                            state.playback.previous_track_image = state.playback.playing_track_image.take();
                        }

                        if let Some(url) = item.image_url {
                            if let Some(ref picker) = state.image_picker {
                                let should_process_image = track_changed
                                    || (state.playback.playing_track_image.is_none()
                                        && state.playback.fetching_track_id.as_deref() != Some(item.id.as_str()));

                                if should_process_image {
                                    state.playback.fetching_track_id = Some(item.id.clone());
                                    spawn_track_image_processing(
                                        item.id.clone(),
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

                    if let Some(url) = image_url
                        && let Some(ref picker) = state.image_picker {
                            spawn_track_image_processing(
                                track_id,
                                url,
                                picker,
                                worker_tx_clone.clone(),
                            );
                        }
                }
                WorkerEvent::TrackImageProcessed { track_id, protocol } => {
                    if state.playback.playing_track_id.as_deref() == Some(track_id.as_str()) {
                        state.playback.playing_track_image = Some(protocol);
                        state.playback.previous_track_image = None;
                        if state.playback.fetching_track_id.as_deref() == Some(track_id.as_str()) {
                            state.playback.fetching_track_id = None;
                        }
                    }
                }
                WorkerEvent::HeaderImageProcessed(protocol) => {
                    state.active_library_header_image = Some(protocol);
                    state.header_image_dirty = true;
                }
                WorkerEvent::ForceRedraw => {
                    let _ = tui.terminal.clear();
                }
                WorkerEvent::AudioVisualizationReady(shared_bands, flag) => {
                    state.playback.audio_visualization = Some(shared_bands);
                    state.playback.enable_visualizer = Some(flag);
                }
                WorkerEvent::SearchResultsLoaded(results) => {
                    state.search_results = results;
                    state.selected_search_index = 0;
                    state.active_view = app::ActiveView::SearchResults;
                    state.status_message = Some(format!("Search: {}", state.search_context_query));
                }
                WorkerEvent::QueueLoaded(tracks) => {
                    state.queue = tracks;
                    state.selected_queue_index = 0;
                }
                WorkerEvent::TracksQueued(count) => {
                    state.recent_queue_count += count;
                    state.status_message = Some(if state.recent_queue_count == 1 {
                        "Added 1 track to queue".to_string()
                    } else {
                        format!("Added {} tracks to queue", state.recent_queue_count)
                    });
                    state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                }
            }
        }

        if needs_draw {
            let force_clear = state.needs_terminal_clear;
            tui.apply_background(state.active_theme.background, force_clear)?;
            state.needs_terminal_clear = false;
            tui.terminal.draw(|f| {
                tui::render::render_app(f, &mut state);
            })?;
        }
    }

    tui.exit()?;
    Ok(())
}
