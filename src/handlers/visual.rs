use crossterm::event::{KeyCode, KeyEvent};
use crate::app::{AppMode, AppState, ActiveView};
use crate::events::AppEvent;

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    if let Some((playlist_id, track_ids)) = state.track_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.track_delete_prompt = None;
            state.mode = AppMode::Normal;
            state.visual_selection_start = None;
            return Some(AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids));
        }
        state.track_delete_prompt = None;
        return None;
    }

    if state.playlist_add_modal_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.playlist_add_modal_open = false;
                state.selected_playlist_modal_index = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                let user_playlists: Vec<_> = state.playlists.iter().filter(|p| Some(&p.owner_id) == state.user_id.as_ref()).collect();
                if state.selected_playlist_modal_index + 1 < user_playlists.len() {
                    state.selected_playlist_modal_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.selected_playlist_modal_index > 0 {
                    state.selected_playlist_modal_index -= 1;
                }
            }
            KeyCode::Enter => {
                let user_playlists: Vec<_> = state.playlists.iter().filter(|p| Some(&p.owner_id) == state.user_id.as_ref()).collect();
                if let Some(playlist) = user_playlists.get(state.selected_playlist_modal_index) {
                    let track_ids = if let Some((start, end)) = state.get_visual_selection_range() {
                        match state.active_view {
                            ActiveView::TrackList => state.tracks[start..=end].iter().map(|t| t.id.clone()).collect(),
                            ActiveView::SearchResults => if state.active_search_tab == crate::app::SearchTab::Tracks {
                                state.search_results.tracks[start..=end].iter().map(|t| t.id.clone()).collect()
                            } else { vec![] },
                            ActiveView::Queue => state.queue[start..=end].iter().map(|t| t.id.clone()).collect(),
                            _ => vec![],
                        }
                    } else { vec![] };
                    state.playlist_add_modal_open = false;
                    state.selected_playlist_modal_index = 0;
                    state.mode = AppMode::Normal;
                    state.visual_selection_start = None;
                    if !track_ids.is_empty() {
                        return Some(AppEvent::AddTracksToPlaylist(playlist.id.clone(), track_ids));
                    }
                }
            }
            _ => {}
        }
        return None;
    }

    if key.code != KeyCode::Char('d') {
        state.pending_d_press = false;
    }

    match key.code {
        KeyCode::Esc => {
            state.mode = AppMode::Normal;
            state.visual_selection_start = None;
            state.status_message = None;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            match state.active_view {
                ActiveView::TrackList => {
                    if state.selected_track_index + 1 < state.tracks.len() {
                        state.selected_track_index += 1;
                    }
                }
                ActiveView::SearchResults => {
                    if state.active_search_tab == crate::app::SearchTab::Tracks
                        && state.selected_search_index + 1 < state.search_results.tracks.len()
                    {
                        state.selected_search_index += 1;
                    }
                }
                ActiveView::Queue => {
                    if state.selected_queue_index + 1 < state.queue.len() {
                        state.selected_queue_index += 1;
                    }
                }
                _ => {}
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            match state.active_view {
                ActiveView::TrackList => {
                    if state.selected_track_index > 0 {
                        state.selected_track_index -= 1;
                    }
                }
                ActiveView::SearchResults => {
                    if state.active_search_tab == crate::app::SearchTab::Tracks
                        && state.selected_search_index > 0
                    {
                        state.selected_search_index -= 1;
                    }
                }
                ActiveView::Queue => {
                    if state.selected_queue_index > 0 {
                        state.selected_queue_index -= 1;
                    }
                }
                _ => {}
            }
        }
        KeyCode::Char('q') => {
            if let Some((start, end)) = state.get_visual_selection_range() {
                let track_ids: Vec<String> = match state.active_view {
                    ActiveView::TrackList => {
                        state.tracks[start..=end].iter().map(|t| t.id.clone()).collect()
                    }
                    ActiveView::SearchResults => {
                        if state.active_search_tab == crate::app::SearchTab::Tracks {
                            state.search_results.tracks[start..=end].iter().map(|t| t.id.clone()).collect()
                        } else {
                            vec![]
                        }
                    }
                    ActiveView::Queue => {
                        state.queue[start..=end].iter().map(|t| t.id.clone()).collect()
                    }
                    _ => vec![],
                };

                state.mode = AppMode::Normal;
                state.visual_selection_start = None;
                state.status_message = Some(format!("Added {} tracks to queue", track_ids.len()));

                if !track_ids.is_empty() {
                    return Some(AppEvent::AddToQueue(track_ids));
                }
            } else {
                state.mode = AppMode::Normal;
                state.visual_selection_start = None;
                state.status_message = None;
            }
        }
        KeyCode::Char('d') => {
            if state.active_view == ActiveView::TrackList {
                if let Some((start, end)) = state.get_visual_selection_range() {
                    if let Some((playlist_id, _, _, playlist_owner_id)) = &state.tracklist_context_metadata {
                        if Some(playlist_owner_id) == state.user_id.as_ref() {
                            if state.pending_d_press {
                                let track_ids = state.tracks[start..=end].iter().map(|t| t.id.clone()).collect();
                                state.track_delete_prompt = Some((playlist_id.clone(), track_ids));
                                state.pending_d_press = false;
                            } else {
                                state.pending_d_press = true;
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('a') => {
            state.playlist_add_modal_open = true;
            state.selected_playlist_modal_index = 0;
        }
        _ => {}
    }
    None
}
