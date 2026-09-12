use crate::models::{Album, Artist, Track, TrackArtist, TrackSource};

pub(crate) fn recent_history(value: &serde_json::Value) -> crate::home::RecentHistory {
    use crate::home::{HomeItemKind, RecentContext, RecentHistory};
    let mut result = RecentHistory::default();
    let mut tracks = std::collections::HashSet::new();
    let mut contexts = std::collections::HashSet::new();
    for item in value
        .get("items")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let Some(raw) = item.get("track") else {
            continue;
        };
        if raw
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|kind| kind != "track")
        {
            continue;
        }
        let Some(mut track) = track(raw) else {
            continue;
        };
        track.added_at = item
            .get("played_at")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if tracks.insert(track.id.clone()) {
            result.tracks.push(track.clone());
        }
        let Some(context) = item.get("context") else {
            continue;
        };
        let kind = match context.get("type").and_then(|v| v.as_str()) {
            Some("playlist") => HomeItemKind::Playlist,
            Some("album") => HomeItemKind::Album,
            Some("artist") => HomeItemKind::Artist,
            _ => continue,
        };
        let Some(uri) = context.get("uri").and_then(|v| v.as_str()) else {
            continue;
        };
        let parts: Vec<_> = uri.split(':').collect();
        if parts.len() != 3
            || parts[0] != "spotify"
            || Some(parts[1]) != context.get("type").and_then(|v| v.as_str())
            || parts[2].is_empty()
        {
            continue;
        }
        if result.contexts.len() < 12 && contexts.insert(uri.to_string()) {
            result.contexts.push(RecentContext {
                uri: uri.to_string(),
                kind,
                first_track: track,
            });
        }
    }
    result
}

pub(crate) fn playlist(value: &serde_json::Value) -> Option<crate::models::Playlist> {
    let owner_id = value.pointer("/owner/id")?.as_str()?.to_string();
    let images = value.get("images").and_then(|v| v.as_array());
    Some(crate::models::Playlist {
        id: value.get("id")?.as_str()?.to_string(),
        name: value.get("name")?.as_str()?.to_string(),
        owner: value
            .pointer("/owner/display_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&owner_id)
            .to_string(),
        owner_id,
        image_url: images
            .and_then(|images| images.first())
            .and_then(|v| v.get("url"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        thumb_url: images
            .and_then(|images| images.last())
            .and_then(|v| v.get("url"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        description: value
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        public: value.get("public").and_then(|v| v.as_bool()),
        collaborative: value
            .get("collaborative")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        track_count: value
            .pointer("/items/total")
            .or_else(|| value.pointer("/tracks/total"))
            .and_then(|v| v.as_u64())
            .and_then(|n| u32::try_from(n).ok()),
        snapshot_id: value
            .get("snapshot_id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    })
}

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
        explicit: track
            .get("explicit")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        added_by: None,
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
        explicit: track.explicit,
        added_by: None,
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
    #[test]
    fn recent_history_filters_non_music_and_keeps_recent_unique_contexts() {
        let mut items = Vec::new();
        for index in 0..15 {
            items.push(serde_json::json!({"track":{"type":"track","id":"same-track","name":"Song","album":{"id":"album","name":"Album"}},"context":{"type":"playlist","uri":format!("spotify:playlist:{index}")}}));
        }
        items.insert(1, items[0].clone());
        items.insert(0, serde_json::json!({"track":{"type":"episode","id":"episode","name":"Podcast"},"context":{"type":"show","uri":"spotify:show:show"}}));
        let history = recent_history(&serde_json::json!({"items":items}));
        assert_eq!(history.tracks.len(), 1);
        assert_eq!(history.contexts.len(), 12);
        assert_eq!(history.contexts[0].uri, "spotify:playlist:0");
        assert_eq!(history.contexts[11].uri, "spotify:playlist:11");
        let invalid = recent_history(
            &serde_json::json!({"items":[{"track":{"id":"t","name":"Song"},"context":{"type":"album","uri":"spotify:show:bad"}}]}),
        );
        assert!(invalid.contexts.is_empty());
    }

    #[test]
    fn playlist_metadata_handles_both_total_fields_and_nulls() {
        let mut value = serde_json::json!({"id":"p", "name":"P", "owner":{"id":"owner", "display_name":"Owner"}, "description":"Description", "public":true, "collaborative":true, "items":{"total":125}, "snapshot_id":"snapshot"});
        let parsed = playlist(&value).unwrap();
        assert_eq!(parsed.description.as_deref(), Some("Description"));
        assert_eq!(parsed.public, Some(true));
        assert!(parsed.collaborative);
        assert_eq!(parsed.owner, "Owner");
        assert_eq!(parsed.track_count, Some(125));
        assert_eq!(parsed.snapshot_id.as_deref(), Some("snapshot"));
        value.as_object_mut().unwrap().remove("items");
        value["tracks"] = serde_json::json!({"total":5});
        value["owner"]["display_name"] = serde_json::Value::Null;
        value["public"] = serde_json::Value::Null;
        let parsed = playlist(&value).unwrap();
        assert_eq!(parsed.track_count, Some(5));
        assert_eq!(parsed.owner, "owner");
        assert_eq!(parsed.public, None);
        assert!(playlist(&serde_json::Value::Null).is_none());
    }

    #[test]
    fn track_explicit_defaults_and_parses() {
        let mut value = serde_json::json!({"id":"t", "name":"T"});
        assert!(!track(&value).unwrap().explicit);
        value["explicit"] = serde_json::Value::Bool(true);
        assert!(track(&value).unwrap().explicit);
    }

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
