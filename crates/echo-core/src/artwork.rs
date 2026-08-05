//! Decoded cover art, as raw pixels, and the half-block sampler that renders it.
//!
//! Replaces `ratatui_image::protocol::StatefulProtocol`. The old type was stateful and expensive
//! to re-render — every draw re-emitted terminal escapes — which is why the views cached an
//! off-screen `Buffer` beside each one and tracked a `header_image_dirty` flag. Raw pixels are
//! stateless and cheap to resample, so all of that bookkeeping is gone: the views just draw.
//!
//! The sampler lives here rather than in the TUI because it is pure pixel math with no terminal
//! dependency: each cell renders `▀` with the foreground carrying the upper sample and the
//! background the lower one, giving two vertical samples per cell — the same technique as
//! `ratatui-image`'s halfblocks backend, but with the cell box under our control.

use std::sync::Arc;

/// The longest edge a cover is kept at.
///
/// Spotify serves covers at 640px, and frontends with real pixel output (a terminal speaking
/// kitty/sixel, or the desktop app) want all of it — capping lower is a visible resolution loss
/// there. The half-block sampler throws away the excess at draw time either way, so the cap only
/// bounds what the caches hold: a handful of covers at ~1.6 MB each.
pub const MAX_COVER_EDGE: u32 = 640;

/// The edge length a library thumbnail is kept at.
///
/// Thumbnails render into a 6x3 cell box — 12 samples across — and there can be hundreds cached.
pub const THUMB_EDGE: u32 = 64;

/// Upper half block: foreground paints the top sample, background the bottom.
const HALF_BLOCK: char = '▀';

/// Vertical samples resolved per character cell.
pub const SAMPLES_PER_ROW: u32 = 2;

/// Cell aspect ratio (height / width) assumed when deriving a square box.
///
/// Very nearly universal for terminal fonts. It must not require the terminal's cooperation —
/// artwork has to render in terminals that never answer a pixel-resolution query.
pub const CELL_ASPECT: u32 = 2;

/// Cell box that displays a 1:1 image squarely, given a height in rows.
///
/// ```text
/// square_box(3)  -> (6, 3)    thumbnails
/// square_box(5)  -> (10, 5)   list headers
/// square_box(10) -> (20, 10)  now-playing cover
/// ```
pub const fn square_box(rows: u32) -> (u32, u32) {
    (rows * CELL_ASPECT, rows)
}

/// An RGBA color sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };

    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// An RGBA8 image, tightly packed, `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artwork {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Artwork {
    /// Decodes and downscales, applying the optional pixelation effect first.
    ///
    /// `pixelate` reproduces the `:pixelate` command: shrink to N pixels with nearest-neighbour,
    /// then blow it back up, so the coarse blocks survive the later downscale.
    pub fn decode(bytes: &[u8], pixelate: u32, max_edge: u32) -> Option<Self> {
        let mut image = image::load_from_memory(bytes).ok()?;

        if pixelate > 0 {
            let (width, height) = (image.width(), image.height());
            image = image
                .resize(pixelate, pixelate, image::imageops::FilterType::Nearest)
                .resize(width, height, image::imageops::FilterType::Nearest);
        }

        if image.width() > max_edge || image.height() > max_edge {
            // Triangle rather than Nearest: this is a plain downscale, and nearest-neighbour makes
            // a 640px cover visibly noisy. Pixelation, when asked for, already happened above.
            image = image.resize(
                max_edge,
                max_edge,
                if pixelate > 0 {
                    image::imageops::FilterType::Nearest
                } else {
                    image::imageops::FilterType::Triangle
                },
            );
        }

        let rgba = image.to_rgba8();
        Some(Self {
            width: rgba.width(),
            height: rgba.height(),
            pixels: rgba.into_raw(),
        })
    }

    /// Bytes held, for reasoning about cache size.
    pub fn byte_len(&self) -> usize {
        self.pixels.len()
    }

    fn sample_box(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> Rgba {
        // Area-average over the source region for this sample. Nearest-neighbour is visibly
        // noisy when a 640px cover collapses to 20 samples.
        let x1 = x1.max(x0 + 1).min(self.width);
        let y1 = y1.max(y0 + 1).min(self.height);
        let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * self.width + x) * 4) as usize;
                if i + 3 >= self.pixels.len() {
                    continue;
                }
                r += self.pixels[i] as u32;
                g += self.pixels[i + 1] as u32;
                b += self.pixels[i + 2] as u32;
                a += self.pixels[i + 3] as u32;
                n += 1;
            }
        }
        if n == 0 {
            return Rgba::TRANSPARENT;
        }
        Rgba::from_rgba((r / n) as u8, (g / n) as u8, (b / n) as u8, (a / n) as u8)
    }
}

/// Shared so a cover can sit in the cache and in playback state without being copied.
pub type SharedArtwork = Arc<Artwork>;

/// One cell's two vertical samples: the upper half and the lower half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub top: Rgba,
    pub bottom: Rgba,
}

impl Cell {
    /// The glyph that renders this cell.
    ///
    /// A uniform cell needs no half-block: drawing a blank with matching colors keeps the frame
    /// diff smaller and avoids a seam on terminals that render `▀` a pixel off.
    pub fn glyph(&self) -> char {
        if self.top == self.bottom { ' ' } else { HALF_BLOCK }
    }
}

/// Resolves `artwork` into a `cols` x `rows` grid of half-block cells, row-major.
///
/// The image is fitted to the box by area-averaging; it is not letterboxed, so pass a box whose
/// aspect already matches (see [`square_box`]) or the image will stretch.
pub fn sample_cells(artwork: &Artwork, cols: u32, rows: u32) -> Vec<Cell> {
    if cols == 0 || rows == 0 || artwork.width == 0 || artwork.height == 0 {
        return Vec::new();
    }
    let sample_rows = rows * SAMPLES_PER_ROW;
    let mut cells = Vec::with_capacity((cols * rows) as usize);

    for cy in 0..rows {
        for cx in 0..cols {
            let sx0 = cx * artwork.width / cols;
            let sx1 = (cx + 1) * artwork.width / cols;

            let top_row = cy * SAMPLES_PER_ROW;
            let ty0 = top_row * artwork.height / sample_rows;
            let ty1 = (top_row + 1) * artwork.height / sample_rows;
            let by0 = ty1;
            let by1 = (top_row + 2) * artwork.height / sample_rows;

            cells.push(Cell {
                top: artwork.sample_box(sx0, ty0, sx1, ty1),
                bottom: artwork.sample_box(sx0, by0, sx1, by1),
            });
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG of a solid red square, encoded at test time so no binary fixture is needed.
    fn red_png(size: u32) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        let image = image::RgbaImage::from_pixel(size, size, image::Rgba([255, 0, 0, 255]));
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .expect("encode");
        buffer.into_inner()
    }

    fn solid(w: u32, h: u32, c: [u8; 4]) -> Artwork {
        Artwork {
            width: w,
            height: h,
            pixels: (0..w * h).flat_map(|_| c).collect(),
        }
    }

    #[test]
    fn decoding_yields_tightly_packed_rgba() {
        let art = Artwork::decode(&red_png(8), 0, MAX_COVER_EDGE).expect("decode");
        assert_eq!((art.width, art.height), (8, 8));
        assert_eq!(art.pixels.len(), 8 * 8 * 4);
        assert_eq!(&art.pixels[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn an_oversized_cover_is_capped_to_the_max_edge() {
        let art = Artwork::decode(&red_png(MAX_COVER_EDGE + 160), 0, MAX_COVER_EDGE).expect("decode");
        assert_eq!(art.width.max(art.height), MAX_COVER_EDGE);
        assert_eq!(art.pixels.len() as u32, art.width * art.height * 4);
    }

    #[test]
    fn a_small_cover_is_left_alone() {
        let art = Artwork::decode(&red_png(32), 0, MAX_COVER_EDGE).expect("decode");
        assert_eq!((art.width, art.height), (32, 32));
    }

    #[test]
    fn thumbnails_are_capped_far_smaller_than_covers() {
        let art = Artwork::decode(&red_png(640), 0, THUMB_EDGE).expect("decode");
        assert_eq!(art.width.max(art.height), THUMB_EDGE);
        // Three hundred of these is a few megabytes, not a few hundred.
        assert!(art.byte_len() <= (THUMB_EDGE * THUMB_EDGE * 4) as usize);
    }

    #[test]
    fn pixelation_survives_the_downscale() {
        // A pixelated cover keeps hard blocks; check it still decodes to the capped size and did
        // not go transparent or empty along the way.
        let art = Artwork::decode(&red_png(MAX_COVER_EDGE + 160), 8, MAX_COVER_EDGE).expect("decode");
        assert_eq!(art.width.max(art.height), MAX_COVER_EDGE);
        assert_eq!(&art.pixels[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn garbage_bytes_fail_rather_than_panic() {
        assert!(Artwork::decode(b"not an image", 0, MAX_COVER_EDGE).is_none());
        assert!(Artwork::decode(&[], 0, MAX_COVER_EDGE).is_none());
    }

    #[test]
    fn square_box_matches_echos_existing_cover_sizes() {
        assert_eq!(square_box(3), (6, 3));
        assert_eq!(square_box(5), (10, 5));
        assert_eq!(square_box(10), (20, 10));
    }

    #[test]
    fn the_sampler_accepts_what_decode_produces() {
        let art = Artwork::decode(&red_png(64), 0, MAX_COVER_EDGE).expect("decode");
        let cells = sample_cells(&art, 6, 3);
        assert_eq!(cells.len(), 18);
        // A solid red source resolves to solid red cells.
        assert_eq!(cells[0].top.r, 255);
        assert_eq!(cells[0].top, cells[0].bottom);
    }

    #[test]
    fn sample_box_averages_the_region() {
        // Left half black, right half white.
        let (w, h) = (4u32, 1u32);
        let mut px = Vec::new();
        for x in 0..w {
            let v = if x < 2 { 0 } else { 255 };
            px.extend_from_slice(&[v, v, v, 255]);
        }
        let img = Artwork { width: w, height: h, pixels: px };
        assert_eq!(img.sample_box(0, 0, 2, 1).r, 0);
        assert_eq!(img.sample_box(2, 0, 4, 1).r, 255);
        // Averaging across the seam gives the midpoint.
        assert_eq!(img.sample_box(0, 0, 4, 1).r, 127);
    }

    #[test]
    fn sample_box_clamps_to_image_bounds() {
        let img = solid(2, 2, [10, 20, 30, 255]);
        let c = img.sample_box(0, 0, 99, 99);
        assert_eq!((c.r, c.g, c.b), (10, 20, 30));
    }

    #[test]
    fn sample_box_of_an_empty_region_is_transparent() {
        let img = solid(2, 2, [255, 255, 255, 255]);
        assert_eq!(img.sample_box(5, 5, 5, 5), Rgba::TRANSPARENT);
    }

    #[test]
    fn vertical_detail_resolves_to_two_samples_per_cell() {
        // Top half red, bottom half blue; one row of cells means the split must land on the
        // cell's top/bottom pair rather than averaging into one colour.
        let (w, h) = (8u32, 8u32);
        let mut px = Vec::new();
        for y in 0..h {
            for _ in 0..w {
                if y < h / 2 {
                    px.extend_from_slice(&[255, 0, 0, 255]);
                } else {
                    px.extend_from_slice(&[0, 0, 255, 255]);
                }
            }
        }
        let img = Artwork { width: w, height: h, pixels: px };
        let cells = sample_cells(&img, 4, 1);
        assert_eq!(cells[0].top.r, 255);
        assert_eq!(cells[0].bottom.b, 255);
        assert_eq!(cells[0].glyph(), HALF_BLOCK);
    }

    #[test]
    fn uniform_cells_avoid_the_half_block_glyph() {
        let img = solid(16, 16, [200, 100, 50, 255]);
        for cell in sample_cells(&img, 4, 2) {
            assert_eq!(cell.glyph(), ' ');
        }
    }

    #[test]
    fn zero_sized_requests_are_ignored() {
        let img = solid(4, 4, [255, 255, 255, 255]);
        assert!(sample_cells(&img, 0, 2).is_empty());
        assert!(sample_cells(&img, 2, 0).is_empty());
    }
}
