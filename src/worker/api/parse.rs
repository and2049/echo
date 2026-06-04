use crate::models::{Album, Artist, Track};

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
        release_year: release_date.split('-').next().unwrap_or("").to_string(),
    })
}

pub(crate) fn artist(artist: &serde_json::Value) -> Option<Artist> {
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
            "images": []
        });

        assert_eq!(album(&value).unwrap().release_year, "2024");
    }
}
