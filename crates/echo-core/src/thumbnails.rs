use std::collections::HashMap;
use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::app::AppState;
use crate::events::WorkerEvent;

/// Width of the cover image in terminal cells.
pub const THUMB_W: u16 = 6;
/// Height of the cover image in terminal cells.
pub const THUMB_H: u16 = 3;
/// Height of one library row in thumbnail mode.
pub const ROW_H: u16 = 3;

const MAX_IN_FLIGHT: usize = 4;
const MAX_MEMORY_ENTRIES: usize = 300;
const MAX_DISK_FILES: usize = 500;

pub enum ThumbState {
    Loading,
    Ready {
        artwork: crate::artwork::SharedArtwork,
    },
    Failed,
}

#[derive(Default)]
pub struct ThumbnailCache {
    pub entries: HashMap<String, ThumbState>,
    pending: Vec<String>,
    disk_pruned: bool,
}

impl ThumbnailCache {
    /// Called from the renderer for each visible row whose thumbnail is not
    /// yet loaded. Actual spawning happens later in `drain_pending`.
    pub fn request(&mut self, url: &str) {
        if self.entries.contains_key(url) || self.pending.iter().any(|u| u == url) {
            return;
        }
        self.pending.push(url.to_string());
    }

    /// Whether any request is waiting for `drain_pending`.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn get(&self, url: &str) -> Option<&ThumbState> {
        self.entries.get(url)
    }

    fn loading_count(&self) -> usize {
        self.entries
            .values()
            .filter(|s| matches!(s, ThumbState::Loading))
            .count()
    }

    /// Drop decoded entries when the cache grows past the cap, keeping
    /// in-flight loads and the most recently requested urls. Evicted covers
    /// reload cheaply from the disk byte cache when they scroll back on
    /// screen.
    fn evict_if_needed(&mut self, keep: &[String]) {
        if self.entries.len() <= MAX_MEMORY_ENTRIES {
            return;
        }
        self.entries.retain(|url, state| {
            matches!(state, ThumbState::Loading) || keep.iter().any(|k| k == url)
        });
    }
}

pub fn thumbs_dir() -> PathBuf {
    crate::config::echo_config_root().join("thumbs")
}

/// Stable on-disk location for a thumbnail. Spotify image URLs end in a
/// unique hex id which doubles as the filename; anything else falls back to
/// an FNV-1a hash of the full URL (DefaultHasher is not stable across runs).
pub fn disk_path(url: &str) -> PathBuf {
    let segment = url
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or_default();
    let name = if segment.len() >= 8 && segment.chars().all(|c| c.is_ascii_alphanumeric()) {
        segment.to_string()
    } else {
        format!("{:016x}", fnv1a(url.as_bytes()))
    };
    thumbs_dir().join(name)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Keep the disk cache bounded: delete oldest files by mtime past the cap.
fn prune_disk(max_files: usize) {
    let Ok(read_dir) = std::fs::read_dir(thumbs_dir()) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = read_dir
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((
                meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                entry.path(),
            ))
        })
        .collect();
    if files.len() <= max_files {
        return;
    }
    files.sort_by_key(|(mtime, _)| *mtime);
    let excess = files.len() - max_files;
    for (_, path) in files.into_iter().take(excess) {
        let _ = std::fs::remove_file(path);
    }
}

/// Called from the main loop after each draw: spawns downloads for
/// thumbnails the renderer requested this frame, bounded to a few at a time.
/// Leftover requests are dropped — the renderer re-requests anything still
/// visible on the next frame, so fast scrolling naturally coalesces.
pub fn drain_pending(state: &mut AppState, tx: &mpsc::Sender<WorkerEvent>) {
    if state.ui.thumbnails.pending.is_empty() {
        return;
    }
    if !state.ui.thumbnails.disk_pruned {
        state.ui.thumbnails.disk_pruned = true;
        prune_disk(MAX_DISK_FILES);
    }
    let visible = std::mem::take(&mut state.ui.thumbnails.pending);
    let mut slots = MAX_IN_FLIGHT.saturating_sub(state.ui.thumbnails.loading_count());
    for url in &visible {
        if slots == 0 {
            break;
        }
        if state.ui.thumbnails.entries.contains_key(url) {
            continue;
        }
        state
            .ui
            .thumbnails
            .entries
            .insert(url.clone(), ThumbState::Loading);
        crate::image_tasks::spawn_thumbnail_processing(url.clone(), tx.clone());
        slots -= 1;
    }
    state.ui.thumbnails.evict_if_needed(&visible);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_path_uses_spotify_url_segment() {
        let url = "https://i.scdn.co/image/ab67616d00004851b0fe40a6e1692822115acfce";
        let path = disk_path(url);
        assert_eq!(
            path.file_name().unwrap().to_str().unwrap(),
            "ab67616d00004851b0fe40a6e1692822115acfce"
        );
    }

    #[test]
    fn disk_path_falls_back_to_stable_hash() {
        let url = "file://C:/music/art/漢字.jpg";
        let first = disk_path(url);
        let second = disk_path(url);
        assert_eq!(first, second);
        let name = first.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(name.len(), 16);
        assert!(name.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, disk_path("file://C:/music/art/other.jpg"));
    }

    #[test]
    fn request_dedupes_pending_and_existing() {
        let mut cache = ThumbnailCache::default();
        cache.request("a");
        cache.request("a");
        assert_eq!(cache.pending.len(), 1);
        cache.entries.insert("b".to_string(), ThumbState::Failed);
        cache.request("b");
        assert_eq!(cache.pending.len(), 1);
    }

    #[test]
    fn eviction_keeps_loading_and_visible() {
        let mut cache = ThumbnailCache::default();
        for i in 0..(MAX_MEMORY_ENTRIES + 10) {
            cache.entries.insert(format!("u{i}"), ThumbState::Failed);
        }
        cache
            .entries
            .insert("loading".to_string(), ThumbState::Loading);
        let visible = vec!["u1".to_string()];
        cache.evict_if_needed(&visible);
        assert!(cache.entries.contains_key("loading"));
        assert!(cache.entries.contains_key("u1"));
        assert!(cache.entries.len() <= 2);
    }
}
