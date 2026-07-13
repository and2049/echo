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
        "spotifylogin",
        "rename",
        "pixelate",
        "seek",
        "mute",
        "open",
    ];
    let mut parts = state.ui.command_buffer.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let arg = parts.next();

    if let Some(arg_str) = arg {
        match cmd {
            "theme" => {
                let mut themes: Vec<String> = state.ui.themes.keys().cloned().collect();
                themes.sort();
                themes
                    .into_iter()
                    .filter(|t| t.starts_with(arg_str))
                    .collect()
            }
            "sort" => {
                let options = vec![
                    "alpha", "creator", "original", "title", "artist", "album", "duration",
                    "added", "reverse",
                ]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>();
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum SpotifyTarget {
    Track(String),
    Album(String),
    Artist(String),
    Playlist(String),
}

fn parse_spotify_target(value: &str) -> Option<SpotifyTarget> {
    let value = value.trim();
    let (kind, id) = if let Some(uri) = value.strip_prefix("spotify:") {
        let mut parts = uri.split(':');
        (parts.next()?, parts.next()?)
    } else {
        let path = value
            .strip_prefix("https://open.spotify.com/")
            .or_else(|| value.strip_prefix("http://open.spotify.com/"))?;
        let mut parts = path.split('/');
        (parts.next()?, parts.next()?.split(['?', '#']).next()?)
    };
    if id.is_empty() || !id.chars().all(|character| character.is_ascii_alphanumeric()) {
        return None;
    }
    Some(match kind {
        "track" => SpotifyTarget::Track(id.to_string()),
        "album" => SpotifyTarget::Album(id.to_string()),
        "artist" => SpotifyTarget::Artist(id.to_string()),
        "playlist" => SpotifyTarget::Playlist(id.to_string()),
        _ => return None,
    })
}

fn open_spotify_target(state: &mut AppState, target: SpotifyTarget) -> Option<AppEvent> {
    match target {
        SpotifyTarget::Track(track_id) => Some(AppEvent::PlayTrack {
            target: crate::models::PlaybackTarget::SpotifyTrack {
                track_id: track_id.clone(),
            },
            track_id,
            title: String::new(),
            artist: String::new(),
            duration_ms: 0,
            image_url: None,
            album_id: None,
        }),
        SpotifyTarget::Album(album_id) => {
            let context = TrackListContext::album(
                album_id,
                "Spotify album".to_string(),
                String::new(),
                None,
            );
            state.begin_tracklist_load(context.clone());
            Some(AppEvent::LoadContextTracks(context))
        }
        SpotifyTarget::Playlist(playlist_id) => {
            let context = TrackListContext::playlist(
                playlist_id,
                "Spotify playlist".to_string(),
                String::new(),
                String::new(),
                None,
            );
            state.begin_tracklist_load(context.clone());
            Some(AppEvent::LoadContextTracks(context))
        }
        SpotifyTarget::Artist(artist_id) => {
            state.begin_artist_page_load(
                artist_id.clone(),
                "Spotify artist".to_string(),
                None,
            );
            Some(AppEvent::LoadArtistPage {
                artist_id,
                artist_name: None,
                artist_image_url: None,
            })
        }
    }
}

pub fn handle_key(state: &mut AppState, key: &KeyEvent) -> Option<AppEvent> {
    match key.code {
        KeyCode::Tab | KeyCode::BackTab => {
            if state.ui.command_suggestions.is_empty() {
                state.ui.command_suggestions = generate_command_suggestions(state);
                state.ui.command_suggestion_index = if state.ui.command_suggestions.is_empty() {
                    None
                } else {
                    Some(0)
                };
                state.ui.command_base_buffer = state.ui.command_buffer.clone();
            } else if let Some(idx) = state.ui.command_suggestion_index {
                if key.code == KeyCode::Tab {
                    state.ui.command_suggestion_index =
                        Some((idx + 1) % state.ui.command_suggestions.len());
                } else {
                    state.ui.command_suggestion_index = Some(
                        (idx + state.ui.command_suggestions.len() - 1)
                            % state.ui.command_suggestions.len(),
                    );
                }
            }

            if let Some(idx) = state.ui.command_suggestion_index {
                let suggestion = &state.ui.command_suggestions[idx];
                let mut parts = state.ui.command_base_buffer.splitn(2, ' ');
                let cmd = parts.next().unwrap_or("");
                let arg = parts.next();

                if arg.is_some() {
                    state.ui.command_buffer = format!("{} {}", cmd, suggestion);
                } else {
                    state.ui.command_buffer = suggestion.clone();
                }
            }
            return None;
        }
        _ => {
            state.ui.command_suggestions.clear();
            state.ui.command_suggestion_index = None;
            state.ui.command_base_buffer.clear();
        }
    }

    match key.code {
        KeyCode::Esc => {
            state.ui.mode = AppMode::Normal;
            state.ui.command_buffer.clear();
            state.ui.needs_terminal_clear = true;
        }
        KeyCode::Backspace => {
            state.ui.command_buffer.pop();
        }
        KeyCode::Char(c) => {
            state.ui.command_buffer.push(c);
        }
        KeyCode::Enter => {
            let cmd = state.ui.command_buffer.clone();
            state.ui.command_buffer.clear();
            state.ui.mode = AppMode::Normal;
            state.ui.needs_terminal_clear = true;

            let mut args = cmd.split_whitespace();
            if let Some(cmd_name) = args.next() {
                match cmd_name {
                    "q" | "qa" | "wq" => {
                        state.ui.is_running = false;
                    }
                    "spotifylogin" => {
                        state.ui.mode = AppMode::Authenticating;
                        return Some(AppEvent::StartAuth);
                    }
                    "seek" => {
                        let Some(value) = args.next() else {
                            state.ui.status_message = Some("Usage: seek <seconds|+seconds|-seconds>".to_string());
                            state.ui.status_message_expiry = Some(
                                std::time::Instant::now() + std::time::Duration::from_secs(3),
                            );
                            return None;
                        };
                        let Ok(seconds) = value.parse::<i64>() else {
                            state.ui.status_message = Some("Seek position must be a number of seconds".to_string());
                            state.ui.status_message_expiry = Some(
                                std::time::Instant::now() + std::time::Duration::from_secs(3),
                            );
                            return None;
                        };
                        let target = if value.starts_with('+') || value.starts_with('-') {
                            state.playback.seek_target(seconds)
                        } else {
                            seconds.max(0).saturating_mul(1_000).min(i64::from(state.playback.duration_ms)) as u32
                        };
                        if state.playback.playing_track_id.is_none() || state.playback.duration_ms == 0 {
                            state.ui.status_message = Some("Nothing is currently seekable".to_string());
                            return None;
                        }
                        state.playback.set_optimistic_progress(target);
                        return Some(AppEvent::SeekTo(target));
                    }
                    "mute" => {
                        let volume = state.playback.toggle_mute_target();
                        state.playback.volume = volume;
                        state.save_volume();
                        return Some(AppEvent::SetVolume(volume as u8));
                    }
                    "open" => {
                        let value = {
                            let remainder = command_remainder(&cmd, "open");
                            if remainder.is_empty() {
                                match crate::platform::read_clipboard() {
                                    Ok(value) => value,
                                    Err(error) => {
                                        state.ui.status_message = Some(format!(
                                            "Unable to read clipboard: {error}"
                                        ));
                                        return None;
                                    }
                                }
                            } else {
                                remainder.to_string()
                            }
                        };
                        let Some(target) = parse_spotify_target(&value) else {
                            state.ui.status_message = Some(
                                "Expected a Spotify track, album, artist, or playlist URL/URI"
                                    .to_string(),
                            );
                            return None;
                        };
                        return open_spotify_target(state, target);
                    }
                    "newfolder" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty() {
                            state.ui.library_config.folders.push(crate::config::Folder {
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
                                    state.ui.library_config.sort_mode =
                                        crate::config::SortMode::Alphabetical
                                }
                                "creator" => {
                                    state.ui.library_config.sort_mode =
                                        crate::config::SortMode::Creator
                                }
                                "original" | "title" | "artist" | "album" | "duration"
                                | "added" | "reverse" => {
                                    if state.ui.active_view != crate::app::ActiveView::TrackList {
                                        state.ui.status_message = Some(
                                            "Track sorting is available from a track list".to_string(),
                                        );
                                    } else {
                                        let sort = match mode {
                                            "title" => crate::app::TrackSort::Title,
                                            "artist" => crate::app::TrackSort::Artist,
                                            "album" => crate::app::TrackSort::Album,
                                            "duration" => crate::app::TrackSort::Duration,
                                            "added" => crate::app::TrackSort::Added,
                                            _ => crate::app::TrackSort::Original,
                                        };
                                        if mode == "reverse" {
                                            state.reverse_tracks();
                                            state.ui.status_message =
                                                Some("Track order reversed".to_string());
                                        } else {
                                            state.sort_tracks(sort);
                                            state.ui.status_message =
                                                Some(format!("Tracks sorted by {mode}"));
                                        }
                                    }
                                    state.ui.status_message_expiry = Some(
                                        std::time::Instant::now()
                                            + std::time::Duration::from_secs(3),
                                    );
                                    return None;
                                }
                                _ => state.ui.status_message = Some(
                                    "Usage: sort <alpha|creator|original|title|artist|album|duration|added|reverse>"
                                        .to_string(),
                                ),
                            }
                            state.save_library_config();
                            state.compute_library_view();
                        }
                    }
                    "index" => {
                        if let Some(base_str) = args.next() {
                            if let Ok(base) = base_str.parse::<isize>() {
                                state.ui.library_config.track_index_base = base;
                                state.save_library_config();
                                state.ui.status_message =
                                    Some(format!("Track index base set to {}", base));
                            } else {
                                state.ui.status_message =
                                    Some("Invalid index base, must be a number".to_string());
                            }
                        } else {
                            state.ui.status_message = Some(format!(
                                "Current index base: {}",
                                state.ui.library_config.track_index_base
                            ));
                        }
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "delfolder" => {
                        // Deletes currently selected folder
                        if state.ui.active_view == crate::app::ActiveView::Library
                            && state.ui.selected_playlist_index < state.data.library_view.len()
                            && let crate::models::LibraryNode::Folder(f) =
                                &state.data.library_view[state.ui.selected_playlist_index]
                        {
                            let name = f.name.clone();
                            state.ui.library_config.folders.retain(|fd| fd.name != name);
                            state.save_library_config();
                            state.compute_library_view();
                        }
                    }
                    "theme" => {
                        if let Some(theme_name) = args.next() {
                            if let Some(theme_config) = state.ui.themes.get(theme_name) {
                                state.ui.active_theme =
                                    crate::app::ResolvedTheme::from_theme(theme_config);
                                state.ui.library_config.active_theme = Some(theme_name.to_string());
                                state.ui.needs_terminal_clear = true;
                                state.save_library_config();
                                state.ui.status_message = Some(format!("Theme: {}", theme_name));
                            } else {
                                let mut theme_names: Vec<&String> =
                                    state.ui.themes.keys().collect();
                                theme_names.sort();
                                state.ui.status_message = Some(format!(
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
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
                    }
                    "lang" => {
                        if let Some(lang_code) = args.next() {
                            if lang_code == "en"
                                || lang_code == "zh"
                                || lang_code == "zh-CN"
                                || lang_code == "zh-TW"
                            {
                                state.ui.library_config.language = lang_code.to_string();
                                state.save_library_config();
                                state.ui.status_message = Some(
                                    crate::i18n::t(
                                        "messages.language_set",
                                        &state.ui.library_config.language,
                                    )
                                    .replace("{}", lang_code),
                                );
                            } else {
                                state.ui.status_message = Some(
                                    crate::i18n::t(
                                        "messages.unknown_language",
                                        &state.ui.library_config.language,
                                    )
                                    .replace("{}", lang_code),
                                );
                            }
                        } else {
                        }
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
                    }
                    "pixelate" => {
                        if let Some(pixel_str) = args.next() {
                            if let Ok(pixels) = pixel_str.parse::<u32>() {
                                state.ui.library_config.cover_img_pixels = pixels;
                                state.save_library_config();
                                state.ui.status_message =
                                    Some(format!("Pixelate effect set to {}", pixels));

                                // Transfer current track image to previous to prevent blanking during re-fetch
                                state.playback.previous_track_image =
                                    state.playback.playing_track_image.take();
                                state.playback.fetching_track_id = None;

                                if state.ui.active_view == crate::app::ActiveView::TrackList {
                                    return Some(crate::events::AppEvent::ReloadHeaderImage);
                                }
                            } else {
                                state.ui.status_message =
                                    Some("Invalid pixel value, must be a number".to_string());
                            }
                        } else {
                            state.ui.status_message = Some(format!(
                                "Current pixelate value: {}",
                                state.ui.library_config.cover_img_pixels
                            ));
                        }
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "search" => {
                        let query = args.collect::<Vec<&str>>().join(" ");
                        if !query.is_empty() {
                            state.ui.search_context_query = query.clone();
                            state.ui.status_message = Some(format!("Searching for '{}'...", query));
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(crate::events::AppEvent::GlobalSearch(query));
                        } else {
                            state.ui.status_message = Some("Usage: search <query>".to_string());
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "album" => {
                        use crate::app::ActiveView;
                        let mut album_id_opt = None;
                        if state.ui.active_view == ActiveView::TrackList {
                            if state.ui.selected_track_index < state.data.tracks.len() {
                                album_id_opt = state.data.tracks[state.ui.selected_track_index]
                                    .album_id
                                    .clone();
                            }
                        } else if state.ui.active_view == ActiveView::Queue {
                            if state.ui.selected_track_index < state.data.queue.len() {
                                album_id_opt = state.data.queue[state.ui.selected_track_index]
                                    .album_id
                                    .clone();
                            }
                        } else if state.ui.active_view == ActiveView::SearchResults
                            && state.ui.active_search_tab == crate::app::SearchTab::Tracks
                            && state.ui.selected_search_index
                                < state.data.search_results.tracks.len()
                        {
                            album_id_opt = state.data.search_results.tracks
                                [state.ui.selected_search_index]
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
                            state.ui.status_message =
                                Some("No album available for this track".to_string());
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "newplaylist" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty() {
                            state.ui.status_message =
                                Some(format!("Creating playlist '{}'...", name));
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(AppEvent::CreatePlaylist(name));
                        }
                    }
                    "newlocalplaylist" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty() {
                            state.ui.status_message =
                                Some(format!("Creating local playlist '{}'...", name));
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(AppEvent::CreateLocalPlaylist(name));
                        }
                    }
                    "localpath" => {
                        let path_text = command_remainder(&cmd, "localpath");
                        if path_text.is_empty() {
                            state.ui.status_message =
                                Some("Usage: localpath <absolute-folder-path>".to_string());
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        } else {
                            let path = std::path::PathBuf::from(path_text);
                            if !path.is_absolute() {
                                state.ui.status_message =
                                    Some("Local path must be absolute".to_string());
                                state.ui.status_message_expiry = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                                );
                            } else if !path.is_dir() {
                                state.ui.status_message =
                                    Some("Local path must be an existing directory".to_string());
                                state.ui.status_message_expiry = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                                );
                            } else {
                                state.ui.library_config.local_music_dir = Some(path.clone());
                                state.save_library_config();
                                state.compute_library_view();
                                state.ui.status_message =
                                    Some(format!("Scanning local music in {}...", path.display()));
                                state.ui.status_message_expiry = Some(
                                    std::time::Instant::now() + std::time::Duration::from_secs(3),
                                );
                                return Some(AppEvent::ScanLocalLibrary(path));
                            }
                        }
                    }
                    "rescanlocal" => {
                        if let Some(path) = state.ui.library_config.local_music_dir.clone() {
                            state.ui.status_message =
                                Some(format!("Rescanning local music in {}...", path.display()));
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                            return Some(AppEvent::RescanLocalLibrary);
                        } else {
                            state.ui.status_message =
                                Some("No local music path configured".to_string());
                            state.ui.status_message_expiry =
                                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                        }
                    }
                    "rename" => {
                        let name = args.collect::<Vec<&str>>().join(" ");
                        if !name.is_empty()
                            && state.ui.active_view == crate::app::ActiveView::Library
                            && let Some(node) = state
                                .data
                                .library_view
                                .get(state.ui.selected_playlist_index)
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
                                        .ui
                                        .library_config
                                        .folders
                                        .iter()
                                        .position(|fd| fd.name == old_name)
                                    {
                                        state.ui.library_config.folders[idx].name = name.clone();
                                    }
                                    state.save_library_config();
                                    state.compute_library_view();
                                    state.ui.status_message =
                                        Some(format!("Renamed folder to '{}'", name));
                                    state.ui.status_message_expiry = Some(
                                        std::time::Instant::now()
                                            + std::time::Duration::from_secs(3),
                                    );
                                }
                            }
                        }
                    }
                    "queue" => {
                        state.ui.active_view = crate::app::ActiveView::Queue;
                        state.ui.selected_queue_index = 0;
                        return Some(crate::events::AppEvent::FetchQueue);
                    }
                    "vis" => {
                        let mut next_val = !state.ui.library_config.enable_visualizer;
                        if let Some(flag) = &state.playback.enable_visualizer {
                            let current = flag.load(std::sync::atomic::Ordering::Relaxed);
                            next_val = !current;
                            flag.store(next_val, std::sync::atomic::Ordering::Relaxed);
                        }
                        state.ui.library_config.enable_visualizer = next_val;
                        state.save_library_config();
                        state.ui.status_message = Some(if next_val {
                            "Visualizer: on".to_string()
                        } else {
                            "Visualizer: off".to_string()
                        });
                        state.ui.status_message_expiry =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
                    }
                    "visbins" => {
                        if let Some(bins_str) = args.next() {
                            if let Ok(bins) = bins_str.parse::<usize>() {
                                if bins >= 5 && bins <= 32 {
                                    state.ui.vis_bins = bins;
                                    state.ui.library_config.vis_bins = bins;
                                    state.save_library_config();
                                    state.ui.status_message =
                                        Some(format!("Visualizer bins set to {}", bins));
                                } else {
                                    state.ui.status_message =
                                        Some("Bins must be between 5 and 32".to_string());
                                }
                            } else {
                                state.ui.status_message = Some("Invalid number".to_string());
                            }
                        } else {
                            state.ui.status_message =
                                Some(format!("Current visbins: {}", state.ui.vis_bins));
                        }
                        state.ui.status_message_expiry =
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
        state.ui.command_buffer = "newlocalplaylist Road Mix".to_string();
        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);

        let Some(AppEvent::CreateLocalPlaylist(name)) = handle_key(&mut state, &key) else {
            panic!("expected CreateLocalPlaylist");
        };

        assert_eq!(name, "Road Mix");
    }

    #[test]
    fn spotifylogin_command_starts_authentication() {
        let mut state = AppState::new();
        state.ui.command_buffer = "spotifylogin".to_string();
        let key = KeyEvent::new(KeyCode::Enter, crossterm::event::KeyModifiers::NONE);

        assert!(matches!(
            handle_key(&mut state, &key),
            Some(AppEvent::StartAuth)
        ));
        assert!(state.ui.mode == AppMode::Authenticating);
    }

    #[test]
    fn parses_spotify_urls_and_uris_without_network_access() {
        assert_eq!(
            parse_spotify_target("spotify:track:abc123"),
            Some(SpotifyTarget::Track("abc123".to_string()))
        );
        assert_eq!(
            parse_spotify_target("https://open.spotify.com/playlist/list123?si=value"),
            Some(SpotifyTarget::Playlist("list123".to_string()))
        );
        assert_eq!(parse_spotify_target("https://example.com/track/abc"), None);
    }
}
