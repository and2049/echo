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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThumbTier {
    Small,
    Card,
}

impl ThumbTier {
    pub fn edge(self) -> u32 {
        match self {
            Self::Small => crate::artwork::THUMB_EDGE,
            Self::Card => 320,
        }
    }
}

pub fn tier_for_edge(edge: f32) -> ThumbTier {
    if edge > ThumbTier::Small.edge() as f32 {
        ThumbTier::Card
    } else {
        ThumbTier::Small
    }
}

pub enum ThumbState {
    Loading,
    Ready {
        artwork: crate::artwork::SharedArtwork,
    },
    Failed,
}

#[derive(Default)]
pub struct ThumbnailCache {
    pub entries: HashMap<(String, ThumbTier), ThumbState>,
    pending: Vec<(String, ThumbTier)>,
    disk_pruned: bool,
}

impl ThumbnailCache {
    /// Called from the renderer for each visible row whose thumbnail is not
    /// yet loaded. Actual spawning happens later in `drain_pending`.
    pub fn request(&mut self, url: &str, tier: ThumbTier) {
        let key = (url.to_string(), tier);
        if self.entries.contains_key(&key) || self.pending.contains(&key) {
            return;
        }
        self.pending.push(key);
    }

    /// Whether any request is waiting for `drain_pending`.
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn get(&self, url: &str, tier: ThumbTier) -> Option<&ThumbState> {
        self.entries.get(&(url.to_string(), tier))
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
    fn evict_if_needed(&mut self, keep: &[(String, ThumbTier)]) {
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
    let mut segments = url.trim_end_matches('/').rsplit('/');
    let segment = segments.next().unwrap_or_default();
    let size = segments
        .next()
        .filter(|parent| !parent.is_empty() && parent.chars().all(|c| c.is_ascii_digit()));
    let name = if segment.len() >= 8 && segment.chars().all(|c| c.is_ascii_alphanumeric()) {
        match size {
            Some(size) => format!("{size}-{segment}"),
            None => segment.to_string(),
        }
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
    for key in &visible {
        if slots == 0 {
            break;
        }
        if state.ui.thumbnails.entries.contains_key(key) {
            continue;
        }
        state
            .ui
            .thumbnails
            .entries
            .insert(key.clone(), ThumbState::Loading);
        crate::image_tasks::spawn_thumbnail_processing(key.0.clone(), key.1, tx.clone());
        slots -= 1;
    }
    state.ui.thumbnails.evict_if_needed(&visible);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_edges_and_display_size_selection() {
        assert_eq!(ThumbTier::Small.edge(), 64);
        assert_eq!(ThumbTier::Card.edge(), 320);
        for edge in [0.0, 26.0, 32.0, 40.0, 56.0, 64.0] {
            assert_eq!(tier_for_edge(edge), ThumbTier::Small);
        }
        for edge in [64.01, 92.0, 160.0, 320.0] {
            assert_eq!(tier_for_edge(edge), ThumbTier::Card);
        }
    }

    #[test]
    fn requests_and_failures_are_independent_per_tier() {
        let mut cache = ThumbnailCache::default();
        cache.request("same", ThumbTier::Small);
        cache.request("same", ThumbTier::Card);
        cache.request("same", ThumbTier::Card);
        assert_eq!(
            cache.pending,
            [
                ("same".into(), ThumbTier::Small),
                ("same".into(), ThumbTier::Card)
            ]
        );
        cache
            .entries
            .insert(("same".into(), ThumbTier::Small), ThumbState::Failed);
        assert!(matches!(
            cache.get("same", ThumbTier::Small),
            Some(ThumbState::Failed)
        ));
        assert!(cache.get("same", ThumbTier::Card).is_none());
    }

    #[tokio::test]
    async fn both_tiers_decode_shared_disk_bytes_and_resolve_through_the_reducer() {
        let url = format!(
            "file://missing-tiered-thumbnail-source-{}",
            std::process::id()
        );
        let path = disk_path(&url);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(640, 480)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        tokio::fs::create_dir_all(thumbs_dir()).await.unwrap();
        tokio::fs::write(&path, bytes.get_ref()).await.unwrap();
        let mut state = AppState::new();
        let (tx, mut rx) = mpsc::channel(2);
        let (app_tx, _) = mpsc::unbounded_channel();
        state.ui.thumbnails.request(&url, ThumbTier::Small);
        state.ui.thumbnails.request(&url, ThumbTier::Card);
        drain_pending(&mut state, &tx);
        assert_eq!(state.ui.thumbnails.loading_count(), 2);
        assert!(!state.ui.thumbnails.has_pending());
        for _ in 0..2 {
            let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
                .await
                .unwrap()
                .unwrap();
            crate::apply_worker_event::apply_worker_event(event, &mut state, &app_tx, &tx);
        }
        for tier in [ThumbTier::Small, ThumbTier::Card] {
            let Some(ThumbState::Ready { artwork }) = state.ui.thumbnails.get(&url, tier) else {
                panic!("tier did not resolve");
            };
            assert_eq!(artwork.width, tier.edge());
            assert_eq!(artwork.height, tier.edge() * 3 / 4);
            state.ui.thumbnails.request(&url, tier);
        }
        assert_eq!(state.ui.thumbnails.entries.len(), 2);
        assert!(!state.ui.thumbnails.has_pending());
        assert_eq!(tokio::fs::read(&path).await.unwrap(), bytes.into_inner());
        tokio::fs::remove_file(path).await.unwrap();
    }

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
    fn disk_path_keeps_mosaic_sizes_apart() {
        let ids =
            "ab67616d00001e02072f74ed41dfd5773f45e837ab67616d00001e0234b50b5c0f1b5f22729a0b04";
        let small = disk_path(&format!("https://mosaic.scdn.co/60/{ids}"));
        let large = disk_path(&format!("https://mosaic.scdn.co/640/{ids}"));
        assert_ne!(small, large);
        assert_eq!(
            large.file_name().unwrap().to_str().unwrap(),
            format!("640-{ids}")
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
        cache.request("a", ThumbTier::Small);
        cache.request("a", ThumbTier::Small);
        assert_eq!(cache.pending.len(), 1);
        cache
            .entries
            .insert(("b".to_string(), ThumbTier::Small), ThumbState::Failed);
        cache.request("b", ThumbTier::Small);
        assert_eq!(cache.pending.len(), 1);
    }

    #[test]
    fn eviction_keeps_loading_and_visible() {
        let mut cache = ThumbnailCache::default();
        for i in 0..(MAX_MEMORY_ENTRIES + 10) {
            cache
                .entries
                .insert((format!("u{i}"), ThumbTier::Small), ThumbState::Failed);
        }
        cache.entries.insert(
            ("loading".to_string(), ThumbTier::Card),
            ThumbState::Loading,
        );
        cache
            .entries
            .insert(("u1".to_string(), ThumbTier::Card), ThumbState::Failed);
        let visible = vec![("u1".to_string(), ThumbTier::Card)];
        cache.evict_if_needed(&visible);
        assert!(cache.get("loading", ThumbTier::Card).is_some());
        assert!(cache.get("u1", ThumbTier::Card).is_some());
        assert!(cache.get("u1", ThumbTier::Small).is_none());
        assert!(cache.entries.len() <= 2);
    }
}
