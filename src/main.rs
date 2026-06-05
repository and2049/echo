mod app;
mod apply_worker_event;
mod config;
mod events;
mod handlers;
mod i18n;
mod image_tasks;
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
    let cache = config::AppConfig::load_cache();
    state.liked_tracks = cache.liked_tracks.clone();
    if let Some(playlists) = cache.get_playlists() {
        state.playlists = playlists;
        state.compute_library_view();
    }
    if let Some(albums) = cache.get_saved_albums() {
        state.saved_albums = albums;
    }
    if let Some(tracks) = cache.get_top_tracks() {
        state.top_tracks = tracks;
    }
    if let Some(tracks) = cache.get_recently_played() {
        state.recently_played = tracks;
    }
    if let Some(artists) = cache.get_followed_artists() {
        state.followed_artists = artists;
    }
    state.library_config = config.library.clone();

    // Initialize image graphics picker (Guesses Sixel, Kitty, or Halfblocks based on terminal)
    if let Ok(picker) = ratatui_image::picker::Picker::from_query_stdio() {
        state.image_picker = Some(picker);
    }

    if config.spotify_credentials.is_some() {
        state.mode = app::AppMode::Authenticating;
        let _ = app_tx.send(AppEvent::StartAuth).await;
    } else if config.library.local_music_dir.is_some() {
        state.mode = app::AppMode::Normal;
    } else {
        state.mode = app::AppMode::Setup;
    }
    if let Some(path) = startup_local_auto_refresh_path(&config) {
        let _ = app_tx
            .send(AppEvent::StartLocalLibraryAutoRefresh(path))
            .await;
    }

    let mut tui = Tui::new()?;
    tui.enter()?;

    let mut is_first_frame = true;

    while state.is_running {
        let mut needs_draw = is_first_frame;
        is_first_frame = false;

        if let Some(expiry) = state.status_message_expiry
            && std::time::Instant::now() >= expiry
        {
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
            && key.kind == KeyEventKind::Press
        {
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
                    if let AppEvent::LoadContextTracks(ref context) = ev {
                        if let Some(url) = context.image_url.as_ref() {
                            state.tracklist_image_url = Some(url.clone());
                            image_tasks::spawn_header_for_url(
                                url,
                                state.image_picker.as_ref(),
                                worker_tx_clone.clone(),
                                state.library_config.cover_img_pixels,
                            );
                        }
                        let _ = app_tx.send(ev).await;
                    } else if let AppEvent::ReloadHeaderImage = ev {
                        if let Some(url) = &state.tracklist_image_url {
                            image_tasks::spawn_header_for_url(
                                url,
                                state.image_picker.as_ref(),
                                worker_tx_clone.clone(),
                                state.library_config.cover_img_pixels,
                            );
                        }
                    } else {
                        let _ = app_tx.send(ev).await;
                    }
                }
            }
        }

        while let Ok(worker_event) = app_rx.try_recv() {
            needs_draw = true;
            apply_worker_event::apply_worker_event(
                worker_event,
                &mut state,
                &app_tx,
                &worker_tx_clone,
                &mut tui,
            )
            .await;
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

fn startup_local_auto_refresh_path(config: &config::AppConfig) -> Option<std::path::PathBuf> {
    config.library.local_music_dir.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_local_path_starts_auto_refresh_on_startup() {
        let path = std::path::PathBuf::from("/music");
        let mut config = config::AppConfig::default();
        config.library.local_music_dir = Some(path.clone());

        assert_eq!(startup_local_auto_refresh_path(&config), Some(path));
    }

    #[test]
    fn missing_local_path_skips_auto_refresh_on_startup() {
        let config = config::AppConfig::default();

        assert_eq!(startup_local_auto_refresh_path(&config), None);
    }
}
