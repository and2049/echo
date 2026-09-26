//! Keeps [`LikedSongs`] in step with Spotify: a one-page head check when that is enough, a
//! paced walk when it is not. Every request goes through the client's rate-limit gate, so a
//! 429 stops the sync, its `Retry-After` is honored, and the walk resumes from its offset on
//! a later sync.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::{AppConfig, now_epoch_secs};
use crate::events::WorkerEvent;
use crate::liked_songs::{HeadOutcome, LikedSongs, Next, WALK_CHECKPOINT_PAGES, WalkOutcome};

use super::api::client::EchoSpotifyClient;

/// Spacing between walk pages. The walk runs in the background, so it trades speed for
/// staying well inside Spotify's rolling rate-limit window: 10,000 songs take about 3 minutes.
const WALK_PAGE_DELAY: Duration = Duration::from_secs(1);
/// A library that keeps changing under the walk stops it here; the next sync starts over.
const MAX_WALK_RESTARTS: u32 = 3;

static SYNCING: AtomicBool = AtomicBool::new(false);

struct SyncGuard;

impl Drop for SyncGuard {
    fn drop(&mut self) {
        SYNCING.store(false, Ordering::SeqCst);
    }
}

/// Starts a sync unless one is already running. `force_head` asks for a head check even
/// when the last one is recent, as an explicit refresh does.
pub fn spawn_sync(api: Option<EchoSpotifyClient>, tx: mpsc::Sender<WorkerEvent>, force_head: bool) {
    let Some(api) = api else {
        return;
    };
    if SYNCING.swap(true, Ordering::SeqCst) {
        return;
    }
    let guard = SyncGuard;
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(error) = sync(&api, &tx, force_head).await {
            log(&format!("liked_songs sync stopped err={error:#}"));
        }
    });
}

async fn sync(
    api: &EchoSpotifyClient,
    tx: &mpsc::Sender<WorkerEvent>,
    force_head: bool,
) -> anyhow::Result<()> {
    let now = now_epoch_secs();
    let mut offset = match LikedSongs::inspect(|liked| liked.next(now, force_head)) {
        Next::Nothing => return Ok(()),
        Next::Walk { offset } => offset,
        Next::HeadCheck => {
            let Some(page) = api.saved_tracks_page(0).await? else {
                return Ok(());
            };
            match LikedSongs::update(|liked| liked.apply_head(page, now)) {
                HeadOutcome::Added(ids) => {
                    log(&format!("liked_songs head_check added={}", ids.len()));
                    publish_likes(tx, ids).await;
                    return Ok(());
                }
                HeadOutcome::NeedsWalk => {
                    log("liked_songs head_check mismatch, walking");
                    0
                }
            }
        }
    };

    let mut pages = 0u32;
    let mut restarts = 0u32;
    loop {
        if pages > 0 {
            tokio::time::sleep(WALK_PAGE_DELAY).await;
        }
        let page = match api.saved_tracks_page(offset).await {
            Ok(Some(page)) => page,
            Ok(None) => {
                checkpoint();
                return Ok(());
            }
            Err(error) => {
                checkpoint();
                return Err(error);
            }
        };
        pages += 1;
        let persist = pages.is_multiple_of(WALK_CHECKPOINT_PAGES);
        match LikedSongs::update_and_persist(persist, |liked| liked.apply_walk_page(page, now)) {
            WalkOutcome::Continue { offset: next } => offset = next,
            WalkOutcome::Restarted { offset: next } => {
                restarts += 1;
                if restarts > MAX_WALK_RESTARTS {
                    log("liked_songs walk gave up: library kept changing");
                    checkpoint();
                    return Ok(());
                }
                offset = next;
            }
            WalkOutcome::Incomplete => {
                log("liked_songs walk ended short of Spotify's total");
                checkpoint();
                return Ok(());
            }
            WalkOutcome::Done => {
                checkpoint();
                log(&format!("liked_songs walk done pages={pages}"));
                publish_walk(tx).await;
                return Ok(());
            }
        }
    }
}

fn checkpoint() {
    LikedSongs::update(|_| ());
}

/// Hearts for rows a head check found.
async fn publish_likes(tx: &mpsc::Sender<WorkerEvent>, ids: Vec<String>) {
    if ids.is_empty() {
        return;
    }
    AppConfig::update_cache(|cache| cache.liked_tracks.extend(ids.iter().cloned()));
    let update = ids.into_iter().map(|id| (id, true)).collect();
    let _ = tx.send(WorkerEvent::LikedStatusUpdate(update)).await;
}

/// A completed walk is the whole truth for Spotify tracks: hearts for anything unliked
/// elsewhere go, local-file likes are echo's own and stay.
async fn publish_walk(tx: &mpsc::Sender<WorkerEvent>) {
    let ids = LikedSongs::inspect(LikedSongs::ids);
    let update: HashMap<String, bool> = AppConfig::update_cache(|cache| {
        let mut update = HashMap::new();
        cache.liked_tracks.retain(|id| {
            let keep = id.starts_with("local:") || ids.contains(id);
            if !keep {
                update.insert(id.clone(), false);
            }
            keep
        });
        for id in &ids {
            if cache.liked_tracks.insert(id.clone()) {
                update.insert(id.clone(), true);
            }
        }
        update
    });
    if !update.is_empty() {
        let _ = tx.send(WorkerEvent::LikedStatusUpdate(update)).await;
    }
}

fn log(message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::config::debug_log_path("echo-debug-api.log"))
    {
        let _ = writeln!(file, "{} {message}", chrono::Utc::now().to_rfc3339());
    }
}
