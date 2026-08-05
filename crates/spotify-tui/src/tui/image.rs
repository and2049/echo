//! Half-block cover art for the ratatui views.
//!
//! Blits the samples [`echo_core::artwork::sample_cells`] produces into a ratatui `Buffer`. Sampling
//! is cheap and stateless, so this runs every frame with no off-screen cache.

use echo_core::artwork::{self, Artwork};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

/// Draws `artwork` into `area`, one half-block per cell.
pub fn draw(buffer: &mut Buffer, area: Rect, artwork: &Artwork) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let cells = artwork::sample_cells(artwork, area.width as u32, area.height as u32);

    for (index, cell) in cells.iter().enumerate() {
        let x = area.x + (index as u16 % area.width);
        let y = area.y + (index as u16 / area.width);
        if x >= buffer.area.width || y >= buffer.area.height {
            continue;
        }
        let target = &mut buffer[(x, y)];
        target.set_symbol(&cell.glyph().to_string());
        target.set_style(
            Style::default()
                .fg(to_color(cell.top))
                .bg(to_color(cell.bottom)),
        );
    }
}

fn to_color(color: artwork::Rgba) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}
