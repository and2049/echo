use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::config::{ARTIST_ALBUMS_REFRESH_TTL, artist_album_metadata_complete};
use crate::models::{Album, Artist, TopItemsRange, Track};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CacheKey {
    ArtistAlbums(String),
    ArtistTopTracks(String),
    TopTracks(TopItemsRange),
    TopArtists(TopItemsRange),
    RecentlyPlayed,
    FollowedArtists,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FetchGate {
    Start,
    InFlight,
    CoolingDown(Duration),
}

#[derive(Clone)]
struct Timed<T> {
    value: T,
    fetched_at: Instant,
    ttl: Duration,
}

impl<T: Clone> Timed<T> {
    fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            fetched_at: Instant::now(),
            ttl,
        }
    }

    fn get(&self) -> Option<T> {
        if self.fetched_at.elapsed() <= self.ttl {
            Some(self.value.clone())
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct SpotifyApiCache {
    top_tracks: HashMap<TopItemsRange, Timed<Vec<Track>>>,
    top_artists: HashMap<TopItemsRange, Timed<Vec<Artist>>>,
    recently_played: Option<Timed<Vec<Track>>>,
    followed_artists: Option<Timed<Vec<Artist>>>,
    artist_albums: HashMap<String, Timed<Vec<Album>>>,
    artist_top_tracks: HashMap<String, Timed<Vec<Track>>>,
    inflight: HashSet<CacheKey>,
    cooldowns: HashMap<CacheKey, Instant>,
}

impl SpotifyApiCache {
    pub fn top_tracks(&self, range: TopItemsRange) -> Option<Vec<Track>> {
        self.top_tracks.get(&range).and_then(Timed::get)
    }

    pub fn set_top_tracks(&mut self, range: TopItemsRange, tracks: Vec<Track>) {
        self.clear_cooldown(&CacheKey::TopTracks(range));
        self.top_tracks
            .insert(range, Timed::new(tracks, Duration::from_secs(6 * 60 * 60)));
    }

    pub fn top_artists(&self, range: TopItemsRange) -> Option<Vec<Artist>> {
        self.top_artists.get(&range).and_then(Timed::get)
    }

    pub fn set_top_artists(&mut self, range: TopItemsRange, artists: Vec<Artist>) {
        self.clear_cooldown(&CacheKey::TopArtists(range));
        self.top_artists
            .insert(range, Timed::new(artists, Duration::from_secs(6 * 60 * 60)));
    }

    pub fn recently_played(&self) -> Option<Vec<Track>> {
        self.recently_played.as_ref().and_then(Timed::get)
    }

    pub fn set_recently_played(&mut self, tracks: Vec<Track>) {
        self.clear_cooldown(&CacheKey::RecentlyPlayed);
        self.recently_played = Some(Timed::new(tracks, Duration::from_secs(5 * 60)));
    }

    pub fn followed_artists(&self) -> Option<Vec<Artist>> {
        self.followed_artists.as_ref().and_then(Timed::get)
    }

    pub fn set_followed_artists(&mut self, artists: Vec<Artist>) {
        self.clear_cooldown(&CacheKey::FollowedArtists);
        self.followed_artists = Some(Timed::new(artists, Duration::from_secs(24 * 60 * 60)));
    }

    pub fn artist_albums(&self, artist_id: &str) -> Option<Vec<Album>> {
        self.artist_albums.get(artist_id).and_then(Timed::get)
    }

    pub fn artist_albums_need_refresh(&self, artist_id: &str) -> Option<bool> {
        let entry = self.artist_albums.get(artist_id)?;
        Some(
            entry.fetched_at.elapsed() > ARTIST_ALBUMS_REFRESH_TTL
                || !artist_album_metadata_complete(&entry.value),
        )
    }

    pub fn set_artist_albums(&mut self, artist_id: String, albums: Vec<Album>) {
        self.clear_cooldown(&CacheKey::ArtistAlbums(artist_id.clone()));
        self.artist_albums.insert(
            artist_id,
            Timed::new(albums, Duration::from_secs(24 * 60 * 60)),
        );
    }

    pub fn artist_top_tracks(&self, artist_id: &str) -> Option<Vec<Track>> {
        self.artist_top_tracks.get(artist_id).and_then(Timed::get)
    }

    pub fn set_artist_top_tracks(&mut self, artist_id: String, tracks: Vec<Track>) {
        self.clear_cooldown(&CacheKey::ArtistTopTracks(artist_id.clone()));
        self.artist_top_tracks.insert(
            artist_id,
            Timed::new(tracks, Duration::from_secs(24 * 60 * 60)),
        );
    }

    pub fn begin(&mut self, key: CacheKey) -> FetchGate {
        if let Some(remaining) = self.cooldown_remaining(&key) {
            return FetchGate::CoolingDown(remaining);
        }

        if self.inflight.insert(key) {
            FetchGate::Start
        } else {
            FetchGate::InFlight
        }
    }

    pub fn finish(&mut self, key: &CacheKey) {
        self.inflight.remove(key);
    }

    pub fn rate_limited(&mut self, key: CacheKey, retry_after: Duration) {
        self.cooldowns.insert(key, Instant::now() + retry_after);
    }

    fn clear_cooldown(&mut self, key: &CacheKey) {
        self.cooldowns.remove(key);
    }

    fn cooldown_remaining(&mut self, key: &CacheKey) -> Option<Duration> {
        let until = *self.cooldowns.get(key)?;
        match until.checked_duration_since(Instant::now()) {
            Some(remaining) => Some(remaining),
            None => {
                self.cooldowns.remove(key);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cooldown_allows_begin() {
        let mut cache = SpotifyApiCache::default();
        assert_eq!(
            cache.begin(CacheKey::TopTracks(TopItemsRange::Medium)),
            FetchGate::Start
        );
    }

    #[test]
    fn active_cooldown_blocks_begin() {
        let mut cache = SpotifyApiCache::default();
        cache.rate_limited(
            CacheKey::TopTracks(TopItemsRange::Medium),
            Duration::from_secs(50),
        );

        match cache.begin(CacheKey::TopTracks(TopItemsRange::Medium)) {
            FetchGate::CoolingDown(remaining) => {
                assert!(remaining <= Duration::from_secs(50));
                assert!(remaining > Duration::from_secs(45));
            }
            other => panic!("expected cooldown, got {other:?}"),
        }
    }

    #[test]
    fn expired_cooldown_allows_begin() {
        let mut cache = SpotifyApiCache::default();
        cache.cooldowns.insert(
            CacheKey::TopTracks(TopItemsRange::Medium),
            Instant::now() - Duration::from_secs(1),
        );

        assert_eq!(
            cache.begin(CacheKey::TopTracks(TopItemsRange::Medium)),
            FetchGate::Start
        );
    }

    #[test]
    fn successful_fetch_clears_cooldown() {
        let mut cache = SpotifyApiCache::default();
        cache.rate_limited(
            CacheKey::ArtistAlbums("artist".to_string()),
            Duration::from_secs(50),
        );

        cache.set_artist_albums("artist".to_string(), Vec::new());

        assert_eq!(
            cache.begin(CacheKey::ArtistAlbums("artist".to_string())),
            FetchGate::Start
        );
    }

    #[test]
    fn independent_cooldown_keys_do_not_block_each_other() {
        let mut cache = SpotifyApiCache::default();
        cache.rate_limited(CacheKey::RecentlyPlayed, Duration::from_secs(50));

        assert!(matches!(
            cache.begin(CacheKey::RecentlyPlayed),
            FetchGate::CoolingDown(_)
        ));
        assert_eq!(cache.begin(CacheKey::FollowedArtists), FetchGate::Start);
    }
}
