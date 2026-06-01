use crate::app::{AppMode, AppState};
use crate::events::AppEvent;
use crossterm::event::{KeyCode, KeyEvent};

fn generate_command_suggestions(state: &AppState) -> Vec<String> {
    let commands = vec!["q", "qa", "wq", "folder", "delfolder", "sort", "index", "theme", "search", "queue"];
    let mut parts = state.command_buffer.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next();

    if let Some(arg_str) = arg {
        match cmd {
            "theme" => {
                let mut themes: Vec<String> = state.themes.keys().cloned().collect();
                themes.sort();
                themes.into_iter().filter(|t| t.starts_with(arg_str)).collect()
            }
            "sort" => {
                let options = vec!["alpha".to_string(), "creator".to_string()];
                options.into_iter().filter(|o| o.starts_with(arg_str)).collect()
            }
            _ => vec![],
        }
    } else {
        commands.into_iter().filter(|c| c.starts_with(cmd)).map(String::from).collect()
    }
}

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            if state.command_suggestions.is_empty() {
                state.command_suggestions = generate_command_suggestions(state);
                state.command_suggestion_index = if state.command_suggestions.is_empty() { None } else { Some(0) };
                state.command_base_buffer = state.command_buffer.clone();
            } else if let Some(idx) = state.command_suggestion_index {
                if key.code == KeyCode::Tab {
                    state.command_suggestion_index = Some((idx + 1) % state.command_suggestions.len());
                } else {
                    state.command_suggestion_index = Some((idx + state.command_suggestions.len() - 1) % state.command_suggestions.len());
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

            let mut args = cmd.split_whitespace();
            if let Some(cmd_name) = args.next() {
                match cmd_name {
                    "q" | "qa" | "wq" => {
                        state.is_running = false;
                    }
                    "folder" => {
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
                                state.status_message = Some(format!("Track index base set to {}", base));
                            } else {
                                state.status_message = Some("Invalid index base, must be a number".to_string());
                            }
                        } else {
                            state.status_message = Some(format!("Current index base: {}", state.library_config.track_index_base));
                        }
                        state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
                                        .into_iter()
                                        .map(String::as_str)
                                        .collect::<Vec<&str>>()
                                        .join(", ")
                                ));
                            }
                        } else {
                            let mut theme_names: Vec<&String> = state.themes.keys().collect();
                            theme_names.sort();
                            state.status_message = Some(format!(
                                "Themes: {}",
                                theme_names
                                    .into_iter()
                                    .map(String::as_str)
                                    .collect::<Vec<&str>>()
                                    .join(", ")
                            ));
                        }
                        state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "search" => {
                        let query = args.collect::<Vec<&str>>().join(" ");
                        if !query.is_empty() {
                            state.search_context_query = query.clone();
                            state.status_message = Some(format!("Searching for '{}'...", query));
                            state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(crate::events::AppEvent::GlobalSearch(query));
                        } else {
                            state.status_message = Some("Usage: search <query>".to_string());
                            state.status_message_expiry = Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "queue" => {
                        state.active_view = crate::app::ActiveView::Queue;
                        state.selected_queue_index = 0;
                        return Some(crate::events::AppEvent::FetchQueue);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    None
}
