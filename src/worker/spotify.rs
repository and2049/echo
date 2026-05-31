use crate::config::AppConfig;
use crate::models::{PlaybackItem, Playlist, Track};
use anyhow::Result;
use rspotify::{AuthCodeSpotify, Credentials, OAuth, prelude::*};
use std::collections::HashSet;

const PLAYBACK_TYPES: [&rspotify::model::AdditionalType; 2] = [
    &rspotify::model::AdditionalType::Track,
    &rspotify::model::AdditionalType::Episode,
];

pub struct SpotifyWorker {
    pub client: AuthCodeSpotify,
    pub device_id: Option<String>,
}

impl SpotifyWorker {
    pub async fn new(config: &AppConfig) -> Result<Self> {
        let creds = config.spotify_credentials.as_ref().unwrap();
        let credentials = Credentials::new(&creds.client_id, &creds.client_secret);

        let scopes: HashSet<String> = [
            "user-read-private",
            "playlist-read-private",
            "playlist-read-collaborative",
            "user-library-read",
            "user-modify-playback-state",
            "user-read-playback-state",
            "streaming",
            "app-remote-control",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let oauth = OAuth {
            redirect_uri: "http://127.0.0.1:8888/callback".to_string(),
            scopes: scopes.clone(),
            ..Default::default()
        };

        let spotify = AuthCodeSpotify::new(credentials, oauth);

        if let Some(tokens) = &config.auth_tokens {
            if let Some(access) = &tokens.access_token {
                if let Some(refresh) = &tokens.refresh_token {
                    use chrono::Utc;
                    use rspotify::model::Token;

                    let token = Token {
                        access_token: access.clone(),
                        refresh_token: Some(refresh.clone()),
                        expires_in: chrono::Duration::seconds(0),
                        expires_at: Some(Utc::now()),
                        scopes: scopes.clone(),
                    };

                    *spotify.get_token().lock().await.unwrap() = Some(token);
                    return Ok(Self {
                        client: spotify,
                        device_id: None,
                    });
                }
            }
        }

        // AuthCodeSpotify standard challenge
        let auth_url = spotify.get_authorize_url(false)?;

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:8888").await?;

        // Open the browser to the REAL Spotify auth URL
        if let Err(_e) = webbrowser::open(&auth_url) {
            // fallback if webbrowser fails
        }

        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = [0; 1024];
            if let Ok(bytes_read) = socket.read(&mut buffer).await {
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                if let Some(code_start) = request.find("code=") {
                    let code_rest = &request[code_start + 5..];
                    let code = code_rest
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .split('&')
                        .next()
                        .unwrap_or("");
                    let code_str = code.to_string();

                    let body = "<!DOCTYPE html><html><head><title>Success</title></head><body style=\"background-color: #121212; color: #ffffff; font-family: sans-serif; text-align: center; margin-top: 20%;\"><h1>Success, return to echo app</h1><p>You can safely close this tab.</p><script>setTimeout(() => window.close(), 3000);</script></body></html>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );

                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.flush().await;
                    drop(socket); // explicitly close the socket so the browser finishes loading

                    spotify.request_token(&code_str).await?;
                }
            }
        }

        // Write the fetched tokens to our config.toml
        let token_mutex = spotify.get_token();
        let token_guard = token_mutex.lock().await;
        if let Some(t) = token_guard.unwrap().as_ref() {
            let mut app_config = AppConfig::load();
            app_config.auth_tokens = Some(crate::config::AuthTokens {
                access_token: Some(t.access_token.clone()),
                refresh_token: t.refresh_token.clone(),
            });
            let _ = app_config.save();
        }

        Ok(Self {
            client: spotify,
            device_id: None,
        })
    }

    pub async fn get_device_id(&mut self) -> Option<String> {
        if self.device_id.is_some() {
            return self.device_id.clone();
        }

        if let Ok(devices) = self.client.device().await {
            for d in devices {
                if d.name == "Echo TUI" {
                    self.device_id = d.id.clone();
                    return self.device_id.clone();
                }
            }
        }
        None
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
        })
    }

    pub fn playback_item_from_playable(item: &rspotify::model::PlayableItem) -> Option<PlaybackItem> {
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
                })
            }
            rspotify::model::PlayableItem::Episode(episode) => Some(PlaybackItem {
                id: episode.id.id().to_string(),
                title: episode.name.clone(),
                artist: episode.show.name.clone(),
                duration_ms: episode.duration.num_milliseconds() as u32,
                image_url: episode.images.first().map(|img| img.url.clone()),
            }),
            rspotify::model::PlayableItem::Unknown(value) => Self::playback_item_from_unknown(value),
        }
    }

    pub async fn playback_snapshot_from_client(
        client: &AuthCodeSpotify,
    ) -> Result<Option<(bool, bool, u32, Option<PlaybackItem>)>> {
        if let Some(playback) = client.current_playback(None, Some(PLAYBACK_TYPES)).await? {
            let is_playing = playback.is_playing;
            let is_shuffled = playback.shuffle_state;
            let progress_ms = playback.progress.unwrap_or_default().num_milliseconds() as u32;
            let item = playback
                .item
                .as_ref()
                .and_then(Self::playback_item_from_playable);

            return Ok(Some((is_playing, is_shuffled, progress_ms, item)));
        }

        Ok(None)
    }

    pub async fn sync_playback_state(
        &mut self,
    ) -> Result<Option<(bool, bool, u32, Option<PlaybackItem>)>> {
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

            // Auto-cache the device ID if we found an active playback
            if self.device_id.is_none() {
                let device = &playback.device;
                if device.name == "Echo TUI" {
                    self.device_id = device.id.clone();
                }
            }

            return Ok(Some((
                is_playing,
                is_shuffled,
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

    pub async fn fetch_playlists(&self) -> Result<Vec<Playlist>> {
        let page = self
            .client
            .current_user_playlists_manual(None, None)
            .await?;
        let mut out = Vec::new();
        for p in page.items {
            let owner = p.owner.display_name.clone().unwrap_or_else(|| p.owner.id.id().to_string());
            out.push(Playlist {
                id: p.id.id().to_string(),
                name: p.name,
                owner,
            });
        }
        Ok(out)
    }

    pub async fn fetch_albums(&self) -> Result<Vec<crate::models::Album>> {
        use futures_util::StreamExt;
        let stream = self.client.current_user_saved_albums(None);
        let mut out = Vec::new();
        
        let mut stream = Box::pin(stream);
        while let Some(item) = stream.next().await {
            if let Ok(saved_album) = item {
                let album = saved_album.album;
                out.push(crate::models::Album {
                    id: album.id.id().to_string(),
                    name: album.name,
                    artist: album.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
                });
            }
            if out.len() >= 100 {
                break;
            }
        }
        Ok(out)
    }

    pub async fn fetch_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        if playlist_id == "LIKED_SONGS" {
            use futures_util::StreamExt;
            let stream = self.client.current_user_saved_tracks(None);
            let mut out = Vec::new();
            
            let mut stream = Box::pin(stream);
            while let Some(item) = stream.next().await {
                if let Ok(saved_track) = item {
                    let track = saved_track.track;
                    if track.is_local {
                        continue;
                    }
                    out.push(Track {
                        id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                        name: track.name,
                        artist: track.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
                        duration_ms: track.duration.num_milliseconds() as u32,
                        image_url: track.album.images.first().map(|img| img.url.clone()),
                    });
                }
                
                if out.len() >= 100 {
                    break;
                }
            }
            return Ok(out);
        }

        let id = rspotify::model::PlaylistId::from_id(playlist_id)?;
        let page = self
            .client
            .playlist_items_manual(id, None, None, None, None)
            .await?;
        let mut out = Vec::new();
        for item in page.items {
            if let Some(rspotify::model::PlayableItem::Track(track)) = item.item {
                out.push(Track {
                    id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                    name: track.name,
                    artist: track
                        .artists
                        .into_iter()
                        .map(|a| a.name)
                        .collect::<Vec<_>>()
                        .join(", "),
                    duration_ms: track.duration.num_milliseconds() as u32,
                    image_url: track.album.images.first().map(|img| img.url.clone()),
                });
            }
        }
        Ok(out)
    }

    pub async fn fetch_album_tracks(&self, album_id: &str) -> Result<Vec<Track>> {
        let id = rspotify::model::AlbumId::from_id(album_id)?;
        let page = self
            .client
            .album_track_manual(id, None, None, None)
            .await?;
        let mut out = Vec::new();
        for track in page.items {
            if track.is_local {
                continue;
            }
            out.push(Track {
                id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                name: track.name,
                artist: track
                    .artists
                    .into_iter()
                    .map(|a| a.name)
                    .collect::<Vec<_>>()
                    .join(", "),
                duration_ms: track.duration.num_milliseconds() as u32,
                image_url: None, // Simplified tracks don't have images directly, normally it inherits from the album, we can omit it or pass it down later
            });
        }
        Ok(out)
    }

    pub async fn play_track(&mut self, context_id: &str, track_id: &str, is_album: bool) -> Result<()> {
        let target_device = self.get_device_id().await;

        if context_id == "LIKED_SONGS" {
            let track_uri = rspotify::model::PlayableId::Track(rspotify::model::TrackId::from_id(track_id)?);
            let res = self
                .client
                .start_uris_playback(
                    [track_uri],
                    target_device.as_deref(),
                    None,
                    None,
                )
                .await;
            res?;
            return Ok(());
        }

        let context_uri = if is_album {
            rspotify::model::PlayContextId::Album(
                rspotify::model::AlbumId::from_id(context_id)?,
            )
        } else {
            rspotify::model::PlayContextId::Playlist(
                rspotify::model::PlaylistId::from_id(context_id)?,
            )
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
}
