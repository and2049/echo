pub mod artist;
pub mod library;
pub mod playback;
pub mod queue;
pub mod render;
pub mod search;

use anyhow::Result;
use crossterm::style::{Color as CrosstermColor, ResetColor, SetBackgroundColor};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{backend::CrosstermBackend, style::Color as RatatuiColor, Terminal};
use std::io::{Stdout, stdout};

pub struct Tui {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
    background: Option<RatatuiColor>,
}

impl Tui {
    pub fn new() -> Result<Self> {
        let backend = CrosstermBackend::new(stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            background: None,
        })
    }

    pub fn enter(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen)?;
        self.terminal.hide_cursor()?;
        Ok(())
    }

    pub fn apply_background(&mut self, color: RatatuiColor, force_clear: bool) -> Result<()> {
        if self.background == Some(color) && !force_clear {
            return Ok(());
        }

        match to_crossterm_color(color) {
            CrosstermColor::Reset => {
                execute!(self.terminal.backend_mut(), ResetColor)?;
            }
            color => {
                execute!(self.terminal.backend_mut(), SetBackgroundColor(color))?;
            }
        }

        self.terminal.clear()?;
        execute!(self.terminal.backend_mut(), ResetColor)?;
        self.background = Some(color);
        Ok(())
    }

    pub fn exit(&mut self) -> Result<()> {
        execute!(stdout(), ResetColor, LeaveAlternateScreen)?;
        disable_raw_mode()?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

fn to_crossterm_color(color: RatatuiColor) -> CrosstermColor {
    match color {
        RatatuiColor::Reset => CrosstermColor::Reset,
        RatatuiColor::Black => CrosstermColor::Black,
        RatatuiColor::Red => CrosstermColor::DarkRed,
        RatatuiColor::Green => CrosstermColor::DarkGreen,
        RatatuiColor::Yellow => CrosstermColor::DarkYellow,
        RatatuiColor::Blue => CrosstermColor::DarkBlue,
        RatatuiColor::Magenta => CrosstermColor::DarkMagenta,
        RatatuiColor::Cyan => CrosstermColor::DarkCyan,
        RatatuiColor::Gray => CrosstermColor::Grey,
        RatatuiColor::DarkGray => CrosstermColor::DarkGrey,
        RatatuiColor::LightRed => CrosstermColor::Red,
        RatatuiColor::LightGreen => CrosstermColor::Green,
        RatatuiColor::LightYellow => CrosstermColor::Yellow,
        RatatuiColor::LightBlue => CrosstermColor::Blue,
        RatatuiColor::LightMagenta => CrosstermColor::Magenta,
        RatatuiColor::LightCyan => CrosstermColor::Cyan,
        RatatuiColor::White => CrosstermColor::White,
        RatatuiColor::Rgb(r, g, b) => CrosstermColor::Rgb { r, g, b },
        RatatuiColor::Indexed(i) => CrosstermColor::AnsiValue(i),
    }
}
