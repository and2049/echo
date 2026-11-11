//! The user's Spotify rootlist: their playlists in library order, with folder markers. The Web
//! API never exposes folders, so this is read over the playback session instead.

use crate::models::RootlistEntry;
use anyhow::{Context, Result, bail};
use librespot_core::session::Session;
use librespot_protocol::playlist4_external::SelectedListContent;
use protobuf::Message;
use std::time::Duration;

const PAGE: usize = 500;
const TIMEOUT: Duration = Duration::from_secs(20);

pub async fn fetch(session: &Session) -> Result<Vec<RootlistEntry>> {
    tokio::time::timeout(TIMEOUT, fetch_pages(session))
        .await
        .context("Timed out reading the Spotify rootlist")?
}

async fn fetch_pages(session: &Session) -> Result<Vec<RootlistEntry>> {
    let mut uris: Vec<String> = Vec::new();
    loop {
        let bytes = session
            .spclient()
            .get_rootlist(uris.len(), Some(PAGE))
            .await
            .map_err(|err| anyhow::anyhow!("Rootlist request failed: {err}"))?;
        let list = SelectedListContent::parse_from_bytes(&bytes)
            .context("Rootlist response was not a playlist list")?;
        let total = usize::try_from(list.length()).unwrap_or(0);
        let items = &list.contents.get_or_default().items;
        // A short answer would silently drop the folders that follow it, and the sidebar would
        // then show their playlists as loose; better to keep what was known before.
        if items.is_empty() && uris.len() < total {
            bail!("Rootlist ended at {} of {total} rows", uris.len());
        }
        uris.extend(items.iter().map(|item| item.uri().to_string()));
        if items.is_empty() || uris.len() >= total {
            break;
        }
    }
    Ok(entries(uris.iter().map(String::as_str)))
}

/// Turns rootlist URIs into entries, dropping stray folder ends and closing folders left open
/// so the result always nests cleanly.
pub fn entries<'a>(uris: impl Iterator<Item = &'a str>) -> Vec<RootlistEntry> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for entry in uris.filter_map(RootlistEntry::from_uri) {
        match entry {
            RootlistEntry::FolderStart { .. } => depth += 1,
            RootlistEntry::FolderEnd if depth == 0 => continue,
            RootlistEntry::FolderEnd => depth -= 1,
            RootlistEntry::Playlist(_) => {}
        }
        out.push(entry);
    }
    out.extend(std::iter::repeat_n(RootlistEntry::FolderEnd, depth));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(id: &str, name: &str) -> RootlistEntry {
        RootlistEntry::FolderStart {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    fn playlist(id: &str) -> RootlistEntry {
        RootlistEntry::Playlist(id.to_string())
    }

    #[test]
    fn folders_nest_and_names_are_decoded() {
        let parsed = entries(
            [
                "spotify:playlist:loose",
                "spotify:start-group:aa:New+Folder",
                "spotify:playlist:one",
                "spotify:start-group:bb:%F0%9F%95%91",
                "spotify:user:someone:playlist:two",
                "spotify:end-group:bb",
                "spotify:end-group:aa",
            ]
            .into_iter(),
        );

        assert_eq!(
            parsed,
            [
                playlist("loose"),
                start("aa", "New Folder"),
                playlist("one"),
                start("bb", "🕑"),
                playlist("two"),
                RootlistEntry::FolderEnd,
                RootlistEntry::FolderEnd,
            ]
        );
        assert_eq!(
            RootlistEntry::foldered_playlists(&parsed),
            ["one".to_string(), "two".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn unbalanced_markers_are_repaired() {
        let parsed = entries(
            [
                "spotify:end-group:stray",
                "spotify:start-group:aa:Open",
                "spotify:playlist:one",
                "spotify:show:ignored",
            ]
            .into_iter(),
        );

        assert_eq!(
            parsed,
            [
                start("aa", "Open"),
                playlist("one"),
                RootlistEntry::FolderEnd
            ]
        );
    }
}
