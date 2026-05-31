use tokio::sync::mpsc;
use crate::events::{AppEvent, WorkerEvent};

pub struct Worker {
    rx: mpsc::Receiver<AppEvent>,
    tx: mpsc::Sender<WorkerEvent>,
}

impl Worker {
    pub fn new(rx: mpsc::Receiver<AppEvent>, tx: mpsc::Sender<WorkerEvent>) -> Self {
        Self { rx, tx }
    }

    pub async fn run(mut self) {
        while let Some(event) = self.rx.recv().await {
            match event {
                AppEvent::Quit => break,
                _ => {
                    // Placeholder for actual Spotify/Librespot calls
                    let _ = self.tx.send(WorkerEvent::Tick).await;
                }
            }
        }
    }
}
