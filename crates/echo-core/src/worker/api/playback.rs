use super::SpotifyWorker;
use crate::models::{PlaybackItem, PlayingContext, Track, TrackSource};
use anyhow::Result;
use rspotify::AuthCodeSpotify;
use rspotify::model::Id;
use rspotify::prelude::*;

const PLAYBACK_TYPES: [&rspotify::model::AdditionalType; 2] = [
    &rspotify::model::AdditionalType::Track,
    &rspotify::model::AdditionalType::Episode,
];

/// Marker in the error chain when echo's own Connect device could not be resolved.
/// Callers match on it to show a device message instead of a raw HTTP 404.
pub const NO_DEVICE_ERROR: &str = "no playback device available";

/// How long to wait before re-polling the device list. The librespot daemon registers a few
/// seconds after login, so a play issued during startup can race it.
const DEVICE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(1500);

impl SpotifyWorker {
    pub async fn get_device_id(&mut self) -> Option<String> {
        if self.device_id.is_some() {
            return self.device_id.clone();
        }

        if let Ok(devices) = self.client.device().await {
            for d in devices {
                if d.name == "echo-rs" {
                    self.device_id = d.id.clone();
                    return self.device_id.clone();
                }
            }
        }
        None
    }

    /// Resolves echo's own Connect device, giving a late-registering daemon one retry.
    ///
    /// Playback always targets our librespot device, so a `None` device id is not a benign
    /// "let Spotify choose" — `/v1/me/player/play` with no `device_id` and nothing active is a
    /// guaranteed 404. Failing here reports the real problem (no device) instead of laundering
    /// it through an HTTP error that reads like a bad track id.
    async fn require_device_id(&mut self) -> Result<String> {
        if let Some(id) = self.get_device_id().await {
            return Ok(id);
        }
        tokio::time::sleep(DEVICE_RETRY_DELAY).await;
        self.get_device_id()
            .await
            .ok_or_else(|| anyhow::anyhow!(NO_DEVICE_ERROR))
    }

    pub async fn wake_up_device(&mut self) -> Result<()> {
        if let Some(device_id) = self.get_device_id().await {
            let _ = self.client.transfer_playback(&device_id, Some(false)).await;
            // Force pause it so it doesn't automatically resume the previous session's playback
            let _ = self.client.pause_playback(Some(&device_id)).await;
        }
        Ok(())
    }

    fn playback_item_from_unknown(value: &serde_json::Value) -> Option<PlaybackItem> {
        let id = value.get("id")?.as_str()?.to_string();
        let title = value.get("name")?.as_str()?.to_string();
        let duration_ms = value.get("duration_ms")?.as_u64()? as u32;

        let artist = value
            .get("artists")
            .and_then(|artists| artists.as_array())
            .map(|artists| {
                artists
                    .iter()
                    .filter_map(|artist| artist.get("name").and_then(|name| name.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|artist| !artist.is_empty())
            .or_else(|| {
                value
                    .get("show")
                    .and_then(|show| show.get("name"))
                    .and_then(|name| name.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_default();

        let image_url = value
            .get("album")
            .and_then(|album| album.get("images"))
            .or_else(|| value.get("images"))
            .and_then(|images| images.as_array())
            .and_then(|images| images.first())
            .and_then(|image| image.get("url"))
            .and_then(|url| url.as_str())
            .map(str::to_string);

        Some(PlaybackItem {
            id,
            source: TrackSource::Spotify,
            local_path: None,
            title,
            artist,
            duration_ms,
            image_url,
            album_id: value
                .get("album")
                .and_then(|a| a.get("id"))
                .and_then(|i| i.as_str())
                .map(str::to_string),
            artist_id: value
                .get("artists")
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|a| a.get("id"))
                .and_then(|i| i.as_str())
                .map(str::to_string),
        })
    }

    pub fn playback_item_from_playable(
        item: &rspotify::model::PlayableItem,
    ) -> Option<PlaybackItem> {
        match item {
            rspotify::model::PlayableItem::Track(track) => {
                let id = track.id.as_ref()?.id().to_string();
                Some(PlaybackItem {
                    id,
                    source: TrackSource::Spotify,
                    local_path: None,
                    title: track.name.clone(),
                    artist: track
                        .artists
                        .iter()
                        .map(|artist| artist.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    duration_ms: track.duration.num_milliseconds() as u32,
                    image_url: track.album.images.first().map(|img| img.url.clone()),
                    album_id: track.album.id.as_ref().map(|id| id.id().to_string()),
                    artist_id: track
                        .artists
                        .first()
                        .and_then(|a| a.id.as_ref())
                        .map(|id| id.id().to_string()),
                })
            }
            rspotify::model::PlayableItem::Episode(episode) => Some(PlaybackItem {
                id: episode.id.id().to_string(),
                source: TrackSource::Spotify,
                local_path: None,
                title: episode.name.clone(),
                artist: episode.show.name.clone(),
                duration_ms: episode.duration.num_milliseconds() as u32,
                image_url: episode.images.first().map(|img| img.url.clone()),
                album_id: None,
                artist_id: None,
            }),
            rspotify::model::PlayableItem::Unknown(value) => {
                Self::playback_item_from_unknown(value)
            }
        }
    }

    pub async fn playback_snapshot_from_client(
        client: &AuthCodeSpotify,
    ) -> Result<
        Option<(
            bool,
            bool,
            String,
            Option<u32>,
            String,
            u32,
            Option<PlaybackItem>,
            Option<PlayingContext>,
        )>,
    > {
        if let Some(playback) = client.current_playback(None, Some(PLAYBACK_TYPES)).await? {
            let is_playing = playback.is_playing;
            let is_shuffled = playback.shuffle_state;
            let repeat_mode = Self::repeat_mode_label(playback.repeat_state);
            let device_name = playback.device.name.clone();
            let volume = playback.device.volume_percent;
            let progress_ms = playback.progress.unwrap_or_default().num_milliseconds() as u32;
            let item = playback
                .item
                .as_ref()
                .and_then(Self::playback_item_from_playable);
            let context = Self::playing_context_from_playback(playback.context.as_ref());

            return Ok(Some((
                is_playing,
                is_shuffled,
                repeat_mode,
                volume,
                device_name,
                progress_ms,
                item,
                context,
            )));
        }

        Ok(None)
    }

    fn playing_context_from_playback(
        context: Option<&rspotify::model::Context>,
    ) -> Option<PlayingContext> {
        let context = context?;
        let is_album = match context._type {
            rspotify::model::Type::Album => true,
            rspotify::model::Type::Playlist => false,
            _ => return None,
        };
        let context_id = context.uri.rsplit(':').next()?.to_string();
        (!context_id.is_empty()).then_some(PlayingContext {
            context_id,
            is_album,
        })
    }

    fn repeat_mode_label(repeat_state: rspotify::model::RepeatState) -> String {
        match repeat_state {
            rspotify::model::RepeatState::Track => "Track".to_string(),
            rspotify::model::RepeatState::Context => "Context".to_string(),
            rspotify::model::RepeatState::Off => "Off".to_string(),
        }
    }

    pub async fn sync_playback_state(
        &mut self,
    ) -> Result<
        Option<(
            bool,
            bool,
            String,
            Option<u32>,
            String,
            u32,
            Option<PlaybackItem>,
            Option<PlayingContext>,
        )>,
    > {
        if let Some(playback) = self
            .client
            .current_playback(None, Some(PLAYBACK_TYPES))
            .await?
        {
            let is_playing = playback.is_playing;
            let is_shuffled = playback.shuffle_state;
            let progress_ms = playback.progress.unwrap_or_default().num_milliseconds() as u32;
            let item = playback
                .item
                .as_ref()
                .and_then(Self::playback_item_from_playable);

            let repeat_mode = Self::repeat_mode_label(playback.repeat_state);

            let device = &playback.device;
            let volume = device.volume_percent;
            let device_name = device.name.clone();

            // Auto-cache the device ID if we found an active playback
            if self.device_id.is_none() && device.name == "echo-rs" {
                self.device_id = device.id.clone();
            }

            return Ok(Some((
                is_playing,
                is_shuffled,
                repeat_mode,
                volume,
                device_name,
                progress_ms,
                item,
                Self::playing_context_from_playback(playback.context.as_ref()),
            )));
        }

        // Fallback: If no active device, check if Spotify remembers the last playing track
        if let Some(playing) = self
            .client
            .current_playing(None, Some(PLAYBACK_TYPES))
            .await?
        {
            let is_playing = playing.is_playing;
            let progress_ms = playing.progress.unwrap_or_default().num_milliseconds() as u32;
            let item = playing
                .item
                .as_ref()
                .and_then(Self::playback_item_from_playable);

            return Ok(Some((
                is_playing,
                false,             // Default shuffle
                "Off".to_string(), // Default repeat
                None,
                "Unknown Device".to_string(),
                progress_ms,
                item,
                Self::playing_context_from_playback(playing.context.as_ref()),
            )));
        }

        Ok(None)
    }

    pub async fn toggle_playback(&mut self, is_playing: bool) -> Result<()> {
        let device_id = self.get_device_id().await;
        match self
            .try_toggle_playback(device_id.as_deref(), is_playing)
            .await
        {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                let fresh = self.get_device_id().await;
                self.try_toggle_playback(fresh.as_deref(), is_playing).await
            }
            other => other,
        }
    }

    async fn try_toggle_playback(&self, device: Option<&str>, is_playing: bool) -> Result<()> {
        if is_playing {
            self.client.resume_playback(device, None).await?;
        } else {
            self.client.pause_playback(device).await?;
        }
        Ok(())
    }

    pub async fn next_track(&mut self) -> Result<()> {
        let device_id = self.get_device_id().await;
        match self.client.next_track(device_id.as_deref()).await {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                let fresh = self.get_device_id().await;
                self.client
                    .next_track(fresh.as_deref())
                    .await
                    .map_err(Into::into)
            }
            other => other.map_err(Into::into),
        }
    }

    pub async fn previous_track(&mut self) -> Result<()> {
        let device_id = self.get_device_id().await;
        match self.client.previous_track(device_id.as_deref()).await {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                let fresh = self.get_device_id().await;
                self.client
                    .previous_track(fresh.as_deref())
                    .await
                    .map_err(Into::into)
            }
            other => other.map_err(Into::into),
        }
    }

    pub async fn toggle_shuffle(&mut self, is_shuffled: bool) -> Result<()> {
        let device_id = self.get_device_id().await;
        match self.client.shuffle(is_shuffled, device_id.as_deref()).await {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                let fresh = self.get_device_id().await;
                self.client
                    .shuffle(is_shuffled, fresh.as_deref())
                    .await
                    .map_err(Into::into)
            }
            other => other.map_err(Into::into),
        }
    }

    pub async fn set_repeat_mode(&mut self, state: rspotify::model::RepeatState) -> Result<()> {
        let device_id = self.get_device_id().await;
        match self.client.repeat(state, device_id.as_deref()).await {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                let fresh = self.get_device_id().await;
                self.client
                    .repeat(state, fresh.as_deref())
                    .await
                    .map_err(Into::into)
            }
            other => other.map_err(Into::into),
        }
    }

    pub async fn set_volume(&mut self, volume: u8) -> Result<()> {
        let device_id = self.get_device_id().await;
        match self.client.volume(volume, device_id.as_deref()).await {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                let fresh = self.get_device_id().await;
                self.client
                    .volume(volume, fresh.as_deref())
                    .await
                    .map_err(Into::into)
            }
            other => other.map_err(Into::into),
        }
    }

    pub async fn seek_to(&mut self, progress_ms: u32) -> Result<()> {
        self.client
            .seek_track(chrono::Duration::milliseconds(i64::from(progress_ms)), None)
            .await?;
        Ok(())
    }

    /// Starts a playlist/album from the top — context playback with no track offset.
    pub async fn play_context(&mut self, context_id: &str, is_album: bool) -> Result<()> {
        let result = self.play_context_inner(context_id, is_album).await;
        match result {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                self.play_context_inner(context_id, is_album).await
            }
            other => other,
        }
    }

    async fn play_context_inner(&mut self, context_id: &str, is_album: bool) -> Result<()> {
        let target_device = self.require_device_id().await?;
        let context_uri = if is_album {
            rspotify::model::PlayContextId::Album(rspotify::model::AlbumId::from_id(context_id)?)
        } else {
            rspotify::model::PlayContextId::Playlist(rspotify::model::PlaylistId::from_id(
                context_id,
            )?)
        };
        self.client
            .start_context_playback(context_uri, Some(target_device.as_str()), None, None)
            .await?;
        Ok(())
    }

    pub async fn play_track(
        &mut self,
        context_id: &str,
        track_id: &str,
        is_album: bool,
    ) -> Result<()> {
        let result = self.play_track_inner(context_id, track_id, is_album).await;
        match result {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                self.play_track_inner(context_id, track_id, is_album).await
            }
            other => other,
        }
    }

    async fn play_track_inner(
        &mut self,
        context_id: &str,
        track_id: &str,
        is_album: bool,
    ) -> Result<()> {
        let target_device = self.require_device_id().await?;

        if context_id == "LIKED_SONGS" {
            let track_uri =
                rspotify::model::PlayableId::Track(rspotify::model::TrackId::from_id(track_id)?);
            let res = self
                .client
                .start_uris_playback([track_uri], Some(target_device.as_str()), None, None)
                .await;
            res?;
            return Ok(());
        }

        let context_uri = if is_album {
            rspotify::model::PlayContextId::Album(rspotify::model::AlbumId::from_id(context_id)?)
        } else {
            rspotify::model::PlayContextId::Playlist(rspotify::model::PlaylistId::from_id(
                context_id,
            )?)
        };

        let track_uri =
            rspotify::model::PlayableId::Track(rspotify::model::TrackId::from_id(track_id)?);
        let offset = rspotify::model::Offset::Uri(track_uri.uri());

        let res = self
            .client
            .start_context_playback(
                context_uri,
                Some(target_device.as_str()),
                Some(offset),
                None,
            )
            .await;

        if let Err(e) = &res {
            let _ = std::fs::write(
                crate::config::debug_log_path("echo-debug.log"),
                format!("Playback error: {:?}\n", e),
            );
        }

        res?;
        Ok(())
    }

    pub async fn play_uris(&mut self, track_ids: &[String], selected_index: usize) -> Result<()> {
        let result = self.play_uris_inner(track_ids, selected_index).await;
        match result {
            Err(ref e) if is_device_not_found(e) => {
                self.device_id = None;
                self.play_uris_inner(track_ids, selected_index).await
            }
            other => other,
        }
    }

    /// Plays an ad-hoc uris list (artist top tracks) starting at `selected_index`,
    /// so up-next is the remainder of the list.
    async fn play_uris_inner(&mut self, track_ids: &[String], selected_index: usize) -> Result<()> {
        let target_device = self.require_device_id().await?;

        let uris = track_ids
            .iter()
            .map(|id| {
                rspotify::model::TrackId::from_id(id.as_str())
                    .map(rspotify::model::PlayableId::Track)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let offset = uris
            .get(selected_index)
            .map(|uri| rspotify::model::Offset::Uri(uri.uri()));

        self.client
            .start_uris_playback(uris, Some(target_device.as_str()), offset, None)
            .await?;
        Ok(())
    }

    pub async fn get_track_metadata(
        &self,
        track_id: &str,
    ) -> anyhow::Result<(String, String, Option<String>)> {
        use rspotify::model::TrackId;
        let id = TrackId::from_id(track_id)?;
        let track = self.client.track(id, None).await?;

        let title = track.name;
        let artist = track
            .artists
            .into_iter()
            .map(|a| a.name)
            .collect::<Vec<_>>()
            .join(", ");
        let image_url = track.album.images.first().map(|img| img.url.clone());

        Ok((title, artist, image_url))
    }

    pub async fn add_to_queue(&self, track_ids: Vec<String>) -> anyhow::Result<()> {
        use rspotify::model::TrackId;
        use rspotify::prelude::OAuthClient;
        for track_id in track_ids {
            if let Ok(id) = TrackId::from_id(&track_id) {
                let _ = self
                    .client
                    .add_item_to_queue(id.into(), self.device_id.as_deref())
                    .await;
            }
        }
        Ok(())
    }

    pub async fn fetch_queue(&self) -> anyhow::Result<Vec<Track>> {
        let queue = match self.client.current_user_queue().await {
            Ok(q) => q,
            Err(e) => {
                let _ = std::fs::write(
                    crate::config::debug_log_path("echo-debug-queue.log"),
                    format!("fetch_queue error: {:?}", e),
                );
                return Err(e.into());
            }
        };
        let _ = std::fs::write(
            crate::config::debug_log_path("echo-debug-queue.log"),
            format!(
                "currently_playing: {:?}\nqueue length: {}\nfirst item type: {:?}",
                queue.currently_playing.as_ref().map(|i| match i {
                    rspotify::model::PlayableItem::Track(t) => format!("Track: {}", t.name),
                    rspotify::model::PlayableItem::Episode(e) => format!("Episode: {}", e.name),
                    _ => "Unknown".to_string(),
                }),
                queue.queue.len(),
                queue.queue.first().map(|i| match i {
                    rspotify::model::PlayableItem::Track(_) => "Track",
                    rspotify::model::PlayableItem::Episode(_) => "Episode",
                    _ => "Unknown",
                }),
            ),
        );
        let mut out = Vec::new();
        for item in queue.queue {
            match item {
                rspotify::model::PlayableItem::Track(track) => {
                    if track.is_local {
                        continue;
                    }
                    let artists = super::parse::track_artists(&track.artists);
                    out.push(Track {
                        id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                        source: TrackSource::Spotify,
                        local_path: None,
                        name: track.name,
                        artist: super::parse::joined_artist_names(&artists),
                        album: track.album.name,
                        added_at: None,
                        duration_ms: track.duration.num_milliseconds() as u32,
                        image_url: track.album.images.first().map(|img| img.url.clone()),
                        album_id: track.album.id.map(|id| id.id().to_string()),
                        artist_id: artists.first().and_then(|a| a.id.clone()),
                        artists,
                    });
                }
                rspotify::model::PlayableItem::Unknown(val) => {
                    // The queue endpoint returns simplified track objects that rspotify
                    // can't deserialize as FullTrack — extract from raw JSON.
                    let item_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "episode" {
                        continue;
                    }

                    let id = val
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if id.is_empty() {
                        continue;
                    }

                    let name = val
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown")
                        .to_string();
                    let artists = super::parse::track_artists_json(
                        val.get("artists").and_then(|a| a.as_array()),
                    );
                    let duration_ms =
                        val.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let image_url = val
                        .get("album")
                        .and_then(|a| a.get("images"))
                        .and_then(|imgs| imgs.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|img| img.get("url"))
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string());

                    let album_id = val
                        .get("album")
                        .and_then(|a| a.get("id"))
                        .and_then(|i| i.as_str())
                        .map(|s| s.to_string());
                    let album = val
                        .get("album")
                        .and_then(|a| a.get("name"))
                        .and_then(|name| name.as_str())
                        .unwrap_or_default()
                        .to_string();

                    out.push(Track {
                        id,
                        source: TrackSource::Spotify,
                        local_path: None,
                        name,
                        artist: super::parse::joined_artist_names(&artists),
                        album,
                        added_at: None,
                        duration_ms,
                        image_url,
                        album_id,
                        artist_id: artists.first().and_then(|a| a.id.clone()),
                        artists,
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub async fn fetch_devices(&self) -> anyhow::Result<Vec<crate::models::Device>> {
        let mut out = Vec::new();
        if let Ok(devices) = self.client.device().await {
            for d in devices {
                out.push(crate::models::Device {
                    id: d.id.unwrap_or_default(),
                    name: d.name,
                    is_active: d.is_active,
                    device_type: format!("{:?}", d._type),
                    volume_percent: d.volume_percent.unwrap_or_default(),
                });
            }
        }
        Ok(out)
    }

    pub async fn transfer_playback(&self, device_id: &str) -> anyhow::Result<()> {
        self.client.transfer_playback(device_id, Some(true)).await?;
        Ok(())
    }
}

/// True when the error is Spotify's "device not found / no active device" 404.
///
/// Matched on the formatted error because the callers hold a mix of `rspotify::ClientError`
/// and `anyhow::Error`. A bare `contains("404")` also fired on any URL, track id, or header
/// containing those digits, so both concrete renderings are matched instead: rspotify's
/// `Debug` prints `status: 404` and its `Display` prints `status code 404`.
fn is_device_not_found<E: std::fmt::Debug>(err: &E) -> bool {
    let rendered = format!("{err:?}");
    rendered.contains("status: 404") || rendered.contains("status code 404")
}
