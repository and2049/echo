use crate::{app::SearchTab, models::SearchResults};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchRow {
    pub tab: SearchTab,
    pub index: usize,
    pub top: bool,
}

#[derive(Clone, Debug)]
pub struct TopResult {
    pub id: String,
    pub kind: SearchTab,
    pub title: String,
    pub subtitle: String,
    pub credit: String,
    pub image_url: Option<String>,
    pub row: SearchRow,
}

impl TopResult {
    pub fn subtitle_key(&self) -> &'static str {
        match self.kind {
            SearchTab::Artists => "desktop.search_artist_type",
            SearchTab::Albums => "desktop.search_album_type",
            SearchTab::Playlists => "desktop.search_playlist_type",
            _ => "desktop.search_song_type",
        }
    }

    pub fn playable(&self) -> bool {
        self.kind == SearchTab::Tracks || !self.id.starts_with("local-")
    }
}

pub fn search_row_item(results: &SearchResults, row: SearchRow) -> Option<TopResult> {
    let (id, title, credit, image_url, label) = match row.tab {
        SearchTab::Artists => {
            let v = results.artists.get(row.index)?;
            (
                v.id.clone(),
                v.name.clone(),
                String::new(),
                v.image_url.clone(),
                "Artist",
            )
        }
        SearchTab::Albums => {
            let v = results.albums.get(row.index)?;
            (
                v.id.clone(),
                v.name.clone(),
                v.artist.clone(),
                v.image_url.clone(),
                "Album",
            )
        }
        SearchTab::Playlists => {
            let v = results.playlists.get(row.index)?;
            (
                v.id.clone(),
                v.name.clone(),
                v.owner.clone(),
                v.image_url.clone(),
                "Playlist",
            )
        }
        SearchTab::Tracks => {
            let v = results.tracks.get(row.index)?;
            (
                v.id.clone(),
                v.name.clone(),
                v.artist.clone(),
                v.image_url.clone(),
                "Song",
            )
        }
        SearchTab::All => return None,
    };
    let subtitle = if row.tab == SearchTab::Artists {
        label.into()
    } else {
        format!("{label} • {credit}")
    };
    Some(TopResult {
        id,
        kind: row.tab,
        title,
        subtitle,
        credit,
        image_url,
        row,
    })
}

pub fn top_result(results: &SearchResults, query: &str) -> Option<TopResult> {
    let query = query.trim().to_lowercase();
    let mut best: Option<(u8, TopResult)> = None;
    for (tab, count) in [
        (SearchTab::Artists, results.artists.len()),
        (SearchTab::Albums, results.albums.len()),
        (SearchTab::Playlists, results.playlists.len()),
        (SearchTab::Tracks, results.tracks.len()),
    ] {
        for index in 0..count {
            let item = search_row_item(
                results,
                SearchRow {
                    tab,
                    index,
                    top: true,
                },
            )?;
            let name = item.title.to_lowercase();
            let score = if query.is_empty() {
                0
            } else if name == query {
                3
            } else if name.starts_with(&query) {
                2
            } else if name.contains(&query) {
                1
            } else {
                0
            };
            if best.as_ref().is_none_or(|(previous, _)| score > *previous) {
                best = Some((score, item));
            }
        }
    }
    best.map(|(_, item)| item)
}

pub fn all_tab_rows(results: &SearchResults, query: &str) -> Vec<SearchRow> {
    let mut rows: Vec<_> = top_result(results, query)
        .into_iter()
        .map(|item| item.row)
        .collect();
    for (tab, count, limit) in [
        (SearchTab::Tracks, results.tracks.len(), 4),
        (SearchTab::Albums, results.albums.len(), 6),
        (SearchTab::Artists, results.artists.len(), 6),
        (SearchTab::Playlists, results.playlists.len(), 6),
    ] {
        rows.extend((0..count.min(limit)).map(|index| SearchRow {
            tab,
            index,
            top: false,
        }));
    }
    rows
}

pub fn remember_search(history: &mut Vec<String>, query: &str) {
    let query = query.trim();
    if query.is_empty() {
        return;
    }
    history.retain(|old| old.to_lowercase() != query.to_lowercase());
    history.insert(0, query.into());
    history.truncate(10);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Artist, Playlist, SearchAlbum, SearchTrack};

    fn track(id: &str, name: &str) -> SearchTrack {
        serde_json::from_value(serde_json::json!({"id": id, "name": name, "artist":"Artist", "album":"Album", "duration_ms":1000, "image_url":"cover", "album_id":"album"})).unwrap()
    }

    fn results() -> SearchResults {
        SearchResults {
            tracks: vec![track("track", "Echo")],
            albums: vec![SearchAlbum { id:"album".into(), name:"Echo".into(), artist:"Artist".into(), image_url:Some("album-cover".into()) }],
            artists: vec![Artist { id:"artist".into(), name:"Echo".into(), image_url:Some("portrait".into()) }],
            playlists: vec![serde_json::from_value::<Playlist>(serde_json::json!({"id":"playlist","name":"Echo","owner":"Owner","owner_id":"owner","image_url":null})).unwrap()],
        }
    }

    #[test]
    fn top_result_orders_match_quality_then_kind_and_preserves_details() {
        let mut results = results();
        let top = top_result(&results, "eCHO").unwrap();
        assert_eq!(top.kind, SearchTab::Artists);
        assert_eq!(top.id, "artist");
        assert_eq!(top.subtitle, "Artist");
        assert_eq!(top.image_url.as_deref(), Some("portrait"));
        results.artists[0].name = "Echoes".into();
        assert_eq!(
            top_result(&results, "echo").unwrap().kind,
            SearchTab::Albums
        );
        results.albums[0].name = "The Echo".into();
        assert_eq!(
            top_result(&results, "echo").unwrap().kind,
            SearchTab::Playlists
        );
        results.playlists[0].name = "No match".into();
        assert_eq!(
            top_result(&results, "echo").unwrap().kind,
            SearchTab::Tracks
        );
        results.tracks[0].name = "A different song".into();
        assert_eq!(
            top_result(&results, "echo").unwrap().kind,
            SearchTab::Artists
        );
        results.artists[0].name = "Unrelated".into();
        assert_eq!(
            top_result(&results, "echo").unwrap().subtitle,
            "Album • Artist"
        );
        assert!(top_result(&SearchResults::default(), "anything").is_none());
    }

    #[test]
    fn matching_is_unicode_case_insensitive_and_ties_are_stable() {
        let results = SearchResults {
            tracks: vec![track("one", "ÉCHO"), track("two", "écho")],
            ..Default::default()
        };
        assert_eq!(top_result(&results, " écho ").unwrap().id, "one");
        assert_eq!(top_result(&results, "").unwrap().id, "one");
        let row = SearchRow {
            tab: SearchTab::Tracks,
            index: 1,
            top: false,
        };
        assert_eq!(
            search_row_item(&results, row).unwrap().subtitle,
            "Song • Artist"
        );
        assert!(search_row_item(&results, SearchRow { index: 2, ..row }).is_none());
    }

    #[test]
    fn all_rows_are_bounded_and_keep_the_underlying_indices() {
        let mut results = results();
        results.tracks = (0..10).map(|n| track(&n.to_string(), "Song")).collect();
        results.albums = vec![results.albums[0].clone(); 10];
        results.artists = vec![results.artists[0].clone(); 10];
        results.playlists = vec![results.playlists[0].clone(); 10];
        results.tracks[9].name = "Winner".into();
        let rows = all_tab_rows(&results, "winner");
        assert_eq!(rows.len(), 23);
        assert_eq!(
            rows[0],
            SearchRow {
                tab: SearchTab::Tracks,
                index: 9,
                top: true
            }
        );
        assert_eq!(
            rows[4],
            SearchRow {
                tab: SearchTab::Tracks,
                index: 3,
                top: false
            }
        );
        assert_eq!(rows[5].tab, SearchTab::Albums);
        assert_eq!(rows[11].tab, SearchTab::Artists);
        assert_eq!(rows[17].tab, SearchTab::Playlists);
        assert_eq!(rows[22].index, 5);
        results.albums.clear();
        results.artists.clear();
        results.playlists.clear();
        assert_eq!(all_tab_rows(&results, "winner").len(), 5);
        assert!(all_tab_rows(&SearchResults::default(), "").is_empty());
    }

    #[test]
    fn recent_searches_are_trimmed_bounded_and_promoted_without_duplicates() {
        let mut history = Vec::new();
        for n in 0..12 {
            remember_search(&mut history, &format!("Query {n}"));
        }
        assert_eq!(history.len(), 10);
        assert_eq!(history[0], "Query 11");
        assert_eq!(history[9], "Query 2");
        remember_search(&mut history, " QUERY 5 ");
        assert_eq!(history[0], "QUERY 5");
        assert_eq!(history.len(), 10);
        remember_search(&mut history, " ");
        assert_eq!(history.len(), 10);
    }
}
