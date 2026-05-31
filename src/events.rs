use crossterm::event::KeyEvent;

pub enum AppEvent {
    Key(KeyEvent),
    Quit,
}

pub enum WorkerEvent {
    Tick,
}
