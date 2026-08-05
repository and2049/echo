//! Fetching and decoding cover art off the render thread.
//!
//! Decoding hands back raw pixels ([`Artwork`]) rather than a terminal protocol object, so no
//! `Picker` is needed and the same payload serves any frontend.

use std::sync::Arc;
use tokio::sync::mpsc;

use crate::artwork::{Artwork, MAX_COVER_EDGE, THUMB_EDGE};
use crate::events::WorkerEvent;

async fn load_image_bytes(source: &str) -> Option<Vec<u8>> {
    if source.starts_with("http://") || source.starts_with("https://") {
        reqwest::get(source)
            .await
            .ok()?
            .bytes()
            .await
            .ok()
            .map(|bytes| bytes.to_vec())
    } else {
        let path = source.strip_prefix("file://").unwrap_or(source);
        tokio::fs::read(path).await.ok()
    }
}

/// Decodes on the blocking pool, since `image` is CPU-bound and would stall the runtime.
async fn decode(bytes: Vec<u8>, pixelate: u32, max_edge: u32) -> Option<Artwork> {
    tokio::task::spawn_blocking(move || Artwork::decode(&bytes, pixelate, max_edge))
        .await
        .ok()
        .flatten()
}

pub fn spawn_track_image_processing(
    track_id: String,
    url: String,
    tx: mpsc::Sender<WorkerEvent>,
    pixels: u32,
) {
    tokio::spawn(async move {
        if let Some(bytes) = load_image_bytes(&url).await
            && let Some(artwork) = decode(bytes, pixels, MAX_COVER_EDGE).await
        {
            let _ = tx
                .send(WorkerEvent::TrackImageProcessed {
                    track_id,
                    artwork: Arc::new(artwork),
                })
                .await;
        }
    });
}

pub fn spawn_header_image_processing(url: String, tx: mpsc::Sender<WorkerEvent>, pixels: u32) {
    tokio::spawn(async move {
        if let Some(bytes) = load_image_bytes(&url).await
            && let Some(artwork) = decode(bytes, pixels, MAX_COVER_EDGE).await
        {
            let _ = tx
                .send(WorkerEvent::HeaderImageProcessed(Arc::new(artwork)))
                .await;
        }
    });
}

pub fn spawn_thumbnail_processing(url: String, tx: mpsc::Sender<WorkerEvent>) {
    tokio::spawn(async move {
        let cache_path = crate::thumbnails::disk_path(&url);
        let bytes = match tokio::fs::read(&cache_path).await {
            Ok(bytes) => Some(bytes),
            Err(_) => {
                let bytes = load_image_bytes(&url).await;
                if let Some(bytes) = &bytes {
                    if let Some(dir) = cache_path.parent() {
                        let _ = tokio::fs::create_dir_all(dir).await;
                    }
                    let _ = tokio::fs::write(&cache_path, bytes).await;
                }
                bytes
            }
        };

        let artwork = match bytes {
            // Thumbnails are never pixelated — the effect is for the large covers.
            Some(bytes) => decode(bytes, 0, THUMB_EDGE).await.map(Arc::new),
            None => None,
        };

        // Always report back, even on failure, so Loading entries resolve.
        let _ = tx.send(WorkerEvent::ThumbnailProcessed { url, artwork }).await;
    });
}

pub fn spawn_header_for_url(url: &str, tx: mpsc::Sender<WorkerEvent>, pixels: u32) {
    if !url.is_empty() {
        spawn_header_image_processing(url.to_string(), tx, pixels);
    }
}
