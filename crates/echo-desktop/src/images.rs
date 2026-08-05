//! [`Artwork`](echo_core::artwork::Artwork) → [`gpui::RenderImage`] conversion, cached.
//!
//! The worker delivers covers as raw RGBA pixels; gpui uploads `RenderImage`s (BGRA byte order)
//! to the GPU and caches the texture by image id, so each artwork must map to one stable
//! `RenderImage` — converting per frame would re-upload per frame. The cache keys by the
//! artwork's `Arc` address and retains the `Arc`, so an address can't be freed and reused by a
//! different image while its entry lives.

use std::collections::HashMap;
use std::sync::Arc;

use echo_core::artwork::SharedArtwork;
use gpui::RenderImage;

/// Room for the ~300-entry thumbnail cache plus the large covers. Entries rebuild cheaply from
/// the retained artwork, so overflow just clears the map.
const CACHE_CAP: usize = 400;

#[derive(Default)]
pub struct ImageCache {
    entries: HashMap<usize, (SharedArtwork, Arc<RenderImage>)>,
}

impl ImageCache {
    /// The GPU-renderable image for `artwork`, converted on first sight.
    pub fn get(&mut self, artwork: &SharedArtwork) -> Option<Arc<RenderImage>> {
        let key = Arc::as_ptr(artwork) as usize;
        if !self.entries.contains_key(&key) {
            let image = convert(artwork)?;
            if self.entries.len() >= CACHE_CAP {
                self.entries.clear();
            }
            self.entries.insert(key, (artwork.clone(), image));
        }
        Some(self.entries[&key].1.clone())
    }
}

fn convert(artwork: &SharedArtwork) -> Option<Arc<RenderImage>> {
    let mut pixels = artwork.pixels.clone();
    // gpui reads frames as BGRA (see its own decoders): swap in the copy.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let buffer = image::RgbaImage::from_raw(artwork.width, artwork.height, pixels)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}
