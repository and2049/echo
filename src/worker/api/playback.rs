use super::SpotifyWorker;
use crate::models::{PlaybackItem, Track};
use anyhow::Result;
use rspotify::prelude::*;
use rspotify::model::Id;
use rspotify::AuthCodeSpotify;

const PLAYBACK_TYPES: [&rspotify::model::AdditionalType; 2] = [
    &rspotify::model::AdditionalType::Track,
    &rspotify::model::AdditionalType::Episode,
];

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
            title,
            artist,
            duration_ms,
            image_url,
            album_id: value.get("album").and_then(|a| a.get("id")).and_then(|i| i.as_str()).map(str::to_string),
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
                })
            }
            rspotify::model::PlayableItem::Episode(episode) => Some(PlaybackItem {
                id: episode.id.id().to_string(),
                title: episode.name.clone(),
                artist: episode.show.name.clone(),
                duration_ms: episode.duration.num_milliseconds() as u32,
                image_url: episode.images.first().map(|img| img.url.clone()),
                album_id: None,
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

            return Ok(Some((
                is_playing,
                is_shuffled,
                repeat_mode,
                volume,
                device_name,
                progress_ms,
                item,
            )));
        }

        Ok(None)
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
            if self.device_id.is_none()
                && device.name == "echo-rs" {
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
                false, // Default shuffle
                "Off".to_string(), // Default repeat
                None,
                "Unknown Device".to_string(),
                progress_ms,
                item,
            )));
        }

        Ok(None)
    }

    pub async fn toggle_playback(&mut self, is_playing: bool) -> Result<()> {
        let device = self.get_device_id().await;
        if is_playing {
            self.client.resume_playback(device.as_deref(), None).await?;
        } else {
            self.client.pause_playback(device.as_deref()).await?;
        }
        Ok(())
    }

    pub async fn next_track(&mut self) -> Result<()> {
        let device = self.get_device_id().await;
        self.client.next_track(device.as_deref()).await?;
        Ok(())
    }

    pub async fn previous_track(&mut self) -> Result<()> {
        let device = self.get_device_id().await;
        self.client.previous_track(device.as_deref()).await?;
        Ok(())
    }

    pub async fn toggle_shuffle(&mut self, is_shuffled: bool) -> Result<()> {
        let device = self.get_device_id().await;
        self.client.shuffle(is_shuffled, device.as_deref()).await?;
        Ok(())
    }

    pub async fn set_repeat_mode(&mut self, state: rspotify::model::RepeatState) -> Result<()> {
        let device = self.get_device_id().await;
        self.client.repeat(state, device.as_deref()).await?;
        Ok(())
    }

    pub async fn set_volume(&mut self, volume: u8) -> Result<()> {
        let device = self.get_device_id().await;
        self.client.volume(volume, device.as_deref()).await?;
        Ok(())
    }


    pub async fn play_track(
        &mut self,
        context_id: &str,
        track_id: &str,
        is_album: bool,
    ) -> Result<()> {
        let target_device = self.get_device_id().await;

        if context_id == "LIKED_SONGS" {
            let track_uri =
                rspotify::model::PlayableId::Track(rspotify::model::TrackId::from_id(track_id)?);
            let res = self
                .client
                .start_uris_playback([track_uri], target_device.as_deref(), None, None)
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
            .start_context_playback(context_uri, target_device.as_deref(), Some(offset), None)
            .await;

        if let Err(e) = &res {
            let _ = std::fs::write("echo-debug.log", format!("Playback error: {:?}\n", e));
        }

        res?;
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
                let _ = self.client.add_item_to_queue(id.into(), self.device_id.as_deref()).await;
            }
        }
        Ok(())
    }

    pub async fn fetch_queue(&self) -> anyhow::Result<Vec<Track>> {
        let queue = match self.client.current_user_queue().await {
            Ok(q) => q,
            Err(e) => {
                let _ = std::fs::write("echo-debug-queue.log", format!("fetch_queue error: {:?}", e));
                return Err(e.into());
            }
        };
        let _ = std::fs::write("echo-debug-queue.log", format!(
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
        ));
        let mut out = Vec::new();
        for item in queue.queue {
            match item {
                rspotify::model::PlayableItem::Track(track) => {
                    if track.is_local { continue; }
                    out.push(Track {
                        id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                        name: track.name,
                        artist: track.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
                        duration_ms: track.duration.num_milliseconds() as u32,
                        image_url: track.album.images.first().map(|img| img.url.clone()),
                        album_id: track.album.id.map(|id| id.id().to_string()),
                    });
                }
                rspotify::model::PlayableItem::Unknown(val) => {
                    // The queue endpoint returns simplified track objects that rspotify
                    // can't deserialize as FullTrack — extract from raw JSON.
                    let item_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if item_type == "episode" { continue; }

                    let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if id.is_empty() { continue; }

                    let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string();
                    let artist = val.get("artists")
                        .and_then(|a| a.as_array())
                        .map(|arr| arr.iter()
                            .filter_map(|a| a.get("name").and_then(|n| n.as_str()))
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default();
                    let duration_ms = val.get("duration_ms")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    let image_url = val.get("album")
                        .and_then(|a| a.get("images"))
                        .and_then(|imgs| imgs.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|img| img.get("url"))
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string());

                    let album_id = val.get("album")
                        .and_then(|a| a.get("id"))
                        .and_then(|i| i.as_str())
                        .map(|s| s.to_string());

                    out.push(Track { id, name, artist, duration_ms, image_url, album_id });
                }
                _ => {}
            }
        }
        Ok(out)
    }

}
