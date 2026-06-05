use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrackSource {
    #[default]
    Spotify,
    Local,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlaybackTarget {
    SpotifyContext {
        context_id: String,
        is_album: bool,
    },
    SpotifyTrack {
        track_id: String,
    },
    LocalTrack {
        track_id: String,
        path: PathBuf,
    },
    LocalContext {
        tracks: Vec<Track>,
        selected_index: usize,
    },
}

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
    #[serde(default)]
    pub track_count: Option<u32>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    #[serde(default)]
    pub source: TrackSource,
    #[serde(default)]
    pub local_path: Option<PathBuf>,
    pub name: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrackListContextKind {
    Playlist,
    Album,
    Generated,
    LocalLibrary,
    LocalPlaylist,
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

    pub fn local_library() -> Self {
        Self {
            id: "local-library".to_string(),
            title: "Local Music".to_string(),
            subtitle: String::new(),
            owner_id: None,
            image_url: None,
            kind: TrackListContextKind::LocalLibrary,
        }
    }

    pub fn local_playlist(id: String, title: String) -> Self {
        Self {
            id,
            title,
            subtitle: "Local playlist".to_string(),
            owner_id: None,
            image_url: None,
            kind: TrackListContextKind::LocalPlaylist,
        }
    }

    pub fn is_album(&self) -> bool {
        self.kind == TrackListContextKind::Album
    }

    pub fn can_modify_playlist(&self, user_id: Option<&String>) -> bool {
        self.kind == TrackListContextKind::Playlist && self.owner_id.as_ref() == user_id
    }

    pub fn requires_worker_load(&self) -> bool {
        matches!(
            self.kind,
            TrackListContextKind::Playlist | TrackListContextKind::Album
        )
    }

    pub fn playback_target_for_track(&self, track: &Track) -> Option<PlaybackTarget> {
        match track.source {
            TrackSource::Spotify => match self.kind {
                TrackListContextKind::Playlist | TrackListContextKind::Album => {
                    Some(PlaybackTarget::SpotifyContext {
                        context_id: self.id.clone(),
                        is_album: self.is_album(),
                    })
                }
                TrackListContextKind::Generated => Some(PlaybackTarget::SpotifyTrack {
                    track_id: track.id.clone(),
                }),
                TrackListContextKind::LocalLibrary | TrackListContextKind::LocalPlaylist => {
                    Some(PlaybackTarget::SpotifyTrack {
                        track_id: track.id.clone(),
                    })
                }
            },
            TrackSource::Local => track
                .local_path
                .clone()
                .map(|path| PlaybackTarget::LocalTrack {
                    track_id: track.id.clone(),
                    path,
                }),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaybackItem {
    pub id: String,
    #[serde(default)]
    pub source: TrackSource,
    #[serde(default)]
    pub local_path: Option<PathBuf>,
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
    pub artist_id: Option<String>,
}

/// Context used by the action menu popup.
#[derive(Clone, Debug)]
pub struct ActionMenuContext {
    pub track_id: String,
    pub source: TrackSource,
    pub track_name: String,
    pub album_id: Option<String>,
    pub artist_id: Option<String>,
    pub artist_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchTrack {
    pub id: String,
    #[serde(default)]
    pub source: TrackSource,
    #[serde(default)]
    pub local_path: Option<PathBuf>,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
    #[serde(default)]
    pub artist_id: Option<String>,
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
    pub artists: Vec<Artist>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalLibrary {
    #[serde(default)]
    pub tracks: Vec<LocalTrack>,
}

impl LocalLibrary {
    pub fn to_tracks(&self) -> Vec<Track> {
        self.tracks
            .iter()
            .map(|track| Track {
                id: track.id.clone(),
                source: TrackSource::Local,
                local_path: Some(track.path.clone()),
                name: track.title.clone(),
                artist: track.artist.clone(),
                duration_ms: track.duration_ms,
                image_url: track
                    .artwork_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                album_id: None,
                artist_id: None,
            })
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalTrack {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub artwork_path: Option<PathBuf>,
    pub file_size: u64,
    pub modified_unix_secs: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalScanReport {
    pub files_found: usize,
    pub tracks_added: usize,
    pub tracks_updated: usize,
    pub tracks_removed: usize,
    pub skipped: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LocalPlaylists {
    #[serde(default)]
    pub playlists: Vec<LocalPlaylist>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalPlaylist {
    pub id: String,
    pub name: String,
    pub created_unix_secs: u64,
    pub updated_unix_secs: u64,
    #[serde(default)]
    pub entries: Vec<LocalPlaylistEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LocalPlaylistEntry {
    LocalTrack {
        track_id: String,
    },
    SpotifyTrack {
        track_id: String,
        title: String,
        artist: String,
        album: String,
        duration_ms: u32,
        image_url: Option<String>,
        album_id: Option<String>,
        artist_id: Option<String>,
    },
}

pub fn stable_local_track_id(path: &Path) -> String {
    let normalized = normalize_local_path(path);
    format!("local:{:016x}", fnv1a_64(normalized.as_bytes()))
}

fn normalize_local_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    let mut hash = OFFSET_BASIS;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn stable_local_track_ids_are_deterministic() {
        let path = PathBuf::from("/music/Artist/Track.flac");

        assert_eq!(stable_local_track_id(&path), stable_local_track_id(&path));
    }

    #[test]
    fn stable_local_track_ids_use_local_namespace() {
        let id = stable_local_track_id(Path::new("/music/track.mp3"));

        assert!(id.starts_with("local:"));
        assert_ne!(id, "spotify-track-id");
    }

    #[test]
    fn local_track_playback_target_keeps_file_path_separate_from_id() {
        let path = PathBuf::from("/music/track.mp3");
        let track = Track {
            id: stable_local_track_id(&path),
            source: TrackSource::Local,
            local_path: Some(path.clone()),
            name: "Track".to_string(),
            artist: "Artist".to_string(),
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
        };
        let context = TrackListContext::local_library();

        assert_eq!(
            context.playback_target_for_track(&track),
            Some(PlaybackTarget::LocalTrack {
                track_id: track.id.clone(),
                path
            })
        );
        assert_ne!(track.id, track.local_path.unwrap().to_string_lossy());
    }

    #[test]
    fn local_storage_json_round_trips() {
        let track_id = "local:abc".to_string();
        let library = LocalLibrary {
            tracks: vec![LocalTrack {
                id: track_id.clone(),
                path: PathBuf::from("/music/track.flac"),
                title: "Track".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                duration_ms: 42_000,
                artwork_path: Some(PathBuf::from("/cache/artwork.png")),
                file_size: 123,
                modified_unix_secs: 456,
            }],
        };
        let playlists = LocalPlaylists {
            playlists: vec![LocalPlaylist {
                id: "local-playlist:one".to_string(),
                name: "Mixed".to_string(),
                created_unix_secs: 1,
                updated_unix_secs: 2,
                entries: vec![
                    LocalPlaylistEntry::LocalTrack {
                        track_id: track_id.clone(),
                    },
                    LocalPlaylistEntry::SpotifyTrack {
                        track_id: "spotify-track".to_string(),
                        title: "Spotify Track".to_string(),
                        artist: "Spotify Artist".to_string(),
                        album: "Spotify Album".to_string(),
                        duration_ms: 1000,
                        image_url: Some("https://example.com/cover.jpg".to_string()),
                        album_id: Some("album".to_string()),
                        artist_id: Some("artist".to_string()),
                    },
                ],
            }],
        };

        let library_json = serde_json::to_string(&library).unwrap();
        let playlists_json = serde_json::to_string(&playlists).unwrap();

        let decoded_library: LocalLibrary = serde_json::from_str(&library_json).unwrap();
        let decoded_playlists: LocalPlaylists = serde_json::from_str(&playlists_json).unwrap();

        assert_eq!(decoded_library.tracks[0].id, track_id);
        assert_eq!(decoded_playlists.playlists[0].entries.len(), 2);
    }
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
    #[serde(default)]
    pub image_url: Option<String>,
    pub albums: Vec<Album>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub followers: u32,
    pub image_url: Option<String>,
}
