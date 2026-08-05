//! Cover art rendering for the ratatui views.
//!
//! Two paths, chosen once at startup by querying the terminal:
//!
//! * A pixel graphics protocol (kitty, sixel, iTerm2) via `ratatui-image` — full-resolution
//!   covers, the same fidelity the pre-workspace-split app had.
//! * The half-block sampler from [`echo_core::artwork`] — two vertical samples per cell, for
//!   terminals that never answer the graphics query.
//!
//! The protocol objects `ratatui-image` needs are stateful and tied to this frontend, so they
//! live here in a thread-local cache keyed by the artwork's `Arc` address, invisible to callers:
//! the views just call [`draw`] with the raw pixels either way. The render loop is
//! single-threaded, so a thread-local is safe and keeps the cache out of every render signature.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use echo_core::artwork::{self, SharedArtwork};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::StatefulWidget;
use ratatui_image::StatefulImage;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

/// Protocol cache cap: room for the ~300-entry thumbnail cache plus the large covers. Entries
/// rebuild cheaply from the retained `Artwork`, so overflow just clears the map.
const PROTOCOL_CACHE_CAP: usize = 400;

struct PixelRenderer {
    picker: Picker,
    /// Keyed by `Arc` address. Each entry retains its `SharedArtwork`, so an address can't be
    /// freed and reused by a different image while the entry lives.
    protocols: HashMap<usize, (SharedArtwork, StatefulProtocol)>,
}

thread_local! {
    static PIXEL: RefCell<Option<PixelRenderer>> = const { RefCell::new(None) };
}

/// Queries the terminal for a pixel graphics protocol. Call once at startup, from the thread
/// that renders. Terminals that answer "halfblocks" (or not at all) keep the built-in sampler,
/// which draws the same thing without protocol state.
pub fn init_picker() {
    if let Ok(picker) = Picker::from_query_stdio()
        && picker.protocol_type() != ProtocolType::Halfblocks
    {
        PIXEL.with(|cell| {
            *cell.borrow_mut() = Some(PixelRenderer {
                picker,
                protocols: HashMap::new(),
            });
        });
    }
}

/// Draws `artwork` into `area` at the best fidelity the terminal offers.
pub fn draw(buffer: &mut Buffer, area: Rect, artwork: &SharedArtwork) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let drawn_as_pixels = PIXEL.with(|cell| {
        let mut cell = cell.borrow_mut();
        let Some(renderer) = cell.as_mut() else {
            return false;
        };
        let key = Arc::as_ptr(artwork) as usize;
        if !renderer.protocols.contains_key(&key) {
            let Some(image) = to_dynamic_image(artwork) else {
                return false;
            };
            if renderer.protocols.len() >= PROTOCOL_CACHE_CAP {
                renderer.protocols.clear();
            }
            let protocol = renderer.picker.new_resize_protocol(image);
            renderer.protocols.insert(key, (artwork.clone(), protocol));
        }
        let (_, protocol) = renderer.protocols.get_mut(&key).expect("just inserted");
        StatefulImage::default().render(area, buffer, protocol);
        true
    });

    if !drawn_as_pixels {
        half_blocks(buffer, area, artwork);
    }
}

fn to_dynamic_image(artwork: &SharedArtwork) -> Option<image::DynamicImage> {
    image::RgbaImage::from_raw(artwork.width, artwork.height, artwork.pixels.clone())
        .map(image::DynamicImage::ImageRgba8)
}

/// The fallback: one `▀` per cell, foreground/background carrying the two vertical samples.
fn half_blocks(buffer: &mut Buffer, area: Rect, artwork: &SharedArtwork) {
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
