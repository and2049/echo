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
            match item {
                Ok(saved_album) => {
                    let album = saved_album.album;
                    out.push(crate::models::Album {
                        id: album.id.id().to_string(),
                        name: album.name,
                        artists: album.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
                        image_url: album.images.first().map(|i| i.url.clone()),
                    });
                }
                Err(e) => {
                    let _ = std::fs::write("echo-debug-albums.log", format!("Albums fetch error: {:?}", e));
                    return Err(e.into());
                }
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
                        album_id: track.album.id.map(|id| id.id().to_string()),
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
                    album_id: track.album.id.map(|id| id.id().to_string()),
                });
            }
        }
        Ok(out)
    }

    pub async fn fetch_album_tracks(&self, album_id: &str) -> Result<(Vec<Track>, Option<(String, String, String, String)>)> {
        let id = rspotify::model::AlbumId::from_id(album_id)?;
        let album = self.client.album(id, None).await?;
        let mut out = Vec::new();
        
        let image_url = album.images.first().map(|i| i.url.clone());
        let metadata = Some((
            album.id.id().to_string(),
            album.name,
            album.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
            image_url.clone().unwrap_or_default(),
        ));

        for track in album.tracks.items {
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
                image_url: image_url.clone(), // Set the album's image on every track!
                album_id: Some(album_id.to_string()),
            });
        }
        Ok((out, metadata))
    }

    pub async fn fetch_top_tracks(&self) -> Result<Vec<Track>> {
        use futures_util::StreamExt;
        let mut stream = Box::pin(self.client.current_user_top_tracks(None));
        let mut out = Vec::new();
        while let Some(item) = stream.next().await {
            if let Ok(track) = item {
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
        }
        Ok(out)
    }

    pub async fn fetch_recently_played(&self) -> Result<Vec<Track>> {
        // rspotify's deserialization fails on missing external_ids for recently played tracks
        // bypass using reqwest directly.
        let token_mutex = self.client.get_token();
        let token_guard = token_mutex.lock().await.unwrap();
        let access_token = if let Some(t) = token_guard.as_ref() {
            t.access_token.clone()
        } else {
            return Ok(vec![]);
        };

        let client = reqwest::Client::new();
        let res = client.get("https://api.spotify.com/v1/me/player/recently-played?limit=50")
            .header("Authorization", format!("Bearer {}", access_token))
            .send().await?;

        let json: serde_json::Value = res.json().await?;
        let mut tracks = Vec::new();
        
        if let Some(items) = json.get("items").and_then(|i| i.as_array()) {
            for history in items {
                if let Some(track) = history.get("track") {
                    let name = track.get("name").and_then(|n| n.as_str()).unwrap_or_default().to_string();
                    let is_local = track.get("is_local").and_then(|l| l.as_bool()).unwrap_or(false);
                    if is_local { continue; }
                    
                    // Deduplicate
                    if !tracks.iter().any(|t: &Track| t.name == name) {
                        let id = track.get("id").and_then(|i| i.as_str()).unwrap_or_default().to_string();
                        let duration_ms = track.get("duration_ms").and_then(|d| d.as_u64()).unwrap_or_default() as u32;
                        
                        let mut artist_names = Vec::new();
                        if let Some(artists) = track.get("artists").and_then(|a| a.as_array()) {
                            for a in artists {
                                if let Some(aname) = a.get("name").and_then(|n| n.as_str()) {
                                    artist_names.push(aname.to_string());
                                }
                            }
                        }
                        
                        let album = track.get("album");
                        let album_id = album.and_then(|a| a.get("id")).and_then(|id| id.as_str()).map(|s| s.to_string());
                        let image_url = album.and_then(|a| a.get("images"))
                                             .and_then(|imgs| imgs.as_array())
                                             .and_then(|imgs| imgs.first())
                                             .and_then(|img| img.get("url"))
                                             .and_then(|url| url.as_str())
                                             .map(|s| s.to_string());
                                             
                        tracks.push(Track {
                            id,
                            name,
                            artist: artist_names.join(", "),
                            duration_ms,
                            image_url,
                            album_id,
                        });
                    }
                }
            }
        }
        Ok(tracks)
    }

    pub async fn fetch_followed_artists(&self) -> Result<Vec<crate::models::Artist>> {
        let first_page = self.client.current_user_followed_artists(None, None).await?;
        let mut artists = first_page.items;
        let mut maybe_next = first_page.next;
        
        while let Some(url) = maybe_next {
            // need to make a raw request or use rspotify's internal cursor handling if available.
            // return the first page (50) for now
            // use reqwest directly if they have more than 50.
            break;
        }

        let mut out = Vec::new();
        for a in artists {
            out.push(crate::models::Artist {
                id: a.id.id().to_string(),
                name: a.name,
                followers: a.followers.total,
                image_url: a.images.first().map(|img| img.url.clone()),
            });
        }
        Ok(out)
    }
}
