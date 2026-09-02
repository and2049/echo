use crate::models::{Album, Artist, Track, TrackArtist, TrackSource};

/// Structured artist credits from a raw API `artists` array.
pub(crate) fn track_artists_json(artists: Option<&Vec<serde_json::Value>>) -> Vec<TrackArtist> {
    artists
        .map(|artists| {
            artists
                .iter()
                .filter_map(|artist| {
                    Some(TrackArtist {
                        id: artist
                            .get("id")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        name: artist.get("name")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Structured artist credits from rspotify's artist list.
pub(crate) fn track_artists(artists: &[rspotify::model::SimplifiedArtist]) -> Vec<TrackArtist> {
    use rspotify::prelude::Id;
    artists
        .iter()
        .map(|artist| TrackArtist {
            id: artist.id.as_ref().map(|id| id.id().to_string()),
            name: artist.name.clone(),
        })
        .collect()
}

/// The joined display string for a track's artists.
pub(crate) fn joined_artist_names(artists: &[TrackArtist]) -> String {
    artists
        .iter()
        .map(|artist| artist.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn track(track: &serde_json::Value) -> Option<Track> {
    if track
        .get("is_local")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return None;
    }
    let id = track.get("id")?.as_str()?.to_string();
    let album = track.get("album");
    let artists = track_artists_json(track.get("artists").and_then(|v| v.as_array()));
    Some(Track {
        id,
        source: TrackSource::Spotify,
        local_path: None,
        name: track.get("name")?.as_str()?.to_string(),
        artist: joined_artist_names(&artists),
        album: album
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        added_at: None,
        artist_id: artists.first().and_then(|artist| artist.id.clone()),
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
        artists,
    })
}

/// A [`Track`] from rspotify's full track model. `None` for local tracks and tracks
/// without an id, mirroring [`track`].
pub(crate) fn track_from_full(track: rspotify::model::FullTrack) -> Option<Track> {
    use rspotify::prelude::Id;
    if track.is_local {
        return None;
    }
    let id = track.id.as_ref()?.id().to_string();
    let artists = track_artists(&track.artists);
    Some(Track {
        id,
        source: TrackSource::Spotify,
        local_path: None,
        name: track.name,
        artist: joined_artist_names(&artists),
        album: track.album.name,
        added_at: None,
        duration_ms: track.duration.num_milliseconds() as u32,
        image_url: track.album.images.first().map(|img| img.url.clone()),
        album_id: track.album.id.map(|id| id.id().to_string()),
        artist_id: artists.first().and_then(|artist| artist.id.clone()),
        artists,
    })
}

pub(crate) fn album(album: &serde_json::Value) -> Option<Album> {
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
        thumb_url: album
            .get("images")
            .and_then(|v| v.as_array())
            .and_then(|images| images.last())
            .and_then(|image| image.get("url"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
        release_year: release_date.split('-').next().unwrap_or("").to_string(),
        release_date: (!release_date.is_empty()).then(|| release_date.to_string()),
        track_count: album
            .get("total_tracks")
            .and_then(|v| v.as_u64())
            .map(|value| value as u32),
    })
}

pub(crate) fn artist(artist: &serde_json::Value) -> Option<Artist> {
    Some(Artist {
        id: artist.get("id")?.as_str()?.to_string(),
        name: artist.get("name")?.as_str()?.to_string(),
        image_url: artist
            .get("images")
            .and_then(|v| v.as_array())
            .and_then(|images| images.first())
            .and_then(|image| image.get("url"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_tracks_are_filtered_out() {
        let value = serde_json::json!({
            "id": "track",
            "name": "Track",
            "is_local": true
        });

        assert!(track(&value).is_none());
    }

    #[test]
    fn album_release_year_uses_year_component() {
        let value = serde_json::json!({
            "id": "album",
            "name": "Album",
            "artists": [{ "name": "Artist" }],
            "release_date": "2024-03-01",
            "images": [],
            "total_tracks": 12
        });

        let album = album(&value).unwrap();
        assert_eq!(album.release_year, "2024");
        assert_eq!(album.track_count, Some(12));
    }
}
