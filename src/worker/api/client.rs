use std::{io::Write, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use rspotify::AuthCodeSpotify;
use tokio::sync::Mutex;

use crate::models::{Album, Artist, Track};

use super::{
    SpotifyWorker,
    cache::{CacheKey, FetchGate, SpotifyApiCache},
    first_party::{SpotifyWebApi, rate_limit_error},
    policy::ApiEndpoint,
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
                    self.first_party_top_tracks().await
                }
            }
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let tracks = result?;
        self.cache.lock().await.set_top_tracks(tracks.clone());
        Ok(Some(tracks))
    }

    pub async fn recently_played(&self) -> Result<Option<Vec<Track>>> {
        if let Some(tracks) = self.cache.lock().await.recently_played() {
            log_api("recently_played route=cache");
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
                    self.first_party_recently_played().await
                }
            }
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let tracks = result?;
        self.cache.lock().await.set_recently_played(tracks.clone());
        Ok(Some(tracks))
    }

    pub async fn followed_artists(&self) -> Result<Option<Vec<Artist>>> {
        if let Some(artists) = self.cache.lock().await.followed_artists() {
            log_api("followed_artists route=cache");
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
                    self.first_party_followed_artists().await
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
        Ok(Some(page))
    }

    fn third_party_worker(&self) -> SpotifyWorker {
        SpotifyWorker::from_client(self.third_party.clone())
    }

    async fn begin_fetch(&self, key: CacheKey, _label: &str) -> Result<bool> {
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
        if let Err(err) = result
            && let Some(rate_limit) = rate_limit_error(err)
        {
            cache.rate_limited(key.clone(), rate_limit.cooldown());
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
            .map(|items| items.iter().filter_map(parse_track).collect())
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
                    if let Some(track) = item.get("track").and_then(parse_track) {
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
            .map(|items| items.iter().filter_map(parse_artist).collect())
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
            .map(|items| items.iter().filter_map(parse_track).collect())
            .unwrap_or_default();

        let albums_url = format!(
            "https://api.spotify.com/v1/artists/{artist_id}/albums?include_groups=album,single&market=from_token&limit=50"
        );
        let albums_json = self
            .first_party_json(ApiEndpoint::ArtistPage, &albums_url)
            .await?;
        let mut albums: Vec<Album> = albums_json
            .get("items")
            .and_then(|v| v.as_array())
            .map(|items| items.iter().filter_map(parse_album).collect())
            .unwrap_or_default();
        albums.sort_by(|a, b| b.release_year.cmp(&a.release_year));

        Ok((artist_name, top_tracks, albums))
    }
}

fn parse_track(track: &serde_json::Value) -> Option<Track> {
    if track
        .get("is_local")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let id = track.get("id")?.as_str()?.to_string();
    let album = track.get("album");
    Some(Track {
        id,
        name: track.get("name")?.as_str()?.to_string(),
        artist: track
            .get("artists")
            .and_then(|v| v.as_array())
            .map(|artists| {
                artists
                    .iter()
                    .filter_map(|artist| artist.get("name").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        duration_ms: track
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or_default() as u32,
        image_url: album
            .and_then(|v| v.get("images"))
            .and_then(|v| v.as_array())
            .and_then(|images| images.first())
            .and_then(|image| image.get("url"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        album_id: album
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
    })
}

fn parse_album(album: &serde_json::Value) -> Option<Album> {
    let release_date = album
        .get("release_date")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    Some(Album {
        id: album.get("id")?.as_str()?.to_string(),
        name: album.get("name")?.as_str()?.to_string(),
        artists: album
            .get("artists")
            .and_then(|v| v.as_array())
            .map(|artists| {
                artists
                    .iter()
                    .filter_map(|artist| artist.get("name").and_then(|v| v.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        image_url: album
            .get("images")
            .and_then(|v| v.as_array())
            .and_then(|images| images.first())
            .and_then(|image| image.get("url"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        release_year: release_date.split('-').next().unwrap_or("").to_string(),
    })
}

fn parse_artist(artist: &serde_json::Value) -> Option<Artist> {
    Some(Artist {
        id: artist.get("id")?.as_str()?.to_string(),
        name: artist.get("name")?.as_str()?.to_string(),
        followers: artist
            .get("followers")
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_u64())
            .unwrap_or_default() as u32,
        image_url: artist
            .get("images")
            .and_then(|v| v.as_array())
            .and_then(|images| images.first())
            .and_then(|image| image.get("url"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
    })
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
