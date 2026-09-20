//! Frontend-neutral user intents.
//!
//! "The user activated library row N" or "asked to play track N" means the same thing whether it
//! arrived as an Enter keypress in the TUI or a click in the desktop app: mutate selection state
//! and return the event the worker should receive. The frontends translate their input idioms
//! into these calls and send whatever comes back over `app_tx`.

use crate::app::{ActiveView, AppMode, AppState, SearchTab};
use crate::events::AppEvent;
mod playlist;
use crate::models::{
    Artist, DiscographyFilter, LibraryNode, PlaybackTarget, PlayingContext, SearchTrack, Track,
    TrackListContext, TrackSource,
};
pub use playlist::*;

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

/// Starts playing library row `index` in place — a Spotify playlist plays as a context from
/// the top, local collections play their local files. Folders and rows the context-playback
/// API can't start (Liked Songs) return None.
pub fn play_library_entry(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let node = state.data.library_view.get(index).cloned()?;
    let LibraryNode::Playlist { playlist, .. } = node else {
        return None;
    };
    state.ui.selected_playlist_index = index;
    if playlist.id == "LIKED_SONGS" {
        return None;
    }
    if playlist.id == "local-library" {
        return play_local_collection(state.data.local_library.to_tracks());
    }
    if playlist.id.starts_with("local-playlist:") {
        let tracks = state
            .data
            .local_playlists
            .tracks_for_playlist(&playlist.id, &state.data.local_library);
        return play_local_collection(tracks);
    }
    let current_track_id = state.playback.playing_track_id.clone();
    restore_or_snapshot_playlist_pref(state, &playlist.id);
    // Note the context optimistically (the poll confirms later) so a shuffle/repeat toggle
    // issued right after starting the play is attributed to this playlist, not the previous one.
    state.playback.playing_context = Some(PlayingContext {
        context_id: playlist.id.clone(),
        is_album: false,
    });
    Some(AppEvent::PlayContext {
        context_id: playlist.id,
        is_album: false,
        current_track_id,
    })
}

/// Starts playing saved album `index` from the top.
pub fn play_album_at(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let album = state.data.saved_albums.get(index)?;
    state.ui.selected_playlist_index = index;
    let current_track_id = state.playback.playing_track_id.clone();
    state.playback.playing_context = Some(PlayingContext {
        context_id: album.id.clone(),
        is_album: true,
    });
    Some(AppEvent::PlayContext {
        context_id: album.id.clone(),
        is_album: true,
        current_track_id,
    })
}

/// Plays a local collection from its first playable file. Spotify entries mixed into a local
/// playlist are skipped — the local engine can only feed the queue from files.
fn play_local_collection(tracks: Vec<Track>) -> Option<AppEvent> {
    let tracks: Vec<Track> = tracks
        .into_iter()
        .filter(|track| track.source == TrackSource::Local && track.local_path.is_some())
        .collect();
    let first = tracks.first()?.clone();
    Some(AppEvent::PlayTrack {
        target: PlaybackTarget::LocalContext {
            tracks,
            selected_index: 0,
        },
        track_id: first.id,
        title: first.name,
        artist: first.artist,
        duration_ms: first.duration_ms,
        image_url: first.image_url,
        album_id: first.album_id,
    })
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
    let track = state.data.tracks.get(index)?.clone();
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
        context.playback_target_for_track(&track)?
    };
    note_playing_context(state, &target);
    if let PlaybackTarget::SpotifyContext {
        context_id,
        is_album: false,
    } = &target
    {
        let context_id = context_id.clone();
        restore_or_snapshot_playlist_pref(state, &context_id);
    }
    play_event_with_target(&track, target)
}

/// Plays row `index` of the live queue, selecting it first.
///
/// Spotify's API has no "jump to queue position", so the closest match is replaying the current
/// context ([`crate::app::PlaybackState::playing_context`], kept fresh by the status poll) with
/// this track as the offset: up-next then continues from it and repeat-context still loops the
/// whole playlist or album. When no such context exists (local engine, Liked Songs, artist
/// radio) — or the track was manually queued and the context play is rejected, which the worker
/// handles as a fallback — the track plays standalone and the device rebuilds its up-next list
/// around it, which is why this can't reuse [`play_track_at`].
pub fn play_queue_track_at(state: &mut AppState, index: usize) -> Option<AppEvent> {
    if index >= state.queue_view_len() {
        return None;
    }
    state.ui.selected_queue_index = index;
    let track = state.queue_view_track(index)?;
    let target = match track.source {
        TrackSource::Local => PlaybackTarget::LocalTrack {
            track_id: track.id.clone(),
            path: track.local_path.clone()?,
        },
        TrackSource::Spotify => match &state.playback.playing_context {
            Some(context) if state.ui.queue_tab == crate::app::QueueTab::Queue => {
                PlaybackTarget::SpotifyContextJump {
                    context_id: context.context_id.clone(),
                    is_album: context.is_album,
                }
            }
            _ => PlaybackTarget::SpotifyTrack {
                track_id: track.id.clone(),
            },
        },
    };
    play_event_with_target(track, target)
}

/// `tab` in the queue view: the live queue and the recent plays share the selection index.
pub fn cycle_queue_tab(state: &mut AppState) {
    state.ui.queue_tab = match state.ui.queue_tab {
        crate::app::QueueTab::Queue => crate::app::QueueTab::Recent,
        crate::app::QueueTab::Recent => crate::app::QueueTab::Queue,
    };
    state.ui.selected_queue_index = 0;
    state.ui.visual_selection_start = None;
}

pub fn clear_play_history(state: &mut AppState) {
    crate::history::clear(state);
    state.ui.selected_queue_index = 0;
    state.ui.status_message = Some(crate::i18n::t(
        "messages.history_cleared",
        &state.ui.library_config.language,
    ));
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
}

/// Optimistically records the context a play `target` starts, so a queue jump issued before the
/// next status poll lands still knows what's playing. The poll remains the source of truth.
fn note_playing_context(state: &mut AppState, target: &PlaybackTarget) {
    state.playback.playing_context = match target {
        PlaybackTarget::SpotifyContext {
            context_id,
            is_album,
        }
        | PlaybackTarget::SpotifyContextJump {
            context_id,
            is_album,
        } => Some(PlayingContext {
            context_id: context_id.clone(),
            is_album: *is_album,
        }),
        PlaybackTarget::SpotifyTrack { .. }
        | PlaybackTarget::SpotifyTracks { .. }
        | PlaybackTarget::LocalTrack { .. }
        | PlaybackTarget::LocalContext { .. } => None,
    };
}

/// The track a row index addresses in the active view: the loaded tracklist, or the live queue
/// when that's what's on screen. Row menus resolve their subject through this so they don't have
/// to know which list they were opened over.
pub fn row_track(state: &AppState, index: usize) -> Option<&Track> {
    match state.ui.active_view {
        ActiveView::Queue => state.queue_view_track(index),
        _ => state.data.tracks.get(index),
    }
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
    if let Some(resume) = crate::session::resume_event(state) {
        return resume;
    }
    state.playback.is_playing = !state.playback.is_playing;
    state.playback.playback_last_updated_at = Some(std::time::Instant::now());
    AppEvent::TogglePlayback(state.playback.is_playing)
}

pub fn active_context_is_playing(state: &AppState) -> bool {
    state
        .data
        .active_tracklist_context
        .as_ref()
        .zip(state.playback.playing_context.as_ref())
        .is_some_and(|(active, playing)| {
            active.id == playing.context_id && active.is_album() == playing.is_album
        })
}

pub fn play_context_from_header(state: &mut AppState) -> Option<AppEvent> {
    if active_context_is_playing(state) {
        Some(toggle_playback(state))
    } else {
        play_track_at(state, 0)
    }
}

pub fn toggle_context_shuffle(state: &mut AppState) -> Vec<AppEvent> {
    if active_context_is_playing(state) {
        return vec![toggle_shuffle(state)];
    }
    let Some(play) = play_track_at(state, 0) else {
        return Vec::new();
    };
    state.playback.is_shuffled = true;
    persist_playlist_playback_pref(state);
    vec![play, AppEvent::ToggleShuffle(true)]
}

pub fn sort_by_column(state: &mut AppState, sort: crate::app::TrackSort) {
    state.ui.track_sort_ascending = sort == crate::app::TrackSort::Original
        || state.ui.track_sort != sort
        || !state.ui.track_sort_ascending;
    state.sort_tracks(sort);
}

/// Pops the queue head optimistically so the queue view moves at once; the playback sync that
/// follows a skip refetches the real queue while that view is open.
pub fn next_track(state: &mut AppState) -> AppEvent {
    if let Some(resume) = crate::session::resume_event(state) {
        return resume;
    }
    if !state.data.queue.is_empty() {
        state.data.queue.remove(0);
        state.ui.selected_queue_index = state.ui.selected_queue_index.saturating_sub(1);
        if !state.data.manual_queue.is_empty() {
            state.data.manual_queue.remove(0);
        }
    }
    AppEvent::NextTrack {
        current_track_id: state.playback.playing_track_id.clone(),
    }
}

pub fn previous_track(state: &AppState) -> AppEvent {
    if let Some(resume) = crate::session::resume_event(state) {
        return resume;
    }
    AppEvent::PreviousTrack {
        current_track_id: state.playback.playing_track_id.clone(),
    }
}

pub fn toggle_shuffle(state: &mut AppState) -> AppEvent {
    state.playback.is_shuffled = !state.playback.is_shuffled;
    persist_playlist_playback_pref(state);
    AppEvent::ToggleShuffle(state.playback.is_shuffled)
}

/// Off → Context → Track → Off, Spotify's own cycle order.
pub fn cycle_repeat(state: &mut AppState) -> AppEvent {
    let mode = match state.playback.repeat_mode.as_str() {
        "Off" => "Context",
        "Context" => "Track",
        _ => "Off",
    };
    state.playback.repeat_mode = mode.to_string();
    persist_playlist_playback_pref(state);
    AppEvent::SetRepeatMode(mode.to_string())
}

/// Records the current shuffle/repeat as the playing playlist's preference. No-op unless a
/// non-album Spotify context is playing; Liked Songs is excluded because it can appear as an
/// optimistic playing context without being a real playlist.
fn persist_playlist_playback_pref(state: &mut AppState) {
    let Some(context) = state.playback.playing_context.clone() else {
        return;
    };
    if context.is_album || context.context_id == "LIKED_SONGS" {
        return;
    }
    state.ui.library_config.playlist_playback.insert(
        context.context_id,
        crate::config::PlaylistPlaybackPref {
            shuffle: state.playback.is_shuffled,
            repeat: state.playback.repeat_mode.clone(),
        },
    );
    state.save_library_config();
}

/// On playlist start: an existing preference is applied optimistically (the worker pushes it to
/// the device once play succeeds); a playlist with no entry snapshots the current shuffle/repeat
/// as its preference, mimicking Spotify's implicit per-playlist stickiness.
fn restore_or_snapshot_playlist_pref(state: &mut AppState, context_id: &str) {
    if context_id == "LIKED_SONGS" {
        return;
    }
    if let Some(pref) = state.ui.library_config.playlist_playback.get(context_id) {
        state.playback.is_shuffled = pref.shuffle;
        state.playback.repeat_mode = pref.repeat.clone();
    } else {
        state.ui.library_config.playlist_playback.insert(
            context_id.to_string(),
            crate::config::PlaylistPlaybackPref {
                shuffle: state.playback.is_shuffled,
                repeat: state.playback.repeat_mode.clone(),
            },
        );
        state.save_library_config();
    }
}

/// Seeks to an absolute position; `None` when nothing seekable is playing.
pub fn seek_to(state: &mut AppState, target_ms: u32) -> Option<AppEvent> {
    if state.playback.playing_track_id.is_none() || state.playback.duration_ms == 0 {
        return None;
    }
    state.playback.set_optimistic_progress(target_ms);
    if let Some(pending) = state.playback.pending_resume.as_mut() {
        pending.progress_ms = state.playback.progress_ms;
        return None;
    }
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

/// Drops the manually queued tracks, keeping the current track and the rest of its context.
/// Only echo's own Connect device can do this, so any other device gets a status message.
pub fn clear_queue(state: &mut AppState) -> Option<AppEvent> {
    if state.data.manual_queue.is_empty() {
        return None;
    }
    let lang = state.ui.library_config.language.clone();
    let cleared = state.playback.device_name == crate::worker::audio::DEVICE_NAME;
    let key = if cleared {
        "messages.queue_cleared"
    } else {
        "messages.clear_queue_needs_device"
    };
    state.ui.status_message = Some(crate::i18n::t(key, &lang));
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    if !cleared {
        return None;
    }
    let count = state.data.manual_queue.len().min(state.data.queue.len());
    state.data.queue.drain(..count);
    state.data.manual_queue.clear();
    state.ui.selected_queue_index = 0;
    Some(AppEvent::ClearQueue)
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
    (!id.is_empty()).then_some(AppEvent::TransferPlayback(id))
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
    crate::search::remember_search(&mut state.ui.library_config.recent_searches, &query);
    state.save_library_config();
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
    let row = if state.ui.active_search_tab == SearchTab::All {
        *crate::search::all_tab_rows(&state.data.search_results, &state.ui.search_context_query)
            .get(index)?
    } else {
        crate::search::SearchRow {
            tab: state.ui.active_search_tab,
            index,
            top: false,
        }
    };
    activate_search_row(state, row)
}

pub fn remove_recent_search(state: &mut AppState, index: usize) {
    if index < state.ui.library_config.recent_searches.len() {
        state.ui.library_config.recent_searches.remove(index);
        state.save_library_config();
    }
}

pub fn clear_recent_searches(state: &mut AppState) {
    state.ui.library_config.recent_searches.clear();
    state.save_library_config();
}

pub fn activate_search_row(
    state: &mut AppState,
    row: crate::search::SearchRow,
) -> Option<AppEvent> {
    let index = row.index;
    match row.tab {
        SearchTab::All => None,
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
        SearchTab::Playlists => {
            // Foreign playlists load like library ones — the worker fetches any id
            // (same path the `:open <uri>` command uses).
            let playlist = state.data.search_results.playlists.get(index)?.clone();
            let context = TrackListContext::playlist(
                playlist.id,
                playlist.name,
                playlist.owner,
                playlist.owner_id,
                playlist.image_url,
            );
            state.begin_tracklist_load(context.clone());
            Some(AppEvent::LoadContextTracks(context))
        }
    }
}

pub fn play_search_row(state: &mut AppState, row: crate::search::SearchRow) -> Option<AppEvent> {
    if row.tab == SearchTab::Tracks {
        let track = state.data.search_results.tracks.get(row.index)?.clone();
        return search_track_play_event(state, &track);
    }
    let item = crate::search::search_row_item(&state.data.search_results, row)?;
    if !item.playable() {
        return None;
    }
    let kind = match row.tab {
        SearchTab::Artists => crate::home::HomeItemKind::Artist,
        SearchTab::Albums => crate::home::HomeItemKind::Album,
        SearchTab::Playlists => crate::home::HomeItemKind::Playlist,
        _ => return None,
    };
    play_home_item(
        state,
        crate::home::HomeItem {
            id: item.id,
            kind,
            title: item.title,
            subtitle: item.credit,
            image_url: item.image_url,
            owner_id: None,
            track: None,
            release_year: None,
        },
    )
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
                explicit: t.explicit,
                added_by: None,
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
                artists: Vec::new(),
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

/// Opens the followed-artists list, fetching it in the background when empty. The
/// artist-list view renders live from state, so a cold open fills in place.
pub fn open_artist_list(state: &mut AppState) -> Option<AppEvent> {
    state.push_view_history();
    state.ui.active_view = ActiveView::ArtistList;
    state.ui.artist_list_source = crate::app::ArtistListSource::Followed;
    state.ui.selected_artist_index = 0;
    state
        .data
        .followed_artists
        .is_empty()
        .then_some(AppEvent::FetchFollowedArtists)
}

/// Opens the user's top artists as an artist list, fetching in the background when
/// empty — same fill-in-place behavior as [`open_artist_list`].
pub fn open_top_artists(state: &mut AppState) -> Option<AppEvent> {
    state.push_view_history();
    state.ui.active_view = ActiveView::ArtistList;
    state.ui.artist_list_source = crate::app::ArtistListSource::Top;
    state.ui.selected_artist_index = 0;
    state
        .data
        .top_artists
        .is_empty()
        .then_some(AppEvent::FetchTopArtists {
            range: state.ui.library_config.top_items_range,
        })
}

/// Opens the What's New feed (recent releases from followed artists), fetching in the
/// background when empty — same fill-in-place behavior as [`open_artist_list`].
pub fn open_whats_new(state: &mut AppState) -> Option<AppEvent> {
    state.push_view_history();
    state.ui.active_view = ActiveView::WhatsNew;
    state.ui.selected_whats_new_index = 0;
    state
        .data
        .whats_new
        .is_empty()
        .then_some(AppEvent::FetchWhatsNew)
}

/// Opens album `index` of the What's New feed as a track list.
pub fn open_whats_new_album(state: &mut AppState, index: usize) -> Option<AppEvent> {
    state.ui.selected_whats_new_index = index;
    let album = state.data.whats_new.get(index)?;
    let context = TrackListContext::album(
        album.id.clone(),
        album.name.clone(),
        album.artists.clone(),
        album.image_url.clone(),
    );
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Enters the artist page for followed artist `index` (sidebar Artists tab).
pub fn open_followed_artist(state: &mut AppState, index: usize) -> Option<AppEvent> {
    state.ui.selected_artist_index = index;
    let artist = state.data.followed_artists.get(index)?.clone();
    open_artist(state, artist)
}

/// Enters the artist page for row `index` of the artist-list view, whichever list
/// ([`crate::app::ArtistListSource`]) it is showing.
pub fn open_artist_at(state: &mut AppState, index: usize) -> Option<AppEvent> {
    state.ui.selected_artist_index = index;
    let artist = state.artist_list().get(index)?.clone();
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
    open_album_from_artist_page(state, index)
}

/// The album-opening body shared by [`open_artist_album`] (TUI, album-relative cursor) and
/// [`activate_artist_page_row`] (desktop, combined cursor): deliberately does not touch the
/// cursor, so each caller's own index space survives the history snapshot.
fn open_album_from_artist_page(state: &mut AppState, album_index: usize) -> Option<AppEvent> {
    let data = state.data.artist_page_data.clone()?;
    let album = data.albums.get(album_index)?;
    let context = TrackListContext::album(
        album.id.clone(),
        album.name.clone(),
        album.artists.clone(),
        album.image_url.clone(),
    );
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

/// Activates row `index` of the artist page in the desktop's combined index space:
/// rows `0..top_tracks.len()` play a Popular track, the rest open the matching album.
pub fn activate_artist_page_row(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let top_len = state
        .data
        .artist_page_data
        .as_ref()
        .map_or(0, |data| data.top_tracks.len());
    state.ui.artist_page_album_index = index;
    if index < top_len {
        play_artist_top_track(state, index)
    } else {
        open_album_from_artist_page(state, index - top_len)
    }
}

/// Plays top track `index` of the current artist page. The whole Popular list is sent as
/// the playback target, so up-next continues down it without leaving the page.
pub fn play_artist_top_track(state: &mut AppState, index: usize) -> Option<AppEvent> {
    let data = state.data.artist_page_data.as_ref()?;
    let track = data.top_tracks.get(index)?.clone();
    let track_ids: Vec<String> = data
        .top_tracks
        .iter()
        .map(|track| track.id.clone())
        .collect();
    let target = PlaybackTarget::SpotifyTracks {
        track_ids,
        selected_index: index,
    };
    note_playing_context(state, &target);
    play_event_with_target(&track, target)
}

/// The Popular row under the artist page cursor, `None` once the cursor has moved down into
/// the albums section (the two share one index space, Popular rows first).
pub fn selected_artist_top_track(state: &AppState) -> Option<&Track> {
    if state.ui.active_view != ActiveView::ArtistPage {
        return None;
    }
    state
        .data
        .artist_page_data
        .as_ref()?
        .top_tracks
        .get(state.ui.artist_page_album_index)
}

/// Switches the artist page's discography chip and puts the cursor back on the first row of
/// the caller's index space (desktop callers move it past the Popular rows themselves).
pub fn set_discography_filter(state: &mut AppState, filter: DiscographyFilter) {
    if state.ui.artist_discography_filter == filter {
        return;
    }
    state.ui.artist_discography_filter = filter;
    state.ui.artist_page_album_index = 0;
    crate::apply_worker_event::artist::apply_discography_filter(state);
}

/// Tab on the artist page: the next discography chip, wrapping.
pub fn cycle_discography_filter(state: &mut AppState) {
    set_discography_filter(state, state.ui.artist_discography_filter.next());
}

/// Follows or unfollows the artist of the open artist page, flipping the followed list at
/// once; the worker reloads that list after the write, which also undoes a failed flip.
pub fn toggle_follow_artist(state: &mut AppState) -> Option<AppEvent> {
    let data = state.data.artist_page_data.as_ref()?;
    let artist_id = data.artist_id.clone();
    let followed = &mut state.data.followed_artists;
    if let Some(position) = followed.iter().position(|artist| artist.id == artist_id) {
        followed.remove(position);
        return Some(AppEvent::UnfollowArtist(artist_id));
    }
    followed.insert(
        0,
        Artist {
            id: artist_id.clone(),
            name: data.artist_name.clone(),
            image_url: data.image_url.clone(),
        },
    );
    Some(AppEvent::FollowArtist(artist_id))
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
/// Opens the top tracks list. A list restored from disk shows at once and refreshes in
/// place; an empty one is fetched first.
pub fn open_top_tracks(state: &mut AppState) -> Option<AppEvent> {
    let refresh = home_feed_fetch(state, crate::home::HomeFeed::TopTracks, false);
    if state.data.top_tracks.is_empty() {
        state.ui.pending_browse_open = Some(crate::models::BrowseNode::TopTracks);
        return Some(refresh.unwrap_or(AppEvent::FetchTopTracks {
            range: state.ui.library_config.top_items_range,
        }));
    }
    state.ui.pending_browse_open = None;
    state.show_generated_tracks(
        state.data.top_tracks.clone(),
        TrackListContext::generated("TOP_TRACKS", "Top Tracks"),
    );
    refresh
}

/// Opens the recently-played list; fetches it first when none is cached yet.
pub fn open_recently_played(state: &mut AppState) -> Option<AppEvent> {
    let refresh = home_feed_fetch(state, crate::home::HomeFeed::RecentlyPlayed, false);
    if state.data.recently_played.is_empty() {
        state.ui.pending_browse_open = Some(crate::models::BrowseNode::RecentlyPlayed);
        return Some(refresh.unwrap_or(AppEvent::FetchRecentlyPlayed));
    }
    state.ui.pending_browse_open = None;
    state.show_generated_tracks(
        state.data.recently_played.clone(),
        TrackListContext::generated("RECENTLY_PLAYED", "Recently Played"),
    );
    refresh
}

/// Switches the Top Tracks / Top Artists time window, persists it, and refetches
/// whichever top list is currently on screen so it updates in place (the reducers
/// refresh an open list without re-navigating).
pub fn set_top_items_range(
    state: &mut AppState,
    range: crate::models::TopItemsRange,
) -> Option<AppEvent> {
    if state.ui.library_config.top_items_range == range {
        return None;
    }
    state.ui.library_config.top_items_range = range;
    state.save_library_config();
    state.data.top_tracks.clear();
    state.data.top_artists.clear();

    let top_tracks_open = state.ui.active_view == ActiveView::TrackList
        && state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.id == "TOP_TRACKS");
    if top_tracks_open {
        return Some(AppEvent::FetchTopTracks { range });
    }
    if state.ui.active_view == ActiveView::ArtistList
        && state.ui.artist_list_source == crate::app::ArtistListSource::Top
    {
        return Some(AppEvent::FetchTopArtists { range });
    }
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
        && let Some(index) = state
            .data
            .tracks
            .iter()
            .position(|track| track.id == track_id)
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

/// `q`: append the selected track (track list, search tracks tab or artist Popular row) to
/// the playback queue.
pub fn queue_selected_track(state: &AppState) -> Option<AppEvent> {
    let track_id = match state.ui.active_view {
        ActiveView::TrackList => state
            .data
            .tracks
            .get(state.ui.selected_track_index)
            .map(|t| t.id.clone()),
        ActiveView::ArtistPage => selected_artist_top_track(state).map(|t| t.id.clone()),
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
/// `library_config` must be kept in sync too: later whole-section saves (window bounds on
/// close, sidebar width) write it back verbatim and would otherwise revert the toggle.
pub fn toggle_condensed_lyrics(state: &mut AppState) {
    state.ui.condensed_lyrics_enabled = !state.ui.condensed_lyrics_enabled;
    state.ui.library_config.condensed_lyrics_enabled = state.ui.condensed_lyrics_enabled;
    state.save_library_config();
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

pub fn open_home(state: &mut AppState) -> Vec<AppEvent> {
    if state.ui.active_view != ActiveView::Home {
        state.push_view_history();
    }
    exit_visual(state);
    if state.ui.mode == AppMode::Command {
        state.ui.mode = AppMode::Normal;
        state.ui.command_buffer.clear();
    }
    state.clear_pending_artist_page();
    state.ui.pending_browse_open = None;
    state.ui.active_view = ActiveView::Home;
    home_fetches(state, false)
}

pub fn refresh_home(state: &mut AppState) -> Vec<AppEvent> {
    home_fetches(state, true)
}

fn home_fetches(state: &mut AppState, force: bool) -> Vec<AppEvent> {
    crate::home::HomeFeed::ALL
        .into_iter()
        .filter_map(|feed| home_feed_fetch(state, feed, force))
        .collect()
}

/// The fetch a feed needs now, marking it in flight: data that is empty, never fetched this
/// session, or older than the feed's TTL. Persisted data restored at startup counts as
/// never fetched, so it shows at once and refreshes behind the view.
fn home_feed_fetch(
    state: &mut AppState,
    feed: crate::home::HomeFeed,
    force: bool,
) -> Option<AppEvent> {
    use crate::home::HomeFeed;
    let range = matches!(feed, HomeFeed::TopArtists | HomeFeed::TopTracks)
        .then_some(state.ui.library_config.top_items_range);
    let empty = match feed {
        HomeFeed::MadeForYou => state.data.playlists.is_empty(),
        HomeFeed::RecentlyPlayed => state.data.recently_played.is_empty(),
        HomeFeed::RecentContexts => state.data.recent_contexts.is_empty(),
        HomeFeed::TopArtists => state.data.top_artists.is_empty(),
        HomeFeed::TopTracks => state.data.top_tracks.is_empty(),
        HomeFeed::NewReleases => state.data.whats_new.is_empty(),
    };
    let fetch = state.data.home_fetches.entry(feed).or_default();
    let now = std::time::Instant::now();
    if fetch.in_flight || (!force && !empty && !fetch.needs_fetch(now, feed.ttl(), range)) {
        return None;
    }
    fetch.in_flight = true;
    Some(match feed {
        HomeFeed::MadeForYou => AppEvent::RefreshLibraryLists,
        HomeFeed::RecentlyPlayed => AppEvent::FetchRecentlyPlayed,
        HomeFeed::RecentContexts => AppEvent::FetchRecentContexts,
        HomeFeed::TopArtists => AppEvent::FetchTopArtists {
            range: range.unwrap(),
        },
        HomeFeed::TopTracks => AppEvent::FetchTopTracks {
            range: range.unwrap(),
        },
        HomeFeed::NewReleases => AppEvent::FetchWhatsNew,
    })
}

pub fn open_home_item(state: &mut AppState, item: crate::home::HomeItem) -> Option<AppEvent> {
    use crate::home::HomeItemKind;
    let context = match item.kind {
        HomeItemKind::Playlist => TrackListContext::playlist(
            item.id.clone(),
            if item.id == "LIKED_SONGS" {
                crate::i18n::t(
                    "desktop.home_liked_songs",
                    &state.ui.library_config.language,
                )
            } else {
                item.title
            },
            item.subtitle,
            item.owner_id.unwrap_or_default(),
            item.image_url,
        ),
        HomeItemKind::Album => {
            TrackListContext::album(item.id, item.title, item.subtitle, item.image_url)
        }
        HomeItemKind::Artist => {
            state.begin_artist_page_load(
                item.id.clone(),
                item.title.clone(),
                item.image_url.clone(),
            );
            return Some(AppEvent::LoadArtistPage {
                artist_id: item.id,
                artist_name: Some(item.title),
                artist_image_url: item.image_url,
            });
        }
        HomeItemKind::Track => return play_home_item(state, item),
    };
    state.begin_tracklist_load(context.clone());
    Some(AppEvent::LoadContextTracks(context))
}

pub fn play_home_item(state: &mut AppState, item: crate::home::HomeItem) -> Option<AppEvent> {
    use crate::home::HomeItemKind;
    match item.kind {
        HomeItemKind::Playlist | HomeItemKind::Album => {
            let is_album = item.kind == HomeItemKind::Album;
            if !is_album && item.id != "LIKED_SONGS" {
                restore_or_snapshot_playlist_pref(state, &item.id);
            }
            state.playback.playing_context = Some(PlayingContext {
                context_id: item.id.clone(),
                is_album,
            });
            Some(AppEvent::PlayContext {
                context_id: item.id,
                is_album,
                current_track_id: state.playback.playing_track_id.clone(),
            })
        }
        HomeItemKind::Track => {
            let track = item.track?;
            state.playback.playing_context = None;
            play_event(&track, &TrackListContext::generated("HOME", ""))
        }
        HomeItemKind::Artist => {
            state.playback.playing_context = None;
            Some(AppEvent::PlayArtist {
                artist_id: item.id,
                current_track_id: state.playback.playing_track_id.clone(),
            })
        }
    }
}

// Library drag-and-drop: moving playlists between folders, the pinned section and the loose
// list, plus reordering within each. Positions are visible `library_view` row indices.

/// True for the fixed rows drag-and-drop must leave alone: Liked Songs and the local-library
/// entry. Local playlists are ordinary draggable rows.
fn is_fixed_library_row(id: &str) -> bool {
    id == "LIKED_SONGS" || id == "local-library"
}

/// Drops the playlist `src_id` onto visible row `dest_index`:
/// - onto a folder header → append to that folder;
/// - onto a playlist inside a folder → insert into that folder before it;
/// - onto a pinned playlist → pin, inserted before it;
/// - onto a loose playlist → unpin/unfolder and reorder before it (persisted as
///   `playlist_order`, which forces `SortMode::Default`).
pub fn move_library_playlist(state: &mut AppState, src_id: &str, dest_index: usize) -> bool {
    if is_fixed_library_row(src_id) {
        return false;
    }
    let Some(dest_node) = state.data.library_view.get(dest_index).cloned() else {
        return false;
    };

    enum Target {
        FolderEnd(String),
        FolderBefore(String, String),
        PinnedBefore(String),
        LooseBefore(String),
    }
    let target = match &dest_node {
        LibraryNode::Folder(folder) => Target::FolderEnd(folder.name.clone()),
        LibraryNode::Playlist { playlist, indent } => {
            if playlist.id == src_id || is_fixed_library_row(&playlist.id) {
                return false;
            }
            if *indent >= 1 {
                let Some(folder) = state
                    .ui
                    .library_config
                    .folders
                    .iter()
                    .find(|folder| folder.playlists.contains(&playlist.id))
                else {
                    return false;
                };
                Target::FolderBefore(folder.name.clone(), playlist.id.clone())
            } else if state.ui.library_config.pinned.contains(&playlist.id) {
                Target::PinnedBefore(playlist.id.clone())
            } else {
                Target::LooseBefore(playlist.id.clone())
            }
        }
    };

    // Pull the playlist out of every container first; the target decides where it lands.
    state.ui.library_config.pinned.retain(|id| id != src_id);
    for folder in &mut state.ui.library_config.folders {
        folder.playlists.retain(|id| id != src_id);
    }

    match target {
        Target::FolderEnd(name) => {
            let Some(folder) = state
                .ui
                .library_config
                .folders
                .iter_mut()
                .find(|folder| folder.name == name)
            else {
                return false;
            };
            folder.playlists.push(src_id.to_string());
        }
        Target::FolderBefore(name, before_id) => {
            let Some(folder) = state
                .ui
                .library_config
                .folders
                .iter_mut()
                .find(|folder| folder.name == name)
            else {
                return false;
            };
            let position = folder
                .playlists
                .iter()
                .position(|id| id == &before_id)
                .unwrap_or(folder.playlists.len());
            folder.playlists.insert(position, src_id.to_string());
        }
        Target::PinnedBefore(before_id) => {
            let pinned = &mut state.ui.library_config.pinned;
            let position = pinned
                .iter()
                .position(|id| id == &before_id)
                .unwrap_or(pinned.len());
            pinned.insert(position, src_id.to_string());
        }
        Target::LooseBefore(before_id) => {
            // A manual order only makes sense without an active sort.
            state.ui.library_config.sort_mode = crate::config::SortMode::Default;
            state.compute_library_view();
            let pinned: std::collections::HashSet<&String> =
                state.ui.library_config.pinned.iter().collect();
            let mut order: Vec<String> = state
                .data
                .library_view
                .iter()
                .filter_map(|node| match node {
                    LibraryNode::Playlist {
                        playlist,
                        indent: 0,
                    } if !is_fixed_library_row(&playlist.id) && !pinned.contains(&playlist.id) => {
                        Some(playlist.id.clone())
                    }
                    _ => None,
                })
                .collect();
            order.retain(|id| id != src_id);
            let position = order
                .iter()
                .position(|id| id == &before_id)
                .unwrap_or(order.len());
            order.insert(position, src_id.to_string());
            state.ui.library_config.playlist_order = order;
        }
    }

    state.save_library_config();
    state.compute_library_view();
    true
}

/// Context-menu "Remove from folder": the playlist returns to the loose list.
pub fn remove_playlist_from_folders(state: &mut AppState, id: &str) -> bool {
    let mut removed = false;
    for folder in &mut state.ui.library_config.folders {
        let before = folder.playlists.len();
        folder.playlists.retain(|pid| pid != id);
        removed |= folder.playlists.len() != before;
    }
    if removed {
        state.save_library_config();
        state.compute_library_view();
    }
    removed
}

// Destructive-action prompts. A frontend sets one of the `*_prompt` fields, shows its own
// confirm UI, and resolves it through these — the TUI with y/other keys, the desktop with
// modal buttons.

// Visual mode: a contiguous range anchored where `v` was pressed, extended by moving the
// selection. `AppState::get_visual_selection_range` resolves the anchor and the live selection
// into an inclusive range; everything below operates on that range.

/// `v` — start a selection anchored at the focused row.
pub fn enter_visual(state: &mut AppState) {
    let anchor = match state.ui.active_view {
        ActiveView::TrackList => state.ui.selected_track_index,
        ActiveView::SearchResults => state.ui.selected_search_index,
        ActiveView::Queue => state.ui.selected_queue_index,
        ActiveView::Library => state.ui.selected_playlist_index,
        // The remaining views have nothing a range operation could act on.
        _ => return,
    };
    state.ui.picked_rows.clear();
    state.ui.mode = crate::app::AppMode::Visual;
    state.ui.visual_selection_start = Some(anchor);
}

/// Leave visual mode, dropping the anchor and any picked rows.
pub fn exit_visual(state: &mut AppState) {
    if state.ui.mode == crate::app::AppMode::Visual {
        state.ui.mode = crate::app::AppMode::Normal;
        state.ui.status_message = None;
    }
    state.ui.visual_selection_start = None;
    state.ui.picked_rows.clear();
    state.ui.pending_d_press = false;
}

/// Anchor the selection at `index` without entering visual mode first — the desktop's
/// shift-click, which selects a range in one gesture.
pub fn set_visual_anchor(state: &mut AppState, index: usize) {
    state.ui.picked_rows.clear();
    state.ui.mode = crate::app::AppMode::Visual;
    state.ui.visual_selection_start = Some(index);
}

/// The focused row of the views that list tracks; `None` elsewhere.
fn focused_track_row(state: &AppState) -> Option<usize> {
    match state.ui.active_view {
        ActiveView::TrackList => Some(state.ui.selected_track_index),
        ActiveView::Queue => Some(state.ui.selected_queue_index),
        ActiveView::SearchResults if state.ui.active_search_tab == SearchTab::Tracks => {
            Some(state.ui.selected_search_index)
        }
        _ => None,
    }
}

fn focus_track_row(state: &mut AppState, index: usize) {
    match state.ui.active_view {
        ActiveView::TrackList => state.ui.selected_track_index = index,
        ActiveView::Queue => state.ui.selected_queue_index = index,
        ActiveView::SearchResults => state.ui.selected_search_index = index,
        _ => {}
    }
}

/// Ctrl-click (cmd-click on macOS): toggle `index` in the discontiguous pick set. The first
/// pick also takes the focused row along, so two clicks select two rows. Picks replace any
/// visual range; the range operations (`a`, `q`, `dd`, `l`, drags) then act on the picks.
pub fn toggle_picked_row(state: &mut AppState, index: usize) {
    let Some(focused) = focused_track_row(state) else {
        return;
    };
    let seed = state.ui.picked_rows.is_empty() && focused != index;
    if state.ui.mode == crate::app::AppMode::Visual {
        state.ui.mode = crate::app::AppMode::Normal;
        state.ui.status_message = None;
    }
    state.ui.visual_selection_start = None;
    if seed {
        state.ui.picked_rows.insert(focused);
    }
    if !state.ui.picked_rows.remove(&index) {
        state.ui.picked_rows.insert(index);
    }
    focus_track_row(state, index);
}

pub fn clear_picks(state: &mut AppState) {
    state.ui.picked_rows.clear();
}

/// Whether a range operation would cover more than the focused row.
pub fn has_multi_selection(state: &AppState) -> bool {
    !state.ui.picked_rows.is_empty() || state.get_visual_selection_range().is_some()
}

/// The rows a range operation covers, in display order: the picks, else the visual range,
/// else the focused row alone.
pub fn selected_rows(state: &AppState) -> Vec<usize> {
    if !state.ui.picked_rows.is_empty() {
        return state.ui.picked_rows.iter().copied().collect();
    }
    if let Some((start, end)) = state.get_visual_selection_range() {
        return (start..=end).collect();
    }
    focused_track_row(state).into_iter().collect()
}

/// The track at `index` in whichever track list is showing.
fn view_track(state: &AppState, index: usize) -> Option<Track> {
    match state.ui.active_view {
        ActiveView::TrackList => state.data.tracks.get(index).cloned(),
        ActiveView::Queue => state.queue_view_track(index).cloned(),
        ActiveView::SearchResults if state.ui.active_search_tab == SearchTab::Tracks => {
            state.data.search_results.tracks.get(index).map(Track::from)
        }
        _ => None,
    }
}

pub fn selected_tracks(state: &AppState) -> Vec<Track> {
    selected_rows(state)
        .into_iter()
        .filter_map(|index| view_track(state, index))
        .collect()
}

/// The tracks the current visual range, or the picked rows, cover in the views that list tracks.
pub fn visual_tracks(state: &AppState) -> Vec<Track> {
    if !state.ui.picked_rows.is_empty() {
        return selected_tracks(state);
    }
    let Some((start, end)) = state.get_visual_selection_range() else {
        return Vec::new();
    };
    // `end` comes from a live selection index, so clamp rather than slice blindly.
    let slice = |len: usize| -> Option<(usize, usize)> {
        (start < len).then(|| (start, end.min(len.saturating_sub(1))))
    };
    match state.ui.active_view {
        ActiveView::TrackList => slice(state.data.tracks.len())
            .map(|(s, e)| state.data.tracks[s..=e].to_vec())
            .unwrap_or_default(),
        ActiveView::Queue => slice(state.queue_view_len())
            .map(|(s, e)| state.queue_view_tracks(s, e))
            .unwrap_or_default(),
        ActiveView::SearchResults if state.ui.active_search_tab == SearchTab::Tracks => {
            slice(state.data.search_results.tracks.len())
                .map(|(s, e)| {
                    state.data.search_results.tracks[s..=e]
                        .iter()
                        .map(Track::from)
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// The library playlists the visual range covers that the user may actually delete. Synthetic
/// rows, folders and other people's playlists are skipped rather than failing the whole range.
fn visual_deletable_playlists(state: &AppState) -> Vec<String> {
    let Some((start, end)) = state.get_visual_selection_range() else {
        return Vec::new();
    };
    let view = &state.data.library_view;
    if start >= view.len() {
        return Vec::new();
    }
    view[start..=end.min(view.len() - 1)]
        .iter()
        .filter_map(|node| match node {
            LibraryNode::Playlist { playlist, .. } => {
                let deletable = playlist.id.starts_with("local-playlist:")
                    || (playlist.id != "LIKED_SONGS"
                        && playlist.id != "local-library"
                        && Some(&playlist.owner_id) == state.data.user_id.as_ref());
                deletable.then(|| playlist.id.clone())
            }
            LibraryNode::Folder(_) => None,
        })
        .collect()
}

/// `q` in visual mode — queue every track in the range, then leave visual mode.
pub fn queue_visual_selection(state: &mut AppState) -> Option<AppEvent> {
    let ids: Vec<String> = visual_tracks(state)
        .into_iter()
        .map(|track| track.id)
        .collect();
    exit_visual(state);
    if ids.is_empty() {
        return None;
    }
    state.ui.status_message = Some(format!("Added {} tracks to queue", ids.len()));
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    Some(AppEvent::AddToQueue(ids))
}

/// `a` in visual mode — stage the range in the operation register and open the playlist picker.
/// `action_menu::commit_playlist_add` reads the register, so the range survives the picker.
pub fn add_visual_selection_to_playlist(state: &mut AppState) {
    state.ui.playlist_add_filter.clear();
    let ids: Vec<String> = visual_tracks(state)
        .into_iter()
        .map(|track| track.id)
        .collect();
    if ids.is_empty() {
        return;
    }
    state.ui.operation_register = ids;
    state.ui.playlist_add_modal_open = true;
    state.ui.selected_playlist_modal_index = 0;
    // Visual mode ends here; the register carries the selection from now on.
    state.ui.mode = crate::app::AppMode::Normal;
    state.ui.visual_selection_start = None;
}

/// `d` twice in visual mode — stage the delete prompt covering the whole range.
/// Likes `track_id`, or — when it is already liked — stages the remove-confirmation prompt
/// the frontends render. The like is applied optimistically (set + persisted cache) and the
/// returned event syncs Spotify; un-liking goes through the prompt's confirm flow instead.
pub fn toggle_like_track(state: &mut AppState, track_id: String) -> Option<AppEvent> {
    if state.data.liked_tracks.contains(&track_id) {
        state.ui.liked_track_remove_prompt = Some(track_id);
        return None;
    }
    state.data.liked_tracks.insert(track_id.clone());
    crate::config::AppConfig::update_cache(|cache| {
        cache.liked_tracks = state.data.liked_tracks.clone()
    });
    state.ui.status_message = Some(crate::i18n::t(
        "messages.added_to_liked",
        &state.ui.library_config.language,
    ));
    state.ui.status_message_expiry =
        Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
    Some(AppEvent::ToggleTrackLike(track_id, true))
}

/// `l`: like/unlike whatever row is focused — the track list, the queue, or the search
/// results' Tracks tab. Other views have no track to like.
pub fn toggle_like_selected(state: &mut AppState) -> Option<AppEvent> {
    let track_id = match state.ui.active_view {
        ActiveView::TrackList => state
            .data
            .tracks
            .get(state.ui.selected_track_index)
            .map(|track| track.id.clone()),
        ActiveView::Queue => state
            .queue_view_track(state.ui.selected_queue_index)
            .map(|track| track.id.clone()),
        ActiveView::SearchResults
            if state.ui.active_search_tab == crate::app::SearchTab::Tracks =>
        {
            state
                .data
                .search_results
                .tracks
                .get(state.ui.selected_search_index)
                .map(|track| track.id.clone())
        }
        _ => None,
    }?;
    toggle_like_track(state, track_id)
}

/// Moves track `from` to position `to` of the current track list, when it is a playlist the
/// user can modify shown in original order (a sorted projection has no meaningful positions
/// to reorder). Applies the move optimistically to both `tracks` and `original_tracks`;
/// the worker syncs the server (or local-playlist file) and rolls back via a context
/// refresh on failure.
pub fn move_track_in_playlist(state: &mut AppState, from: usize, to: usize) -> Option<AppEvent> {
    if state.ui.active_view != ActiveView::TrackList {
        return None;
    }
    let context = state.data.active_tracklist_context.clone()?;
    if !context.can_modify_playlist(state.data.user_id.as_ref()) {
        return None;
    }
    if state.ui.track_sort != crate::app::TrackSort::Original || !state.ui.track_sort_ascending {
        state.ui.status_message = Some(crate::i18n::t(
            "messages.reorder_requires_original",
            &state.ui.library_config.language,
        ));
        state.ui.status_message_expiry =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
        return None;
    }
    let len = state.data.tracks.len();
    if from >= len || from == to {
        return None;
    }
    let to = to.min(len - 1);
    let track = state.data.tracks.remove(from);
    let track_id = track.id.clone();
    state.data.tracks.insert(to, track);
    let original = state.data.original_tracks.remove(from);
    state.data.original_tracks.insert(to, original);
    state.ui.selected_track_index = to;
    Some(AppEvent::MoveTrack {
        playlist_id: context.id,
        track_id,
        from,
        to,
    })
}

pub fn delete_visual_selection(state: &mut AppState) {
    let armed = state.ui.pending_d_press;
    match state.ui.active_view {
        ActiveView::TrackList => {
            let Some(context) = state.data.active_tracklist_context.clone() else {
                return;
            };
            if !context.can_modify_playlist(state.data.user_id.as_ref()) {
                return;
            }
            let ids: Vec<String> = visual_tracks(state)
                .into_iter()
                .map(|track| track.id)
                .collect();
            if ids.is_empty() {
                return;
            }
            if armed {
                state.ui.track_delete_prompt = Some((context.id, ids));
                state.ui.pending_d_press = false;
            } else {
                state.ui.pending_d_press = true;
            }
        }
        ActiveView::Library if state.ui.active_library_tab == crate::app::LibraryTab::Albums => {
            let Some((start, end)) = state.get_visual_selection_range() else {
                return;
            };
            let albums = &state.data.saved_albums;
            if start >= albums.len() {
                return;
            }
            let ids: Vec<String> = albums[start..=end.min(albums.len() - 1)]
                .iter()
                .map(|album| album.id.clone())
                .collect();
            if armed {
                state.ui.album_mass_delete_prompt = Some(ids);
                state.ui.pending_d_press = false;
            } else {
                state.ui.pending_d_press = true;
            }
        }
        ActiveView::Library => {
            let ids = visual_deletable_playlists(state);
            if ids.is_empty() {
                return;
            }
            if armed {
                state.ui.playlist_delete_prompt = Some(ids);
                state.ui.pending_d_press = false;
            } else {
                state.ui.pending_d_press = true;
            }
        }
        _ => {}
    }
}

/// `d` pressed on the focused row — the first press arms `pending_d_press`, the second stages
/// the `*_prompt` matching whatever is focused, which the frontend then confirms.
///
/// Rows with nothing to delete (Liked Songs, the local library, a playlist someone else owns)
/// leave the flag untouched, so a stray `d` never arms a delete that a later `d` on a
/// different row would fire.
pub fn mark_selected_for_delete(state: &mut AppState) {
    // Second press of the pair; `armed` is consumed by whichever branch stages a prompt.
    let armed = state.ui.pending_d_press;
    let stage = |state: &mut AppState, set: &mut dyn FnMut(&mut AppState)| {
        if armed {
            set(state);
            state.ui.pending_d_press = false;
        } else {
            state.ui.pending_d_press = true;
        }
    };

    match state.ui.active_view {
        ActiveView::TrackList => {
            let Some(track) = state.data.tracks.get(state.ui.selected_track_index) else {
                return;
            };
            let Some(context) = state.data.active_tracklist_context.clone() else {
                return;
            };
            if !context.can_modify_playlist(state.data.user_id.as_ref()) {
                return;
            }
            let track_id = track.id.clone();
            stage(state, &mut |state| {
                state.ui.track_delete_prompt = Some((context.id.clone(), vec![track_id.clone()]));
            });
        }
        ActiveView::Library => {
            if state.ui.active_library_tab == crate::app::LibraryTab::Albums {
                let Some(album) = state
                    .data
                    .saved_albums
                    .get(state.ui.selected_playlist_index)
                else {
                    return;
                };
                let album_id = album.id.clone();
                stage(state, &mut |state| {
                    state.ui.album_mass_delete_prompt = Some(vec![album_id.clone()]);
                });
                return;
            }
            let Some(node) = state
                .data
                .library_view
                .get(state.ui.selected_playlist_index)
                .cloned()
            else {
                return;
            };
            match node {
                LibraryNode::Playlist { playlist, .. } => {
                    // Liked Songs and the local library are synthetic rows, not deletable; a
                    // Spotify playlist someone else owns can only be unfollowed, which the
                    // delete prompt does not model.
                    if playlist.id == "LIKED_SONGS" || playlist.id == "local-library" {
                        return;
                    }
                    let deletable = playlist.id.starts_with("local-playlist:")
                        || Some(&playlist.owner_id) == state.data.user_id.as_ref();
                    if !deletable {
                        return;
                    }
                    let playlist_id = playlist.id.clone();
                    stage(state, &mut |state| {
                        state.ui.playlist_delete_prompt = Some(vec![playlist_id.clone()]);
                    });
                }
                LibraryNode::Folder(folder) => {
                    let name = folder.name.clone();
                    stage(state, &mut |state| {
                        state.ui.folder_delete_prompt = Some(name.clone());
                    });
                }
            }
        }
        _ => {}
    }
}

/// Whether any delete/remove confirmation is pending.
pub fn prompt_active(state: &AppState) -> bool {
    state.ui.duplicate_prompt.is_some()
        || state.ui.folder_delete_prompt.is_some()
        || state.ui.playlist_delete_prompt.is_some()
        || state.ui.album_mass_delete_prompt.is_some()
        || state.ui.track_delete_prompt.is_some()
        || state.ui.liked_track_remove_prompt.is_some()
}

/// Confirms the pending prompt, performing the action or returning the worker event for it.
pub fn confirm_prompt(state: &mut AppState) -> Option<AppEvent> {
    if let Some(prompt) = state.ui.duplicate_prompt.take() {
        return Some(crate::action_menu::playlist_add_event(
            prompt.playlist_id,
            prompt.tracks,
            prompt.position,
        ));
    }
    if let Some(folder_name) = state.ui.folder_delete_prompt.take() {
        state.ui.playlist_delete_prompt = None;
        state
            .ui
            .library_config
            .folders
            .retain(|folder| folder.name != folder_name);
        state.save_library_config();
        state.compute_library_view();
        if state.ui.selected_playlist_index >= state.data.library_view.len() {
            state.ui.selected_playlist_index = state.data.library_view.len().saturating_sub(1);
        }
        return None;
    }
    if let Some(playlist_ids) = state.ui.playlist_delete_prompt.take() {
        return Some(AppEvent::DeletePlaylists(playlist_ids));
    }
    if let Some(album_ids) = state.ui.album_mass_delete_prompt.take() {
        return Some(AppEvent::RemoveAlbums(album_ids));
    }
    if let Some((playlist_id, track_ids)) = state.ui.track_delete_prompt.take() {
        return Some(AppEvent::RemoveTracksFromPlaylist(playlist_id, track_ids));
    }
    if let Some(track_id) = state.ui.liked_track_remove_prompt.take() {
        state.data.liked_tracks.remove(&track_id);
        crate::config::AppConfig::update_cache(|cache| {
            cache.liked_tracks = state.data.liked_tracks.clone()
        });
        return Some(AppEvent::ToggleTrackLike(track_id, false));
    }
    None
}

/// Dismisses whatever prompt is pending without acting on it.
pub fn cancel_prompt(state: &mut AppState) {
    state.ui.duplicate_prompt = None;
    state.ui.folder_delete_prompt = None;
    state.ui.playlist_delete_prompt = None;
    state.ui.album_mass_delete_prompt = None;
    state.ui.track_delete_prompt = None;
    state.ui.liked_track_remove_prompt = None;
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
    #[test]
    fn all_search_activation_uses_the_same_track_route_and_keeps_all_selected() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::SearchResults;
        state.ui.active_search_tab = SearchTab::All;
        state.ui.search_context_query = "Song".into();
        state.data.search_results.tracks = vec![serde_json::from_value(serde_json::json!({"id":"track","name":"Song","artist":"Artist","album":"Album","duration_ms":1000,"image_url":null,"album_id":null})).unwrap()];
        assert!(
            matches!(activate_search_result(&mut state, 0), Some(AppEvent::PlayTrack { track_id, .. }) if track_id == "track")
        );
        assert!(
            matches!(activate_search_result(&mut state, 1), Some(AppEvent::PlayTrack { track_id, .. }) if track_id == "track")
        );
        assert_eq!(state.ui.active_search_tab, SearchTab::All);
        assert_eq!(state.ui.selected_search_index, 1);
        assert!(activate_search_result(&mut state, 2).is_none());
    }

    #[test]
    fn recent_search_intents_remove_clear_and_leave_tui_default_alone() {
        let mut state = AppState::new();
        assert!(global_search(&mut state, " ").is_none());
        assert!(
            matches!(global_search(&mut state, " Echo "), Some(AppEvent::GlobalSearch(query)) if query == "Echo")
        );
        global_search(&mut state, "Other");
        global_search(&mut state, "eCHO");
        assert_eq!(state.ui.library_config.recent_searches, ["eCHO", "Other"]);
        assert_eq!(state.ui.active_search_tab, SearchTab::Tracks);
        remove_recent_search(&mut state, 10);
        assert_eq!(state.ui.library_config.recent_searches.len(), 2);
        remove_recent_search(&mut state, 0);
        assert_eq!(state.ui.library_config.recent_searches, ["Other"]);
        clear_recent_searches(&mut state);
        assert!(state.ui.library_config.recent_searches.is_empty());
    }
    #[test]
    fn restored_top_tracks_open_at_once_and_refresh_behind_the_view() {
        let mut state = AppState::new();
        state.data.top_tracks = vec![track("restored")];
        assert!(matches!(
            open_top_tracks(&mut state),
            Some(AppEvent::FetchTopTracks { .. })
        ));
        assert_eq!(state.ui.active_view, ActiveView::TrackList);
        assert!(state.data.home_fetches[&crate::home::HomeFeed::TopTracks].in_flight);
        assert!(open_top_tracks(&mut state).is_none());
    }

    #[test]
    fn home_navigation_fetches_once_then_refreshes_and_restores_selection() {
        let mut state = AppState::new();
        assert_eq!(open_home(&mut state).len(), 6);
        assert_eq!(state.ui.active_view, ActiveView::Home);
        assert!(open_home(&mut state).is_empty());
        for fetch in state.data.home_fetches.values_mut() {
            fetch.in_flight = false;
            fetch.fetched_at = Some(std::time::Instant::now());
        }
        for feed in [
            crate::home::HomeFeed::TopArtists,
            crate::home::HomeFeed::TopTracks,
        ] {
            state.data.home_fetches.get_mut(&feed).unwrap().range =
                Some(state.ui.library_config.top_items_range);
        }
        assert_eq!(open_home(&mut state).len(), 6);
        for fetch in state.data.home_fetches.values_mut() {
            fetch.in_flight = false;
        }
        state.ui.selected_home_index = 3;
        let item = crate::home::HomeItem {
            id: "album".into(),
            kind: crate::home::HomeItemKind::Album,
            title: "Album".into(),
            subtitle: "Artist".into(),
            image_url: Some("cover".into()),
            owner_id: None,
            track: None,
            release_year: None,
        };
        assert!(matches!(
            open_home_item(&mut state, item.clone()),
            Some(AppEvent::LoadContextTracks(_))
        ));
        assert_eq!(state.ui.active_view, ActiveView::TrackList);
        state.pop_view_history();
        assert_eq!(state.ui.active_view, ActiveView::Home);
        assert_eq!(state.ui.selected_home_index, 3);
        assert_eq!(refresh_home(&mut state).len(), 6);
        assert!(matches!(
            play_home_item(&mut state, item),
            Some(AppEvent::PlayContext { is_album: true, .. })
        ));
    }

    #[test]
    fn home_items_use_music_specific_open_and_play_routes() {
        crate::i18n::init();
        let mut state = AppState::new();
        let song = crate::home::HomeItem::from(&track("track"));
        assert!(matches!(
            open_home_item(&mut state, song),
            Some(AppEvent::PlayTrack {
                target: PlaybackTarget::SpotifyTrack { .. },
                ..
            })
        ));
        let artist = crate::home::HomeItem::from(&Artist {
            id: "artist".into(),
            name: "Artist".into(),
            image_url: None,
        });
        assert!(matches!(
            play_home_item(&mut state, artist.clone()),
            Some(AppEvent::PlayArtist { .. })
        ));
        assert!(matches!(
            open_home_item(&mut state, artist),
            Some(AppEvent::LoadArtistPage { .. })
        ));
        let liked = crate::home::home_shelves(&state).remove(0).items.remove(0);
        assert!(
            matches!(play_home_item(&mut state, liked.clone()), Some(AppEvent::PlayContext { ref context_id, .. }) if context_id == "LIKED_SONGS")
        );
        assert!(matches!(
            open_home_item(&mut state, liked),
            Some(AppEvent::LoadContextTracks(_))
        ));
        assert_eq!(
            state.data.active_tracklist_context.as_ref().unwrap().title,
            "Liked Songs"
        );
    }

    #[test]
    fn column_sort_toggles_direction_and_preserves_selection() {
        use crate::app::TrackSort;
        let mut state = AppState::new();
        let mut a = track("a");
        a.name = "Alpha".into();
        a.artist = "Alpha".into();
        a.album = "Alpha".into();
        a.duration_ms = 1;
        a.added_at = Some("2026-01-01T00:00:00Z".into());
        a.added_by = Some("alice".into());
        let mut b = a.clone();
        b.id = "b".into();
        b.name = "Beta".into();
        b.artist = "Beta".into();
        b.album = "Beta".into();
        b.duration_ms = 2;
        b.added_at = Some("2026-01-02T00:00:00Z".into());
        b.added_by = Some("Bob".into());
        state.data.tracks = vec![b, a];
        state.data.original_tracks = state.data.tracks.clone();
        for sort in [
            TrackSort::Title,
            TrackSort::Artist,
            TrackSort::Album,
            TrackSort::Duration,
            TrackSort::Added,
            TrackSort::AddedBy,
        ] {
            sort_by_column(&mut state, sort);
            assert_eq!(state.data.tracks[0].id, "a");
            assert_eq!(state.ui.selected_track_index, 1);
            sort_by_column(&mut state, sort);
            assert_eq!(state.data.tracks[0].id, "b");
            assert_eq!(state.ui.selected_track_index, 0);
            state.sort_tracks(sort);
            assert_eq!(state.data.tracks[0].id, "b");
        }
        sort_by_column(&mut state, TrackSort::Original);
        assert!(state.ui.track_sort_ascending);
        assert_eq!(state.data.tracks, state.data.original_tracks);
        sort_by_column(&mut state, TrackSort::Original);
        assert_eq!(state.data.tracks, state.data.original_tracks);
    }

    #[test]
    fn header_playback_and_shuffle_use_the_active_context() {
        let mut state = AppState::new();
        assert!(play_context_from_header(&mut state).is_none());
        assert!(toggle_context_shuffle(&mut state).is_empty());
        state.data.active_tracklist_context = Some(TrackListContext::album(
            "album".into(),
            "Album".into(),
            "Artist".into(),
            None,
        ));
        state.data.tracks = vec![track("a")];
        assert!(!active_context_is_playing(&state));
        assert!(matches!(
            play_context_from_header(&mut state),
            Some(AppEvent::PlayTrack { .. })
        ));
        assert!(active_context_is_playing(&state));
        state.playback.is_playing = true;
        assert!(matches!(
            play_context_from_header(&mut state),
            Some(AppEvent::TogglePlayback(false))
        ));
        assert!(matches!(
            play_context_from_header(&mut state),
            Some(AppEvent::TogglePlayback(true))
        ));
        state.playback.is_shuffled = false;
        assert!(matches!(
            toggle_context_shuffle(&mut state).as_slice(),
            [AppEvent::ToggleShuffle(true)]
        ));
        state.playback.playing_context = None;
        assert!(matches!(
            toggle_context_shuffle(&mut state).as_slice(),
            [AppEvent::PlayTrack { .. }, AppEvent::ToggleShuffle(true)]
        ));
    }

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

    fn library_playlist(id: &str) -> crate::models::Playlist {
        crate::models::Playlist {
            description: None,
            public: None,
            collaborative: false,
            track_count: None,
            snapshot_id: None,
            id: id.to_string(),
            name: id.to_string(),
            owner: String::new(),
            owner_id: "owner".to_string(),
            image_url: None,
            thumb_url: None,
        }
    }

    fn loose_view_ids(state: &AppState) -> Vec<String> {
        state
            .data
            .library_view
            .iter()
            .filter_map(|node| match node {
                LibraryNode::Playlist { playlist, .. } if !is_fixed_library_row(&playlist.id) => {
                    Some(playlist.id.clone())
                }
                _ => None,
            })
            .collect()
    }

    fn library_with(playlists: Vec<crate::models::Playlist>) -> AppState {
        let mut state = AppState::new();
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.ui.active_view = ActiveView::Library;
        state.data.user_id = Some("owner".to_string());
        state.data.playlists = playlists;
        state.compute_library_view();
        state
    }

    fn select_row(state: &mut AppState, id: &str) {
        state.ui.selected_playlist_index = state
            .data
            .library_view
            .iter()
            .position(|node| {
                matches!(
                    node,
                    LibraryNode::Playlist { playlist, .. } if playlist.id == id
                )
            })
            .expect("row exists");
    }

    fn track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
            id: id.to_string(),
            source: TrackSource::Spotify,
            local_path: None,
            name: id.to_string(),
            artist: String::new(),
            album: String::new(),
            added_at: None,
            duration_ms: 1000,
            image_url: None,
            album_id: None,
            artist_id: None,
            artists: Vec::new(),
        }
    }

    #[test]
    fn discography_filter_narrows_the_visible_albums_and_resets_the_cursor() {
        use crate::models::AlbumGroup;
        let mut state = artist_page_with(&["t0"], 4);
        if let Some(data) = state.data.artist_page_data.as_mut() {
            data.discography = data.albums.clone();
            for (album, group) in data.discography.iter_mut().zip([
                AlbumGroup::Album,
                AlbumGroup::Single,
                AlbumGroup::Compilation,
                AlbumGroup::AppearsOn,
            ]) {
                album.group = Some(group);
            }
        }
        state.ui.artist_page_album_index = 3;
        let names = |state: &AppState| -> Vec<String> {
            state
                .data
                .artist_page_data
                .as_ref()
                .map_or_else(Vec::new, |data| {
                    data.albums.iter().map(|album| album.id.clone()).collect()
                })
        };

        set_discography_filter(&mut state, DiscographyFilter::Singles);
        assert_eq!(names(&state), ["album-1"]);
        assert_eq!(state.ui.artist_page_album_index, 0);
        set_discography_filter(&mut state, DiscographyFilter::Albums);
        assert_eq!(names(&state), ["album-0", "album-2"]);
        set_discography_filter(&mut state, DiscographyFilter::AppearsOn);
        assert_eq!(names(&state), ["album-3"]);
        cycle_discography_filter(&mut state);
        assert_eq!(state.ui.artist_discography_filter, DiscographyFilter::All);
        assert_eq!(names(&state), ["album-0", "album-1", "album-2"]);
    }

    #[test]
    fn follow_toggle_flips_the_followed_list_before_the_write() {
        let mut state = artist_page_with(&[], 0);
        assert!(matches!(
            toggle_follow_artist(&mut state),
            Some(AppEvent::FollowArtist(id)) if id == "artist"
        ));
        assert_eq!(state.data.followed_artists[0].name, "Artist");
        assert!(matches!(
            toggle_follow_artist(&mut state),
            Some(AppEvent::UnfollowArtist(id)) if id == "artist"
        ));
        assert!(state.data.followed_artists.is_empty());
        state.data.artist_page_data = None;
        assert!(toggle_follow_artist(&mut state).is_none());
    }

    #[test]
    fn queueing_on_the_artist_page_takes_the_popular_row_under_the_cursor() {
        let mut state = artist_page_with(&["t0", "t1"], 2);
        state.ui.artist_page_album_index = 1;
        assert!(
            matches!(queue_selected_track(&state), Some(AppEvent::AddToQueue(ids)) if ids == ["t1"])
        );
        state.ui.artist_page_album_index = 2;
        assert!(queue_selected_track(&state).is_none());
        state.ui.active_view = ActiveView::ArtistList;
        assert!(selected_artist_top_track(&state).is_none());
    }

    fn artist_page_with(top_ids: &[&str], album_count: usize) -> AppState {
        let mut state = AppState::new();
        state.begin_artist_page_load("artist".to_string(), "Artist".to_string(), None);
        if let Some(data) = state.data.artist_page_data.as_mut() {
            data.top_tracks = top_ids.iter().map(|id| track(id)).collect();
            data.albums = (0..album_count)
                .map(|i| crate::models::Album {
                    id: format!("album-{i}"),
                    name: format!("Album {i}"),
                    artists: "Artist".to_string(),
                    image_url: None,
                    thumb_url: None,
                    release_year: "2024".to_string(),
                    release_date: None,
                    track_count: None,
                    group: None,
                })
                .collect();
        }
        state
    }

    #[test]
    fn artist_page_row_inside_popular_plays_that_top_track() {
        let mut state = artist_page_with(&["t0", "t1", "t2"], 2);

        let event = activate_artist_page_row(&mut state, 1);

        assert_eq!(state.ui.artist_page_album_index, 1);
        match event {
            Some(AppEvent::PlayTrack {
                target:
                    PlaybackTarget::SpotifyTracks {
                        track_ids,
                        selected_index,
                    },
                track_id,
                ..
            }) => {
                assert_eq!(track_ids, vec!["t0", "t1", "t2"]);
                assert_eq!(selected_index, 1);
                assert_eq!(track_id, "t1");
            }
            _ => panic!("expected a SpotifyTracks play event"),
        }
    }

    #[test]
    fn artist_page_row_at_popular_boundary_opens_the_first_album() {
        let mut state = artist_page_with(&["t0", "t1", "t2"], 2);

        let event = activate_artist_page_row(&mut state, 3);

        assert_eq!(state.ui.artist_page_album_index, 3);
        match event {
            Some(AppEvent::LoadContextTracks(context)) => assert_eq!(context.id, "album-0"),
            _ => panic!("expected LoadContextTracks for the first album"),
        }
    }

    #[test]
    fn playlist_search_result_opens_a_playlist_context() {
        let mut state = AppState::new();
        state.ui.active_search_tab = SearchTab::Playlists;
        state
            .data
            .search_results
            .playlists
            .push(crate::models::Playlist {
                description: None,
                public: None,
                collaborative: false,
                track_count: None,
                snapshot_id: None,
                id: "pl".to_string(),
                name: "Mix".to_string(),
                owner: "Owner".to_string(),
                owner_id: "owner-id".to_string(),
                image_url: Some("cover".to_string()),
                thumb_url: None,
            });

        let event = activate_search_result(&mut state, 0);

        match event {
            Some(AppEvent::LoadContextTracks(context)) => {
                assert_eq!(context.id, "pl");
                assert_eq!(context.kind, crate::models::TrackListContextKind::Playlist);
                assert_eq!(context.subtitle, "Owner");
                assert_eq!(context.owner_id.as_deref(), Some("owner-id"));
                assert_eq!(context.image_url.as_deref(), Some("cover"));
            }
            _ => panic!("expected a playlist LoadContextTracks"),
        }
    }

    #[test]
    fn open_top_artists_opens_the_list_and_fetches_when_empty() {
        let mut state = AppState::new();

        let event = open_top_artists(&mut state);

        assert!(matches!(event, Some(AppEvent::FetchTopArtists { .. })));
        assert_eq!(state.ui.active_view, ActiveView::ArtistList);
        assert_eq!(
            state.ui.artist_list_source,
            crate::app::ArtistListSource::Top
        );

        state.data.top_artists.push(crate::models::Artist {
            id: "a".to_string(),
            name: "A".to_string(),
            image_url: None,
        });
        assert!(open_top_artists(&mut state).is_none());
    }

    #[test]
    fn open_artist_at_reads_the_active_list_source() {
        let mut state = AppState::new();
        state.data.followed_artists.push(crate::models::Artist {
            id: "followed".to_string(),
            name: "Followed".to_string(),
            image_url: None,
        });
        state.data.top_artists.push(crate::models::Artist {
            id: "top".to_string(),
            name: "Top".to_string(),
            image_url: None,
        });

        state.ui.artist_list_source = crate::app::ArtistListSource::Top;
        let event = open_artist_at(&mut state, 0);
        match event {
            Some(AppEvent::LoadArtistPage { artist_id, .. }) => assert_eq!(artist_id, "top"),
            _ => panic!("expected LoadArtistPage for the top-artists row"),
        }
    }

    #[test]
    fn switching_range_refetches_an_open_top_tracks_list_in_place() {
        let mut state = AppState::new();
        state.data.top_tracks = vec![track("t")];
        open_top_tracks(&mut state);
        assert_eq!(state.ui.active_view, ActiveView::TrackList);
        let history_depth = state.ui.view_history.len();

        let event = set_top_items_range(&mut state, crate::models::TopItemsRange::Short);

        assert!(matches!(
            event,
            Some(AppEvent::FetchTopTracks {
                range: crate::models::TopItemsRange::Short
            })
        ));
        assert!(state.data.top_tracks.is_empty());
        assert_eq!(state.ui.pending_browse_open, None);
        assert_eq!(state.ui.view_history.len(), history_depth);
    }

    #[test]
    fn switching_to_the_same_range_is_inert() {
        let mut state = AppState::new();

        assert!(set_top_items_range(&mut state, crate::models::TopItemsRange::Medium).is_none());
    }

    fn tracklist_with(ids: &[&str]) -> AppState {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::TrackList;
        state.data.tracks = ids.iter().map(|id| track(id)).collect();
        state
    }

    #[test]
    fn a_visual_range_covers_every_row_between_anchor_and_cursor() {
        let mut state = tracklist_with(&["a", "b", "c", "d"]);
        state.ui.selected_track_index = 1;
        enter_visual(&mut state);
        state.ui.selected_track_index = 3;

        let ids: Vec<String> = visual_tracks(&state).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["b", "c", "d"]);
    }

    #[test]
    fn a_visual_range_works_when_dragged_upwards() {
        let mut state = tracklist_with(&["a", "b", "c", "d"]);
        state.ui.selected_track_index = 2;
        enter_visual(&mut state);
        state.ui.selected_track_index = 0;

        let ids: Vec<String> = visual_tracks(&state).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["a", "b", "c"]);
    }

    #[test]
    fn queueing_a_range_emits_one_event_and_leaves_visual_mode() {
        let mut state = tracklist_with(&["a", "b", "c"]);
        enter_visual(&mut state);
        state.ui.selected_track_index = 1;

        let event = queue_visual_selection(&mut state);

        match event {
            Some(AppEvent::AddToQueue(ids)) => assert_eq!(ids, ["a", "b"]),
            other => panic!("expected AddToQueue, got {}", other.is_some()),
        }
        assert!(state.ui.visual_selection_start.is_none());
        assert!(state.ui.mode == crate::app::AppMode::Normal);
    }

    #[test]
    fn adding_a_range_to_a_playlist_stages_it_in_the_operation_register() {
        let mut state = tracklist_with(&["a", "b", "c"]);
        enter_visual(&mut state);
        state.ui.selected_track_index = 2;

        add_visual_selection_to_playlist(&mut state);

        // commit_playlist_add reads the register, so the range outlives visual mode.
        assert_eq!(state.ui.operation_register, ["a", "b", "c"]);
        assert!(state.ui.playlist_add_modal_open);
        assert!(state.ui.visual_selection_start.is_none());
    }

    #[test]
    fn a_stale_cursor_index_does_not_panic_the_range() {
        let mut state = tracklist_with(&["a", "b"]);
        enter_visual(&mut state);
        // Longer list replaced by a shorter one while the range was open.
        state.ui.selected_track_index = 9;

        let ids: Vec<String> = visual_tracks(&state).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["a", "b"]);
    }

    #[test]
    fn a_single_d_arms_and_the_second_stages_the_delete_prompt() {
        let mut state = library_with(vec![library_playlist("mine")]);
        select_row(&mut state, "mine");

        mark_selected_for_delete(&mut state);
        assert!(state.ui.pending_d_press);
        assert!(state.ui.playlist_delete_prompt.is_none());

        mark_selected_for_delete(&mut state);
        assert!(!state.ui.pending_d_press);
        assert_eq!(
            state.ui.playlist_delete_prompt.as_deref(),
            Some(["mine".to_string()].as_slice())
        );
    }

    #[test]
    fn delete_never_arms_on_rows_that_cannot_be_deleted() {
        let mut state = library_with(vec![library_playlist("mine")]);

        // Liked Songs is a synthetic row with nothing behind it to delete.
        select_row(&mut state, "LIKED_SONGS");
        mark_selected_for_delete(&mut state);
        assert!(!state.ui.pending_d_press);
        assert!(state.ui.playlist_delete_prompt.is_none());

        // A playlist owned by someone else can only be unfollowed, which this does not model.
        let mut other = library_playlist("theirs");
        other.owner_id = "someone-else".to_string();
        let mut state = library_with(vec![other]);
        select_row(&mut state, "theirs");
        mark_selected_for_delete(&mut state);
        mark_selected_for_delete(&mut state);
        assert!(!state.ui.pending_d_press);
        assert!(state.ui.playlist_delete_prompt.is_none());
    }

    #[test]
    fn dragging_a_loose_playlist_reorders_and_persists_a_custom_order() {
        let mut state = AppState::new();
        // In unit tests AppState::new() loads from the redirected (temp) config root, but an
        // explicit default keeps the view predictable regardless of what other tests saved.
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.data.playlists = vec![
            library_playlist("a"),
            library_playlist("b"),
            library_playlist("c"),
        ];
        state.compute_library_view();
        let row_of_a = state
            .data
            .library_view
            .iter()
            .position(|node| {
                matches!(
                    node,
                    LibraryNode::Playlist { playlist, .. } if playlist.id == "a"
                )
            })
            .expect("row for a");
        // Drop "c" onto "a" to move it before "a".
        assert!(move_library_playlist(&mut state, "c", row_of_a));

        assert_eq!(loose_view_ids(&state), ["c", "a", "b"]);
        assert_eq!(state.ui.library_config.playlist_order, ["c", "a", "b"]);
    }

    #[test]
    fn dropping_onto_a_folder_header_moves_the_playlist_inside() {
        let mut state = AppState::new();
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.data.playlists = vec![library_playlist("a"), library_playlist("b")];
        state.ui.library_config.folders.push(crate::config::Folder {
            name: "Mix".to_string(),
            is_open: true,
            playlists: vec![],
        });
        state.compute_library_view();
        let folder_row = state
            .data
            .library_view
            .iter()
            .position(|node| matches!(node, LibraryNode::Folder(_)))
            .expect("folder row");

        assert!(move_library_playlist(&mut state, "a", folder_row));

        assert_eq!(state.ui.library_config.folders[0].playlists, ["a"]);
        assert!(state.data.library_view.iter().any(|node| matches!(
            node,
            LibraryNode::Playlist { playlist, indent: 1 } if playlist.id == "a"
        )));
    }

    #[test]
    fn a_local_playlist_can_be_dragged_into_a_folder() {
        let mut state = AppState::new();
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.data.playlists = vec![library_playlist("a")];
        state.data.local_playlists = crate::models::LocalPlaylists {
            playlists: vec![crate::models::LocalPlaylist {
                id: "local-playlist:one".to_string(),
                name: "Road".to_string(),
                created_unix_secs: 1,
                updated_unix_secs: 1,
                entries: Vec::new(),
            }],
        };
        state.ui.library_config.folders.push(crate::config::Folder {
            name: "Mix".to_string(),
            is_open: true,
            playlists: vec![],
        });
        state.compute_library_view();
        let folder_row = state
            .data
            .library_view
            .iter()
            .position(|node| matches!(node, LibraryNode::Folder(_)))
            .expect("folder row");

        assert!(move_library_playlist(
            &mut state,
            "local-playlist:one",
            folder_row
        ));

        assert_eq!(
            state.ui.library_config.folders[0].playlists,
            ["local-playlist:one"]
        );
        assert!(state.data.library_view.iter().any(|node| matches!(
            node,
            LibraryNode::Playlist { playlist, indent: 1 } if playlist.id == "local-playlist:one"
        )));
    }

    #[test]
    fn fixed_rows_reject_drag_and_drop() {
        let mut state = AppState::new();
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.data.playlists = vec![library_playlist("a")];
        state.compute_library_view();

        assert!(!move_library_playlist(&mut state, "LIKED_SONGS", 1));
        // Dropping onto Liked Songs (row 0) is rejected too.
        assert!(!move_library_playlist(&mut state, "a", 0));
    }

    #[test]
    fn confirming_a_playlist_delete_prompt_emits_the_event() {
        let mut state = AppState::new();
        state.ui.playlist_delete_prompt = Some(vec!["p".to_string()]);

        assert!(prompt_active(&state));
        let Some(AppEvent::DeletePlaylists(ids)) = confirm_prompt(&mut state) else {
            panic!("expected DeletePlaylists");
        };
        assert_eq!(ids, ["p"]);
        assert!(!prompt_active(&state));
    }

    #[test]
    fn cancelling_a_prompt_clears_it_without_an_event() {
        let mut state = AppState::new();
        state.ui.album_mass_delete_prompt = Some(vec!["album".to_string()]);

        cancel_prompt(&mut state);

        assert!(!prompt_active(&state));
    }

    #[test]
    fn selecting_followed_artist_opens_partial_shell_immediately() {
        let mut state = AppState::new();
        state.data.followed_artists.push(Artist {
            id: "artist".to_string(),
            name: "Artist".to_string(),
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
            explicit: false,
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
            search_track(
                "local:a",
                "Local A",
                TrackSource::Local,
                Some("/music/a.wav"),
            ),
            search_track(
                "local:b",
                "Local B",
                TrackSource::Local,
                Some("/music/b.wav"),
            ),
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
    fn skipping_pops_the_queue_head_and_keeps_the_cursor_on_its_track() {
        let mut state = AppState::new();
        state.data.queue = vec![
            spotify_track("q:a"),
            spotify_track("ctx:a"),
            spotify_track("ctx:b"),
        ];
        state.data.manual_queue = vec!["q:a".to_string()];
        state.ui.selected_queue_index = 2;

        assert!(matches!(next_track(&mut state), AppEvent::NextTrack { .. }));
        assert_eq!(state.data.queue[0].id, "ctx:a");
        assert!(state.data.manual_queue.is_empty());
        assert_eq!(state.ui.selected_queue_index, 1);
    }

    #[test]
    fn clear_queue_drops_the_manual_head_and_emits_the_event() {
        crate::i18n::init();
        let mut state = AppState::new();
        state.data.queue = vec![spotify_track("q:a"), spotify_track("ctx:a")];
        state.data.manual_queue = vec!["q:a".to_string()];
        state.ui.selected_queue_index = 1;

        assert!(matches!(
            clear_queue(&mut state),
            Some(AppEvent::ClearQueue)
        ));
        assert!(state.data.manual_queue.is_empty());
        assert_eq!(state.data.queue.len(), 1);
        assert_eq!(state.ui.selected_queue_index, 0);
    }

    #[test]
    fn clear_queue_is_a_no_op_without_manual_tracks_or_on_another_device() {
        crate::i18n::init();
        let mut state = AppState::new();
        state.data.queue = vec![spotify_track("ctx:a")];
        assert!(clear_queue(&mut state).is_none());
        assert!(state.ui.status_message.is_none());

        state.data.manual_queue = vec!["ctx:a".to_string()];
        state.playback.device_name = "phone".to_string();
        assert!(clear_queue(&mut state).is_none());
        assert_eq!(state.data.queue.len(), 1);
        assert!(state.ui.status_message.is_some());
    }

    #[test]
    fn queue_jump_replays_the_live_context_with_the_track_as_offset() {
        let mut state = AppState::new();
        state.playback.playing_context = Some(PlayingContext {
            context_id: "playlist-1".to_string(),
            is_album: false,
        });
        state.data.queue = vec![spotify_track("queue:a"), spotify_track("queue:b")];

        let Some(AppEvent::PlayTrack {
            target, track_id, ..
        }) = play_queue_track_at(&mut state, 1)
        else {
            panic!("expected play event");
        };

        assert_eq!(track_id, "queue:b");
        assert_eq!(state.ui.selected_queue_index, 1);
        assert_eq!(
            target,
            PlaybackTarget::SpotifyContextJump {
                context_id: "playlist-1".to_string(),
                is_album: false,
            }
        );
    }

    #[test]
    fn queue_jump_without_a_context_plays_the_track_standalone() {
        let mut state = AppState::new();
        state.data.queue = vec![spotify_track("queue:a")];

        let Some(AppEvent::PlayTrack { target, .. }) = play_queue_track_at(&mut state, 0) else {
            panic!("expected play event");
        };

        assert_eq!(
            target,
            PlaybackTarget::SpotifyTrack {
                track_id: "queue:a".to_string()
            }
        );
    }

    fn owned_playlist_state(ids: &[&str]) -> AppState {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::TrackList;
        state.data.user_id = Some("owner-id".to_string());
        state.data.active_tracklist_context = Some(TrackListContext::playlist(
            "playlist-1".to_string(),
            "Playlist".to_string(),
            "owner".to_string(),
            "owner-id".to_string(),
            None,
        ));
        state.data.tracks = ids.iter().map(|id| track(id)).collect();
        state.data.original_tracks = state.data.tracks.clone();
        state
    }

    #[test]
    fn move_track_reorders_optimistically_and_reports_the_move() {
        let mut state = owned_playlist_state(&["a", "b", "c"]);

        let event = move_track_in_playlist(&mut state, 0, 2);

        let order: Vec<&str> = state.data.tracks.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(order, vec!["b", "c", "a"]);
        let original: Vec<&str> = state
            .data
            .original_tracks
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(original, vec!["b", "c", "a"]);
        assert_eq!(state.ui.selected_track_index, 2);
        assert!(matches!(
            event,
            Some(AppEvent::MoveTrack { from: 0, to: 2, ref track_id, .. }) if track_id == "a"
        ));
    }

    #[test]
    fn move_track_refuses_sorted_views_and_foreign_playlists() {
        crate::i18n::init();
        let mut state = owned_playlist_state(&["a", "b"]);
        state.ui.track_sort = crate::app::TrackSort::Title;
        assert!(move_track_in_playlist(&mut state, 0, 1).is_none());
        assert_eq!(state.data.tracks[0].id, "a");

        let mut state = owned_playlist_state(&["a", "b"]);
        state.data.user_id = Some("someone-else".to_string());
        assert!(move_track_in_playlist(&mut state, 0, 1).is_none());
        assert_eq!(state.data.tracks[0].id, "a");

        let mut state = owned_playlist_state(&["a", "b"]);
        assert!(move_track_in_playlist(&mut state, 0, 0).is_none());
        assert!(move_track_in_playlist(&mut state, 5, 0).is_none());
    }

    #[test]
    fn context_plays_note_the_playing_context_before_the_next_poll() {
        let mut state = AppState::new();
        state.data.active_tracklist_context = Some(TrackListContext::playlist(
            "playlist-1".to_string(),
            "Playlist".to_string(),
            "owner".to_string(),
            "owner-id".to_string(),
            None,
        ));
        state.data.tracks = vec![spotify_track("track:a")];

        assert!(play_track_at(&mut state, 0).is_some());
        assert_eq!(
            state.playback.playing_context,
            Some(PlayingContext {
                context_id: "playlist-1".to_string(),
                is_album: false,
            })
        );

        state.data.active_tracklist_context =
            Some(TrackListContext::generated("TOP_TRACKS", "Top Tracks"));
        assert!(play_track_at(&mut state, 0).is_some());
        assert_eq!(state.playback.playing_context, None);
    }

    fn playlist_tracklist_state(context_id: &str) -> AppState {
        let mut state = AppState::new();
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.data.active_tracklist_context = Some(TrackListContext::playlist(
            context_id.to_string(),
            "Playlist".to_string(),
            "owner".to_string(),
            "owner-id".to_string(),
            None,
        ));
        state.data.tracks = vec![spotify_track("track:a")];
        state
    }

    #[test]
    fn shuffle_and_repeat_toggles_record_the_playing_playlists_pref() {
        let mut state = AppState::new();
        state.ui.library_config = crate::config::LibraryConfig::default();
        state.playback.playing_context = Some(PlayingContext {
            context_id: "playlist-1".to_string(),
            is_album: false,
        });

        toggle_shuffle(&mut state);
        cycle_repeat(&mut state);

        assert_eq!(
            state.ui.library_config.playlist_playback.get("playlist-1"),
            Some(&crate::config::PlaylistPlaybackPref {
                shuffle: true,
                repeat: "Context".to_string(),
            })
        );
    }

    #[test]
    fn toggles_record_nothing_for_albums_liked_songs_or_no_context() {
        for context in [
            None,
            Some(PlayingContext {
                context_id: "album-1".to_string(),
                is_album: true,
            }),
            Some(PlayingContext {
                context_id: "LIKED_SONGS".to_string(),
                is_album: false,
            }),
        ] {
            let mut state = AppState::new();
            state.ui.library_config = crate::config::LibraryConfig::default();
            state.playback.playing_context = context;
            toggle_shuffle(&mut state);
            cycle_repeat(&mut state);
            assert!(state.ui.library_config.playlist_playback.is_empty());
        }
    }

    #[test]
    fn playlist_play_applies_the_saved_pref_optimistically() {
        let mut state = playlist_tracklist_state("playlist-1");
        state.ui.library_config.playlist_playback.insert(
            "playlist-1".to_string(),
            crate::config::PlaylistPlaybackPref {
                shuffle: true,
                repeat: "Context".to_string(),
            },
        );

        assert!(matches!(
            play_track_at(&mut state, 0),
            Some(AppEvent::PlayTrack { .. })
        ));
        assert!(state.playback.is_shuffled);
        assert_eq!(state.playback.repeat_mode, "Context");
    }

    #[test]
    fn playlist_play_snapshots_current_state_when_no_pref_exists() {
        let mut state = library_with(vec![library_playlist("playlist-1")]);
        state.playback.is_shuffled = true;
        let index = state
            .data
            .library_view
            .iter()
            .position(|node| {
                matches!(
                    node,
                    LibraryNode::Playlist { playlist, .. } if playlist.id == "playlist-1"
                )
            })
            .expect("row exists");

        assert!(matches!(
            play_library_entry(&mut state, index),
            Some(AppEvent::PlayContext {
                is_album: false,
                ..
            })
        ));
        assert_eq!(
            state.ui.library_config.playlist_playback.get("playlist-1"),
            Some(&crate::config::PlaylistPlaybackPref {
                shuffle: true,
                repeat: "Off".to_string(),
            })
        );
        assert_eq!(
            state.playback.playing_context,
            Some(PlayingContext {
                context_id: "playlist-1".to_string(),
                is_album: false,
            })
        );
    }

    #[test]
    fn context_plays_carry_the_ui_track_id_for_the_post_play_sync() {
        let mut state = library_with(vec![library_playlist("playlist-1")]);
        state.playback.playing_track_id = Some("old-track".to_string());
        let index = state
            .data
            .library_view
            .iter()
            .position(|node| {
                matches!(
                    node,
                    LibraryNode::Playlist { playlist, .. } if playlist.id == "playlist-1"
                )
            })
            .expect("row exists");
        assert!(matches!(
            play_library_entry(&mut state, index),
            Some(AppEvent::PlayContext { current_track_id: Some(ref id), .. }) if id == "old-track"
        ));

        let mut state = AppState::new();
        state.playback.playing_track_id = Some("old-track".to_string());
        state.data.saved_albums = vec![crate::models::Album {
            id: "album-1".to_string(),
            name: "Album".to_string(),
            artists: "Artist".to_string(),
            image_url: None,
            thumb_url: None,
            release_year: "2024".to_string(),
            release_date: None,
            track_count: None,
            group: None,
        }];
        assert!(matches!(
            play_album_at(&mut state, 0),
            Some(AppEvent::PlayContext { current_track_id: Some(ref id), is_album: true, .. }) if id == "old-track"
        ));
    }

    #[test]
    fn liked_songs_play_neither_applies_nor_snapshots_a_pref() {
        let mut state = playlist_tracklist_state("LIKED_SONGS");
        state.ui.library_config.playlist_playback.insert(
            "LIKED_SONGS".to_string(),
            crate::config::PlaylistPlaybackPref {
                shuffle: true,
                repeat: "Track".to_string(),
            },
        );

        assert!(play_track_at(&mut state, 0).is_some());
        assert!(!state.playback.is_shuffled);
        assert_eq!(state.playback.repeat_mode, "Off");

        let mut state = playlist_tracklist_state("LIKED_SONGS");
        assert!(play_track_at(&mut state, 0).is_some());
        assert!(state.ui.library_config.playlist_playback.is_empty());
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
            explicit: false,
            added_by: None,
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
            artists: Vec::new(),
        }
    }

    fn spotify_track(id: &str) -> Track {
        Track {
            explicit: false,
            added_by: None,
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
            artists: Vec::new(),
        }
    }

    #[test]
    fn the_recent_tab_plays_its_rows_standalone_and_shares_the_selection() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::Queue;
        state.playback.playing_context = Some(PlayingContext {
            context_id: "pl".to_string(),
            is_album: false,
        });
        state.data.queue = vec![spotify_track("q")];
        state.data.recent_plays = vec![
            crate::history::PlayRecord {
                track: spotify_track("r1"),
                played_at: "2026-09-16T10:00:00Z".to_string(),
                context: None,
            },
            crate::history::PlayRecord {
                track: spotify_track("r2"),
                played_at: "2026-09-16T09:00:00Z".to_string(),
                context: None,
            },
        ];
        state.ui.selected_queue_index = 0;

        let Some(AppEvent::PlayTrack { target, .. }) = play_queue_track_at(&mut state, 0) else {
            panic!("expected a queue play");
        };
        assert!(matches!(target, PlaybackTarget::SpotifyContextJump { .. }));

        cycle_queue_tab(&mut state);
        assert_eq!(state.ui.queue_tab, crate::app::QueueTab::Recent);
        assert_eq!(state.queue_view_len(), 2);
        assert_eq!(
            row_track(&state, 1).map(|track| track.id.as_str()),
            Some("r2")
        );
        let Some(AppEvent::PlayTrack {
            target, track_id, ..
        }) = play_queue_track_at(&mut state, 1)
        else {
            panic!("expected a recent play");
        };
        assert_eq!(track_id, "r2");
        assert_eq!(
            target,
            PlaybackTarget::SpotifyTrack {
                track_id: "r2".to_string()
            }
        );
        assert_eq!(state.ui.selected_queue_index, 1);

        cycle_queue_tab(&mut state);
        assert_eq!(state.ui.queue_tab, crate::app::QueueTab::Queue);
        assert_eq!(state.ui.selected_queue_index, 0);
    }

    #[test]
    fn picks_seed_with_the_focused_row_and_drive_the_range_operations() {
        let mut state = AppState::new();
        state.ui.active_view = ActiveView::TrackList;
        state.data.tracks = (0..5).map(|ix| spotify_track(&ix.to_string())).collect();
        state.ui.selected_track_index = 1;

        toggle_picked_row(&mut state, 3);
        assert_eq!(selected_rows(&state), vec![1, 3]);
        assert_eq!(state.ui.selected_track_index, 3);
        assert!(has_multi_selection(&state));
        let ids: Vec<_> = visual_tracks(&state).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["1", "3"]);

        toggle_picked_row(&mut state, 1);
        assert_eq!(selected_rows(&state), vec![3]);

        set_visual_anchor(&mut state, 0);
        assert!(state.ui.picked_rows.is_empty());
        state.ui.selected_track_index = 2;
        assert_eq!(selected_rows(&state), vec![0, 1, 2]);
        toggle_picked_row(&mut state, 4);
        assert!(state.get_visual_selection_range().is_none());
        assert_eq!(selected_rows(&state), vec![2, 4]);

        exit_visual(&mut state);
        assert!(!has_multi_selection(&state));
        assert_eq!(selected_rows(&state), vec![4]);

        state.ui.active_view = ActiveView::Home;
        assert!(selected_rows(&state).is_empty());
        toggle_picked_row(&mut state, 0);
        assert!(state.ui.picked_rows.is_empty());
    }
}
