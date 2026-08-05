//! Frontend-neutral user intents.
//!
//! "The user activated library row N" or "asked to play track N" means the same thing whether it
//! arrived as an Enter keypress in the TUI or a click in the desktop app: mutate selection state
//! and return the event the worker should receive. The frontends translate their input idioms
//! into these calls and send whatever comes back over `app_tx`.

use crate::app::{ActiveView, AppState, SearchTab};
use crate::events::AppEvent;
use crate::models::{
    Artist, LibraryNode, PlaybackTarget, SearchTrack, Track, TrackListContext, TrackSource,
};

/// Activates row `index` of the playlists sidebar (the `library_view` tree): opens playlists,
/// shows local collections, toggles folders.
pub fn open_library_entry(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let node = state.data.library_view.get(index).cloned()?;
    state.ui.selected_playlist_index = index;
    let context = match node {
        LibraryNode::Playlist { playlist, .. } => {
            if playlist.id == "local-library" {
                state.show_local_library();
                None
            } else if playlist.id.starts_with("local-playlist:") {
                state.show_local_playlist(&playlist.id, playlist.name.clone());
                None
            } else {
                Some(TrackListContext::playlist(
                    playlist.id.clone(),
                    playlist.name.clone(),
                    playlist.owner.clone(),
                    playlist.owner_id.clone(),
                    playlist.image_url.clone(),
                ))
            }
        }
        LibraryNode::Folder(f) => {
            let folder_name = f.name.clone();
            if let Some(folder) = state
                .ui
                .library_config
                .folders
                .iter_mut()
                .find(|fd| fd.name == folder_name)
            {
                folder.is_open = !folder.is_open;
            }
            state.save_library_config();
            state.compute_library_view();
            None
        }
    };

    let context = context?;
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Activates row `index` of the saved-albums list.
pub fn open_album(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let album = state.data.saved_albums.get(index)?;
    state.ui.selected_playlist_index = index;
    let context = TrackListContext::album(
        album.id.clone(),
        album.name.clone(),
        album.artists.clone(),
        album.image_url.clone(),
    );
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Plays track `index` of the current track list, selecting it first.
pub fn play_track_at(state: &mut AppState, index: usize) -> Option<AppEvent> {
    if index >= state.data.tracks.len() {
        return None;
    }
    state.ui.selected_track_index = index;
    let track = state.data.tracks.get(index)?;
    let context = state.data.active_tracklist_context.as_ref()?;
    let target = if track.source == TrackSource::Local {
        let tracks: Vec<_> = state
            .data
            .tracks
            .iter()
            .filter(|track| track.source == TrackSource::Local)
            .cloned()
            .collect();
        let selected_index = tracks
            .iter()
            .position(|local_track| local_track.id == track.id)
            .unwrap_or(0);
        PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        }
    } else {
        context.playback_target_for_track(track)?
    };
    play_event_with_target(track, target)
}

/// The play event for `track` inside `context`, with no selection side effects.
pub fn play_event(track: &Track, context: &TrackListContext) -> Option<AppEvent> {
    let target = context.playback_target_for_track(track)?;
    play_event_with_target(track, target)
}

fn play_event_with_target(track: &Track, target: PlaybackTarget) -> Option<AppEvent> {
    Some(AppEvent::PlayTrack {
        target,
        track_id: track.id.clone(),
        title: track.name.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
    })
}

// Playback transport. Each applies the optimistic state flip the next SyncPlaybackState will
// confirm or correct, and returns the event for the worker.

pub fn toggle_playback(state: &mut AppState) -> AppEvent {
    state.playback.is_playing = !state.playback.is_playing;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());
    AppEvent::TogglePlayback(state.playback.is_playing)
}

pub fn next_track(state: &AppState) -> AppEvent {
    AppEvent::NextTrack {
        current_track_id: state.playback.playing_track_id.clone(),
    }
}

pub fn previous_track(state: &AppState) -> AppEvent {
    AppEvent::PreviousTrack {
        current_track_id: state.playback.playing_track_id.clone(),
    }
}

pub fn toggle_shuffle(state: &mut AppState) -> AppEvent {
    state.playback.is_shuffled = !state.playback.is_shuffled;
    AppEvent::ToggleShuffle(state.playback.is_shuffled)
}

/// Off → Track → Context → Off, Spotify's own cycle order.
pub fn cycle_repeat(state: &mut AppState) -> AppEvent {
    let mode = match state.playback.repeat_mode.as_str() {
        "Off" => "Track",
        "Track" => "Context",
        _ => "Off",
    };
    state.playback.repeat_mode = mode.to_string();
    AppEvent::SetRepeatMode(mode.to_string())
}

/// Seeks to an absolute position; `None` when nothing seekable is playing.
pub fn seek_to(state: &mut AppState, target_ms: u32) -> Option<AppEvent> {
    if state.playback.playing_track_id.is_none() || state.playback.duration_ms == 0 {
        return None;
    }
    state.playback.set_optimistic_progress(target_ms);
    Some(AppEvent::SeekTo(target_ms))
}

pub fn seek_by(state: &mut AppState, seconds: i64) -> Option<AppEvent> {
    seek_to(state, state.playback.seek_target(seconds))
}

pub fn set_volume(state: &mut AppState, volume: u8) -> AppEvent {
    state.playback.volume = volume as u32;
    state.save_volume();
    AppEvent::SetVolume(volume)
}

pub fn toggle_mute(state: &mut AppState) -> AppEvent {
    let volume = state.playback.toggle_mute_target();
    state.playback.volume = volume;
    state.save_volume();
    AppEvent::SetVolume(volume as u8)
}

/// Switches to the queue view and asks the worker for the live queue.
pub fn open_queue(state: &mut AppState) -> AppEvent {
    state.push_view_history();
    state.ui.active_view = crate::app::ActiveView::Queue;
    state.ui.selected_queue_index = 0;
    AppEvent::FetchQueue
}

/// Opens the device-picker modal and asks the worker for the device list.
pub fn open_device_picker(state: &mut AppState) -> AppEvent {
    state.ui.device_modal_open = true;
    state.ui.selected_device_index = 0;
    AppEvent::FetchDevices
}

/// Transfers playback to device `index` and closes the picker. `None` if the row doesn't exist
/// or the device has no id (restricted devices come back id-less from the API).
pub fn transfer_to_device(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let device = state.data.devices.get(index)?;
    state.ui.device_modal_open = false;
    let id = device.id.clone();
    (!id.is_empty()).then(|| AppEvent::TransferPlayback(id))
}

// Global search and result activation.

/// Kicks off a worker-side search across Spotify and the local library. `None` for an empty
/// query; the results arrive as `SearchResultsLoaded`, which switches to the results view.
pub fn global_search(state: &mut AppState, query: &str) -> Option<AppEvent> {
    let query = query.trim().to_string();
    if query.is_empty() {
        return None;
    }
    state.ui.search_context_query = query.clone();
    state.ui.status_message = Some(format!("Searching for '{query}'..."));
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    Some(AppEvent::GlobalSearch(query))
}

/// Activates row `index` of the search results under the active tab: plays a track, opens an
/// album, or enters an artist page. Local results resolve against the local library instead of
/// the API.
pub fn activate_search_result(state: &mut AppState, index: usize) -> Option<AppEvent> {
    state.ui.selected_search_index = index;
    match state.ui.active_search_tab {
        SearchTab::Tracks => {
            let track = state.data.search_results.tracks.get(index)?.clone();
            search_track_play_event(state, &track)
        }
        SearchTab::Albums => {
            let album = state.data.search_results.albums.get(index)?.clone();
            if album.id.starts_with("local-album:") {
                let album_name = album.name.clone();
                let tracks: Vec<_> = state
                    .data
                    .local_library
                    .to_tracks()
                    .into_iter()
                    .filter(|track| {
                        state
                            .data
                            .local_library
                            .tracks
                            .iter()
                            .find(|local| local.id == track.id)
                            .is_some_and(|local| local.album == album_name)
                    })
                    .collect();
                if !tracks.is_empty() {
                    state.show_generated_tracks(
                        tracks,
                        TrackListContext::generated(album.id.clone(), album.name.clone()),
                    );
                }
                return None;
            }
            let context = TrackListContext::album(
                album.id.clone(),
                album.name.clone(),
                album.artist.clone(),
                album.image_url.clone(),
            );
            state.begin_tracklist_load(context.clone());
            Some(AppEvent::LoadContextTracks(context))
        }
        SearchTab::Artists => {
            let artist = state.data.search_results.artists.get(index)?.clone();
            if artist.id.starts_with("local-artist:") {
                let artist_name = artist.name.clone();
                let tracks: Vec<_> = state
                    .data
                    .local_library
                    .to_tracks()
                    .into_iter()
                    .filter(|track| track.artist == artist_name)
                    .collect();
                if !tracks.is_empty() {
                    state.show_generated_tracks(
                        tracks,
                        TrackListContext::generated(artist.id.clone(), artist.name.clone()),
                    );
                }
                return None;
            }
            open_artist(state, artist)
        }
    }
}

fn search_track_play_event(state: &AppState, track: &SearchTrack) -> Option<AppEvent> {
    let target = if track.source == TrackSource::Local {
        // Local matches play as a context of all local results, so next/previous work.
        let tracks: Vec<_> = state
            .data
            .search_results
            .tracks
            .iter()
            .filter(|result| result.source == TrackSource::Local)
            .map(|t| Track {
                id: t.id.clone(),
                source: t.source,
                local_path: t.local_path.clone(),
                name: t.name.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                added_at: None,
                duration_ms: t.duration_ms,
                image_url: t.image_url.clone(),
                album_id: t.album_id.clone(),
                artist_id: t.artist_id.clone(),
            })
            .collect();
        let selected_index = tracks
            .iter()
            .position(|candidate| candidate.id == track.id)
            .unwrap_or(0);
        PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        }
    } else {
        PlaybackTarget::SpotifyTrack {
            track_id: track.id.clone(),
        }
    };

    Some(AppEvent::PlayTrack {
        target,
        track_id: track.id.clone(),
        title: track.name.clone(),
        artist: track.artist.clone(),
        duration_ms: track.duration_ms,
        image_url: track.image_url.clone(),
        album_id: track.album_id.clone(),
    })
}

// Artist pages.

/// Opens the followed-artists list, fetching it first when empty.
pub fn open_artist_list(state: &mut AppState) -> Option<AppEvent> {
    state.push_view_history();
    state.ui.active_view = ActiveView::ArtistList;
    state.ui.selected_artist_index = 0;
    state
        .data
        .followed_artists
        .is_empty()
        .then_some(AppEvent::FetchFollowedArtists)
}

/// Enters the artist page for followed artist `index`.
pub fn open_followed_artist(state: &mut AppState, index: usize) -> Option<AppEvent> {
    state.ui.selected_artist_index = index;
    let artist = state.data.followed_artists.get(index)?.clone();
    open_artist(state, artist)
}

fn open_artist(state: &mut AppState, artist: Artist) -> Option<AppEvent> {
    let artist_id = artist.id.clone();
    let artist_name = artist.name.clone();
    let artist_image_url = artist.image_url.clone();
    state.begin_artist_page_load(
        artist_id.clone(),
        artist_name.clone(),
        artist_image_url.clone(),
    );
    Some(AppEvent::LoadArtistPage {
        artist_id,
        artist_name: Some(artist_name),
        artist_image_url,
    })
}

/// Opens album `index` of the current artist page as a track list.
pub fn open_artist_album(state: &mut AppState, index: usize) -> Option<AppEvent> {
    state.ui.artist_page_album_index = index;
    let data = state.data.artist_page_data.clone()?;
    let album = data.albums.get(index)?;
    let context = TrackListContext::album(
        album.id.clone(),
        album.name.clone(),
        album.artists.clone(),
        album.image_url.clone(),
    );
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Backs out of an artist page to the artist list, cancelling any in-flight page load.
pub fn back_to_artist_list(state: &mut AppState) -> AppEvent {
    state.ui.active_view = ActiveView::ArtistList;
    state.clear_pending_artist_page();
    AppEvent::CancelArtistPageLoad
}

/// Saves the pasted Spotify developer credentials and starts authentication. `None` while
/// either field is still empty.
pub fn submit_setup_credentials(state: &mut AppState) -> Option<AppEvent> {
    if state.ui.setup_client_id.is_empty() || state.ui.setup_client_secret.is_empty() {
        return None;
    }
    let mut config = crate::config::AppConfig::load();
    config.spotify_credentials = Some(crate::config::SpotifyCredentials {
        client_id: state.ui.setup_client_id.clone(),
        client_secret: state.ui.setup_client_secret.clone(),
    });
    let _ = config.save();

    state.ui.mode = crate::app::AppMode::Authenticating;
    Some(AppEvent::StartAuth)
}

/// Applies a loaded theme by name and persists the choice. False if the name is unknown.
pub fn apply_theme(state: &mut AppState, name: &str) -> bool {
    let Some(theme) = state.ui.themes.get(name) else {
        return false;
    };
    state.ui.active_theme = crate::theme::ResolvedTheme::from_theme(theme);
    state.ui.library_config.active_theme = Some(name.to_string());
    state.ui.needs_terminal_clear = true;
    state.save_library_config();
    true
}

// Browse nodes: generated lists that fetch on first use.

/// Opens the user's top tracks; fetches them first when none are cached yet.
pub fn open_top_tracks(state: &mut AppState) -> Option<AppEvent> {
    if state.data.top_tracks.is_empty() {
        return Some(AppEvent::FetchTopTracks);
    }
    state.show_generated_tracks(
        state.data.top_tracks.clone(),
        TrackListContext::generated("TOP_TRACKS", "Top Tracks"),
    );
    None
}

/// Opens the recently-played list; fetches it first when none is cached yet.
pub fn open_recently_played(state: &mut AppState) -> Option<AppEvent> {
    if state.data.recently_played.is_empty() {
        return Some(AppEvent::FetchRecentlyPlayed);
    }
    state.show_generated_tracks(
        state.data.recently_played.clone(),
        TrackListContext::generated("RECENTLY_PLAYED", "Recently Played"),
    );
    None
}

// Vim-style motions and toggles shared by both frontends.

/// `g c`: jump the selection (or the whole view) to whatever is currently playing.
pub fn jump_to_current_context(state: &mut AppState) -> Option<AppEvent> {
    let Some(track_id) = state.playback.playing_track_id.clone() else {
        state.ui.status_message = Some("Nothing is currently playing".to_string());
        return None;
    };
    if state.ui.active_view == ActiveView::TrackList
        && let Some(index) = state.data.tracks.iter().position(|track| track.id == track_id)
    {
        state.ui.selected_track_index = index;
        return None;
    }
    if state.playback.playing_track_source == Some(TrackSource::Local) {
        state.show_local_library();
        state.ui.selected_track_index = state
            .data
            .tracks
            .iter()
            .position(|track| track.id == track_id)
            .unwrap_or(0);
        return None;
    }
    if let Some(album_id) = state.playback.playing_track_album_id.clone() {
        let context = TrackListContext::album(
            album_id,
            "Current album".to_string(),
            state.playback.playing_track_artist.clone(),
            None,
        );
        state.begin_tracklist_load(context.clone());
        return Some(AppEvent::LoadContextTracks(context));
    }
    state.ui.status_message = Some("The current playback context is unavailable".to_string());
    None
}

/// `q`: append the selected track (track list or search tracks tab) to the playback queue.
pub fn queue_selected_track(state: &AppState) -> Option<AppEvent> {
    let track_id = match state.ui.active_view {
        ActiveView::TrackList => state
            .data
            .tracks
            .get(state.ui.selected_track_index)
            .map(|t| t.id.clone()),
        ActiveView::SearchResults if state.ui.active_search_tab == SearchTab::Tracks => state
            .data
            .search_results
            .tracks
            .get(state.ui.selected_search_index)
            .map(|t| t.id.clone()),
        _ => None,
    };
    track_id.map(|id| AppEvent::AddToQueue(vec![id]))
}

/// `m`: pin or unpin the selected sidebar playlist.
pub fn toggle_pin_selected(state: &mut AppState) {
    if state.ui.active_view != ActiveView::Library
        || state.ui.active_library_tab == crate::app::LibraryTab::Albums
    {
        return;
    }
    if state.ui.selected_playlist_index < state.data.library_view.len()
        && let LibraryNode::Playlist { playlist, .. } =
            &state.data.library_view[state.ui.selected_playlist_index]
    {
        let id = &playlist.id;
        if id == "LIKED_SONGS" || id == "local-library" {
            return;
        }
        if state.ui.library_config.pinned.contains(id) {
            state.ui.library_config.pinned.retain(|p| p != id);
        } else {
            state.ui.library_config.pinned.push(id.clone());
        }
        state.save_library_config();
        state.compute_library_view();
    }
}

/// `=`/`-`/`+`/`_`: step the volume by `delta`, clamped to 0–100.
pub fn adjust_volume(state: &mut AppState, delta: i32) -> AppEvent {
    let next = (state.playback.volume as i32 + delta).clamp(0, 100) as u8;
    set_volume(state, next)
}

/// Ctrl-L: toggle the inline lyric line in the playback bar, persisted like the TUI does.
pub fn toggle_condensed_lyrics(state: &mut AppState) {
    state.ui.condensed_lyrics_enabled = !state.ui.condensed_lyrics_enabled;
    let mut app_config = crate::config::AppConfig::load();
    app_config.library.condensed_lyrics_enabled = state.ui.condensed_lyrics_enabled;
    let _ = app_config.save();
}

/// `R`: refresh whatever the active view shows (artist albums or the library lists).
pub fn refresh_view(state: &mut AppState) -> Option<AppEvent> {
    if state.ui.active_view == ActiveView::ArtistPage
        && let Some(data) = state.data.artist_page_data.as_ref()
    {
        if state.data.artist_albums_loading {
            state.ui.status_message =
                Some("Artist albums refresh already in progress.".to_string());
            state.ui.status_message_expiry =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
            return None;
        }
        state.data.artist_albums_loading = true;
        state.ui.status_message = Some("Refreshing artist albums...".to_string());
        return Some(AppEvent::RefreshArtistAlbums {
            artist_id: data.artist_id.clone(),
        });
    }
    if state.ui.active_view == ActiveView::Library {
        state.ui.status_message = Some("Refreshing library...".to_string());
        return Some(AppEvent::RefreshLibraryLists);
    }
    None
}

// The `/` filter: an incsearch over the loaded track list, navigated with n/N.

/// Recomputes `search_matches` for the current filter query and jumps to the first hit.
pub fn update_search_matches(state: &mut AppState) {
    state.ui.search_matches.clear();
    if state.ui.search_query.is_empty() {
        return;
    }

    let query = state.ui.search_query.to_lowercase();

    // Only the track list is filterable.
    if state.ui.active_view == ActiveView::TrackList {
        for (i, track) in state.data.tracks.iter().enumerate() {
            if track.name.to_lowercase().contains(&query)
                || track.artist.to_lowercase().contains(&query)
            {
                state.ui.search_matches.push(i);
            }
        }

        // incsearch: jump to the first match immediately.
        if !state.ui.search_matches.is_empty() {
            state.ui.selected_track_index = state.ui.search_matches[0];
        }
    }
}

/// `n`/`N`: cycle the selection through the filter matches, wrapping at either end.
pub fn next_search_match(state: &mut AppState, forward: bool) {
    if state.ui.search_matches.is_empty() {
        return;
    }
    if forward {
        if let Some(&next_idx) = state
            .ui
            .search_matches
            .iter()
            .find(|&&i| i > state.ui.selected_track_index)
        {
            state.ui.selected_track_index = next_idx;
        } else {
            state.ui.selected_track_index = state.ui.search_matches[0];
        }
    } else if let Some(&prev_idx) = state
        .ui
        .search_matches
        .iter()
        .rev()
        .find(|&&i| i < state.ui.selected_track_index)
    {
        state.ui.selected_track_index = prev_idx;
    } else {
        state.ui.selected_track_index = *state.ui.search_matches.last().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TrackListContextKind;
    use std::path::PathBuf;

    #[test]
    fn backing_out_cancels_pending_artist_without_clearing_page_data() {
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string(), None);

        let event = back_to_artist_list(&mut state);

        assert!(matches!(event, AppEvent::CancelArtistPageLoad));
        assert!(state.ui.active_view == ActiveView::ArtistList);
        assert!(state.data.pending_artist_page_id.is_none());
    }

    #[test]
    fn selecting_followed_artist_opens_partial_shell_immediately() {
        let mut state = AppState::new();
        state.data.followed_artists.push(Artist {
            id: "artist".to_string(),
            name: "Artist".to_string(),
            followers: 0,
            image_url: Some("image".to_string()),
        });

        let event = open_followed_artist(&mut state, 0);

        assert!(matches!(event, Some(AppEvent::LoadArtistPage { .. })));
        assert!(matches!(state.ui.active_view, ActiveView::ArtistPage));
        assert_eq!(state.data.pending_artist_page_id.as_deref(), Some("artist"));
        let page = state.data.artist_page_data.as_ref().expect("artist shell");
        assert_eq!(page.artist_name, "Artist");
        assert_eq!(page.image_url.as_deref(), Some("image"));
        assert!(page.albums.is_empty());
        assert!(state.data.artist_albums_loading);
    }

    #[test]
    fn selecting_search_artist_opens_partial_shell_with_image() {
        let mut state = AppState::new();
        state.ui.active_search_tab = SearchTab::Artists;
        state.data.search_results.artists.push(Artist {
            id: "artist".to_string(),
            name: "Search Artist".to_string(),
            followers: 0,
            image_url: Some("search-image".to_string()),
        });

        let event = activate_search_result(&mut state, 0);

        assert!(matches!(event, Some(AppEvent::LoadArtistPage { .. })));
        assert!(matches!(state.ui.active_view, ActiveView::ArtistPage));
        let page = state.data.artist_page_data.as_ref().expect("artist shell");
        assert_eq!(page.artist_name, "Search Artist");
        assert_eq!(page.image_url.as_deref(), Some("search-image"));
    }

    #[test]
    fn local_search_track_playback_uses_local_context() {
        let mut state = AppState::new();
        state.ui.active_search_tab = SearchTab::Tracks;
        let search_track = |id: &str, name: &str, source, path: Option<&str>| SearchTrack {
            id: id.to_string(),
            source,
            local_path: path.map(PathBuf::from),
            name: name.to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            duration_ms: 1,
            image_url: None,
            album_id: None,
            artist_id: None,
        };
        state.data.search_results.tracks = vec![
            search_track("spotify", "Spotify", TrackSource::Spotify, None),
            search_track("local:a", "Local A", TrackSource::Local, Some("/music/a.wav")),
            search_track("local:b", "Local B", TrackSource::Local, Some("/music/b.wav")),
        ];

        let Some(AppEvent::PlayTrack {
            target, track_id, ..
        }) = activate_search_result(&mut state, 2)
        else {
            panic!("expected play event");
        };

        assert_eq!(track_id, "local:b");
        let PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        } = target
        else {
            panic!("expected local context");
        };
        // Only the two local results form the context, with the clicked one selected.
        assert_eq!(tracks.len(), 2);
        assert_eq!(selected_index, 1);
    }

    #[test]
    fn opening_the_artist_list_fetches_only_when_empty() {
        let mut state = AppState::new();
        assert!(matches!(
            open_artist_list(&mut state),
            Some(AppEvent::FetchFollowedArtists)
        ));

        state.data.followed_artists.push(Artist {
            id: "artist".to_string(),
            name: "Artist".to_string(),
            followers: 0,
            image_url: None,
        });
        assert!(open_artist_list(&mut state).is_none());
        assert!(state.ui.active_view == ActiveView::ArtistList);
    }

    #[test]
    fn generated_context_playback_never_masquerades_as_playlist() {
        let track = spotify_track("track");
        let context = TrackListContext::generated("TOP_TRACKS", "Top Tracks");

        let Some(AppEvent::PlayTrack { target, .. }) = play_event(&track, &context) else {
            panic!("expected play event");
        };

        assert_eq!(context.kind, TrackListContextKind::Generated);
        assert_eq!(
            target,
            PlaybackTarget::SpotifyTrack {
                track_id: "track".to_string()
            }
        );
    }

    #[test]
    fn playing_a_local_track_uses_ordered_local_context_target() {
        let mut state = AppState::new();
        state.data.active_tracklist_context = Some(TrackListContext::local_library());
        state.data.tracks = vec![
            local_track("local:a", "/music/a.wav"),
            local_track("local:b", "/music/b.wav"),
            local_track("local:c", "/music/c.wav"),
        ];

        let Some(AppEvent::PlayTrack {
            target, track_id, ..
        }) = play_track_at(&mut state, 1)
        else {
            panic!("expected local play event");
        };

        assert_eq!(track_id, "local:b");
        assert_eq!(state.ui.selected_track_index, 1);
        let PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        } = target
        else {
            panic!("expected local context target");
        };
        assert_eq!(selected_index, 1);
        assert_eq!(tracks.len(), 3);
        assert_eq!(tracks[1].id, "local:b");
    }

    #[test]
    fn playing_a_local_track_excludes_spotify_entries_from_the_context() {
        let mut state = AppState::new();
        state.data.active_tracklist_context = Some(TrackListContext::local_playlist(
            "local-playlist:a".to_string(),
            "Mixed".to_string(),
        ));
        state.data.tracks = vec![
            local_track("local:a", "/music/a.wav"),
            spotify_track("spotify:a"),
            local_track("local:b", "/music/b.wav"),
        ];

        let Some(AppEvent::PlayTrack { target, .. }) = play_track_at(&mut state, 2) else {
            panic!("expected local play event");
        };

        let PlaybackTarget::LocalContext {
            tracks,
            selected_index,
        } = target
        else {
            panic!("expected local context target");
        };
        assert_eq!(selected_index, 1);
        assert_eq!(tracks.len(), 2);
        assert!(
            tracks
                .iter()
                .all(|track| track.source == TrackSource::Local)
        );
    }

    #[test]
    fn out_of_range_indices_are_ignored() {
        let mut state = AppState::new();
        assert!(play_track_at(&mut state, 0).is_none());
        assert!(open_album(&mut state, 0).is_none());
        assert!(open_library_entry(&mut state, 0).is_none());
    }

    fn local_track(id: &str, path: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Local,
            local_path: Some(PathBuf::from(path)),
            name: id.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        }
    }

    fn spotify_track(id: &str) -> Track {
        Track {
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: "Artist".to_string(),
            album: String::new(),
            added_at: None,
            artist_id: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
        }
    }
}
