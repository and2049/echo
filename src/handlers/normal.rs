use crate::app::{ActiveView, AppState};
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    if let Some(folder_name) = state.folder_delete_prompt.clone() {
        if key.code == KeyCode::Char('y') {
            state.library_config.folders.retain(|fd| fd.name != folder_name);
            state.save_library_config();
            state.compute_library_view();
            
            // Adjust selection index if it goes out of bounds
            if state.selected_playlist_index >= state.library_view.len() {
                state.selected_playlist_index = state.library_view.len().saturating_sub(1);
            }
        }
        state.folder_delete_prompt = None;
        return None;
    }

    if key.code != KeyCode::Char('d') {
        state.pending_d_press = false;
    }

    match key.code {
        KeyCode::Char('j') => match state.active_view {
            ActiveView::Library => {
                let max_len = if state.active_library_tab == crate::app::LibraryTab::Albums {
                    state.saved_albums.len()
                } else {
                    state.library_view.len()
                };
                if max_len > 0 && state.selected_playlist_index < max_len.saturating_sub(1) {
                    state.selected_playlist_index += 1;
                }
            }
            ActiveView::TrackList => {
                if state.selected_track_index < state.tracks.len().saturating_sub(1) {
                    state.selected_track_index += 1;
                }
            }
        },
        KeyCode::Char('k') => match state.active_view {
            ActiveView::Library => {
                if state.selected_playlist_index > 0 {
                    state.selected_playlist_index -= 1;
                }
            }
            ActiveView::TrackList => {
                if state.selected_track_index > 0 {
                    state.selected_track_index -= 1;
                }
            }
        },
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Char('z') => {
            if state.active_view == ActiveView::Library {
                let context_id = if state.active_library_tab == crate::app::LibraryTab::Albums {
                    if state.selected_playlist_index < state.saved_albums.len() {
                        state.saved_albums[state.selected_playlist_index].id.clone()
                    } else {
                        String::new()
                    }
                } else {
                    if state.selected_playlist_index < state.library_view.len() {
                        match &state.library_view[state.selected_playlist_index] {
                            crate::models::LibraryNode::Playlist { playlist, .. } => playlist.id.clone(),
                            crate::models::LibraryNode::Folder(f) => {
                                let folder_name = f.name.clone();
                                if let Some(folder) = state.library_config.folders.iter_mut().find(|fd| fd.name == folder_name) {
                                    folder.is_open = !folder.is_open;
                                }
                                state.save_library_config();
                                state.compute_library_view();
                                String::new()
                            }
                        }
                    } else {
                        String::new()
                    }
                };
                
                if !context_id.is_empty() {
                    let is_album = state.active_library_tab == crate::app::LibraryTab::Albums;
                    state.active_view = ActiveView::TrackList;
                    state.tracks.clear();
                    state.selected_track_index = 0;
                    return Some(AppEvent::LoadContextTracks(context_id, is_album));
                }
            } else if state.active_view == ActiveView::TrackList {
                if state.selected_track_index < state.tracks.len() {
                    let track = &state.tracks[state.selected_track_index];
                    let context_id = if state.active_library_tab == crate::app::LibraryTab::Albums {
                        if state.selected_playlist_index < state.saved_albums.len() {
                            state.saved_albums[state.selected_playlist_index].id.clone()
                        } else {
                            String::new()
                        }
                    } else {
                        if state.selected_playlist_index < state.library_view.len() {
                            match &state.library_view[state.selected_playlist_index] {
                                crate::models::LibraryNode::Playlist { playlist, .. } => playlist.id.clone(),
                                _ => String::new(),
                            }
                        } else {
                            String::new()
                        }
                    };
                    
                    if !context_id.is_empty() {
                        return Some(AppEvent::PlayTrack {
                            context_id,
                            track_id: track.id.clone(),
                            is_album: state.active_library_tab == crate::app::LibraryTab::Albums,
                            title: track.name.clone(),
                            artist: track.artist.clone(),
                            duration_ms: track.duration_ms,
                            image_url: track.image_url.clone(),
                        });
                    }
                }
            }
        }
        KeyCode::Char(':') => {
            state.mode = crate::app::AppMode::Command;
            state.command_buffer.clear();
        }
        KeyCode::Char('d') | KeyCode::Char('x') => {
            if state.active_view == ActiveView::Library {
                if state.active_library_tab == crate::app::LibraryTab::Albums {
                    return None;
                }
                if state.selected_playlist_index < state.library_view.len() {
                    match &state.library_view[state.selected_playlist_index] {
                        crate::models::LibraryNode::Playlist { playlist, .. } => {
                            if playlist.id == "LIKED_SONGS" {
                                return None;
                            }

                            // Put in cut register
                            state.operation_register = vec![playlist.id.clone()];
                            
                            // Remove from any folders
                            for f in &mut state.library_config.folders {
                                f.playlists.retain(|id| id != &playlist.id);
                            }
                            state.save_library_config();
                            state.compute_library_view();
                        }
                        crate::models::LibraryNode::Folder(f) => {
                            if key.code == KeyCode::Char('d') {
                                if state.pending_d_press {
                                    state.folder_delete_prompt = Some(f.name.clone());
                                    state.pending_d_press = false;
                                } else {
                                    state.pending_d_press = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        KeyCode::Char('p') => {
            if state.active_view == ActiveView::Library && !state.operation_register.is_empty() {
                if state.selected_playlist_index < state.library_view.len() {
                    let node = &state.library_view[state.selected_playlist_index];
                    match node {
                        crate::models::LibraryNode::Folder(f) => {
                            let folder_name = f.name.clone();
                            if let Some(folder) = state.library_config.folders.iter_mut().find(|fd| fd.name == folder_name) {
                                for id in &state.operation_register {
                                    if !folder.playlists.contains(id) {
                                        folder.playlists.push(id.clone());
                                    }
                                }
                            }
                            // Unpin anything moved into a folder to avoid duplicates
                            for id in &state.operation_register {
                                state.library_config.pinned.retain(|p| p != id);
                            }
                        }
                        crate::models::LibraryNode::Playlist { .. } => {
                            // If focused on a standard playlist, pasting it here keeps it at the root level.
                            // Since we already removed it from folders during "cut", it is implicitly at root.
                        }
                    }
                    state.operation_register.clear();
                    state.save_library_config();
                    state.compute_library_view();
                }
            }
        }
        KeyCode::Char('m') => {
            if state.active_view == ActiveView::Library {
                if state.active_library_tab == crate::app::LibraryTab::Albums {
                    return None;
                }
                if state.selected_playlist_index < state.library_view.len() {
                    if let crate::models::LibraryNode::Playlist { playlist, .. } = &state.library_view[state.selected_playlist_index] {
                        let id = &playlist.id;
                        if id == "LIKED_SONGS" {
                            return None;
                        }
                        if state.library_config.pinned.contains(id) {
                            state.library_config.pinned.retain(|p| p != id);
                        } else {
                            state.library_config.pinned.push(id.clone());
                        }
                        state.save_library_config();
                        state.compute_library_view();
                    }
                }
            }
        }
        KeyCode::Char('h') | KeyCode::Esc | KeyCode::Backspace => {
            if state.active_view == ActiveView::TrackList {
                state.active_view = ActiveView::Library;
            }
        }
        KeyCode::Char(' ') => {
            state.playback.is_playing = !state.playback.is_playing;
            return Some(AppEvent::TogglePlayback(state.playback.is_playing));
        }
        KeyCode::Char('s') => {
            state.playback.is_shuffled = !state.playback.is_shuffled;
            return Some(AppEvent::ToggleShuffle(state.playback.is_shuffled));
        }
        KeyCode::Char(']') | KeyCode::Char('>') => {
            state.playback.progress_ms = 0;
            state.playback.duration_ms = 0;
            return Some(AppEvent::NextTrack {
                current_track_id: state.playback.playing_track_id.clone(),
            });
        }
        KeyCode::Char('[') | KeyCode::Char('<') => {
            state.playback.progress_ms = 0;
            state.playback.duration_ms = 0;
            return Some(AppEvent::PreviousTrack {
                current_track_id: state.playback.playing_track_id.clone(),
            });
        }
        KeyCode::Tab => {
            if state.active_view == ActiveView::Library {
                state.active_library_tab = match state.active_library_tab {
                    crate::app::LibraryTab::Playlists => crate::app::LibraryTab::Albums,
                    crate::app::LibraryTab::Albums => crate::app::LibraryTab::Playlists,
                };
                state.selected_playlist_index = 0;
            }
        }
        _ => {}
    }
    None
}
