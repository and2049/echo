//! Decoded cover art stored as raw pixels for frontend rendering.

use std::sync::Arc;

/// The longest edge a cover is kept at.
///
/// Spotify serves covers at 640px, and the desktop app wants all of it. The cap bounds what the
/// caches hold to a handful of covers at about 1.6 MB each.
pub const MAX_COVER_EDGE: u32 = 640;

/// The edge length a library thumbnail is kept at.
///
/// There can be hundreds of thumbnails cached, so they use a smaller decoded size.
pub const THUMB_EDGE: u32 = 64;

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
}

/// Shared so a cover can sit in the cache and in playback state without being copied.
pub type SharedArtwork = Arc<Artwork>;

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

    #[test]
    fn decoding_yields_tightly_packed_rgba() {
        let art = Artwork::decode(&red_png(8), 0, MAX_COVER_EDGE).expect("decode");
        assert_eq!((art.width, art.height), (8, 8));
        assert_eq!(art.pixels.len(), 8 * 8 * 4);
        assert_eq!(&art.pixels[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn an_oversized_cover_is_capped_to_the_max_edge() {
        let art =
            Artwork::decode(&red_png(MAX_COVER_EDGE + 160), 0, MAX_COVER_EDGE).expect("decode");
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
        assert!(art.pixels.len() <= (THUMB_EDGE * THUMB_EDGE * 4) as usize);
    }

    #[test]
    fn pixelation_survives_the_downscale() {
        // A pixelated cover keeps hard blocks; check it still decodes to the capped size and did
        // not go transparent or empty along the way.
        let art =
            Artwork::decode(&red_png(MAX_COVER_EDGE + 160), 8, MAX_COVER_EDGE).expect("decode");
        assert_eq!(art.width.max(art.height), MAX_COVER_EDGE);
        assert_eq!(&art.pixels[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn garbage_bytes_fail_rather_than_panic() {
        assert!(Artwork::decode(b"not an image", 0, MAX_COVER_EDGE).is_none());
        assert!(Artwork::decode(&[], 0, MAX_COVER_EDGE).is_none());
    }
}
