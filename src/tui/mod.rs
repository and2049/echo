pub mod layout;
pub mod render;

use std::io::{stdout, Stdout};
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use anyhow::Result;

pub struct Tui {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Tui {
    pub fn new() -> Result<Self> {
        let backend = CrosstermBackend::new(stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self { terminal })
    }

    pub fn enter(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        self.terminal.hide_cursor()?;
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        execute!(stdout(), LeaveAlternateScreen)?;
        disable_raw_mode()?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}
