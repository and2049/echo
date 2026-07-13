use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    app::{ActiveView, AppState, LibraryTab, SearchTab},
    events::AppEvent,
    handlers::browse,
};

const PAGE_ROWS: usize = 20;
const SEQUENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationCommand {
    First,
    Last,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
}

pub struct NavigationKey {
    pub consumed: bool,
    pub command: Option<NavigationCommand>,
}

pub fn command_for_key(state: &mut AppState, key: &KeyEvent) -> NavigationKey {
    if state
        .ui
        .pending_key_sequence
        .is_some_and(|(_, started)| started.elapsed() >= SEQUENCE_TIMEOUT)
    {
        state.ui.pending_key_sequence = None;
    }

    if key.code == KeyCode::Char('g') && key.modifiers.is_empty() {
        if state
            .ui
            .pending_key_sequence
            .take()
            .is_some_and(|(key, _)| key == 'g')
        {
            return NavigationKey {
                consumed: true,
                command: Some(NavigationCommand::First),
            };
        }
        state.ui.pending_key_sequence = Some(('g', std::time::Instant::now()));
        return NavigationKey {
            consumed: true,
            command: None,
        };
    }

    state.ui.pending_key_sequence = None;
    let command = match (key.code, key.modifiers) {
        (KeyCode::Char('G') | KeyCode::End, _) => Some(NavigationCommand::Last),
        (KeyCode::Home, _) => Some(NavigationCommand::First),
        (KeyCode::PageUp, _) => Some(NavigationCommand::PageUp),
        (KeyCode::PageDown, _) => Some(NavigationCommand::PageDown),
        (KeyCode::Char('b'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(NavigationCommand::PageUp)
        }
        (KeyCode::Char('f'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(NavigationCommand::PageDown)
        }
        (KeyCode::Char('u'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(NavigationCommand::HalfPageUp)
        }
        (KeyCode::Char('d'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            Some(NavigationCommand::HalfPageDown)
        }
        _ => None,
    };
    NavigationKey {
        consumed: command.is_some(),
        command,
    }
}

pub fn execute(state: &mut AppState, command: NavigationCommand) -> Option<AppEvent> {
    let len = selected_list_len(state);
    if len == 0 {
        return None;
    }
    let current = selected_index(state);
    let target = match command {
        NavigationCommand::First => 0,
        NavigationCommand::Last => len - 1,
        NavigationCommand::PageUp => current.saturating_sub(PAGE_ROWS),
        NavigationCommand::PageDown => (current + PAGE_ROWS).min(len - 1),
        NavigationCommand::HalfPageUp => current.saturating_sub(PAGE_ROWS / 2),
        NavigationCommand::HalfPageDown => (current + PAGE_ROWS / 2).min(len - 1),
    };
    set_selected_index(state, target);
    if state.ui.active_view == ActiveView::Library && state.ui.active_library_tab == LibraryTab::Browse {
        browse::select_node_from_library_index(state);
        return browse::load_event_if_needed(state);
    }
    None
}

fn selected_list_len(state: &AppState) -> usize {
    match state.ui.active_view {
        ActiveView::Library => match state.ui.active_library_tab {
            LibraryTab::Playlists => state.data.library_view.len(),
            LibraryTab::Albums => state.data.saved_albums.len(),
            LibraryTab::Browse => 3,
        },
        ActiveView::TrackList => state.data.tracks.len(),
        ActiveView::SearchResults => match state.ui.active_search_tab {
            SearchTab::Tracks => state.data.search_results.tracks.len(),
            SearchTab::Albums => state.data.search_results.albums.len(),
            SearchTab::Artists => state.data.search_results.artists.len(),
        },
        ActiveView::Queue => state.data.queue.len(),
        ActiveView::Devices => state.data.devices.len(),
        ActiveView::ArtistList => state.data.followed_artists.len(),
        ActiveView::ArtistPage => state
            .data
            .artist_page_data
            .as_ref()
            .map_or(0, |data| data.albums.len()),
    }
}

fn selected_index(state: &AppState) -> usize {
    match state.ui.active_view {
        ActiveView::Library => state.ui.selected_playlist_index,
        ActiveView::TrackList => state.ui.selected_track_index,
        ActiveView::SearchResults => state.ui.selected_search_index,
        ActiveView::Queue => state.ui.selected_queue_index,
        ActiveView::Devices => state.ui.selected_device_index,
        ActiveView::ArtistList => state.ui.selected_artist_index,
        ActiveView::ArtistPage => state.ui.artist_page_album_index,
    }
}

fn set_selected_index(state: &mut AppState, index: usize) {
    match state.ui.active_view {
        ActiveView::Library => state.ui.selected_playlist_index = index,
        ActiveView::TrackList => state.ui.selected_track_index = index,
        ActiveView::SearchResults => state.ui.selected_search_index = index,
        ActiveView::Queue => state.ui.selected_queue_index = index,
        ActiveView::Devices => state.ui.selected_device_index = index,
        ActiveView::ArtistList => state.ui.selected_artist_index = index,
        ActiveView::ArtistPage => state.ui.artist_page_album_index = index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gg_and_g_move_to_list_boundaries() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::TrackList;
        state.data.tracks = (0..30)
            .map(|index| crate::models::Track {
                id: index.to_string(),
                source: crate::models::TrackSource::Spotify,
                local_path: None,
                name: index.to_string(),
                artist: String::new(),
                album: String::new(),
                added_at: None,
                duration_ms: 0,
                image_url: None,
                album_id: None,
                artist_id: None,
            })
            .collect();
        state.ui.selected_track_index = 12;

        let first_g = command_for_key(&mut state, &KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert!(first_g.consumed && first_g.command.is_none());
        let second_g = command_for_key(&mut state, &KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        execute(&mut state, second_g.command.unwrap());
        assert_eq!(state.ui.selected_track_index, 0);

        execute(&mut state, NavigationCommand::Last);
        assert_eq!(state.ui.selected_track_index, 29);
    }

    #[test]
    fn page_motion_clamps_to_list_bounds() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::Queue;
        state.data.queue = vec![crate::models::Track {
            id: "one".to_string(),
            source: crate::models::TrackSource::Spotify,
            local_path: None,
            name: String::new(),
            artist: String::new(),
            album: String::new(),
            added_at: None,
            duration_ms: 0,
            image_url: None,
            album_id: None,
            artist_id: None,
        }];
        execute(&mut state, NavigationCommand::PageDown);
        assert_eq!(state.ui.selected_queue_index, 0);
    }
}
