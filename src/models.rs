use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub owner_id: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artists: String,
    pub image_url: Option<String>,
    pub release_year: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LibraryNode {
    Playlist { playlist: Playlist, indent: usize },
    Folder(crate::config::Folder),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BrowseNode {
    TopTracks,
    RecentlyPlayed,
    FollowedArtists,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrackListContextKind {
    Playlist,
    Album,
    Generated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrackListContext {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub owner_id: Option<String>,
    pub image_url: Option<String>,
    pub kind: TrackListContextKind,
}

impl TrackListContext {
    pub fn playlist(
        id: String,
        title: String,
        owner: String,
        owner_id: String,
        image_url: Option<String>,
    ) -> Self {
        Self {
            id,
            title,
            subtitle: owner,
            owner_id: Some(owner_id),
            image_url,
            kind: TrackListContextKind::Playlist,
        }
    }

    pub fn album(id: String, title: String, artists: String, image_url: Option<String>) -> Self {
        Self {
            id,
            title,
            subtitle: artists,
            owner_id: None,
            image_url,
            kind: TrackListContextKind::Album,
        }
    }

    pub fn generated(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: String::new(),
            owner_id: None,
            image_url: None,
            kind: TrackListContextKind::Generated,
        }
    }

    pub fn is_album(&self) -> bool {
        self.kind == TrackListContextKind::Album
    }

    pub fn can_modify_playlist(&self, user_id: Option<&String>) -> bool {
        self.kind == TrackListContextKind::Playlist && self.owner_id.as_ref() == user_id
    }

    pub fn requires_worker_load(&self) -> bool {
        self.kind != TrackListContextKind::Generated
    }

    pub fn playback_context_id(&self) -> &str {
        if self.kind == TrackListContextKind::Generated {
            "LIKED_SONGS"
        } else {
            &self.id
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaybackItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchTrack {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchAlbum {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SearchResults {
    pub tracks: Vec<SearchTrack>,
    pub albums: Vec<SearchAlbum>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub device_type: String,
    pub volume_percent: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LyricLine {
    pub start_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistPageData {
    pub artist_id: String,
    pub artist_name: String,
    pub top_tracks: Vec<Track>,
    pub albums: Vec<Album>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub followers: u32,
    pub image_url: Option<String>,
}
