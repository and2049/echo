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

    let worker = Worker::new(worker_rx, worker_tx);
    tokio::spawn(async move {
        worker.run().await;
    });

    let config = config::AppConfig::load();
    let mut state = AppState::default();
    
    if config.spotify_credentials.is_none() {
        state.mode = app::AppMode::Setup;
    } else {
        state.mode = app::AppMode::Authenticating;
        let _ = app_tx.send(AppEvent::StartAuth).await;
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
                _ => {}
            }
        }
    }

    tui.exit()?;
    Ok(())
}
