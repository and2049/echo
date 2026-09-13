use super::SpotifyWorker;
use crate::models::{Playlist, Track, TrackSource};
use anyhow::Result;
use rspotify::model::Id;
use rspotify::prelude::*;

/// rspotify's wire enum for our persisted range preference, kept out of the core models.
fn time_range(range: crate::models::TopItemsRange) -> rspotify::model::TimeRange {
    match range {
        crate::models::TopItemsRange::Short => rspotify::model::TimeRange::ShortTerm,
        crate::models::TopItemsRange::Medium => rspotify::model::TimeRange::MediumTerm,
        crate::models::TopItemsRange::Long => rspotify::model::TimeRange::LongTerm,
    }
}

impl SpotifyWorker {
    pub async fn fetch_playlists(&self) -> Result<Vec<Playlist>> {
        let api = super::client::EchoSpotifyClient::new(self.client.clone(), None);
        playlist_pages(|offset| {
            let api = &api;
            async move {
                api.third_party_json(&format!(
                    "https://api.spotify.com/v1/me/playlists?limit=50&offset={offset}"
                ))
                .await
            }
        })
        .await
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
                        artists: album
                            .artists
                            .into_iter()
                            .map(|a| a.name)
                            .collect::<Vec<_>>()
                            .join(", "),
                        image_url: album.images.first().map(|i| i.url.clone()),
                        thumb_url: album.images.last().map(|i| i.url.clone()),
                        release_year: album.release_date.chars().take(4).collect(),
                        release_date: (!album.release_date.is_empty())
                            .then(|| album.release_date.clone()),
                        track_count: None,
                    });
                }
                Err(e) => {
                    let _ = std::fs::write(
                        crate::config::debug_log_path("echo-debug-albums.log"),
                        format!("Albums fetch error: {:?}", e),
                    );
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
                    let added_at = Some(saved_track.added_at.to_rfc3339());
                    let track = saved_track.track;
                    if track.is_local {
                        continue;
                    }
                    let artists = super::parse::track_artists(&track.artists);
                    out.push(Track {
                        explicit: track.explicit,
                        added_by: None,
                        id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                        source: TrackSource::Spotify,
                        local_path: None,
                        name: track.name,
                        artist: super::parse::joined_artist_names(&artists),
                        album: track.album.name,
                        added_at,
                        duration_ms: track.duration.num_milliseconds() as u32,
                        image_url: track.album.images.first().map(|img| img.url.clone()),
                        album_id: track.album.id.map(|id| id.id().to_string()),
                        artist_id: artists.first().and_then(|a| a.id.clone()),
                        artists,
                    });
                }

                if out.len() >= 100 {
                    break;
                }
            }
            return Ok(out);
        }

        let id = rspotify::model::PlaylistId::from_id(playlist_id)?;

        let mut out = Vec::new();
        let mut offset = 0;
        loop {
            let page = self
                .client
                .playlist_items_manual(id.clone(), None, None, Some(50), Some(offset))
                .await?;
            let has_next = page.next.is_some();
            for item in page.items {
                let added_at = item.added_at.map(|value| value.to_rfc3339());
                if let Some(rspotify::model::PlayableItem::Track(track)) = item.item {
                    let artists = super::parse::track_artists(&track.artists);
                    out.push(Track {
                        explicit: track.explicit,
                        added_by: item.added_by.map(|user| user.id.id().to_string()),
                        id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                        source: TrackSource::Spotify,
                        local_path: None,
                        name: track.name,
                        artist: super::parse::joined_artist_names(&artists),
                        album: track.album.name,
                        added_at,
                        duration_ms: track.duration.num_milliseconds() as u32,
                        image_url: track.album.images.first().map(|img| img.url.clone()),
                        album_id: track.album.id.map(|id| id.id().to_string()),
                        artist_id: artists.first().and_then(|a| a.id.clone()),
                        artists,
                    });
                }
            }
            if !has_next {
                break;
            }
            offset += 50;
        }
        Ok(out)
    }

    pub async fn fetch_album_tracks(
        &self,
        album_id: &str,
    ) -> Result<(Vec<Track>, Option<(String, String, String, String)>)> {
        let id = rspotify::model::AlbumId::from_id(album_id)?;
        let album = self.client.album(id, None).await?;
        let mut out = Vec::new();

        let image_url = album.images.first().map(|i| i.url.clone());
        let metadata = Some((
            album.id.id().to_string(),
            album.name.clone(),
            album
                .artists
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
                .join(", "),
            image_url.clone().unwrap_or_default(),
        ));

        for track in album.tracks.items {
            if track.is_local {
                continue;
            }
            let artists = super::parse::track_artists(&track.artists);
            out.push(Track {
                explicit: track.explicit,
                added_by: None,
                id: track.id.map(|i| i.id().to_string()).unwrap_or_default(),
                source: TrackSource::Spotify,
                local_path: None,
                name: track.name,
                artist: super::parse::joined_artist_names(&artists),
                album: album.name.clone(),
                added_at: None,
                duration_ms: track.duration.num_milliseconds() as u32,
                image_url: image_url.clone(), // Set the album's image on every track!
                album_id: Some(album_id.to_string()),
                artist_id: artists.first().and_then(|a| a.id.clone()),
                artists,
            });
        }
        Ok((out, metadata))
    }

    /// Every page of the user's top tracks (hundreds, at 50 a request). The first page
    /// goes to `on_first_page` as soon as it lands so a view can show it while the walk
    /// continues; a list that fits in one page skips the callback.
    pub async fn fetch_top_tracks(
        &self,
        range: crate::models::TopItemsRange,
        on_first_page: impl FnMut(&[Track]),
    ) -> Result<Vec<Track>> {
        top_track_pages(
            |offset| async move {
                let page = self
                    .client
                    .current_user_top_tracks_manual(Some(time_range(range)), Some(50), Some(offset))
                    .await?;
                let has_next = page.next.is_some();
                let tracks = page
                    .items
                    .into_iter()
                    .filter_map(super::parse::track_from_full)
                    .collect();
                Ok((tracks, has_next))
            },
            on_first_page,
        )
        .await
    }

    pub async fn fetch_top_artists(
        &self,
        range: crate::models::TopItemsRange,
    ) -> Result<Vec<crate::models::Artist>> {
        use rspotify::prelude::OAuthClient;
        // One bounded request rather than the paginator: 50 top artists is the ceiling
        // anyone scrolls, and it keeps this to a single call.
        let page = self
            .client
            .current_user_top_artists_manual(Some(time_range(range)), Some(50), Some(0))
            .await?;
        Ok(page
            .items
            .into_iter()
            .map(|artist| crate::models::Artist {
                id: artist.id.id().to_string(),
                name: artist.name,
                image_url: artist.images.first().map(|img| img.url.clone()),
            })
            .collect())
    }

    pub async fn fetch_recently_played(&self) -> Result<crate::home::RecentHistory> {
        let api = super::client::EchoSpotifyClient::new(self.client.clone(), None);
        let json = api
            .third_party_json("https://api.spotify.com/v1/me/player/recently-played?limit=50")
            .await?;
        Ok(super::parse::recent_history(&json))
    }

    pub async fn fetch_followed_artists(&self) -> Result<Vec<crate::models::Artist>> {
        let page = self
            .client
            .current_user_followed_artists(None, Some(50))
            .await?;
        Ok(page
            .items
            .into_iter()
            .map(|artist| crate::models::Artist {
                id: artist.id.id().to_string(),
                name: artist.name,
                image_url: artist.images.first().map(|img| img.url.clone()),
            })
            .collect())
    }
}

async fn top_track_pages<F, Fut>(
    mut fetch: F,
    mut on_first_page: impl FnMut(&[Track]),
) -> Result<Vec<Track>>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<(Vec<Track>, bool)>>,
{
    let mut tracks = Vec::new();
    let mut offset = 0;
    loop {
        let (page, has_next) = fetch(offset).await?;
        tracks.extend(page);
        if !has_next {
            return Ok(tracks);
        }
        if offset == 0 {
            on_first_page(&tracks);
        }
        offset += 50;
    }
}

async fn playlist_pages<F, Fut>(mut fetch: F) -> Result<Vec<Playlist>>
where
    F: FnMut(u32) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value>>,
{
    let mut playlists = Vec::new();
    let mut offset = 0;
    loop {
        let page = fetch(offset).await?;
        playlists.extend(
            page.get("items")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(super::parse::playlist),
        );
        if page.get("next").is_none_or(|next| next.is_null()) {
            break;
        }
        offset += 50;
    }
    Ok(playlists)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u32) -> Track {
        serde_json::from_value(serde_json::json!({"id": id.to_string(), "name": "Song", "artist": "Artist", "album": "Album", "duration_ms": 1000, "artists": []})).unwrap()
    }

    #[tokio::test]
    async fn top_track_pages_hand_over_the_first_page_then_keep_walking() {
        let mut first = Vec::new();
        let tracks = top_track_pages(
            |offset| async move {
                let ids = (offset..offset + if offset < 100 { 50 } else { 3 }).map(track);
                Ok((ids.collect(), offset < 100))
            },
            |page| first.push(page.len()),
        )
        .await
        .unwrap();
        assert_eq!(first, [50]);
        assert_eq!(tracks.len(), 103);
        assert_eq!(tracks[100].id, "100");
    }

    #[tokio::test]
    async fn a_single_page_of_top_tracks_skips_the_early_hand_over() {
        let mut calls = 0;
        let tracks = top_track_pages(
            |_| async move { Ok((vec![track(1)], false)) },
            |_| calls += 1,
        )
        .await
        .unwrap();
        assert_eq!((calls, tracks.len()), (0, 1));
    }

    #[tokio::test]
    async fn playlist_pagination_uses_fifty_item_offsets_until_null_next() {
        let mut offsets = Vec::new();
        let playlists = playlist_pages(|offset| {
            offsets.push(offset);
            async move { Ok(serde_json::json!({"items":[null, {"id":offset.to_string(),"name":"Playlist","owner":{"id":"owner"}}], "next": if offset < 100 { Some("next") } else { None }})) }
        }).await.unwrap();
        assert_eq!(offsets, [0, 50, 100]);
        assert_eq!(
            playlists.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["0", "50", "100"]
        );
    }

    #[tokio::test]
    async fn playlist_pagination_propagates_rate_limits_without_retrying() {
        let mut offsets = Vec::new();
        let result = playlist_pages(|offset| {
            offsets.push(offset);
            async move {
                if offset > 0 {
                    anyhow::bail!("429 Too Many Requests");
                }
                Ok(serde_json::json!({"items":[], "next":"next"}))
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(offsets, [0, 50]);
    }
}
