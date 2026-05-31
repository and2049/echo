#[derive(Clone, Debug)]
pub struct Playlist {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PlaybackItem {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub duration_ms: u32,
    pub image_url: Option<String>,
}
