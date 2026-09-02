//! The `:` command registry shared by both frontends.
//!
//! The TUI's command mode and the desktop's command bar both edit
//! `state.ui.command_buffer` and call [`submit`] on Enter; Tab completion goes through
//! [`cycle_suggestion`]. Frontends own only the key handling — everything the commands *do*
//! lives here so `:sort`, `:open`, `:theme` and friends behave identically everywhere.

use crate::app::{AppMode, AppState};
use crate::events::AppEvent;
use crate::models::TrackListContext;

/// Every `:` command, as `(usage, description)`.
///
/// This is the single source for both Tab completion and the desktop's help overlay — adding a
/// command here makes it complete *and* documented, so the two cannot drift apart. The name is
/// the first whitespace-separated token of the usage string.
pub const COMMANDS: &[(&str, &str)] = &[
    ("q", "Quit"),
    ("qa", "Quit"),
    ("wq", "Quit"),
    ("newfolder <name>", "Create a folder to organise playlists"),
    ("delfolder", "Delete the selected folder"),
    ("sort <mode>", "Sort the library, or the loaded track list"),
    ("index <n>", "Track numbering base (0 or 1)"),
    ("theme <name>", "Switch theme"),
    ("search <query>", "Search Spotify and local tracks"),
    ("queue", "Open the queue"),
    ("clearqueue", "Clear the manually queued tracks"),
    ("vis", "Toggle the audio visualizer"),
    ("visbins <5-32>", "Visualizer frequency bands"),
    ("album", "Jump to the selected track's album"),
    ("lang <en|zh-CN|zh-TW>", "Switch language"),
    ("newplaylist <name>", "Create a Spotify playlist"),
    ("newlocalplaylist <name>", "Create a local playlist"),
    ("localpath <abs-path>", "Set and scan the local music folder"),
    ("rescanlocal", "Rescan the local music folder"),
    ("spotifylogin", "Re-authenticate with Spotify"),
    ("rename <name>", "Rename the selected playlist or folder"),
    ("pixelate <n>", "Retro pixelation on cover art; 0 disables"),
    ("backdrop <name>", "Immersive view backdrop (desktop)"),
    ("thumbs [on|off]", "Cover thumbnails in the sidebar"),
    ("tray [on|off]", "Close button hides echo to the tray (desktop)"),
    ("seek <s|+s|-s>", "Seek to, or by, a number of seconds"),
    ("sleep <30m|1h|off>", "Pause playback after a delay"),
    ("mute", "Mute, or restore the previous volume"),
    ("open [url|uri]", "Open a Spotify link, or read the clipboard"),
    ("relative <on|off|toggle>", "Vim-style relative line numbers"),
    ("range <short|medium|long>", "Top tracks/artists time range"),
    ("redraw", "Clear and redraw (TUI only)"),
];

/// Just the command names, for completion matching.
fn command_names() -> Vec<&'static str> {
    COMMANDS
        .iter()
        .map(|(usage, _)| usage.split_whitespace().next().unwrap_or(usage))
        .collect()
}

fn generate_command_suggestions(state: &AppState) -> Vec<String> {
    let commands = command_names();
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
                    "default", "alpha", "creator", "original", "title", "artist", "album",
                    "duration", "added", "reverse",
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
            "range" => ["short", "medium", "long"]
                .into_iter()
                .filter(|o| o.starts_with(arg_str))
                .map(String::from)
                .collect(),
            "sleep" => ["15m", "30m", "45m", "1h", "90m", "off"]
                .into_iter()
                .filter(|o| o.starts_with(arg_str))
                .map(String::from)
                .collect(),
            "backdrop" => crate::config::BackdropMode::ALL
                .into_iter()
                .map(|mode| mode.name())
                .filter(|o| o.starts_with(arg_str))
                .map(String::from)
                .collect(),
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

/// Tab / shift-Tab: generate suggestions on the first press, then cycle through them,
/// rewriting the command buffer with the selected completion.
pub fn cycle_suggestion(state: &mut AppState, forward: bool) {
    if state.ui.command_suggestions.is_empty() {
        state.ui.command_suggestions = generate_command_suggestions(state);
        state.ui.command_suggestion_index = if state.ui.command_suggestions.is_empty() {
            None
        } else {
            Some(0)
        };
        state.ui.command_base_buffer = state.ui.command_buffer.clone();
    } else if let Some(idx) = state.ui.command_suggestion_index {
        if forward {
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
}

/// Any edit to the buffer invalidates the completion state.
pub fn clear_suggestions(state: &mut AppState) {
    state.ui.command_suggestions.clear();
    state.ui.command_suggestion_index = None;
    state.ui.command_base_buffer.clear();
}

/// Enter: consume the buffer, leave command mode and run the command. `:q` and friends set
/// `state.ui.is_running = false` — frontends check it after calling this and exit.
pub fn submit(state: &mut AppState) -> Option<AppEvent> {
    clear_suggestions(state);
    let cmd = state.ui.command_buffer.clone();
    state.ui.command_buffer.clear();
    state.ui.mode = AppMode::Normal;
    state.ui.needs_terminal_clear = true;
    execute(state, &cmd)
}

pub fn run(state: &mut AppState, cmd: &str) -> Option<AppEvent> {
    execute(state, cmd)
}

fn unquote_path(text: &str) -> &str {
    let text = text.trim();
    let unquoted = text
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|inner| inner.strip_suffix('\''))
        })
        .unwrap_or(text);
    unquoted.trim()
}

fn command_remainder<'a>(command: &'a str, command_name: &str) -> &'a str {
    command
        .trim()
        .strip_prefix(command_name)
        .map(str::trim)
        .unwrap_or_default()
}

/// `:sleep` argument: `off` clears the timer (`Some(None)`), `<n>m`/`<n>h`/bare minutes set
/// it; anything else is invalid (`None`).
fn parse_sleep_arg(arg: &str) -> Option<Option<std::time::Duration>> {
    let arg = arg.trim();
    if arg.eq_ignore_ascii_case("off") {
        return Some(None);
    }
    let (number, unit_secs) = match arg.chars().last() {
        Some('m') | Some('M') => (&arg[..arg.len() - 1], 60u64),
        Some('h') | Some('H') => (&arg[..arg.len() - 1], 3600u64),
        _ => (arg, 60u64),
    };
    let minutes: u64 = number.parse().ok().filter(|n| *n > 0)?;
    Some(Some(std::time::Duration::from_secs(minutes * unit_secs)))
}

fn set_status(state: &mut AppState, message: impl Into<String>) {
    state.ui.status_message = Some(message.into());
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
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
            let context =
                TrackListContext::album(album_id, "Spotify album".to_string(), String::new(), None);
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
            state.begin_artist_page_load(artist_id.clone(), "Spotify artist".to_string(), None);
            Some(AppEvent::LoadArtistPage {
                artist_id,
                artist_name: None,
                artist_image_url: None,
            })
        }
    }
}

fn execute(state: &mut AppState, cmd: &str) -> Option<AppEvent> {
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
                    set_status(state, "Usage: seek <seconds|+seconds|-seconds>");
                    return None;
                };
                let Ok(seconds) = value.parse::<i64>() else {
                    set_status(state, "Seek position must be a number of seconds");
                    return None;
                };
                let target = if value.starts_with('+') || value.starts_with('-') {
                    state.playback.seek_target(seconds)
                } else {
                    seconds
                        .max(0)
                        .saturating_mul(1_000)
                        .min(i64::from(state.playback.duration_ms))
                        as u32
                };
                if state.playback.playing_track_id.is_none()
                    || state.playback.duration_ms == 0
                {
                    set_status(state, "Nothing is currently seekable");
                    return None;
                }
                state.playback.set_optimistic_progress(target);
                return Some(AppEvent::SeekTo(target));
            }
            "range" => {
                let parsed = match args.next() {
                    Some("short") => Some(crate::models::TopItemsRange::Short),
                    Some("medium") => Some(crate::models::TopItemsRange::Medium),
                    Some("long") => Some(crate::models::TopItemsRange::Long),
                    _ => None,
                };
                let Some(range) = parsed else {
                    set_status(state, "Usage: range <short|medium|long>");
                    return None;
                };
                return crate::intent::set_top_items_range(state, range);
            }
            "mute" => {
                let volume = state.playback.toggle_mute_target();
                state.playback.volume = volume;
                state.save_volume();
                return Some(AppEvent::SetVolume(volume as u8));
            }
            "sleep" => {
                let arg = command_remainder(cmd, "sleep");
                let Some(duration) = parse_sleep_arg(arg) else {
                    set_status(state, "Usage: sleep <30m|1h|off>");
                    return None;
                };
                match duration {
                    Some(_) => set_status(state, format!("Sleep timer: {arg}")),
                    None => set_status(state, "Sleep timer off"),
                }
                return Some(AppEvent::SetSleepTimer { duration });
            }
            "open" => {
                let value = {
                    let remainder = command_remainder(cmd, "open");
                    if remainder.is_empty() {
                        match crate::platform::read_clipboard() {
                            Ok(value) => value,
                            Err(error) => {
                                set_status(
                                    state,
                                    format!("Unable to read clipboard: {error}"),
                                );
                                return None;
                            }
                        }
                    } else {
                        remainder.to_string()
                    }
                };
                let Some(target) = parse_spotify_target(&value) else {
                    set_status(
                        state,
                        "Expected a Spotify track, album, artist, or playlist URL/URI",
                    );
                    return None;
                };
                return open_spotify_target(state, target);
            }
            "relative" => {
                state.ui.library_config.relative_line_numbers = match args.next() {
                    Some("on") => true,
                    Some("off") => false,
                    Some("toggle") | None => !state.ui.library_config.relative_line_numbers,
                    Some(_) => {
                        set_status(state, "Usage: relative <on|off|toggle>");
                        return None;
                    }
                };
                state.save_library_config();
                set_status(
                    state,
                    format!(
                        "Relative line numbers {}",
                        if state.ui.library_config.relative_line_numbers {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                );
            }
            "backdrop" => {
                let names = crate::config::BackdropMode::ALL.map(|mode| mode.name()).join("|");
                match args.next().and_then(crate::config::BackdropMode::parse) {
                    Some(mode) => {
                        state.ui.library_config.immersive_backdrop = mode;
                        state.save_library_config();
                        set_status(state, format!("Backdrop: {}", mode.name()));
                    }
                    None => set_status(state, format!("Usage: backdrop <{names}>")),
                }
            }
            "redraw" => {
                state.ui.needs_terminal_clear = true;
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
                        // The manual order drag-and-drop builds in `playlist_order`. Without
                        // this there is no way back to it once alpha or creator is picked.
                        "default" => {
                            state.ui.library_config.sort_mode = crate::config::SortMode::Default
                        }
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
                                set_status(
                                    state,
                                    "Track sorting is available from a track list",
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
                                    set_status(state, "Track order reversed");
                                } else {
                                    state.sort_tracks(sort);
                                    set_status(state, format!("Tracks sorted by {mode}"));
                                }
                            }
                            return None;
                        }
                        _ => set_status(
                            state,
                            "Usage: sort <default|alpha|creator|original|title|artist|album|duration|added|reverse>",
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
                    if crate::intent::apply_theme(state, theme_name) {
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
                            return Some(AppEvent::ReloadHeaderImage);
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
            "thumbs" => {
                let enabled = match args.next() {
                    Some("on") => true,
                    Some("off") => false,
                    Some(other) => {
                        state.ui.status_message =
                            Some(format!("Usage: thumbs [on|off], got '{}'", other));
                        state.ui.status_message_expiry = Some(
                            std::time::Instant::now() + std::time::Duration::from_secs(3),
                        );
                        return None;
                    }
                    None => !state.ui.library_config.library_thumbnails,
                };
                state.set_library_thumbnails(enabled);
            }
            "tray" => {
                state.ui.library_config.close_to_tray = match args.next() {
                    Some("on") => true,
                    Some("off") => false,
                    None => !state.ui.library_config.close_to_tray,
                    Some(_) => {
                        set_status(state, "Usage: tray [on|off]");
                        return None;
                    }
                };
                state.save_library_config();
                set_status(
                    state,
                    if state.ui.library_config.close_to_tray {
                        "Close to tray on"
                    } else {
                        "Close to tray off"
                    },
                );
            }
            "search" => {
                let query = args.collect::<Vec<&str>>().join(" ");
                if let Some(event) = crate::intent::global_search(state, &query) {
                    return Some(event);
                }
                state.ui.status_message = Some("Usage: search <query>".to_string());
                state.ui.status_message_expiry =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
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
                let path_text = unquote_path(command_remainder(cmd, "localpath"));
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
                // Renames the selected sidebar node regardless of the active view — the
                // desktop's context menu reaches here while a track list is showing.
                let name = args.collect::<Vec<&str>>().join(" ");
                if !name.is_empty()
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
            "clearqueue" => return crate::intent::clear_queue(state),
            "queue" => {
                state.ui.active_view = crate::app::ActiveView::Queue;
                state.ui.selected_queue_index = 0;
                return Some(AppEvent::FetchQueue);
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
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_arg_parses_minutes_hours_and_off() {
        use std::time::Duration;
        assert_eq!(parse_sleep_arg("30m"), Some(Some(Duration::from_secs(30 * 60))));
        assert_eq!(parse_sleep_arg("2h"), Some(Some(Duration::from_secs(2 * 3600))));
        assert_eq!(parse_sleep_arg("45"), Some(Some(Duration::from_secs(45 * 60))));
        assert_eq!(parse_sleep_arg("off"), Some(None));
        assert_eq!(parse_sleep_arg("OFF"), Some(None));
        assert_eq!(parse_sleep_arg(""), None);
        assert_eq!(parse_sleep_arg("0m"), None);
        assert_eq!(parse_sleep_arg("soon"), None);
    }

    fn submit_command(state: &mut AppState, command: &str) -> Option<AppEvent> {
        state.ui.command_buffer = command.to_string();
        submit(state)
    }

    #[test]
    fn backdrop_sets_a_known_mode_and_lists_the_rest() {
        use crate::config::BackdropMode;
        let mut state = AppState::new();
        submit_command(&mut state, "backdrop nebula");
        assert_eq!(state.ui.library_config.immersive_backdrop, BackdropMode::Nebula);
        submit_command(&mut state, "backdrop plaid");
        assert_eq!(state.ui.library_config.immersive_backdrop, BackdropMode::Nebula);
        assert!(state.ui.status_message.as_deref().is_some_and(|m| m.contains("lights|mesh")));
        assert_eq!(BackdropMode::parse("lights"), Some(BackdropMode::Lights));
        assert_eq!(BackdropMode::parse("Lights"), None);
    }

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
    fn unquote_path_strips_surrounding_quotes_and_whitespace() {
        assert_eq!(
            unquote_path("\"C:\\Users\\user\\Music\""),
            "C:\\Users\\user\\Music"
        );
        assert_eq!(
            unquote_path("'/Users/user/Music Library'"),
            "/Users/user/Music Library"
        );
        assert_eq!(unquote_path("  /Users/user/Music  "), "/Users/user/Music");
        assert_eq!(unquote_path("\"/Users/user/Music"), "\"/Users/user/Music");
    }

    #[test]
    fn newlocalplaylist_command_emits_local_playlist_event() {
        let mut state = AppState::new();

        let Some(AppEvent::CreateLocalPlaylist(name)) =
            submit_command(&mut state, "newlocalplaylist Road Mix")
        else {
            panic!("expected CreateLocalPlaylist");
        };

        assert_eq!(name, "Road Mix");
    }

    #[test]
    fn spotifylogin_command_starts_authentication() {
        let mut state = AppState::new();

        assert!(matches!(
            submit_command(&mut state, "spotifylogin"),
            Some(AppEvent::StartAuth)
        ));
        assert!(state.ui.mode == AppMode::Authenticating);
    }

    #[test]
    fn every_documented_command_is_offered_by_completion() {
        let mut state = AppState::new();
        state.ui.command_buffer.clear();
        let suggestions = generate_command_suggestions(&state);
        for name in command_names() {
            assert!(
                suggestions.iter().any(|s| s == name),
                "{name} is documented but never completes"
            );
        }
    }

    #[test]
    fn command_names_are_unique_and_documented() {
        let names = command_names();
        for (index, name) in names.iter().enumerate() {
            assert!(!name.is_empty());
            assert!(
                !names[index + 1..].contains(name),
                "{name} is listed twice in COMMANDS"
            );
        }
        for (usage, description) in COMMANDS {
            assert!(!description.is_empty(), "{usage} has no description");
        }
    }

    #[test]
    fn quit_commands_stop_the_app() {
        for command in ["q", "qa", "wq"] {
            let mut state = AppState::new();
            assert!(submit_command(&mut state, command).is_none());
            assert!(!state.ui.is_running, "{command}");
        }
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

    #[test]
    fn tray_command_toggles_and_takes_on_off() {
        let mut state = AppState::new();
        state.ui.library_config.close_to_tray = true;
        assert!(submit_command(&mut state, "tray").is_none());
        assert!(!state.ui.library_config.close_to_tray);
        assert_eq!(state.ui.status_message.as_deref(), Some("Close to tray off"));
        assert!(submit_command(&mut state, "tray on").is_none());
        assert!(state.ui.library_config.close_to_tray);
        assert_eq!(state.ui.status_message.as_deref(), Some("Close to tray on"));
        assert!(submit_command(&mut state, "tray off").is_none());
        assert!(!state.ui.library_config.close_to_tray);
        assert!(submit_command(&mut state, "tray sideways").is_none());
        assert!(!state.ui.library_config.close_to_tray);
        assert_eq!(state.ui.status_message.as_deref(), Some("Usage: tray [on|off]"));
    }

    #[test]
    fn relative_command_status_expires() {
        let mut state = AppState::new();

        assert!(submit_command(&mut state, "relative on").is_none());
        assert_eq!(
            state.ui.status_message.as_deref(),
            Some("Relative line numbers enabled")
        );
        assert!(state.ui.status_message_expiry.is_some());
    }

    #[test]
    fn added_command_errors_expire() {
        for command in [
            "seek",
            "open invalid",
            "relative invalid",
            "sort invalid",
            "tray invalid",
        ] {
            let mut state = AppState::new();
            assert!(submit_command(&mut state, command).is_none(), "{command}");
            assert!(state.ui.status_message.is_some(), "{command}");
            assert!(state.ui.status_message_expiry.is_some(), "{command}");
        }
    }

    #[test]
    fn redraw_command_requests_terminal_clear() {
        let mut state = AppState::new();

        assert!(submit_command(&mut state, "redraw").is_none());
        assert!(state.ui.needs_terminal_clear);
    }

    #[test]
    fn submitting_returns_to_normal_mode_and_clears_the_buffer() {
        let mut state = AppState::new();
        state.ui.mode = AppMode::Command;

        submit_command(&mut state, "theme");

        assert_eq!(state.ui.mode, AppMode::Normal);
        assert!(state.ui.command_buffer.is_empty());
    }

    #[test]
    fn tab_completion_cycles_through_matching_commands() {
        let mut state = AppState::new();
        state.ui.command_buffer = "ne".to_string();

        cycle_suggestion(&mut state, true);
        assert_eq!(state.ui.command_buffer, "newfolder");
        cycle_suggestion(&mut state, true);
        assert_eq!(state.ui.command_buffer, "newplaylist");
        cycle_suggestion(&mut state, false);
        assert_eq!(state.ui.command_buffer, "newfolder");
    }
}
