use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    config::{CacheData, stale_value},
    models::{Album, Artist, Playlist, TopItemsRange, Track},
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum HomeItemKind {
    Playlist,
    Album,
    Artist,
    Track,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HomeItem {
    pub id: String,
    pub kind: HomeItemKind,
    pub title: String,
    pub subtitle: String,
    pub image_url: Option<String>,
    pub owner_id: Option<String>,
    pub track: Option<Track>,
    #[serde(default)]
    pub release_year: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentContext {
    pub uri: String,
    pub kind: HomeItemKind,
    pub first_track: Track,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedRecentContext {
    pub context: RecentContext,
    pub item: HomeItem,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RecentHistory {
    pub tracks: Vec<Track>,
    pub contexts: Vec<RecentContext>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HomeFeed {
    MadeForYou,
    RecentlyPlayed,
    RecentContexts,
    TopArtists,
    TopTracks,
    NewReleases,
}

impl HomeFeed {
    pub const ALL: [Self; 6] = [
        Self::MadeForYou,
        Self::RecentlyPlayed,
        Self::RecentContexts,
        Self::TopArtists,
        Self::TopTracks,
        Self::NewReleases,
    ];

    pub fn ttl(self) -> Duration {
        match self {
            Self::MadeForYou => crate::config::LIBRARY_LIST_REFRESH_TTL,
            Self::RecentlyPlayed | Self::RecentContexts => crate::config::RECENTLY_PLAYED_CACHE_TTL,
            _ => crate::config::TOP_TRACKS_CACHE_TTL,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HomeFetch {
    pub in_flight: bool,
    pub fetched_at: Option<Instant>,
    pub range: Option<TopItemsRange>,
}

impl HomeFetch {
    pub fn needs_fetch(&self, now: Instant, ttl: Duration, range: Option<TopItemsRange>) -> bool {
        !self.in_flight
            && (self.range != range
                || self
                    .fetched_at
                    .is_none_or(|at| now.saturating_duration_since(at) >= ttl))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HomeShelfKind {
    QuickPicks,
    MadeForYou,
    RecentlyPlayed,
    TopArtists,
    TopSongs,
    NewReleases,
}

impl HomeShelfKind {
    pub const ALL: [Self; 6] = [
        Self::QuickPicks,
        Self::MadeForYou,
        Self::RecentlyPlayed,
        Self::TopArtists,
        Self::TopSongs,
        Self::NewReleases,
    ];
    pub fn title_key(self) -> &'static str {
        match self {
            Self::QuickPicks => "desktop.home_quick_picks",
            Self::MadeForYou => "desktop.home_made_for_you",
            Self::RecentlyPlayed => "desktop.home_recently_played",
            Self::TopArtists => "desktop.home_top_artists",
            Self::TopSongs => "desktop.home_top_songs",
            Self::NewReleases => "desktop.home_new_releases",
        }
    }
}

pub struct HomeShelf {
    pub kind: HomeShelfKind,
    pub items: Vec<HomeItem>,
}

pub fn greeting_key(hour: u32) -> &'static str {
    match hour {
        5..=11 => "desktop.home_morning",
        12..=17 => "desktop.home_afternoon",
        _ => "desktop.home_evening",
    }
}

pub fn local_greeting_key() -> &'static str {
    use chrono::Timelike;
    greeting_key(chrono::Local::now().hour())
}

impl From<&Playlist> for HomeItem {
    fn from(value: &Playlist) -> Self {
        Self {
            id: value.id.clone(),
            kind: HomeItemKind::Playlist,
            title: value.name.clone(),
            subtitle: value.owner.clone(),
            image_url: value.image_url.clone(),
            owner_id: Some(value.owner_id.clone()),
            track: None,
            release_year: None,
        }
    }
}

impl From<&Album> for HomeItem {
    fn from(value: &Album) -> Self {
        Self {
            id: value.id.clone(),
            kind: HomeItemKind::Album,
            title: value.name.clone(),
            subtitle: value.artists.clone(),
            image_url: value.image_url.clone(),
            owner_id: None,
            track: None,
            release_year: Some(value.release_year.clone()),
        }
    }
}

impl From<&Artist> for HomeItem {
    fn from(value: &Artist) -> Self {
        Self {
            id: value.id.clone(),
            kind: HomeItemKind::Artist,
            title: value.name.clone(),
            subtitle: String::new(),
            image_url: value.image_url.clone(),
            owner_id: None,
            track: None,
            release_year: None,
        }
    }
}

impl From<&Track> for HomeItem {
    fn from(value: &Track) -> Self {
        Self {
            id: value.id.clone(),
            kind: HomeItemKind::Track,
            title: value.name.clone(),
            subtitle: value.artist.clone(),
            image_url: value.image_url.clone(),
            owner_id: None,
            track: Some(value.clone()),
            release_year: None,
        }
    }
}

/// The persisted playlists and artists a recent context can be matched against, fresh or not.
pub fn recent_lookup(cache: &CacheData) -> (Vec<Playlist>, Vec<Artist>) {
    let mut artists = stale_value(&cache.followed_artists).unwrap_or_default();
    artists.extend(stale_value(&cache.top_artists).unwrap_or_default());
    (stale_value(&cache.playlists).unwrap_or_default(), artists)
}

/// Recent contexts resolved from the persisted history alone, whatever its age, so the
/// shelf renders before the refresh lands; contexts nothing on disk can name are skipped.
pub fn stale_recent_contexts(cache: &CacheData) -> Vec<ResolvedRecentContext> {
    let Some(history) = stale_value(&cache.recent_history) else {
        return Vec::new();
    };
    let (mut playlists, artists) = recent_lookup(cache);
    playlists.extend(
        cache
            .recent_playlists
            .values()
            .map(|entry| entry.value.clone()),
    );
    history
        .contexts
        .iter()
        .filter_map(|context| resolve_recent_context(context, &playlists, &artists))
        .collect()
}

pub fn resolve_recent_context(
    context: &RecentContext,
    playlists: &[Playlist],
    artists: &[Artist],
) -> Option<ResolvedRecentContext> {
    let id = context.uri.rsplit(':').next()?;
    let track = &context.first_track;
    let item = match context.kind {
        HomeItemKind::Playlist => playlists.iter().find(|p| p.id == id).map(HomeItem::from)?,
        HomeItemKind::Album => HomeItem {
            id: id.into(),
            kind: HomeItemKind::Album,
            title: track.album.clone(),
            subtitle: track.artist.clone(),
            image_url: track.image_url.clone(),
            owner_id: None,
            track: None,
            release_year: None,
        },
        HomeItemKind::Artist => artists
            .iter()
            .find(|artist| artist.id == id)
            .map(HomeItem::from)
            .or_else(|| {
                track
                    .artists
                    .iter()
                    .find(|artist| artist.id.as_deref() == Some(id))
                    .map(|artist| HomeItem {
                        id: id.into(),
                        kind: HomeItemKind::Artist,
                        title: artist.name.clone(),
                        subtitle: String::new(),
                        image_url: None,
                        owner_id: None,
                        track: None,
                        release_year: None,
                    })
            })?,
        HomeItemKind::Track => return None,
    };
    Some(ResolvedRecentContext {
        context: context.clone(),
        item,
    })
}

pub fn home_shelves(state: &AppState) -> Vec<HomeShelf> {
    let mut recent = Vec::new();
    let mut seen_recent = HashSet::new();
    let artists: Vec<_> = state
        .data
        .followed_artists
        .iter()
        .chain(&state.data.top_artists)
        .cloned()
        .collect();
    for context in &state.data.recent_contexts {
        if !seen_recent.insert((context.item.kind, context.item.id.clone())) {
            continue;
        }
        recent.push(
            resolve_recent_context(&context.context, &state.data.playlists, &artists)
                .map(|resolved| resolved.item)
                .unwrap_or_else(|| context.item.clone()),
        );
    }
    let mut quick = vec![HomeItem {
        id: "LIKED_SONGS".into(),
        kind: HomeItemKind::Playlist,
        title: String::new(),
        subtitle: String::new(),
        image_url: None,
        owner_id: state.data.user_id.clone(),
        track: None,
        release_year: None,
    }];
    let mut seen = HashSet::from([(HomeItemKind::Playlist, "LIKED_SONGS".to_string())]);
    quick.extend(
        recent
            .iter()
            .filter(|item| matches!(item.kind, HomeItemKind::Playlist | HomeItemKind::Album))
            .filter(|item| seen.insert((item.kind, item.id.clone())))
            .take(7)
            .cloned(),
    );
    vec![
        HomeShelf {
            kind: HomeShelfKind::QuickPicks,
            items: quick,
        },
        HomeShelf {
            kind: HomeShelfKind::MadeForYou,
            items: state
                .data
                .playlists
                .iter()
                .filter(|p| p.owner_id == "spotify")
                .map(HomeItem::from)
                .collect(),
        },
        HomeShelf {
            kind: HomeShelfKind::RecentlyPlayed,
            items: recent,
        },
        HomeShelf {
            kind: HomeShelfKind::TopArtists,
            items: state.data.top_artists.iter().map(HomeItem::from).collect(),
        },
        HomeShelf {
            kind: HomeShelfKind::TopSongs,
            items: state
                .data
                .top_tracks
                .iter()
                .take(12)
                .map(HomeItem::from)
                .collect(),
        },
        HomeShelf {
            kind: HomeShelfKind::NewReleases,
            items: state
                .data
                .whats_new
                .iter()
                .take(12)
                .map(HomeItem::from)
                .collect(),
        },
    ]
    .into_iter()
    .filter(|shelf| !shelf.items.is_empty())
    .collect()
}

pub fn pending_shelves(fetches: &HashMap<HomeFeed, HomeFetch>) -> Vec<HomeShelfKind> {
    [
        (HomeFeed::MadeForYou, HomeShelfKind::MadeForYou),
        (HomeFeed::RecentContexts, HomeShelfKind::RecentlyPlayed),
        (HomeFeed::TopArtists, HomeShelfKind::TopArtists),
        (HomeFeed::TopTracks, HomeShelfKind::TopSongs),
        (HomeFeed::NewReleases, HomeShelfKind::NewReleases),
    ]
    .into_iter()
    .filter(|(feed, _)| fetches.get(feed).is_some_and(|fetch| fetch.in_flight))
    .map(|(_, shelf)| shelf)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        serde_json::from_value(serde_json::json!({"id":"track", "name":"Song", "artist":"Artist", "album":"Album", "album_id":"album", "image_url":"cover", "duration_ms":1000, "artists":[{"id":"artist","name":"Artist"}]})).unwrap()
    }

    #[test]
    fn stale_recent_contexts_use_expired_entries_and_skip_unknown_playlists() {
        let mut cache = CacheData::default();
        let mut history = RecentHistory::default();
        for (uri, kind) in [
            ("spotify:album:album", HomeItemKind::Album),
            ("spotify:playlist:known", HomeItemKind::Playlist),
            ("spotify:playlist:unknown", HomeItemKind::Playlist),
        ] {
            history.contexts.push(RecentContext {
                uri: uri.into(),
                kind,
                first_track: track(),
            });
        }
        fn expired<T>(value: T) -> crate::config::CachedEntry<T> {
            crate::config::CachedEntry {
                fetched_at: 0,
                value,
            }
        }
        cache.recent_history = Some(expired(history));
        let playlist: Playlist = serde_json::from_value(serde_json::json!({
            "id": "known", "name": "Known", "owner": "Owner", "owner_id": "owner", "image_url": null
        }))
        .unwrap();
        cache.playlists = Some(expired(vec![playlist]));
        let ids: Vec<String> = stale_recent_contexts(&cache)
            .into_iter()
            .map(|resolved| resolved.item.id)
            .collect();
        assert_eq!(ids, ["album", "known"]);
        assert!(stale_recent_contexts(&CacheData::default()).is_empty());
    }

    #[test]
    fn greeting_covers_every_boundary() {
        for hour in 0..24 {
            assert_eq!(
                greeting_key(hour),
                if (5..12).contains(&hour) {
                    "desktop.home_morning"
                } else if (12..18).contains(&hour) {
                    "desktop.home_afternoon"
                } else {
                    "desktop.home_evening"
                }
            );
        }
        assert_eq!(greeting_key(24), "desktop.home_evening");
    }

    #[test]
    fn fetch_state_expires_and_tracks_range_and_inflight_requests() {
        let now = Instant::now();
        let mut fetch = HomeFetch::default();
        assert!(fetch.needs_fetch(now, HomeFeed::TopTracks.ttl(), None));
        fetch.fetched_at = Some(now);
        assert!(!fetch.needs_fetch(now, HomeFeed::TopTracks.ttl(), None));
        assert!(fetch.needs_fetch(
            now + HomeFeed::TopTracks.ttl(),
            HomeFeed::TopTracks.ttl(),
            None
        ));
        assert!(fetch.needs_fetch(now, HomeFeed::TopTracks.ttl(), Some(TopItemsRange::Short)));
        fetch.in_flight = true;
        assert!(!fetch.needs_fetch(now, Duration::ZERO, None));
        assert_eq!(HomeFeed::RecentlyPlayed.ttl(), Duration::from_secs(300));
        assert_eq!(HomeFeed::RecentContexts.ttl(), Duration::from_secs(300));
    }

    #[test]
    fn contexts_resolve_known_entities_without_requests() {
        let mut context = RecentContext {
            uri: "spotify:album:album".into(),
            kind: HomeItemKind::Album,
            first_track: track(),
        };
        let album = resolve_recent_context(&context, &[], &[]).unwrap().item;
        assert_eq!(
            (
                album.id.as_str(),
                album.title.as_str(),
                album.subtitle.as_str()
            ),
            ("album", "Album", "Artist")
        );
        assert_eq!(album.image_url.as_deref(), Some("cover"));
        context.uri = "spotify:artist:artist".into();
        context.kind = HomeItemKind::Artist;
        let artist = Artist {
            id: "artist".into(),
            name: "Resolved artist".into(),
            image_url: Some("portrait".into()),
        };
        assert_eq!(
            resolve_recent_context(&context, &[], &[artist])
                .unwrap()
                .item
                .image_url
                .as_deref(),
            Some("portrait")
        );
        assert_eq!(
            resolve_recent_context(&context, &[], &[])
                .unwrap()
                .item
                .title,
            "Artist"
        );
        context.uri = "spotify:playlist:p".into();
        context.kind = HomeItemKind::Playlist;
        assert!(resolve_recent_context(&context, &[], &[]).is_none());
        let playlist: Playlist = serde_json::from_value(serde_json::json!({"id":"p","name":"Playlist","owner":"Owner","owner_id":"owner","image_url":"playlist-cover"})).unwrap();
        let item = resolve_recent_context(&context, &[playlist], &[])
            .unwrap()
            .item;
        assert_eq!(
            (item.title.as_str(), item.subtitle.as_str()),
            ("Playlist", "Owner")
        );
        context.kind = HomeItemKind::Track;
        assert!(resolve_recent_context(&context, &[], &[]).is_none());
    }

    #[test]
    fn shelves_are_ordered_bounded_and_quick_picks_are_unique() {
        let mut state = AppState::new();
        assert_eq!(home_shelves(&state).len(), 1);
        for id in 0..15 {
            let context = RecentContext {
                uri: format!("spotify:album:{id}"),
                kind: HomeItemKind::Album,
                first_track: track(),
            };
            state
                .data
                .recent_contexts
                .push(resolve_recent_context(&context, &[], &[]).unwrap());
        }
        state
            .data
            .recent_contexts
            .insert(1, state.data.recent_contexts[0].clone());
        state.data.playlists = vec![serde_json::from_value(serde_json::json!({"id":"made","name":"Made","owner":"Spotify","owner_id":"spotify","image_url":null})).unwrap()];
        state.data.top_artists = vec![Artist {
            id: "artist".into(),
            name: "Artist".into(),
            image_url: None,
        }];
        state.data.top_tracks = vec![track(); 20];
        state.data.whats_new = vec![
            Album {
                id: "release".into(),
                name: "Release".into(),
                artists: "Artist".into(),
                image_url: None,
                thumb_url: None,
                release_year: "2026".into(),
                release_date: None,
                track_count: None
            };
            20
        ];
        let shelves = home_shelves(&state);
        assert_eq!(
            shelves.iter().map(|s| s.kind).collect::<Vec<_>>(),
            [
                HomeShelfKind::QuickPicks,
                HomeShelfKind::MadeForYou,
                HomeShelfKind::RecentlyPlayed,
                HomeShelfKind::TopArtists,
                HomeShelfKind::TopSongs,
                HomeShelfKind::NewReleases
            ]
        );
        assert_eq!(shelves[0].items.len(), 8);
        assert_eq!(shelves[0].items[0].id, "LIKED_SONGS");
        assert_eq!(
            shelves[0]
                .items
                .iter()
                .map(|i| &i.id)
                .collect::<HashSet<_>>()
                .len(),
            8
        );
        assert_eq!(shelves[4].items.len(), 12);
        assert!(shelves[4].items[0].track.is_some());
        assert_eq!(shelves[5].items.len(), 12);
        for shelf in &shelves {
            assert!(shelf.kind.title_key().starts_with("desktop.home_"));
        }
        state.data.playlists[0].owner_id = "user".into();
        assert!(
            !home_shelves(&state)
                .iter()
                .any(|s| s.kind == HomeShelfKind::MadeForYou)
        );
    }

    #[test]
    fn loading_shelves_only_include_active_requests() {
        let mut fetches = HashMap::new();
        assert!(pending_shelves(&fetches).is_empty());
        for feed in HomeFeed::ALL {
            fetches.insert(
                feed,
                HomeFetch {
                    in_flight: true,
                    ..Default::default()
                },
            );
        }
        assert_eq!(
            pending_shelves(&fetches),
            [
                HomeShelfKind::MadeForYou,
                HomeShelfKind::RecentlyPlayed,
                HomeShelfKind::TopArtists,
                HomeShelfKind::TopSongs,
                HomeShelfKind::NewReleases
            ]
        );
    }
}
