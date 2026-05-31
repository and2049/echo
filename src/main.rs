mod app;
mod events;
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
    // Setup custom panic hook to restore terminal state
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(panic_info);
    }));

    // Setup MPSC channels
    let (app_tx, worker_rx) = mpsc::channel::<AppEvent>(32);
    let (worker_tx, mut app_rx) = mpsc::channel::<WorkerEvent>(32);

    // Spawn background worker
    let worker = Worker::new(worker_rx, worker_tx);
    tokio::spawn(async move {
        worker.run().await;
    });

    let mut state = AppState::default();
    let mut tui = Tui::new()?;
    tui.enter()?;

    while state.is_running {
        tui.terminal.draw(|f| {
            tui::render::render_app(f, &state);
        })?;

        // Poll for terminal events without blocking the thread indefinitely
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let event = AppEvent::Key(key);
                    handlers::handle_event(&mut state, &event);
                    if state.is_running {
                        let _ = app_tx.send(event).await;
                    } else {
                        let _ = app_tx.send(AppEvent::Quit).await;
                    }
                }
            }
        }
        
        // Process any updates from the background worker
        while let Ok(_worker_event) = app_rx.try_recv() {
            // Future: update UI state with data from the worker
        }
    }

    tui.exit()?;
    Ok(())
}
