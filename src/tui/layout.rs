use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub header: Rect,
    pub main_content: Rect,
    pub command_bar: Rect,
}

impl AppLayout {
    pub fn compute(area: Rect) -> Self {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Header
                Constraint::Min(0),    // Main content
                Constraint::Length(1), // Command bar
            ])
            .split(area);

        Self {
            header: chunks[0],
            main_content: chunks[1],
            command_bar: chunks[2],
        }
    }
}
