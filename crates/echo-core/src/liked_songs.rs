//! The whole Liked Songs library, kept in its own `liked_songs.json` rather than `cache.json`,
//! which is rewritten on every cache update and should not carry thousands of rows.
//!
//! Spotify lists saved tracks newest first, so a like made anywhere lands on the first page.
//! A head check reads that one page, prepends what is new, and compares Spotify's `total`
//! with the cached count: if they agree nothing was removed elsewhere and no walk is needed.
//! A full walk runs only on first use, when a head check cannot account for the difference,
//! and weekly as a backstop for an add and a remove elsewhere that cancel out in the total.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::models::Track;

pub const PAGE_LIMIT: u32 = 50;
/// How long a head check stays good when nothing local made it stale.
pub const HEAD_CHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);
pub const FULL_WALK_INTERVAL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Walk progress is written out every this many pages, so a restart resumes close to where
/// it stopped without rewriting a large file once per request.
pub const WALK_CHECKPOINT_PAGES: u32 = 10;

/// One `/me/tracks` page.
#[derive(Clone, Debug, Default)]
pub struct SavedPage {
    pub offset: u32,
    pub total: u32,
    /// One entry per item Spotify returned, `None` for rows echo cannot show (local files).
    /// Kept so offsets and counts line up with Spotify's.
    pub items: Vec<Option<Track>>,
    pub has_next: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LikedSongs {
    /// Newest first, as Spotify lists them. Complete once `total` is set.
    #[serde(default)]
    pub tracks: Vec<Track>,
    /// Spotify's count behind `tracks`, local files included. `None` until a walk completes.
    #[serde(default)]
    pub total: Option<u32>,
    #[serde(default)]
    pub walked_at: Option<u64>,
    #[serde(default)]
    pub checked_at: Option<u64>,
    /// Something was liked in echo since the last head check. The like call carries only an
    /// id, so the row itself arrives with the next check.
    #[serde(default)]
    pub head_stale: bool,
    /// A head check could not reconcile the first page with the cache.
    #[serde(default)]
    pub needs_walk: bool,
    /// An unfinished walk. The committed `tracks` stay in use until it completes.
    #[serde(default)]
    pub walk: Option<Walk>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Walk {
    pub tracks: Vec<Track>,
    /// Items consumed, local files included: the next page's offset.
    pub seen: u32,
    pub total: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    Nothing,
    HeadCheck,
    Walk { offset: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeadOutcome {
    /// Ids newly at the head, newest first. Empty when nothing changed.
    Added(Vec<String>),
    NeedsWalk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WalkOutcome {
    Continue {
        offset: u32,
    },
    /// The library changed under the walk (its total moved); it starts over from `offset`.
    Restarted {
        offset: u32,
    },
    /// Spotify stopped short of its own total. Nothing is committed; a later sync retries.
    Incomplete,
    Done,
}

impl LikedSongs {
    /// What a Liked Songs view can show now: the committed list, or while the first walk is
    /// still running, the rows it has so far.
    pub fn visible(&self) -> Option<&[Track]> {
        if self.total.is_some() {
            return Some(&self.tracks);
        }
        self.walk.as_ref().map(|walk| walk.tracks.as_slice())
    }

    /// Spotify's count for the list [`Self::visible`] returns.
    pub fn count(&self) -> Option<u32> {
        self.total.or(self.walk.as_ref().map(|walk| walk.total))
    }

    pub fn ids(&self) -> HashSet<String> {
        self.tracks.iter().map(|track| track.id.clone()).collect()
    }

    pub fn next(&self, now: u64, force_head: bool) -> Next {
        if let Some(walk) = &self.walk {
            return Next::Walk { offset: walk.seen };
        }
        if self.total.is_none()
            || self.needs_walk
            || older_than(self.walked_at, now, FULL_WALK_INTERVAL)
        {
            return Next::Walk { offset: 0 };
        }
        if force_head || self.head_stale || older_than(self.checked_at, now, HEAD_CHECK_INTERVAL) {
            return Next::HeadCheck;
        }
        Next::Nothing
    }

    /// Reconciles the first page with the cache. New rows are prepended when the rest of the
    /// page matches the cached head and the totals add up; anything else needs a walk.
    pub fn apply_head(&mut self, page: SavedPage, now: u64) -> HeadOutcome {
        let Some(total) = self.total.filter(|_| page.offset == 0) else {
            self.needs_walk = true;
            return HeadOutcome::NeedsWalk;
        };
        let split = match self.tracks.first() {
            Some(head) => page
                .items
                .iter()
                .position(|item| item.as_ref().is_some_and(|track| track.id == head.id)),
            None if total == 0 => Some(page.items.len()),
            None => None,
        };
        let consistent = split.is_some_and(|split| {
            page.total == total.saturating_add(split as u32)
                && page.items[split..]
                    .iter()
                    .flatten()
                    .zip(&self.tracks)
                    .all(|(fetched, cached)| fetched.id == cached.id)
        });
        let Some(split) = split.filter(|_| consistent) else {
            self.needs_walk = true;
            return HeadOutcome::NeedsWalk;
        };
        let added: Vec<Track> = page.items.into_iter().take(split).flatten().collect();
        let ids = added.iter().map(|track| track.id.clone()).collect();
        self.tracks.splice(0..0, added);
        self.total = Some(page.total);
        self.checked_at = Some(now);
        self.head_stale = false;
        HeadOutcome::Added(ids)
    }

    /// Adds one walk page. A page from a different offset or with a different total than the
    /// walk started with means rows shifted underneath it, so the walk starts over rather
    /// than risk skipped or repeated rows.
    pub fn apply_walk_page(&mut self, page: SavedPage, now: u64) -> WalkOutcome {
        let walk = self.walk.get_or_insert_with(|| Walk {
            total: page.total,
            ..Walk::default()
        });
        if page.offset != walk.seen || page.total != walk.total {
            *walk = Walk {
                total: page.total,
                ..Walk::default()
            };
            if page.offset != 0 {
                return WalkOutcome::Restarted { offset: 0 };
            }
        }
        walk.seen = walk.seen.saturating_add(page.items.len() as u32);
        walk.tracks.extend(page.items.into_iter().flatten());
        if walk.seen < walk.total && page.has_next && walk.seen > page.offset {
            return WalkOutcome::Continue { offset: walk.seen };
        }
        let walk = self.walk.take().unwrap_or_default();
        if walk.seen != walk.total {
            return WalkOutcome::Incomplete;
        }
        let mut seen = HashSet::new();
        self.tracks = walk
            .tracks
            .into_iter()
            .filter(|track| seen.insert(track.id.clone()))
            .collect();
        self.total = Some(walk.total);
        self.walked_at = Some(now);
        self.checked_at = Some(now);
        self.needs_walk = false;
        self.head_stale = false;
        WalkOutcome::Done
    }

    /// A like echo made. Its row comes with the next head check.
    pub fn liked(&mut self, track_id: &str) {
        if !self.tracks.iter().any(|track| track.id == track_id) {
            self.head_stale = true;
        }
    }

    /// An unlike echo made, applied without a request. Returns whether a row was removed.
    pub fn unliked(&mut self, track_id: &str) -> bool {
        let Some(index) = self.tracks.iter().position(|track| track.id == track_id) else {
            return false;
        };
        self.tracks.remove(index);
        self.total = self.total.map(|total| total.saturating_sub(1));
        true
    }
}

fn older_than(at: Option<u64>, now: u64, age: Duration) -> bool {
    at.is_none_or(|at| now.saturating_sub(at) > age.as_secs())
}

impl LikedSongs {
    pub fn path() -> PathBuf {
        crate::config::echo_config_root().join("liked_songs.json")
    }

    /// A copy of the process-wide state. Only the first call reads the disk.
    pub fn load() -> Self {
        cell()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Reads the process-wide state in place, for callers that need a little of a large list.
    pub fn inspect<R>(read: impl FnOnce(&Self) -> R) -> R {
        read(&cell().lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Changes the process-wide state under its lock and writes it out.
    pub fn update<R>(change: impl FnOnce(&mut Self) -> R) -> R {
        Self::update_and_persist(true, change)
    }

    /// Like [`Self::update`], but skips the write when `persist` is false. Walk pages use it
    /// between checkpoints.
    pub fn update_and_persist<R>(persist: bool, change: impl FnOnce(&mut Self) -> R) -> R {
        let mut state = cell().lock().unwrap_or_else(PoisonError::into_inner);
        let result = change(&mut state);
        if persist {
            let _ = write(&state);
        }
        result
    }
}

fn cell() -> &'static Mutex<LikedSongs> {
    static STATE: OnceLock<Mutex<LikedSongs>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(read()))
}

fn read() -> LikedSongs {
    fs::read_to_string(LikedSongs::path())
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

/// Writes to a temporary file first, so an interrupted write leaves the previous copy intact.
fn write(state: &LikedSongs) -> anyhow::Result<()> {
    let path = LikedSongs::path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec(state)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackSource;

    const NOW: u64 = 1_000_000;

    fn track(n: u32) -> Track {
        Track {
            explicit: false,
            added_by: None,
            id: format!("t{n}"),
            source: TrackSource::Spotify,
            local_path: None,
            name: format!("Song {n}"),
            artist: "Artist".into(),
            album: "Album".into(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    /// Items `ids[offset..offset + PAGE_LIMIT]` of a library listed newest first.
    fn page(ids: &[u32], offset: u32) -> SavedPage {
        let end = (offset + PAGE_LIMIT).min(ids.len() as u32);
        SavedPage {
            offset,
            total: ids.len() as u32,
            items: ids[offset as usize..end as usize]
                .iter()
                .map(|&n| Some(track(n)))
                .collect(),
            has_next: end < ids.len() as u32,
        }
    }

    fn walked(ids: &[u32]) -> LikedSongs {
        let mut liked = LikedSongs::default();
        let mut offset = 0;
        loop {
            match liked.apply_walk_page(page(ids, offset), NOW) {
                WalkOutcome::Continue { offset: next } => offset = next,
                WalkOutcome::Done => return liked,
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    fn visible_ids(liked: &LikedSongs) -> Vec<String> {
        liked
            .visible()
            .unwrap()
            .iter()
            .map(|track| track.id.clone())
            .collect()
    }

    #[test]
    fn a_first_walk_commits_every_page() {
        let ids: Vec<u32> = (0..120).collect();
        let liked = walked(&ids);
        assert_eq!(liked.tracks.len(), 120);
        assert_eq!(liked.total, Some(120));
        assert_eq!(liked.walk, None);
        assert_eq!(liked.next(NOW, false), Next::Nothing);
    }

    #[test]
    fn an_empty_library_walks_in_one_request() {
        let liked = walked(&[]);
        assert_eq!(liked.total, Some(0));
        assert_eq!(liked.visible(), Some(&[][..]));
    }

    #[test]
    fn the_partial_walk_is_visible_and_resumes_from_its_offset() {
        let ids: Vec<u32> = (0..120).collect();
        let mut liked = LikedSongs::default();
        assert_eq!(
            liked.apply_walk_page(page(&ids, 0), NOW),
            WalkOutcome::Continue { offset: 50 }
        );
        assert_eq!(liked.visible().unwrap().len(), 50);
        assert_eq!(liked.count(), Some(120));
        let restored: LikedSongs =
            serde_json::from_slice(&serde_json::to_vec(&liked).unwrap()).unwrap();
        assert_eq!(restored.next(NOW, false), Next::Walk { offset: 50 });
    }

    #[test]
    fn a_walk_restarts_when_the_total_moves_under_it() {
        let ids: Vec<u32> = (0..120).collect();
        let mut liked = LikedSongs::default();
        liked.apply_walk_page(page(&ids, 0), NOW);
        let shifted: Vec<u32> = (0..121).collect();
        assert_eq!(
            liked.apply_walk_page(page(&shifted, 50), NOW),
            WalkOutcome::Restarted { offset: 0 }
        );
        assert_eq!(liked.walk.as_ref().unwrap().seen, 0);
        assert_eq!(liked.total, None);
    }

    #[test]
    fn a_walk_short_of_its_total_commits_nothing() {
        let ids: Vec<u32> = (0..60).collect();
        let mut liked = walked(&ids);
        let before = liked.tracks.clone();
        liked.needs_walk = true;
        let mut short = page(&ids, 0);
        short.has_next = false;
        assert_eq!(
            liked.apply_walk_page(short, NOW + 1),
            WalkOutcome::Incomplete
        );
        assert_eq!(liked.tracks, before);
        assert!(liked.needs_walk);
        assert_eq!(liked.walk, None);
    }

    #[test]
    fn local_files_count_toward_the_total_without_showing() {
        let ids: Vec<u32> = (0..3).collect();
        let mut first = page(&ids, 0);
        first.items[1] = None;
        let mut liked = LikedSongs::default();
        assert_eq!(liked.apply_walk_page(first, NOW), WalkOutcome::Done);
        assert_eq!(visible_ids(&liked), ["t0", "t2"]);
        assert_eq!(liked.total, Some(3));
    }

    #[test]
    fn a_head_check_prepends_new_likes() {
        let mut liked = walked(&(0..80).collect::<Vec<_>>());
        let newer: Vec<u32> = [1000, 1001].into_iter().chain(0..80).collect();
        assert_eq!(
            liked.apply_head(page(&newer, 0), NOW + 10),
            HeadOutcome::Added(vec!["t1000".into(), "t1001".into()])
        );
        assert_eq!(liked.tracks.len(), 82);
        assert_eq!(&visible_ids(&liked)[..3], ["t1000", "t1001", "t0"]);
        assert_eq!(liked.total, Some(82));
        assert_eq!(liked.checked_at, Some(NOW + 10));
    }

    #[test]
    fn an_unchanged_head_check_only_stamps_the_time() {
        let ids: Vec<u32> = (0..80).collect();
        let mut liked = walked(&ids);
        assert_eq!(
            liked.apply_head(page(&ids, 0), NOW + 10),
            HeadOutcome::Added(Vec::new())
        );
        assert_eq!(liked.tracks.len(), 80);
        assert_eq!(liked.checked_at, Some(NOW + 10));
    }

    #[test]
    fn a_remote_unlike_below_the_head_needs_a_walk() {
        let mut liked = walked(&(0..80).collect::<Vec<_>>());
        let removed: Vec<u32> = (0..80).filter(|&n| n != 70).collect();
        assert_eq!(
            liked.apply_head(page(&removed, 0), NOW + 10),
            HeadOutcome::NeedsWalk
        );
        assert_eq!(liked.next(NOW + 10, false), Next::Walk { offset: 0 });
    }

    #[test]
    fn a_remote_unlike_of_the_head_needs_a_walk() {
        let mut liked = walked(&(0..80).collect::<Vec<_>>());
        let removed: Vec<u32> = (1..80).collect();
        assert_eq!(
            liked.apply_head(page(&removed, 0), NOW + 10),
            HeadOutcome::NeedsWalk
        );
    }

    #[test]
    fn a_remote_unlike_within_the_first_page_needs_a_walk() {
        let mut liked = walked(&(0..80).collect::<Vec<_>>());
        let removed: Vec<u32> = [1000]
            .into_iter()
            .chain((0..80).filter(|&n| n != 3))
            .collect();
        assert_eq!(
            liked.apply_head(page(&removed, 0), NOW + 10),
            HeadOutcome::NeedsWalk
        );
    }

    #[test]
    fn more_new_likes_than_a_page_needs_a_walk() {
        let mut liked = walked(&(0..10).collect::<Vec<_>>());
        let newer: Vec<u32> = (1000..1060).chain(0..10).collect();
        assert_eq!(
            liked.apply_head(page(&newer, 0), NOW + 10),
            HeadOutcome::NeedsWalk
        );
    }

    #[test]
    fn local_edits_keep_the_next_head_check_consistent() {
        let mut liked = walked(&(0..80).collect::<Vec<_>>());
        assert!(liked.unliked("t5"));
        assert!(!liked.unliked("t5"));
        assert_eq!(liked.total, Some(79));
        liked.liked("t1000");
        assert_eq!(liked.next(NOW, false), Next::HeadCheck);
        let server: Vec<u32> = [1000]
            .into_iter()
            .chain((0..80).filter(|&n| n != 5))
            .collect();
        assert_eq!(
            liked.apply_head(page(&server, 0), NOW + 10),
            HeadOutcome::Added(vec!["t1000".into()])
        );
        assert!(!liked.head_stale);
        assert_eq!(liked.total, Some(80));
    }

    #[test]
    fn schedules_follow_the_intervals() {
        let liked = walked(&(0..10).collect::<Vec<_>>());
        assert_eq!(liked.next(NOW, false), Next::Nothing);
        assert_eq!(liked.next(NOW, true), Next::HeadCheck);
        let later = NOW + HEAD_CHECK_INTERVAL.as_secs() + 1;
        assert_eq!(liked.next(later, false), Next::HeadCheck);
        let much_later = NOW + FULL_WALK_INTERVAL.as_secs() + 1;
        assert_eq!(liked.next(much_later, false), Next::Walk { offset: 0 });
        assert_eq!(
            LikedSongs::default().next(NOW, false),
            Next::Walk { offset: 0 }
        );
    }

    #[test]
    fn a_rewalk_keeps_the_old_list_visible_until_it_completes() {
        let ids: Vec<u32> = (0..120).collect();
        let mut liked = walked(&ids);
        liked.needs_walk = true;
        let fewer: Vec<u32> = (1..120).collect();
        liked.apply_walk_page(page(&fewer, 0), NOW + 1);
        assert_eq!(liked.visible().unwrap().len(), 120);
        liked.apply_walk_page(page(&fewer, 50), NOW + 1);
        assert_eq!(
            liked.apply_walk_page(page(&fewer, 100), NOW + 1),
            WalkOutcome::Done
        );
        assert_eq!(liked.tracks.len(), 119);
        assert!(!liked.needs_walk);
    }
}
