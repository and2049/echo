use std::{io::Write, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use rspotify::AuthCodeSpotify;
use tokio::sync::Mutex;

use crate::{
    config::{AppConfig, CacheData},
    models::{Album, Artist, ArtistPageData, Track},
};

use super::{
    SpotifyWorker,
    cache::{CacheKey, FetchGate, SpotifyApiCache},
    first_party::{SpotifyWebApi, rate_limit_error},
    parse,
    policy::ApiEndpoint,
    rate_limit::{DEFAULT_RATE_LIMIT_COOLDOWN, is_probable_rate_limit},
};

#[derive(Clone)]
pub struct EchoSpotifyClient {
    third_party: AuthCodeSpotify,
    first_party: Option<SpotifyWebApi>,
    cache: Arc<Mutex<SpotifyApiCache>>,
}

impl EchoSpotifyClient {
    pub fn new(third_party: AuthCodeSpotify, first_party: Option<SpotifyWebApi>) -> Self {
        Self {
            third_party,
            first_party,
            cache: Arc::new(Mutex::new(SpotifyApiCache::default())),
        }
    }

    pub async fn top_tracks(&self) -> Result<Option<Vec<Track>>> {
        if let Some(tracks) = self.cache.lock().await.top_tracks() {
            log_api("top_tracks route=cache");
            return Ok(Some(tracks));
        }
        if let Some(tracks) = AppConfig::load_cache().get_top_tracks() {
            log_api("top_tracks route=persistent_cache");
            self.cache.lock().await.set_top_tracks(tracks.clone());
            return Ok(Some(tracks));
        }

        let key = CacheKey::TopTracks;
        if !self.begin_fetch(key.clone(), "Top tracks").await? {
            return Ok(None);
        }

        let result = async {
            match self.third_party_worker().fetch_top_tracks().await {
                Ok(tracks) => {
                    log_api("top_tracks route=third_party");
                    Ok(tracks)
                }
                Err(err) => {
                    log_api(&format!("top_tracks route=third_party failed={err:?}"));
                    if is_probable_rate_limit(&err) {
                        Err(err)
                    } else {
                        self.first_party_top_tracks().await
                    }
                }
            }
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let tracks = result?;
        self.cache.lock().await.set_top_tracks(tracks.clone());
        update_persistent_cache(|cache| cache.set_top_tracks(tracks.clone()));
        Ok(Some(tracks))
    }

    pub async fn recently_played(&self) -> Result<Option<Vec<Track>>> {
        if let Some(tracks) = self.cache.lock().await.recently_played() {
            log_api("recently_played route=cache");
            return Ok(Some(tracks));
        }
        if let Some(tracks) = AppConfig::load_cache().get_recently_played() {
            log_api("recently_played route=persistent_cache");
            self.cache.lock().await.set_recently_played(tracks.clone());
            return Ok(Some(tracks));
        }

        let key = CacheKey::RecentlyPlayed;
        if !self.begin_fetch(key.clone(), "Recently played").await? {
            return Ok(None);
        }

        let result = async {
            match self.third_party_worker().fetch_recently_played().await {
                Ok(tracks) => {
                    log_api("recently_played route=third_party");
                    Ok(tracks)
                }
                Err(err) => {
                    log_api(&format!("recently_played route=third_party failed={err:?}"));
                    if is_probable_rate_limit(&err) {
                        Err(err)
                    } else {
                        self.first_party_recently_played().await
                    }
                }
            }
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let tracks = result?;
        self.cache.lock().await.set_recently_played(tracks.clone());
        update_persistent_cache(|cache| cache.set_recently_played(tracks.clone()));
        Ok(Some(tracks))
    }

    pub async fn followed_artists(&self) -> Result<Option<Vec<Artist>>> {
        if let Some(artists) = self.cache.lock().await.followed_artists() {
            log_api("followed_artists route=cache");
            return Ok(Some(artists));
        }
        if let Some(artists) = AppConfig::load_cache().get_followed_artists() {
            log_api("followed_artists route=persistent_cache");
            self.cache
                .lock()
                .await
                .set_followed_artists(artists.clone());
            return Ok(Some(artists));
        }

        let key = CacheKey::FollowedArtists;
        if !self.begin_fetch(key.clone(), "Followed artists").await? {
            return Ok(None);
        }

        let result = async {
            match self.third_party_worker().fetch_followed_artists().await {
                Ok(artists) => {
                    log_api("followed_artists route=third_party");
                    Ok(artists)
                }
                Err(err) => {
                    log_api(&format!(
                        "followed_artists route=third_party failed={err:?}"
                    ));
                    if is_probable_rate_limit(&err) {
                        Err(err)
                    } else {
                        self.first_party_followed_artists().await
                    }
                }
            }
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let artists = result?;
        self.cache
            .lock()
            .await
            .set_followed_artists(artists.clone());
        update_persistent_cache(|cache| cache.set_followed_artists(artists.clone()));
        Ok(Some(artists))
    }

    pub async fn artist_page(
        &self,
        artist_id: &str,
        known_artist_name: Option<String>,
    ) -> Result<Option<(String, Vec<Track>, Vec<Album>)>> {
        if let Some(page) = self.cache.lock().await.artist_page(artist_id) {
            log_api(&format!("artist_page route=cache artist={artist_id}"));
            return Ok(Some(page));
        }
        if let Some(page) = AppConfig::load_cache().get_artist_page(artist_id) {
            log_api(&format!(
                "artist_page route=persistent_cache artist={artist_id}"
            ));
            let tuple = (page.artist_name, page.top_tracks, page.albums);
            self.cache
                .lock()
                .await
                .set_artist_page(artist_id.to_string(), tuple.clone());
            return Ok(Some(tuple));
        }

        let key = CacheKey::ArtistPage(artist_id.to_string());
        if !self.begin_fetch(key.clone(), "Artist page").await? {
            return Ok(None);
        }

        let result = self
            .first_party_artist_page(artist_id, known_artist_name)
            .await;
        self.finish_fetch(&key, &result).await;

        let page = result?;
        self.cache
            .lock()
            .await
            .set_artist_page(artist_id.to_string(), page.clone());
        update_persistent_cache(|cache| {
            let cached_albums = cache
                .artist_pages
                .get(artist_id)
                .map(|entry| entry.value.albums.clone())
                .unwrap_or_default();
            cache.set_artist_page(
                artist_id.to_string(),
                ArtistPageData {
                    artist_id: artist_id.to_string(),
                    artist_name: page.0.clone(),
                    top_tracks: page.1.clone(),
                    albums: if page.2.is_empty() {
                        cached_albums
                    } else {
                        page.2.clone()
                    },
                },
            );
        });
        Ok(Some(page))
    }

    pub async fn artist_albums(&self, artist_id: &str) -> Result<Option<Vec<Album>>> {
        if let Some(albums) = self.cache.lock().await.artist_albums(artist_id) {
            log_api(&format!("artist_albums route=cache artist={artist_id}"));
            return Ok(Some(albums));
        }

        if let Some(page) = AppConfig::load_cache().get_artist_page(artist_id)
            && !page.albums.is_empty()
        {
            log_api(&format!(
                "artist_albums route=persistent_cache artist={artist_id}"
            ));
            self.cache
                .lock()
                .await
                .set_artist_albums(artist_id.to_string(), page.albums.clone());
            return Ok(Some(page.albums));
        }

        let key = CacheKey::ArtistAlbums(artist_id.to_string());
        if !self.begin_fetch(key.clone(), "Artist albums").await? {
            return Ok(None);
        }

        let result = self.first_party_artist_albums(artist_id).await;
        self.finish_fetch(&key, &result).await;
        let albums = result?;
        self.cache
            .lock()
            .await
            .set_artist_albums(artist_id.to_string(), albums.clone());
        update_persistent_cache(|cache| {
            let mut page = cache.get_artist_page(artist_id).unwrap_or(ArtistPageData {
                artist_id: artist_id.to_string(),
                artist_name: "Unknown Artist".to_string(),
                top_tracks: Vec::new(),
                albums: Vec::new(),
            });
            page.albums = albums.clone();
            cache.set_artist_page(artist_id.to_string(), page);
        });
        Ok(Some(albums))
    }

    fn third_party_worker(&self) -> SpotifyWorker {
        SpotifyWorker::from_client(self.third_party.clone())
    }

    async fn begin_fetch(&self, key: CacheKey, _label: &str) -> Result<bool> {
        let persistent_key = persistent_cooldown_key(&key);
        let mut persistent_cache = AppConfig::load_cache();
        if let Some(remaining) = persistent_cache.cooldown_remaining(&persistent_key) {
            let _ = AppConfig::save_cache(&persistent_cache);
            anyhow::bail!(
                "rate limited. Try again in {}.",
                format_retry_after(remaining)
            );
        }

        match self.cache.lock().await.begin(key) {
            FetchGate::Start => Ok(true),
            FetchGate::InFlight => Ok(false),
            FetchGate::CoolingDown(remaining) => {
                anyhow::bail!(
                    "rate limited. Try again in {}.",
                    format_retry_after(remaining)
                )
            }
        }
    }

    async fn finish_fetch<T>(&self, key: &CacheKey, result: &Result<T>) {
        let mut cache = self.cache.lock().await;
        cache.finish(key);
        let persistent_key = persistent_cooldown_key(key);
        if let Err(err) = result
            && let Some(rate_limit) = rate_limit_error(err)
        {
            cache.rate_limited(key.clone(), rate_limit.cooldown());
            update_persistent_cache(|cache| {
                cache.set_cooldown(persistent_key.clone(), rate_limit.cooldown())
            });
        } else if let Err(err) = result
            && is_probable_rate_limit(err)
        {
            cache.rate_limited(key.clone(), DEFAULT_RATE_LIMIT_COOLDOWN);
            update_persistent_cache(|cache| {
                cache.set_cooldown(persistent_key.clone(), DEFAULT_RATE_LIMIT_COOLDOWN)
            });
        } else if result.is_ok() {
            update_persistent_cache(|cache| cache.clear_cooldown(&persistent_key));
        }
    }

    async fn first_party_json(
        &self,
        endpoint: ApiEndpoint,
        url: &str,
    ) -> Result<serde_json::Value> {
        let first_party = self.first_party.as_ref().with_context(|| {
            format!(
                "First-party route required for {} but no first-party client is available",
                endpoint.label()
            )
        })?;
        let value = first_party.get_json(url).await?;
        log_api(&format!(
            "{} route={:?}",
            endpoint.label(),
            endpoint.route()
        ));
        Ok(value)
    }

    async fn first_party_top_tracks(&self) -> Result<Vec<Track>> {
        let json = self
            .first_party_json(
                ApiEndpoint::TopTracks,
                "https://api.spotify.com/v1/me/top/tracks?limit=50",
            )
            .await?;
        Ok(json
            .get("items")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(parse::track).collect())
            .unwrap_or_default())
    }

    async fn first_party_recently_played(&self) -> Result<Vec<Track>> {
        let json = self
            .first_party_json(
                ApiEndpoint::RecentlyPlayed,
                "https://api.spotify.com/v1/me/player/recently-played?limit=50",
            )
            .await?;
        Ok(json
            .get("items")
            .and_then(|v| v.as_array())
            .map(|items| {
                let mut tracks = Vec::new();
                for item in items {
                    if let Some(track) = item.get("track").and_then(parse::track) {
                        if !tracks
                            .iter()
                            .any(|existing: &Track| existing.id == track.id)
                        {
                            tracks.push(track);
                        }
                    }
                }
                tracks
            })
            .unwrap_or_default())
    }

    async fn first_party_followed_artists(&self) -> Result<Vec<Artist>> {
        let json = self
            .first_party_json(
                ApiEndpoint::FollowedArtists,
                "https://api.spotify.com/v1/me/following?type=artist&limit=50",
            )
            .await?;
        Ok(json
            .get("artists")
            .and_then(|v| v.get("items"))
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(parse::artist).collect())
            .unwrap_or_default())
    }

    async fn first_party_artist_page(
        &self,
        artist_id: &str,
        known_artist_name: Option<String>,
    ) -> Result<(String, Vec<Track>, Vec<Album>)> {
        let artist_name = if let Some(name) = known_artist_name {
            name
        } else {
            let artist_url = format!("https://api.spotify.com/v1/artists/{artist_id}");
            let artist_json = self
                .first_party_json(ApiEndpoint::ArtistPage, &artist_url)
                .await?;
            artist_json
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown Artist")
                .to_string()
        };

        let tracks_url =
            format!("https://api.spotify.com/v1/artists/{artist_id}/top-tracks?market=from_token");
        let tracks_json = self
            .first_party_json(ApiEndpoint::ArtistPage, &tracks_url)
            .await?;
        let top_tracks = tracks_json
            .get("tracks")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(parse::track).collect())
            .unwrap_or_default();

        Ok((artist_name, top_tracks, Vec::new()))
    }

    async fn first_party_artist_albums(&self, artist_id: &str) -> Result<Vec<Album>> {
        let albums_url = format!(
            "https://api.spotify.com/v1/artists/{artist_id}/albums?include_groups=album,single&market=from_token&limit=50"
        );
        let albums_json = self
            .first_party_json(ApiEndpoint::ArtistPage, &albums_url)
            .await?;
        let mut albums: Vec<Album> = albums_json
            .get("items")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(parse::album).collect())
            .unwrap_or_default();
        albums.sort_by(|a, b| b.release_year.cmp(&a.release_year));

        Ok(albums)
    }
}

fn update_persistent_cache(update: impl FnOnce(&mut CacheData)) {
    let mut cache = AppConfig::load_cache();
    update(&mut cache);
    let _ = AppConfig::save_cache(&cache);
}

fn persistent_cooldown_key(key: &CacheKey) -> String {
    match key {
        CacheKey::TopTracks => "top_tracks".to_string(),
        CacheKey::RecentlyPlayed => "recently_played".to_string(),
        CacheKey::FollowedArtists => "followed_artists".to_string(),
        CacheKey::ArtistPage(artist_id) => format!("artist_page:{artist_id}"),
        CacheKey::ArtistAlbums(artist_id) => format!("artist_albums:{artist_id}"),
    }
}

pub fn format_retry_after(duration: Duration) -> String {
    let secs = duration.as_secs().max(1);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 60 * 60 {
        let minutes = secs.div_ceil(60);
        format!("{minutes}m")
    } else {
        let hours = secs.div_ceil(60 * 60);
        format!("{hours}h")
    }
}

fn log_api(message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("echo-debug-api.log")
    {
        let _ = writeln!(file, "{message}");
    }
}
