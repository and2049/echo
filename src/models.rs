#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub owner_id: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artists: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug)]
pub enum LibraryNode {
    Playlist { playlist: Playlist, indent: usize },
    Folder(crate::config::Folder),
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PlaybackItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SearchTrack {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SearchAlbum {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    pub tracks: Vec<SearchTrack>,
    pub albums: Vec<SearchAlbum>,
}

#[derive(Clone, Debug)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub device_type: String,
    pub volume_percent: u32,
}
