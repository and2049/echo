use super::SpotifyWorker;

impl SpotifyWorker {
    pub async fn search_catalog(
        &self,
        query: &str,
    ) -> anyhow::Result<crate::models::SearchResults> {
        use rspotify::model::{Id, SearchType};
        use rspotify::prelude::BaseClient;

        let mut results = crate::models::SearchResults::default();

        if let Ok(rspotify::model::SearchResult::Tracks(page)) = self
            .client
            .search(query, SearchType::Track, None, None, None, None)
            .await
        {
            results.tracks = page
                .items
                .into_iter()
                .filter_map(|t| {
                    let id = t.id?.id().to_string();
                    let name = t.name;
                    let artist = t
                        .artists
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
                        name,
                        artist,
                        album,
                        duration_ms,
                        image_url,
                        album_id,
                    })
                })
                .collect();
        }

        if let Ok(rspotify::model::SearchResult::Albums(page)) = self
            .client
            .search(query, SearchType::Album, None, None, None, None)
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

        Ok(results)
    }
}
