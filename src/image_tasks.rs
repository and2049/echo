use tokio::sync::mpsc;

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

pub fn spawn_track_image_processing(
    track_id: String,
    url: String,
    picker: &ratatui_image::picker::Picker,
    tx: mpsc::Sender<WorkerEvent>,
    pixels: u32,
) {
    let picker_clone = picker.clone();

    tokio::spawn(async move {
        if let Some(bytes) = load_image_bytes(&url).await
            && let Ok(image_handle) = tokio::task::spawn_blocking(move || {
                if let Ok(mut dyn_img) = image::load_from_memory(&bytes) {
                    if pixels > 0 {
                        let pixelated =
                            dyn_img.resize(pixels, pixels, image::imageops::FilterType::Nearest);
                        dyn_img = pixelated.resize(
                            dyn_img.width(),
                            dyn_img.height(),
                            image::imageops::FilterType::Nearest,
                        );
                    }
                    let protocol = picker_clone.new_resize_protocol(dyn_img);
                    return Some(protocol);
                }
                None
            })
            .await
            && let Some(protocol) = image_handle
        {
            let _ = tx
                .send(WorkerEvent::TrackImageProcessed { track_id, protocol })
                .await;
        }
    });
}

pub fn spawn_header_image_processing(
    url: String,
    picker: &ratatui_image::picker::Picker,
    tx: mpsc::Sender<WorkerEvent>,
    pixels: u32,
) {
    let picker_clone = picker.clone();

    tokio::spawn(async move {
        if let Some(bytes) = load_image_bytes(&url).await
            && let Ok(image_handle) = tokio::task::spawn_blocking(move || {
                if let Ok(mut dyn_img) = image::load_from_memory(&bytes) {
                    if pixels > 0 {
                        let pixelated =
                            dyn_img.resize(pixels, pixels, image::imageops::FilterType::Nearest);
                        dyn_img = pixelated.resize(
                            dyn_img.width(),
                            dyn_img.height(),
                            image::imageops::FilterType::Nearest,
                        );
                    }
                    let protocol = picker_clone.new_resize_protocol(dyn_img);
                    return Some(protocol);
                }
                None
            })
            .await
            && let Some(protocol) = image_handle
        {
            let _ = tx.send(WorkerEvent::HeaderImageProcessed(protocol)).await;
        }
    });
}

pub fn spawn_thumbnail_processing(
    url: String,
    picker: &ratatui_image::picker::Picker,
    tx: mpsc::Sender<WorkerEvent>,
) {
    let picker_clone = picker.clone();

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

        let protocol = match bytes {
            Some(bytes) => tokio::task::spawn_blocking(move || {
                image::load_from_memory(&bytes)
                    .ok()
                    .map(|dyn_img| picker_clone.new_resize_protocol(dyn_img.thumbnail(64, 64)))
            })
            .await
            .ok()
            .flatten(),
            None => None,
        };

        // Always report back, even on failure, so Loading entries resolve.
        let _ = tx.send(WorkerEvent::ThumbnailProcessed { url, protocol }).await;
    });
}

pub fn spawn_header_for_url(
    url: &str,
    picker: Option<&ratatui_image::picker::Picker>,
    tx: mpsc::Sender<WorkerEvent>,
    pixels: u32,
) {
    if !url.is_empty()
        && let Some(picker) = picker
    {
        spawn_header_image_processing(url.to_string(), picker, tx, pixels);
    }
}
