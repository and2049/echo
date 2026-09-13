use std::{io::Write, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use reqwest::header::RETRY_AFTER;
use rspotify::AuthCodeSpotify;
use rspotify::prelude::*;
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
    rate_limit::{DEFAULT_RATE_LIMIT_COOLDOWN, is_probable_rate_limit, parse_retry_after},
};

#[derive(Clone)]
pub struct EchoSpotifyClient {
    third_party_only: bool,
    history_lock: Arc<Mutex<()>>,
    third_party: AuthCodeSpotify,
    first_party: Option<SpotifyWebApi>,
    cache: Arc<Mutex<SpotifyApiCache>>,
    http: reqwest::Client,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtistAlbumsCachePolicy {
    UseCache,
    Refresh,
}

#[derive(Clone, Debug, Default)]
pub struct ArtistAlbumsResponse {
    pub cached: Option<Vec<Album>>,
    pub refreshed: Option<Vec<Album>>,
    pub refresh_skipped: bool,
}

impl EchoSpotifyClient {
    pub fn third_party_only(mut self) -> Self {
        self.third_party_only = true;
        self
    }

    pub async fn recent_history(&self) -> Result<crate::home::RecentHistory> {
        let _guard = self.history_lock.lock().await;
        if let Some(history) = self
            .cache
            .lock()
            .await
            .recent_history
            .as_ref()
            .and_then(|entry| entry.fresh_value(crate::config::RECENTLY_PLAYED_CACHE_TTL))
        {
            return Ok(history);
        }
        if let Some(entry) = AppConfig::load_cache().recent_history
            && let Some(history) = entry.fresh_value(crate::config::RECENTLY_PLAYED_CACHE_TTL)
        {
            self.cache.lock().await.recent_history = Some(entry);
            return Ok(history);
        }
        let key = CacheKey::RecentlyPlayed;
        if !self.begin_fetch(key.clone(), "Recently played").await? {
            anyhow::bail!("Recently played request is in progress");
        }
        let result = self.third_party_worker().fetch_recently_played().await;
        self.finish_fetch(&key, &result).await;
        let history = result?;
        let entry = crate::config::CachedEntry::new(history.clone());
        self.cache.lock().await.recent_history = Some(entry.clone());
        AppConfig::update_cache(|cache| {
            cache.set_recently_played(history.tracks.clone());
            cache.recent_history = Some(entry);
        });
        Ok(history)
    }

    pub async fn recent_contexts(&self) -> Result<Vec<crate::home::ResolvedRecentContext>> {
        use crate::home::{HomeItemKind, resolve_recent_context};
        let history = self.recent_history().await?;
        let persistent = AppConfig::load_cache();
        let mut playlists = persistent
            .get_playlists_entry()
            .map(|entry| entry.value)
            .unwrap_or_default();
        let mut artists = persistent.get_followed_artists().unwrap_or_default();
        artists.extend(persistent.get_top_artists().unwrap_or_default());
        let mut resolved = Vec::new();
        let mut lookups = 0;
        for context in history.contexts {
            if let Some(item) = resolve_recent_context(&context, &playlists, &artists) {
                resolved.push(item);
                continue;
            }
            if context.kind != HomeItemKind::Playlist {
                continue;
            }
            let Some(id) = context.uri.rsplit(':').next() else {
                continue;
            };
            let cached = self
                .cache
                .lock()
                .await
                .recent_playlists
                .get(id)
                .cloned()
                .or_else(|| persistent.recent_playlists.get(id).cloned())
                .filter(|entry| {
                    entry
                        .fresh_value(crate::config::ARTIST_PAGE_CACHE_TTL)
                        .is_some()
                });
            let entry = if let Some(entry) = cached {
                entry
            } else {
                if lookups >= 6 {
                    continue;
                }
                lookups += 1;
                let url = recent_playlist_url(id);
                let json = match self.third_party_json(&url).await {
                    Ok(json) => json,
                    Err(error) if is_probable_rate_limit(&error) => return Err(error),
                    Err(_) => continue,
                };
                let Some(playlist) = parse::playlist(&json) else {
                    continue;
                };
                AppConfig::update_cache(|cache| cache.set_recent_playlist(playlist.clone()));
                crate::config::CachedEntry::new(playlist)
            };
            self.cache
                .lock()
                .await
                .recent_playlists
                .insert(id.to_string(), entry.clone());
            playlists.push(entry.value);
            if let Some(item) = resolve_recent_context(&context, &playlists, &artists) {
                resolved.push(item);
            }
        }
        Ok(resolved)
    }

    pub async fn context_details(
        &self,
        context: &crate::models::TrackListContext,
        tracks: &mut [Track],
    ) -> Result<crate::context_details::ContextDetails> {
        use crate::context_details::{ContextDetails, total_duration};
        use rspotify::model::Id;
        let mut details = ContextDetails {
            owner: context.subtitle.clone(),
            duration_ms: total_duration(tracks),
            ..Default::default()
        };
        if context.id == "LIKED_SONGS" {
            let page = self
                .third_party
                .current_user_saved_tracks_manual(None, Some(1), Some(0))
                .await?;
            details.track_count = Some(page.total);
            return Ok(details);
        }
        let resource = if context.is_album() {
            "albums"
        } else {
            "playlists"
        };
        let json = self
            .third_party_json(&format!(
                "https://api.spotify.com/v1/{resource}/{}",
                context.id
            ))
            .await?;
        details.description = json
            .get("description")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        details.track_count = json
            .pointer("/items/total")
            .or_else(|| json.pointer("/tracks/total"))
            .or_else(|| json.get("total_tracks"))
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok());
        if context.is_album() {
            details.album_type = json
                .get("album_type")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            details.release_year = json
                .get("release_date")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(4).collect());
            details.owner = parse::joined_artist_names(&parse::track_artists_json(
                json.get("artists").and_then(|v| v.as_array()),
            ));
            return Ok(details);
        }
        details.public = json.get("public").and_then(|v| v.as_bool());
        details.collaborative = json
            .get("collaborative")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let owner_id = json
            .pointer("/owner/id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        details.owner = json
            .pointer("/owner/display_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(owner_id)
            .to_string();
        {
            let mut cache = self.cache.lock().await;
            cache
                .user_names
                .insert(owner_id.to_string(), details.owner.clone());
            for playlist in AppConfig::load_cache().get_playlists().unwrap_or_default() {
                cache.user_names.insert(playlist.owner_id, playlist.owner);
            }
        }
        if !self.cache.lock().await.current_user_loaded {
            let user = self.third_party.current_user().await?;
            let id = user.id.id().to_string();
            let mut cache = self.cache.lock().await;
            cache
                .user_names
                .insert(id.clone(), user.display_name.unwrap_or(id));
            cache.current_user_loaded = true;
        }
        let mut contributors = Vec::new();
        for track in tracks.iter() {
            if let Some(id) = &track.added_by
                && id != owner_id
                && !contributors.contains(id)
            {
                contributors.push(id.clone());
            }
        }
        for id in &contributors {
            if !self.cache.lock().await.reserve_user_lookup(&context.id, id) {
                continue;
            }
            let user_id = rspotify::model::UserId::from_id(id)?;
            let user = match self
                .third_party_json(&format!(
                    "https://api.spotify.com/v1/users/{}",
                    user_id.id()
                ))
                .await
            {
                Ok(user) => user,
                Err(error) if is_probable_rate_limit(&error) => return Err(error),
                Err(_) => serde_json::Value::Null,
            };
            self.cache.lock().await.user_names.insert(
                id.clone(),
                user.get("display_name")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(id)
                    .to_string(),
            );
        }
        let cache = self.cache.lock().await;
        for id in contributors {
            let name = cache.user_names.get(&id).cloned().unwrap_or(id);
            if !details.collaborators.contains(&name) {
                details.collaborators.push(name);
            }
        }
        for track in tracks {
            if let Some(id) = &track.added_by
                && let Some(name) = cache.user_names.get(id)
            {
                track.added_by = Some(name.clone());
            }
        }
        Ok(details)
    }

    pub fn new(third_party: AuthCodeSpotify, first_party: Option<SpotifyWebApi>) -> Self {
        Self {
            third_party_only: false,
            history_lock: Arc::new(Mutex::new(())),
            third_party,
            first_party,
            cache: Arc::new(Mutex::new(SpotifyApiCache::default())),
            http: reqwest::Client::new(),
        }
    }

    pub async fn top_tracks(
        &self,
        range: crate::models::TopItemsRange,
    ) -> Result<Option<Vec<Track>>> {
        // Only the default range is persisted; the others live in the session cache.
        let persist = range == crate::models::TopItemsRange::Medium;
        if let Some(tracks) = self.cache.lock().await.top_tracks(range) {
            log_api("top_tracks route=cache");
            return Ok(Some(tracks));
        }
        if persist && let Some(tracks) = AppConfig::load_cache().get_top_tracks() {
            log_api("top_tracks route=persistent_cache");
            self.cache
                .lock()
                .await
                .set_top_tracks(range, tracks.clone());
            return Ok(Some(tracks));
        }

        let key = CacheKey::TopTracks(range);
        if !self.begin_fetch(key.clone(), "Top tracks").await? {
            return Ok(None);
        }

        let result = async {
            match self.third_party_worker().fetch_top_tracks(range).await {
                Ok(tracks) => {
                    log_api("top_tracks route=third_party");
                    Ok(tracks)
                }
                Err(err) => {
                    log_api(&format!("top_tracks route=third_party failed={err:?}"));
                    if self.third_party_only || is_probable_rate_limit(&err) {
                        Err(err)
                    } else {
                        self.first_party_top_tracks(range).await
                    }
                }
            }
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let tracks = result?;
        self.cache
            .lock()
            .await
            .set_top_tracks(range, tracks.clone());
        if persist {
            AppConfig::update_cache(|cache| cache.set_top_tracks(tracks.clone()));
        }
        Ok(Some(tracks))
    }

    /// The user's top artists. Third-party only: the first-party route's shared client
    /// id is quota-throttled, and this list is decorative enough to fail soft.
    pub async fn top_artists(
        &self,
        range: crate::models::TopItemsRange,
    ) -> Result<Option<Vec<Artist>>> {
        let persist = range == crate::models::TopItemsRange::Medium;
        if let Some(artists) = self.cache.lock().await.top_artists(range) {
            log_api("top_artists route=cache");
            return Ok(Some(artists));
        }
        if persist && let Some(artists) = AppConfig::load_cache().get_top_artists() {
            log_api("top_artists route=persistent_cache");
            self.cache
                .lock()
                .await
                .set_top_artists(range, artists.clone());
            return Ok(Some(artists));
        }

        let key = CacheKey::TopArtists(range);
        if !self.begin_fetch(key.clone(), "Top artists").await? {
            return Ok(None);
        }

        let result = self.third_party_worker().fetch_top_artists(range).await;
        self.finish_fetch(&key, &result).await;
        let artists = result?;
        log_api("top_artists route=third_party");
        self.cache
            .lock()
            .await
            .set_top_artists(range, artists.clone());
        if persist {
            AppConfig::update_cache(|cache| cache.set_top_artists(artists.clone()));
        }
        Ok(Some(artists))
    }

    pub async fn recently_played(&self) -> Result<Option<Vec<Track>>> {
        if self.third_party_only {
            return self
                .recent_history()
                .await
                .map(|history| Some(history.tracks));
        }
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
            match self
                .third_party_worker()
                .fetch_recently_played()
                .await
                .map(|history| history.tracks)
            {
                Ok(tracks) => {
                    log_api("recently_played route=third_party");
                    Ok(tracks)
                }
                Err(err) => {
                    log_api(&format!("recently_played route=third_party failed={err:?}"));
                    if self.third_party_only || is_probable_rate_limit(&err) {
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
        AppConfig::update_cache(|cache| cache.set_recently_played(tracks.clone()));
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
                    if self.third_party_only || is_probable_rate_limit(&err) {
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
        AppConfig::update_cache(|cache| cache.set_followed_artists(artists.clone()));
        Ok(Some(artists))
    }

    /// An artist's top tracks, approximated by a third-party track search filtered to
    /// the artist's own credits: the real `/artists/{id}/top-tracks` endpoint is gone
    /// from the dev-mode API and the first-party route is quota-throttled. Session-
    /// cached, never persisted (see `ArtistPageData`).
    pub async fn artist_top_tracks(
        &self,
        artist_id: &str,
        artist_name: Option<&str>,
    ) -> Result<Option<Vec<Track>>> {
        if let Some(tracks) = self.cache.lock().await.artist_top_tracks(artist_id) {
            log_api(&format!("artist_top_tracks route=cache artist={artist_id}"));
            return Ok(Some(tracks));
        }

        let key = CacheKey::ArtistTopTracks(artist_id.to_string());
        if !self.begin_fetch(key.clone(), "Artist top tracks").await? {
            return Ok(None);
        }

        let result = async {
            let name = match artist_name {
                Some(name) => name.to_string(),
                // A page opened by bare id (pasted URI) has no name yet; searching
                // for the "Unknown Artist" placeholder would return garbage.
                None => self.fetch_artist_name(artist_id).await?,
            };
            let tracks = self
                .third_party_worker()
                .search_artist_top_tracks(artist_id, &name)
                .await?;
            log_api(&format!(
                "artist_top_tracks route=third_party_search artist={artist_id} count={}",
                tracks.len()
            ));
            Ok(tracks)
        }
        .await;

        self.finish_fetch(&key, &result).await;
        let tracks = result?;
        self.cache
            .lock()
            .await
            .set_artist_top_tracks(artist_id.to_string(), tracks.clone());
        Ok(Some(tracks))
    }

    async fn fetch_artist_name(&self, artist_id: &str) -> Result<String> {
        let url = format!("https://api.spotify.com/v1/artists/{artist_id}");
        let json = self.third_party_json(&url).await?;
        json.get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .context("Artist profile has no name")
    }

    pub async fn artist_albums_with_policy(
        &self,
        artist_id: &str,
        policy: ArtistAlbumsCachePolicy,
    ) -> Result<ArtistAlbumsResponse> {
        let force_refresh = policy == ArtistAlbumsCachePolicy::Refresh;
        let mut response = ArtistAlbumsResponse::default();
        let mut needs_refresh = force_refresh;

        if !force_refresh {
            let memory = {
                let cache = self.cache.lock().await;
                cache.artist_albums(artist_id).map(|albums| {
                    (
                        albums,
                        cache.artist_albums_need_refresh(artist_id).unwrap_or(true),
                    )
                })
            };
            if let Some((albums, memory_needs_refresh)) = memory {
                log_api(&format!(
                    "artist_albums route=cache artist={artist_id} needs_refresh={memory_needs_refresh}"
                ));
                response.cached = Some(albums);
                needs_refresh = memory_needs_refresh;
            } else if let Some(entry) = AppConfig::load_cache().get_artist_page_entry(artist_id) {
                let persistent_needs_refresh = CacheData::artist_page_needs_album_refresh(&entry);
                log_api(&format!(
                    "artist_albums route=persistent_cache artist={artist_id} needs_refresh={persistent_needs_refresh}"
                ));
                response.cached = Some(entry.value.albums.clone());
                needs_refresh = persistent_needs_refresh;
                if !persistent_needs_refresh {
                    self.cache
                        .lock()
                        .await
                        .set_artist_albums(artist_id.to_string(), entry.value.albums);
                }
            } else {
                needs_refresh = true;
            }
        }

        if !needs_refresh && response.cached.is_some() {
            return Ok(response);
        }

        let key = CacheKey::ArtistAlbums(artist_id.to_string());
        if !self.begin_fetch(key.clone(), "Artist albums").await? {
            response.refresh_skipped = true;
            return Ok(response);
        }

        let result = self.fetch_artist_albums_network(artist_id).await;
        self.finish_fetch(&key, &result).await;
        let albums = result?;
        self.store_artist_albums(artist_id, albums.clone()).await;
        response.refreshed = Some(albums);
        Ok(response)
    }

    fn third_party_worker(&self) -> SpotifyWorker {
        SpotifyWorker::from_client(self.third_party.clone())
    }

    async fn begin_fetch(&self, key: CacheKey, _label: &str) -> Result<bool> {
        let persistent_key = persistent_cooldown_key(&key);
        if let Some(remaining) = AppConfig::load_cache().cooldown_remaining(&persistent_key) {
            log_api(&format!(
                "fetch gate=persistent_cooldown key={key:?} persistent_key={persistent_key} remaining={}",
                format_retry_after(remaining)
            ));
            anyhow::bail!(
                "rate limited. Try again in {}.",
                format_retry_after(remaining)
            );
        }

        match self.cache.lock().await.begin(key.clone()) {
            FetchGate::Start => Ok(true),
            FetchGate::InFlight => {
                log_api(&format!("fetch gate=inflight key={key:?}"));
                Ok(false)
            }
            FetchGate::CoolingDown(remaining) => {
                log_api(&format!(
                    "fetch gate=memory_cooldown key={key:?} remaining={}",
                    format_retry_after(remaining)
                ));
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
            let cooldown = record_rate_limit_cooldown(&persistent_key, rate_limit.cooldown());
            cache.rate_limited(key.clone(), cooldown);
            log_api(&format!(
                "fetch cooldown=set key={key:?} persistent_key={persistent_key} remaining={} err=typed_429",
                format_retry_after(cooldown)
            ));
        } else if let Err(err) = result
            && is_probable_rate_limit(err)
        {
            let cooldown = record_rate_limit_cooldown(&persistent_key, DEFAULT_RATE_LIMIT_COOLDOWN);
            cache.rate_limited(key.clone(), cooldown);
            log_api(&format!(
                "fetch cooldown=set key={key:?} persistent_key={persistent_key} remaining={} err=probable_429",
                format_retry_after(cooldown)
            ));
        } else if result.is_ok() {
            log_api(&format!(
                "fetch cooldown=clear key={key:?} persistent_key={persistent_key}"
            ));
            AppConfig::update_cache(|cache| cache.clear_cooldown(&persistent_key));
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

    async fn first_party_top_tracks(
        &self,
        range: crate::models::TopItemsRange,
    ) -> Result<Vec<Track>> {
        let time_range = match range {
            crate::models::TopItemsRange::Short => "short_term",
            crate::models::TopItemsRange::Medium => "medium_term",
            crate::models::TopItemsRange::Long => "long_term",
        };
        let json = self
            .first_party_json(
                ApiEndpoint::TopTracks,
                &format!(
                    "https://api.spotify.com/v1/me/top/tracks?limit=50&time_range={time_range}"
                ),
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
                    if let Some(track) = item.get("track").and_then(parse::track)
                        && !tracks
                            .iter()
                            .any(|existing: &Track| existing.id == track.id)
                    {
                        tracks.push(track);
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

    async fn fetch_artist_albums_network(&self, artist_id: &str) -> Result<Vec<Album>> {
        match self.third_party_artist_albums(artist_id).await {
            Ok(albums) => {
                log_api(&format!(
                    "artist_albums route=third_party artist={artist_id}"
                ));
                Ok(albums)
            }
            Err(err) => {
                log_api(&format!(
                    "artist_albums route=third_party failed artist={artist_id} err={err:?}"
                ));
                if self.third_party_only || is_probable_rate_limit(&err) {
                    Err(err)
                } else {
                    self.first_party_artist_albums(artist_id)
                        .await
                        .inspect(|_| {
                            log_api(&format!(
                                "artist_albums route=first_party artist={artist_id}"
                            ));
                        })
                }
            }
        }
    }

    async fn store_artist_albums(&self, artist_id: &str, albums: Vec<Album>) {
        self.cache
            .lock()
            .await
            .set_artist_albums(artist_id.to_string(), albums.clone());
        AppConfig::update_cache(|cache| {
            let mut page = cache
                .get_artist_page_entry(artist_id)
                .map(|entry| entry.value)
                .unwrap_or(ArtistPageData {
                    artist_id: artist_id.to_string(),
                    artist_name: "Unknown Artist".to_string(),
                    image_url: None,
                    albums: Vec::new(),
                    top_tracks: Vec::new(),
                });
            page.albums = albums.clone();
            cache.set_artist_page(artist_id.to_string(), page);
        });
    }

    pub(super) async fn third_party_json(&self, url: &str) -> Result<serde_json::Value> {
        self.third_party.auto_reauth().await?;
        let token_mutex = self.third_party.get_token();
        let token_guard = token_mutex.lock().await.unwrap();
        let access_token = token_guard
            .as_ref()
            .map(|token| token.access_token.clone())
            .context("Third-party Spotify token is unavailable")?;
        drop(token_guard);

        let response = self.http.get(url).bearer_auth(access_token).send().await?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let body = response.text().await?;
        log_api(&format!(
            "http route=third_party status={} retry_after={retry_after:?} url={url}",
            status.as_u16()
        ));

        if status.as_u16() == 429 {
            return Err(super::rate_limit::SpotifyRateLimitError { retry_after, body }.into());
        }

        if !status.is_success() {
            anyhow::bail!("Spotify Web API request failed ({status}): {body}");
        }

        Ok(serde_json::from_str(&body)?)
    }

    async fn third_party_artist_albums(&self, artist_id: &str) -> Result<Vec<Album>> {
        let mut albums = Vec::new();
        let mut offset = 0usize;
        loop {
            let albums_json = self
                .third_party_json(&artist_albums_url(artist_id, offset))
                .await?;
            let (mut page_albums, has_next) = parse_artist_albums_page(&albums_json);
            let page_len = page_albums.len();
            albums.append(&mut page_albums);
            if !has_next || page_len == 0 {
                break;
            }
            offset += ARTIST_ALBUMS_PAGE_LIMIT;
        }
        albums.sort_by(|a, b| {
            let key = |album: &Album| {
                album
                    .release_date
                    .clone()
                    .unwrap_or_else(|| album.release_year.clone())
            };
            key(b).cmp(&key(a))
        });

        Ok(albums)
    }

    async fn first_party_artist_albums(&self, artist_id: &str) -> Result<Vec<Album>> {
        let mut albums = Vec::new();
        let mut offset = 0usize;
        loop {
            let albums_json = self
                .first_party_json(
                    ApiEndpoint::ArtistPage,
                    &artist_albums_url(artist_id, offset),
                )
                .await?;
            let (mut page_albums, has_next) = parse_artist_albums_page(&albums_json);
            let page_len = page_albums.len();
            albums.append(&mut page_albums);
            if !has_next || page_len == 0 {
                break;
            }
            offset += ARTIST_ALBUMS_PAGE_LIMIT;
        }
        albums.sort_by(|a, b| {
            let key = |album: &Album| {
                album
                    .release_date
                    .clone()
                    .unwrap_or_else(|| album.release_year.clone())
            };
            key(b).cmp(&key(a))
        });

        Ok(albums)
    }

    /// Fetch an artist's profile image URL via the standard third-party API.
    /// Returns `None` if the request fails or the artist has no images.
    pub async fn fetch_artist_image_url(&self, artist_id: &str) -> Option<String> {
        let url = format!("https://api.spotify.com/v1/artists/{artist_id}");
        let json = self.third_party_json(&url).await.ok()?;
        json.get("images")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|img| img.get("url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
    }
}

const ARTIST_ALBUMS_PAGE_LIMIT: usize = 10;

fn recent_playlist_url(id: &str) -> String {
    let mut url = reqwest::Url::parse("https://api.spotify.com/v1/playlists/").unwrap();
    url.path_segments_mut().unwrap().pop_if_empty().push(id);
    url.query_pairs_mut().append_pair(
        "fields",
        "id,name,images,owner(display_name,id),public,collaborative,description,tracks.total",
    );
    url.into()
}

fn artist_albums_url(artist_id: &str, offset: usize) -> String {
    format!(
        "https://api.spotify.com/v1/artists/{artist_id}/albums?include_groups=album,single&market=from_token&limit={ARTIST_ALBUMS_PAGE_LIMIT}&offset={offset}"
    )
}

fn parse_artist_albums_page(json: &serde_json::Value) -> (Vec<Album>, bool) {
    let albums = json
        .get("items")
        .and_then(|v| v.as_array())
        .map(|items| items.iter().filter_map(parse::album).collect())
        .unwrap_or_default();
    let has_next = json.get("next").is_some_and(|v| !v.is_null());
    (albums, has_next)
}

fn record_rate_limit_cooldown(key: &str, retry_after: Duration) -> Duration {
    AppConfig::update_cache(|cache| cache.record_rate_limit_cooldown(key.to_string(), retry_after))
}

fn persistent_cooldown_key(key: &CacheKey) -> String {
    match key {
        // Range-independent: rate limits are endpoint-scoped, not per time window.
        CacheKey::TopTracks(_) => "top_tracks".to_string(),
        CacheKey::TopArtists(_) => "top_artists".to_string(),
        CacheKey::RecentlyPlayed => "recently_played".to_string(),
        CacheKey::FollowedArtists => "followed_artists".to_string(),
        CacheKey::ArtistAlbums(artist_id) => format!("artist_albums:{artist_id}"),
        CacheKey::ArtistTopTracks(artist_id) => format!("artist_top_tracks:{artist_id}"),
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
        .open(crate::config::debug_log_path("echo-debug-api.log"))
    {
        let _ = writeln!(file, "{} {message}", chrono::Utc::now().to_rfc3339());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn recent_playlist_lookup_requests_only_the_needed_third_party_fields() {
        let url = reqwest::Url::parse(&recent_playlist_url("playlist")).unwrap();
        assert_eq!(url.host_str(), Some("api.spotify.com"));
        assert_eq!(url.path(), "/v1/playlists/playlist");
        let fields = url
            .query_pairs()
            .find(|(key, _)| key == "fields")
            .unwrap()
            .1;
        assert_eq!(
            fields,
            "id,name,images,owner(display_name,id),public,collaborative,description,tracks.total"
        );
    }

    use super::*;

    #[test]
    fn artist_album_url_uses_spotify_limit_ten() {
        let url = artist_albums_url("artist", 20);

        assert!(url.contains("/artists/artist/albums"));
        assert!(url.contains("limit=10"));
        assert!(url.contains("offset=20"));
    }

    #[test]
    fn artist_album_page_parser_reads_items_and_next() {
        let json = serde_json::json!({
            "next": "https://api.spotify.com/v1/artists/artist/albums?offset=10&limit=10",
            "items": [
                {
                    "id": "album-a",
                    "name": "Album A",
                    "artists": [{ "name": "Artist" }],
                    "release_date": "2024-01-01",
                    "images": [{ "url": "cover-a" }]
                },
                {
                    "id": "album-b",
                    "name": "Album B",
                    "artists": [{ "name": "Artist" }],
                    "release_date": "2023",
                    "images": []
                }
            ]
        });

        let (albums, has_next) = parse_artist_albums_page(&json);

        assert!(has_next);
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].id, "album-a");
        assert_eq!(albums[0].release_year, "2024");
        assert_eq!(albums[1].id, "album-b");
    }

    #[test]
    fn artist_album_page_parser_stops_on_null_next() {
        let json = serde_json::json!({
            "next": null,
            "items": []
        });

        let (albums, has_next) = parse_artist_albums_page(&json);

        assert!(albums.is_empty());
        assert!(!has_next);
    }
}
