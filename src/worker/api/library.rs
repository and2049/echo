use crate::models::{Playlist, Track};
use anyhow::Result;
use rspotify::prelude::*;
use rspotify::model::Id;
use super::SpotifyWorker;

impl SpotifyWorker {
    pub async fn fetch_playlists(&self) -> Result<Vec<Playlist>> {
        let page = match self
            .client
            .current_user_playlists_manual(None, None)
            .await {
            Ok(p) => p,
            Err(e) => {
                let _ = std::fs::write("echo-debug-user-playlists.log", format!("User playlists fetch error: {:?}", e));
                return Err(e.into());
            }
        };
        let mut out = Vec::new();
        for p in page.items {
            let owner = p
                .owner
                .display_name
                .clone()
                .unwrap_or_else(|| p.owner.id.id().to_string());
            let owner_id = p.owner.id.id().to_string();
            out.push(Playlist {
                id: p.id.id().to_string(),
                name: p.name,
                owner,
                owner_id,
                image_url: p.images.first().map(|i| i.url.clone()),
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
                    artists: album.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
                    image_url: album.images.first().map(|i| i.url.clone()),
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

                if out.len() >= 100 {
                    break;
                }
            }
            return Ok(out);
        }

        let id = rspotify::model::PlaylistId::from_id(playlist_id)?;
        
        // Debug raw request
        if let Ok(token_mutex) = self.client.get_token().lock().await
            && let Some(token) = token_mutex.as_ref()
        {
            let raw_url = format!("https://api.spotify.com/v1/playlists/{}/items", id.id());
            let client = reqwest::Client::new();
            if let Ok(res) = client.get(&raw_url).bearer_auth(&token.access_token).send().await {
                let status = res.status();
                if let Ok(body) = res.text().await {
                    let _ = std::fs::write("echo-debug-playlist.log", format!("RAW REQUEST RESPONSE ({}): {}", status, body));
                }
            }
        }

        let page = match self
            .client
            .playlist_items_manual(id, None, None, None, None)
            .await {
            Ok(p) => p,
            Err(e) => {
                // The raw request above already wrote the body, so we can just return
                return Err(e.into());
            }
        };
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
        let page = self.client.album_track_manual(id, None, None, None).await?;
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
}
