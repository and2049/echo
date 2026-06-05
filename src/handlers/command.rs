use crate::app::{AppMode, AppState};
use crate::events::AppEvent;
use crate::models::TrackListContext;
use crossterm::event::{KeyCode, KeyEvent};

fn generate_command_suggestions(state: &AppState) -> Vec<String> {
    let commands = vec![
        "q",
        "qa",
        "wq",
        "newfolder",
        "delfolder",
        "sort",
        "index",
        "theme",
        "search",
        "queue",
        "vis",
        "visbins",
        "album",
        "lang",
        "newplaylist",
        "newlocalplaylist",
        "localpath",
        "rescanlocal",
        "rename",
        "pixelate",
    ];
    let mut parts = state.command_buffer.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next();

    if let Some(arg_str) = arg {
        match cmd {
            "theme" => {
                let mut themes: Vec<String> = state.themes.keys().cloned().collect();
                themes.sort();
                themes
                    .into_iter()
                    .filter(|t| t.starts_with(arg_str))
                    .collect()
            }
            "sort" => {
                let options = vec!["alpha".to_string(), "creator".to_string()];
                options
                    .into_iter()
                    .filter(|o| o.starts_with(arg_str))
                    .collect()
            }
            "lang" => {
                let options = vec![
                    "en".to_string(),
                    "zh".to_string(),
                    "zh-CN".to_string(),
                    "zh-TW".to_string(),
                ];
                options
                    .into_iter()
                    .filter(|o| o.starts_with(arg_str))
                    .collect()
            }
            _ => vec![],
        }
    } else {
        commands
            .into_iter()
            .filter(|c| c.starts_with(cmd))
            .map(String::from)
            .collect()
    }
}

fn command_remainder<'a>(command: &'a str, command_name: &str) -> &'a str {
    command
        .trim()
        .strip_prefix(command_name)
        .map(str::trim)
        .unwrap_or_default()
}

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            if state.command_suggestions.is_empty() {
                state.command_suggestions = generate_command_suggestions(state);
                state.command_suggestion_index = if state.command_suggestions.is_empty() {
                    None
                } else {
                    Some(0)
                };
                state.command_base_buffer = state.command_buffer.clone();
            } else if let Some(idx) = state.command_suggestion_index {
                if key.code == KeyCode::Tab {
                    state.command_suggestion_index =
                        Some((idx + 1) % state.command_suggestions.len());
                } else {
                    state.command_suggestion_index = Some(
                        (idx + state.command_suggestions.len() - 1)
                            % state.command_suggestions.len(),
                    );
                }
            }

            if let Some(idx) = state.command_suggestion_index {
                let suggestion = &state.command_suggestions[idx];
                let mut parts = state.command_base_buffer.splitn(2, ' ');
                let cmd = parts.next().unwrap_or("");
                let arg = parts.next();

                if arg.is_some() {
                    state.command_buffer = format!("{} {}", cmd, suggestion);
                } else {
                    state.command_buffer = suggestion.clone();
                }
            }
            return None;
        }
        _ => {
            state.command_suggestions.clear();
            state.command_suggestion_index = None;
            state.command_base_buffer.clear();
        }
    }

    match key.code {
        KeyCode::Esc => {
            state.mode = AppMode::Normal;
            state.command_buffer.clear();
            state.needs_terminal_clear = true;
        }
        KeyCode::Backspace => {
            state.command_buffer.pop();
        }
        KeyCode::Char(c) => {
            state.command_buffer.push(c);
        }
        KeyCode::Enter => {
            let cmd = state.command_buffer.clone();
            state.command_buffer.clear();
            state.mode = AppMode::Normal;
            state.needs_terminal_clear = true;

            let mut args = cmd.split_whitespace();
            if let Some(cmd_name) = args.next() {
                match cmd_name {
                    "q" | "qa" | "wq" => {
                        state.is_running = false;
                    }
                    "newfolder" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty() {
                            state.library_config.folders.push(crate::config::Folder {
                                name,
                                is_open: true,
                                playlists: vec![],
                            });
                            state.save_library_config();
                            state.compute_library_view();
                        }
                    }
                    "sort" => {
                        if let Some(mode) = args.next() {
                            match mode {
                                "alpha" => {
                                    state.library_config.sort_mode =
                                        crate::config::SortMode::Alphabetical
                                }
                                "creator" => {
                                    state.library_config.sort_mode =
                                        crate::config::SortMode::Creator
                                }
                                _ => {
                                    state.library_config.sort_mode =
                                        crate::config::SortMode::Default
                                }
                            }
                            state.save_library_config();
                            state.compute_library_view();
                        }
                    }
                    "index" => {
                        if let Some(base_str) = args.next() {
                            if let Ok(base) = base_str.parse::<isize>() {
                                state.library_config.track_index_base = base;
                                state.save_library_config();
                                state.status_message =
                                    Some(format!("Track index base set to {}", base));
                            } else {
                                state.status_message =
                                    Some("Invalid index base, must be a number".to_string());
                            }
                        } else {
                            state.status_message = Some(format!(
                                "Current index base: {}",
                                state.library_config.track_index_base
                            ));
                        }
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "delfolder" => {
                        // Deletes currently selected folder
                        if state.active_view == crate::app::ActiveView::Library
                            && state.selected_playlist_index < state.library_view.len()
                            && let crate::models::LibraryNode::Folder(f) =
                                &state.library_view[state.selected_playlist_index]
                        {
                            let name = f.name.clone();
                            state.library_config.folders.retain(|fd| fd.name != name);
                            state.save_library_config();
                            state.compute_library_view();
                        }
                    }
                    "theme" => {
                        if let Some(theme_name) = args.next() {
                            if let Some(theme_config) = state.themes.get(theme_name) {
                                state.active_theme =
                                    crate::app::ResolvedTheme::from_theme(theme_config);
                                state.library_config.active_theme = Some(theme_name.to_string());
                                state.needs_terminal_clear = true;
                                state.save_library_config();
                                state.status_message = Some(format!("Theme: {}", theme_name));
                            } else {
                                let mut theme_names: Vec<&String> = state.themes.keys().collect();
                                theme_names.sort();
                                state.status_message = Some(format!(
                                    "Unknown theme '{}'. Available: {}",
                                    theme_name,
                                    theme_names
                                        .iter()
                                        .map(|s| s.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ));
                            }
                        } else {
                        }
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
                    }
                    "lang" => {
                        if let Some(lang_code) = args.next() {
                            if lang_code == "en"
                                || lang_code == "zh"
                                || lang_code == "zh-CN"
                                || lang_code == "zh-TW"
                            {
                                state.library_config.language = lang_code.to_string();
                                state.save_library_config();
                                state.status_message = Some(
                                    crate::i18n::t(
                                        "messages.language_set",
                                        &state.library_config.language,
                                    )
                                    .replace("{}", lang_code),
                                );
                            } else {
                                state.status_message = Some(
                                    crate::i18n::t(
                                        "messages.unknown_language",
                                        &state.library_config.language,
                                    )
                                    .replace("{}", lang_code),
                                );
                            }
                        } else {
                        }
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
                    }
                    "pixelate" => {
                        if let Some(pixel_str) = args.next() {
                            if let Ok(pixels) = pixel_str.parse::<u32>() {
                                state.library_config.cover_img_pixels = pixels;
                                state.save_library_config();
                                state.status_message =
                                    Some(format!("Pixelate effect set to {}", pixels));

                                // Transfer current track image to previous to prevent blanking during re-fetch
                                state.playback.previous_track_image =
                                    state.playback.playing_track_image.take();
                                state.playback.fetching_track_id = None;

                                if state.active_view == crate::app::ActiveView::TrackList {
                                    return Some(crate::events::AppEvent::ReloadHeaderImage);
                                }
                            } else {
                                state.status_message =
                                    Some("Invalid pixel value, must be a number".to_string());
                            }
                        } else {
                            state.status_message = Some(format!(
                                "Current pixelate value: {}",
                                state.library_config.cover_img_pixels
                            ));
                        }
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "search" => {
                        let query = args.collect::<Vec<&str>>().join(" ");
                        if !query.is_empty() {
                            state.search_context_query = query.clone();
                            state.status_message = Some(format!("Searching for '{}'...", query));
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(crate::events::AppEvent::GlobalSearch(query));
                        } else {
                            state.status_message = Some("Usage: search <query>".to_string());
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "album" => {
                        use crate::app::ActiveView;
                        let mut album_id_opt = None;
                        if state.active_view == ActiveView::TrackList {
                            if state.selected_track_index < state.tracks.len() {
                                album_id_opt =
                                    state.tracks[state.selected_track_index].album_id.clone();
                            }
                        } else if state.active_view == ActiveView::Queue {
                            if state.selected_track_index < state.queue.len() {
                                album_id_opt =
                                    state.queue[state.selected_track_index].album_id.clone();
                            }
                        } else if state.active_view == ActiveView::SearchResults
                            && state.active_search_tab == crate::app::SearchTab::Tracks
                            && state.selected_search_index < state.search_results.tracks.len()
                        {
                            album_id_opt = state.search_results.tracks[state.selected_search_index]
                                .album_id
                                .clone();
                        }

                        if let Some(album_id) = album_id_opt {
                            let context = TrackListContext::album(
                                album_id.clone(),
                                "Album".to_string(),
                                String::new(),
                                None,
                            );
                            state.begin_tracklist_load(context.clone());
                            return Some(AppEvent::LoadContextTracks(context));
                        } else {
                            state.status_message =
                                Some("No album available for this track".to_string());
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "newplaylist" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty() {
                            state.status_message = Some(format!("Creating playlist '{}'...", name));
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(AppEvent::CreatePlaylist(name));
                        }
                    }
                    "newlocalplaylist" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty() {
                            state.status_message =
                                Some(format!("Creating local playlist '{}'...", name));
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(AppEvent::CreateLocalPlaylist(name));
                        }
                    }
                    "localpath" => {
                        let path_text = command_remainder(&cmd, "localpath");
                        if path_text.is_empty() {
                            state.status_message =
                                Some("Usage: localpath <absolute-folder-path>".to_string());
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        } else {
                            let path = std::path::PathBuf::from(path_text);
                            if !path.is_absolute() {
                                state.status_message =
                                    Some("Local path must be absolute".to_string());
                                state.status_message_expiry = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                                );
                            } else if !path.is_dir() {
                                state.status_message =
                                    Some("Local path must be an existing directory".to_string());
                                state.status_message_expiry = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                                );
                            } else {
                                state.library_config.local_music_dir = Some(path.clone());
                                state.save_library_config();
                                state.compute_library_view();
                                state.status_message =
                                    Some(format!("Scanning local music in {}...", path.display()));
                                state.status_message_expiry = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                                );
                                return Some(AppEvent::ScanLocalLibrary(path));
                            }
                        }
                    }
                    "rescanlocal" => {
                        if let Some(path) = state.library_config.local_music_dir.clone() {
                            state.status_message =
                                Some(format!("Rescanning local music in {}...", path.display()));
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(AppEvent::RescanLocalLibrary);
                        } else {
                            state.status_message =
                                Some("No local music path configured".to_string());
                            state.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "rename" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty()
                            && state.active_view == crate::app::ActiveView::Library
                            && let Some(node) =
                                state.library_view.get(state.selected_playlist_index)
                        {
                            match node {
                                crate::models::LibraryNode::Playlist { playlist, .. } => {
                                    return Some(AppEvent::RenamePlaylist(
                                        playlist.id.clone(),
                                        name,
                                    ));
                                }
                                crate::models::LibraryNode::Folder(f) => {
                                    let old_name = f.name.clone();
                                    if let Some(idx) = state
                                        .library_config
                                        .folders
                                        .iter()
                                        .position(|fd| fd.name == old_name)
                                    {
                                        state.library_config.folders[idx].name = name.clone();
                                    }
                                    state.save_library_config();
                                    state.compute_library_view();
                                    state.status_message =
                                        Some(format!("Renamed folder to '{}'", name));
                                    state.status_message_expiry = Some(
                                        std::time::Instant::now()
                                            + std::time::Duration::from_secs(3),
                                    );
                                }
                            }
                        }
                    }
                    "queue" => {
                        state.active_view = crate::app::ActiveView::Queue;
                        state.selected_queue_index = 0;
                        return Some(crate::events::AppEvent::FetchQueue);
                    }
                    "vis" => {
                        let mut next_val = !state.library_config.enable_visualizer;
                        if let Some(flag) = &state.playback.enable_visualizer {
                            let current = flag.load(std::sync::atomic::Ordering::Relaxed);
                            next_val = !current;
                            flag.store(next_val, std::sync::atomic::Ordering::Relaxed);
                        }
                        state.library_config.enable_visualizer = next_val;
                        state.save_library_config();
                        state.status_message = Some(if next_val {
                            "Visualizer: on".to_string()
                        } else {
                            "Visualizer: off".to_string()
                        });
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "visbins" => {
                        if let Some(bins_str) = args.next() {
                            if let Ok(bins) = bins_str.parse::<usize>() {
                                if bins >= 5 && bins <= 32 {
                                    state.vis_bins = bins;
                                    state.library_config.vis_bins = bins;
                                    state.save_library_config();
                                    state.status_message =
                                        Some(format!("Visualizer bins set to {}", bins));
                                } else {
                                    state.status_message =
                                        Some("Bins must be between 5 and 32".to_string());
                                }
                            } else {
                                state.status_message = Some("Invalid number".to_string());
                            }
                        } else {
                            state.status_message =
                                Some(format!("Current visbins: {}", state.vis_bins));
                        }
                        state.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localpath_remainder_preserves_spaces() {
        assert_eq!(
            command_remainder("localpath C:\\Users\\sun\\Music Folder", "localpath"),
            "C:\\Users\\sun\\Music Folder"
        );
    }

    #[test]
    fn localpath_remainder_trims_outer_whitespace() {
        assert_eq!(
            command_remainder("  localpath   /Users/sun/Music Library  ", "localpath"),
            "/Users/sun/Music Library"
        );
    }

    #[test]
    fn newlocalplaylist_command_emits_local_playlist_event() {
        let mut state = AppState::new();
        state.command_buffer = "newlocalplaylist Road Mix".to_string();
        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);

        let Some(AppEvent::CreateLocalPlaylist(name)) = handle_key(&mut state, &key) else {
            panic!("expected CreateLocalPlaylist");
        };

        assert_eq!(name, "Road Mix");
    }
}
