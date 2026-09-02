use super::SpotifyWorker;
use crate::models::Track;

/// Keeps only tracks actually credited to the artist: a name search also surfaces
/// covers, tributes and same-named artists.
fn filter_tracks_by_artist(tracks: Vec<Track>, artist_id: &str) -> Vec<Track> {
    tracks
        .into_iter()
        .filter(|track| {
            track
                .artists
                .iter()
                .any(|artist| artist.id.as_deref() == Some(artist_id))
        })
        .collect()
}

impl SpotifyWorker {
    pub async fn search_catalog(
        &self,
        query: &str,
    ) -> anyhow::Result<crate::models::SearchResults> {
        use rspotify::model::{Id, SearchType};
        use rspotify::prelude::BaseClient;

        let mut results = crate::models::SearchResults::default();

        // Dev-mode Spotify rejects limit > 10 with a 400 and defaults to 5 when omitted.
        if let Ok(rspotify::model::SearchResult::Tracks(page)) = self
            .client
            .search(query, SearchType::Track, None, None, Some(10), None)
            .await
        {
            results.tracks = page
                .items
                .into_iter()
                .filter_map(|t| {
                    let id = t.id?.id().to_string();
                    let name = t.name;
                    let artists = t.artists;
                    let artist_id = artists
                        .first()
                        .and_then(|a| a.id.as_ref())
                        .map(|id| id.id().to_string());
                    let artist = artists
                        .into_iter()
                        .map(|a| a.name)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let album = t.album.name;
                    let duration_ms = t.duration.num_milliseconds() as u32;
                    let image_url = t.album.images.first().map(|i| i.url.clone());
                    let album_id = t.album.id.map(|id| id.id().to_string());
                    Some(crate::models::SearchTrack {
                        id,
                        source: crate::models::TrackSource::Spotify,
                        local_path: None,
                        name,
                        artist,
                        album,
                        duration_ms,
                        image_url,
                        album_id,
                        artist_id,
                    })
                })
                .collect();
        }

        if let Ok(rspotify::model::SearchResult::Albums(page)) = self
            .client
            .search(query, SearchType::Album, None, None, Some(10), None)
            .await
        {
            results.albums = page
                .items
                .into_iter()
                .filter_map(|a| {
                    let id = a.id?.id().to_string();
                    let name = a.name;
                    let artist = a
                        .artists
                        .into_iter()
                        .map(|a| a.name)
                        .collect::<Vec<_>>()
                        .join(", ");
                    let image_url = a.images.first().map(|i| i.url.clone());
                    Some(crate::models::SearchAlbum {
                        id,
                        name,
                        artist,
                        image_url,
                    })
                })
                .collect();
        }

        if let Ok(rspotify::model::SearchResult::Artists(page)) = self
            .client
            .search(query, SearchType::Artist, None, None, Some(10), None)
            .await
        {
            results.artists = page
                .items
                .into_iter()
                .map(|a| crate::models::Artist {
                    id: a.id.id().to_string(),
                    name: a.name,
                    image_url: a.images.first().map(|i| i.url.clone()),
                })
                .collect();
        }

        // Playlist payloads are the flakiest search bucket (null items are a known
        // Spotify quirk); the `if let Ok` keeps a failure here from touching the
        // other tabs.
        if let Ok(rspotify::model::SearchResult::Playlists(page)) = self
            .client
            .search(query, SearchType::Playlist, None, None, Some(10), None)
            .await
        {
            results.playlists = page
                .items
                .into_iter()
                .map(|p| {
                    let owner_id = p.owner.id.id().to_string();
                    crate::models::Playlist {
                        id: p.id.id().to_string(),
                        name: p.name,
                        owner: p.owner.display_name.unwrap_or_else(|| owner_id.clone()),
                        owner_id,
                        image_url: p.images.first().map(|i| i.url.clone()),
                        thumb_url: p.images.last().map(|i| i.url.clone()),
                    }
                })
                .collect();
        }

        Ok(results)
    }

    /// Approximates an artist's top tracks with a track search: results come back
    /// roughly popularity-ordered, and the dev-mode API no longer offers the real
    /// `/artists/{id}/top-tracks` endpoint.
    pub async fn search_artist_top_tracks(
        &self,
        artist_id: &str,
        artist_name: &str,
    ) -> anyhow::Result<Vec<Track>> {
        use rspotify::model::SearchType;
        use rspotify::prelude::BaseClient;

        let query = format!("artist:\"{artist_name}\"");
        let result = self
            .client
            .search(&query, SearchType::Track, None, None, Some(10), None)
            .await?;
        let rspotify::model::SearchResult::Tracks(page) = result else {
            anyhow::bail!("Unexpected search result shape for artist top tracks");
        };
        let tracks = page
            .items
            .into_iter()
            .filter_map(super::parse::track_from_full)
            .collect();
        Ok(filter_tracks_by_artist(tracks, artist_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackArtist;

    fn track_credited_to(ids: &[Option<&str>]) -> Track {
        Track {
            id: "track".to_string(),
            source: crate::models::TrackSource::Spotify,
            local_path: None,
            name: "Track".to_string(),
            artist: String::new(),
            album: String::new(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: ids
                .iter()
                .map(|id| TrackArtist {
                    id: id.map(str::to_string),
                    name: "A".to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn artist_filter_keeps_only_credited_tracks() {
        let tracks = vec![
            track_credited_to(&[Some("wanted")]),
            track_credited_to(&[Some("other"), Some("wanted")]),
            track_credited_to(&[Some("other")]),
            track_credited_to(&[None]),
            track_credited_to(&[]),
        ];

        let filtered = filter_tracks_by_artist(tracks, "wanted");

        assert_eq!(filtered.len(), 2);
    }
}
