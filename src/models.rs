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
}
