use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{
    app::{AppState, TrackSort},
    events::AppEvent,
    handlers::navigation::{self, NavigationCommand},
};

const SEQUENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeymapAction {
    First,
    Last,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    CurrentContext,
    PlayPause,
    Next,
    Previous,
    Shuffle,
    Repeat,
    SeekBackward,
    SeekForward,
    SeekStart,
    Mute,
    SortOriginal,
    SortTitle,
    SortArtist,
    SortAlbum,
    SortDuration,
    SortAdded,
    ReverseTracks,
}

pub struct ConfiguredKey {
    pub consumed: bool,
    pub action: Option<KeymapAction>,
}

pub fn configured_action(state: &mut AppState, key: &KeyEvent) -> ConfiguredKey {
    if state.ui.library_config.keybindings.is_empty() {
        return ConfiguredKey { consumed: false, action: None };
    }
    if state
        .ui
        .pending_key_sequence
        .as_ref()
        .is_some_and(|(_, started)| started.elapsed() >= SEQUENCE_TIMEOUT)
    {
        state.ui.pending_key_sequence = None;
    }
    let token = key_token(key);
    if let Some((prefix, _)) = state.ui.pending_key_sequence.as_ref() {
        let sequence = format!("{prefix} {token}");
        if let Some(action) = state
            .ui
            .library_config
            .keybindings
            .get(&sequence)
            .and_then(|name| parse_action(name))
        {
            state.ui.pending_key_sequence = None;
            return ConfiguredKey { consumed: true, action: Some(action) };
        }
    }
    if let Some(action) = state
        .ui
        .library_config
        .keybindings
        .get(&token)
        .and_then(|name| parse_action(name))
    {
        return ConfiguredKey { consumed: true, action: Some(action) };
    }
    let prefix = format!("{token} ");
    if state
        .ui
        .library_config
        .keybindings
        .keys()
        .any(|binding| binding.starts_with(&prefix))
    {
        state.ui.pending_key_sequence = Some((token, std::time::Instant::now()));
        return ConfiguredKey { consumed: true, action: None };
    }
    ConfiguredKey { consumed: false, action: None }
}

pub fn execute(state: &mut AppState, action: KeymapAction) -> Option<AppEvent> {
    let navigation_command = match action {
        KeymapAction::First => Some(NavigationCommand::First),
        KeymapAction::Last => Some(NavigationCommand::Last),
        KeymapAction::PageUp => Some(NavigationCommand::PageUp),
        KeymapAction::PageDown => Some(NavigationCommand::PageDown),
        KeymapAction::HalfPageUp => Some(NavigationCommand::HalfPageUp),
        KeymapAction::HalfPageDown => Some(NavigationCommand::HalfPageDown),
        KeymapAction::CurrentContext => Some(NavigationCommand::CurrentContext),
        _ => None,
    };
    if let Some(command) = navigation_command {
        return navigation::execute(state, command);
    }
    match action {
        KeymapAction::PlayPause => {
            state.playback.is_playing = !state.playback.is_playing;
            state.playback.playback_last_updated_at = Some(std::time::Instant::now());
            Some(AppEvent::TogglePlayback(state.playback.is_playing))
        }
        KeymapAction::Next => Some(AppEvent::NextTrack {
            current_track_id: state.playback.playing_track_id.clone(),
        }),
        KeymapAction::Previous => Some(AppEvent::PreviousTrack {
            current_track_id: state.playback.playing_track_id.clone(),
        }),
        KeymapAction::Shuffle => {
            state.playback.is_shuffled = !state.playback.is_shuffled;
            Some(AppEvent::ToggleShuffle(state.playback.is_shuffled))
        }
        KeymapAction::Repeat => {
            let mode = match state.playback.repeat_mode.as_str() {
                "Off" => "Track",
                "Track" => "Context",
                _ => "Off",
            };
            state.playback.repeat_mode = mode.to_string();
            Some(AppEvent::SetRepeatMode(mode.to_string()))
        }
        KeymapAction::SeekBackward => seek_by(state, -5),
        KeymapAction::SeekForward => seek_by(state, 5),
        KeymapAction::SeekStart => seek_to(state, 0),
        KeymapAction::Mute => {
            let volume = state.playback.toggle_mute_target();
            state.playback.volume = volume;
            state.save_volume();
            Some(AppEvent::SetVolume(volume as u8))
        }
        KeymapAction::SortOriginal => sort(state, TrackSort::Original),
        KeymapAction::SortTitle => sort(state, TrackSort::Title),
        KeymapAction::SortArtist => sort(state, TrackSort::Artist),
        KeymapAction::SortAlbum => sort(state, TrackSort::Album),
        KeymapAction::SortDuration => sort(state, TrackSort::Duration),
        KeymapAction::SortAdded => sort(state, TrackSort::Added),
        KeymapAction::ReverseTracks => {
            state.reverse_tracks();
            None
        }
        KeymapAction::First
        | KeymapAction::Last
        | KeymapAction::PageUp
        | KeymapAction::PageDown
        | KeymapAction::HalfPageUp
        | KeymapAction::HalfPageDown
        | KeymapAction::CurrentContext => unreachable!(),
    }
}

fn sort(state: &mut AppState, mode: TrackSort) -> Option<AppEvent> {
    if state.ui.active_view == crate::app::ActiveView::TrackList {
        state.sort_tracks(mode);
    }
    None
}

fn seek_by(state: &mut AppState, seconds: i64) -> Option<AppEvent> {
    seek_to(state, state.playback.seek_target(seconds))
}

fn seek_to(state: &mut AppState, target: u32) -> Option<AppEvent> {
    if state.playback.playing_track_id.is_none() || state.playback.duration_ms == 0 {
        return None;
    }
    state.playback.set_optimistic_progress(target);
    Some(AppEvent::SeekTo(target))
}

fn key_token(key: &KeyEvent) -> String {
    let key_name = match key.code {
        KeyCode::Char(' ') => "space".to_string(),
        KeyCode::Char(character) => character.to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Esc => "esc".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::BackTab => "backtab".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Up => "up".to_string(),
        KeyCode::Down => "down".to_string(),
        KeyCode::Left => "left".to_string(),
        KeyCode::Right => "right".to_string(),
        KeyCode::Home => "home".to_string(),
        KeyCode::End => "end".to_string(),
        KeyCode::PageUp => "pageup".to_string(),
        KeyCode::PageDown => "pagedown".to_string(),
        _ => return format!("{:?}", key.code).to_lowercase(),
    };
    let mut modifiers = Vec::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        modifiers.push("ctrl");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        modifiers.push("alt");
    }
    if modifiers.is_empty() {
        key_name
    } else {
        format!("{}-{key_name}", modifiers.join("-"))
    }
}

fn parse_action(name: &str) -> Option<KeymapAction> {
    Some(match name {
        "first" => KeymapAction::First,
        "last" => KeymapAction::Last,
        "page_up" => KeymapAction::PageUp,
        "page_down" => KeymapAction::PageDown,
        "half_page_up" => KeymapAction::HalfPageUp,
        "half_page_down" => KeymapAction::HalfPageDown,
        "current_context" => KeymapAction::CurrentContext,
        "play_pause" => KeymapAction::PlayPause,
        "next" => KeymapAction::Next,
        "previous" => KeymapAction::Previous,
        "shuffle" => KeymapAction::Shuffle,
        "repeat" => KeymapAction::Repeat,
        "seek_backward" => KeymapAction::SeekBackward,
        "seek_forward" => KeymapAction::SeekForward,
        "seek_start" => KeymapAction::SeekStart,
        "mute" => KeymapAction::Mute,
        "sort_original" => KeymapAction::SortOriginal,
        "sort_title" => KeymapAction::SortTitle,
        "sort_artist" => KeymapAction::SortArtist,
        "sort_album" => KeymapAction::SortAlbum,
        "sort_duration" => KeymapAction::SortDuration,
        "sort_added" => KeymapAction::SortAdded,
        "reverse_tracks" => KeymapAction::ReverseTracks,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_sequences_resolve_to_semantic_actions() {
        let mut state = AppState::new();
        state
            .ui
            .library_config
            .keybindings
            .insert("s d".to_string(), "sort_duration".to_string());
        let first = configured_action(
            &mut state,
            &KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE),
        );
        assert!(first.consumed && first.action.is_none());
        let second = configured_action(
            &mut state,
            &KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
        );
        assert_eq!(second.action, Some(KeymapAction::SortDuration));
    }

    #[test]
    fn control_keys_use_stable_names() {
        assert_eq!(
            key_token(&KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
            "ctrl-f"
        );
    }
}
