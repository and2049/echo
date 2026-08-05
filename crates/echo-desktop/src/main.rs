//! Echo — the GPUI desktop frontend.
//!
//! Same architecture as the `spotify` TUI: [`echo_core::bootstrap::init`] spawns the worker on a
//! tokio runtime and hands back the two event channels; the frontend applies worker events to
//! [`AppState`](echo_core::app::AppState) and draws from it. Here the 16ms poll loop is replaced
//! by GPUI's reactive model — a spawned task awaits worker events and calls `cx.notify()`.
//!
//! The tokio runtime lives on the main function's stack and stays entered for the lifetime of
//! the UI, so worker tasks keep running on its threads while GPUI blocks in `run()`.

mod assets;
mod images;
mod theme;
mod views;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use echo_core::app::{ActiveView, AppMode, LibraryTab, SearchTab};
use echo_core::apply_worker_event::apply_worker_event;
use echo_core::events::AppEvent;
use gpui::{
    App, Bounds, ClickEvent, Context, FocusHandle, Hsla, KeyBinding, Pixels, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, canvas,
    div, img, prelude::*, px, size, svg,
};
use gpui_platform::application;
use theme::{ToGpui, WINDOW_BG, WINDOW_FG};

actions!(
    echo,
    [
        Quit,
        TogglePlayback,
        MoveUp,
        MoveDown,
        PageUp,
        PageDown,
        HalfPageUp,
        HalfPageDown,
        SelectFirst,
        SelectLast,
        Activate,
        FocusLibrary,
        FocusTracks,
        NextTrack,
        PreviousTrack,
        ToggleShuffle,
        CycleRepeat,
        ToggleMute,
        SeekForward,
        SeekBackward,
        SeekStart,
        VolumeUp,
        VolumeDown,
        VolumeUpBig,
        VolumeDownBig,
        ToggleQueue,
        OpenDevices,
        Dismiss,
        FocusSearch,
        ToggleLyrics,
        ToggleInlineLyrics,
        ToggleThemes,
        JumpToCurrent,
        OpenCommand,
        CommandSearch,
        NewPlaylistPrompt,
        RenamePrompt,
        AddToQueue,
        TogglePin,
        CycleTab,
        Refresh,
        OpenFilter,
        NextMatch,
        PrevMatch
    ]
);

/// Key context for the list bindings; the search input carries [`SEARCH_CONTEXT`] instead, and
/// every list binding is predicated on `list && !search` so plain letters type into the box.
const LIST_CONTEXT: &str = "list";
const SEARCH_CONTEXT: &str = "search";
const LIST_KEYS: Option<&str> = Some("list && !search");

/// Keyboard page distance; the TUI uses a similar fixed stride.
const PAGE_ROWS: isize = 10;

/// A right-click menu anchored where the click landed, over sidebar row `index` (of the
/// active library tab's list).
#[derive(Clone)]
pub(crate) struct ContextMenuState {
    pub index: usize,
    pub position: gpui::Point<Pixels>,
}

/// What a context-menu item does; resolved against the row the menu was opened on.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum MenuAction {
    Open,
    TogglePin,
    Rename,
    RemoveFromFolder,
    DeletePlaylist,
    DeleteFolder,
    RemoveAlbum,
}

pub(crate) struct EchoApp {
    pub(crate) state: echo_core::app::AppState,
    pub(crate) app_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    pub(crate) worker_tx: tokio::sync::mpsc::Sender<echo_core::events::WorkerEvent>,
    pub(crate) images: images::ImageCache,
    pub(crate) search_input: String,
    pub(crate) search_focus: FocusHandle,
    /// Focus target for the vim-style `:` command bar and `/` filter bar.
    command_focus: FocusHandle,
    pub(crate) setup_id_focus: FocusHandle,
    pub(crate) setup_secret_focus: FocusHandle,
    pub(crate) library_scroll: UniformListScrollHandle,
    pub(crate) tracks_scroll: UniformListScrollHandle,
    pub(crate) queue_scroll: UniformListScrollHandle,
    pub(crate) search_scroll: UniformListScrollHandle,
    pub(crate) artists_scroll: UniformListScrollHandle,
    pub(crate) artist_albums_scroll: UniformListScrollHandle,
    pub(crate) lyrics_scroll: UniformListScrollHandle,
    /// Desktop-only modal; the TUI picks themes through the `:theme` command instead.
    pub(crate) theme_modal_open: bool,
    /// Right-click menu over a sidebar row; the TUI reaches these through keys instead.
    pub(crate) context_menu: Option<ContextMenuState>,
    /// Written by a canvas overlay each paint; read by click handlers to turn a click's window
    /// position into a fraction of the bar.
    seek_bounds: Rc<Cell<Bounds<Pixels>>>,
    volume_bounds: Rc<Cell<Bounds<Pixels>>>,
    focus_handle: FocusHandle,
}

impl EchoApp {
    fn new(
        boot: echo_core::bootstrap::Bootstrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let echo_core::bootstrap::Bootstrap {
            state,
            config: _,
            app_tx,
            mut app_rx,
            worker_tx,
        } = boot;

        // Bridge: worker events land here and become repaints. tokio's mpsc futures don't need
        // the tokio reactor, so awaiting on GPUI's foreground executor is fine.
        cx.spawn(async move |this, cx| {
            while let Some(event) = app_rx.recv().await {
                let applied = this.update(cx, |app: &mut EchoApp, cx| {
                    apply_worker_event(event, &mut app.state, &app.app_tx, &app.worker_tx);
                    cx.notify();
                });
                if applied.is_err() {
                    break; // entity dropped — app is shutting down
                }
            }
        })
        .detach();

        // Progress tick: elapsed time is interpolated at render time, so while playing the bar
        // needs a repaint even when no worker event arrives. The visualizer needs a much faster
        // cadence, so the interval tightens while it is live.
        cx.spawn(async move |this, cx| {
            loop {
                let interval = this.update(cx, |app: &mut EchoApp, cx| {
                    if app.state.playback.is_playing {
                        cx.notify();
                    }
                    // Status messages carry an expiry the TUI checks each frame; here the
                    // periodic tick retires them.
                    if app
                        .state
                        .ui
                        .status_message_expiry
                        .is_some_and(|expiry| expiry <= std::time::Instant::now())
                    {
                        app.state.ui.status_message = None;
                        app.state.ui.status_message_expiry = None;
                        cx.notify();
                    }
                    let visualizing = app.state.playback.is_playing
                        && app.state.ui.library_config.enable_visualizer
                        && app.state.playback.audio_visualization.is_some();
                    if visualizing { 66 } else { 500 }
                });
                let Ok(interval) = interval else {
                    break; // entity dropped — app is shutting down
                };
                cx.background_executor()
                    .timer(Duration::from_millis(interval))
                    .await;
            }
        })
        .detach();

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        Self {
            state,
            app_tx,
            worker_tx,
            images: images::ImageCache::default(),
            search_input: String::new(),
            search_focus: cx.focus_handle(),
            command_focus: cx.focus_handle(),
            setup_id_focus: cx.focus_handle(),
            setup_secret_focus: cx.focus_handle(),
            library_scroll: UniformListScrollHandle::new(),
            tracks_scroll: UniformListScrollHandle::new(),
            queue_scroll: UniformListScrollHandle::new(),
            search_scroll: UniformListScrollHandle::new(),
            artists_scroll: UniformListScrollHandle::new(),
            artist_albums_scroll: UniformListScrollHandle::new(),
            lyrics_scroll: UniformListScrollHandle::new(),
            theme_modal_open: false,
            context_menu: None,
            seek_bounds: Rc::default(),
            volume_bounds: Rc::default(),
            focus_handle,
        }
    }

    /// Rows in whatever currently has keyboard focus: the device modal when open, else the
    /// `active_view` list — the same routing the TUI's navigation handler does.
    fn list_len(&self) -> usize {
        if self.state.ui.device_modal_open {
            return self.state.data.devices.len();
        }
        match self.state.ui.active_view {
            ActiveView::TrackList => self.state.data.tracks.len(),
            ActiveView::Queue => self.state.data.queue.len(),
            ActiveView::SearchResults => match self.state.ui.active_search_tab {
                echo_core::app::SearchTab::Tracks => self.state.data.search_results.tracks.len(),
                echo_core::app::SearchTab::Albums => self.state.data.search_results.albums.len(),
                echo_core::app::SearchTab::Artists => self.state.data.search_results.artists.len(),
            },
            ActiveView::ArtistList => self.state.data.followed_artists.len(),
            ActiveView::ArtistPage => self
                .state
                .data
                .artist_page_data
                .as_ref()
                .map_or(0, |data| data.albums.len()),
            _ => match self.state.ui.active_library_tab {
                LibraryTab::Albums => self.state.data.saved_albums.len(),
                _ => self.state.data.library_view.len(),
            },
        }
    }

    fn set_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.state.ui.device_modal_open {
            self.state.ui.selected_device_index = index;
        } else {
            match self.state.ui.active_view {
                ActiveView::TrackList => {
                    self.state.ui.selected_track_index = index;
                    self.tracks_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::Queue => {
                    self.state.ui.selected_queue_index = index;
                    self.queue_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::SearchResults => {
                    self.state.ui.selected_search_index = index;
                    self.search_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::ArtistList => {
                    self.state.ui.selected_artist_index = index;
                    self.artists_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::ArtistPage => {
                    self.state.ui.artist_page_album_index = index;
                    self.artist_albums_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                _ => {
                    self.state.ui.selected_playlist_index = index;
                    self.library_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
                }
            }
        }
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let current = if self.state.ui.device_modal_open {
            self.state.ui.selected_device_index
        } else {
            match self.state.ui.active_view {
                ActiveView::TrackList => self.state.ui.selected_track_index,
                ActiveView::Queue => self.state.ui.selected_queue_index,
                ActiveView::SearchResults => self.state.ui.selected_search_index,
                ActiveView::ArtistList => self.state.ui.selected_artist_index,
                ActiveView::ArtistPage => self.state.ui.artist_page_album_index,
                _ => self.state.ui.selected_playlist_index,
            }
        };
        self.set_selection(current.saturating_add_signed(delta).min(len - 1), cx);
    }

    fn select_last(&mut self, cx: &mut Context<Self>) {
        let len = self.list_len();
        if len > 0 {
            self.set_selection(len - 1, cx);
        }
    }

    /// Enter on the focused list — the same intents the row click handlers use.
    fn activate_selection(&mut self, cx: &mut Context<Self>) {
        let event = if self.state.ui.device_modal_open {
            let index = self.state.ui.selected_device_index;
            echo_core::intent::transfer_to_device(&mut self.state, index)
        } else {
            match self.state.ui.active_view {
                ActiveView::TrackList => {
                    let index = self.state.ui.selected_track_index;
                    echo_core::intent::play_track_at(&mut self.state, index)
                }
                // The queue is browse-only: the API can't jump to a queue position.
                ActiveView::Queue => None,
                ActiveView::SearchResults => {
                    let index = self.state.ui.selected_search_index;
                    echo_core::intent::activate_search_result(&mut self.state, index)
                }
                ActiveView::ArtistList => {
                    let index = self.state.ui.selected_artist_index;
                    echo_core::intent::open_followed_artist(&mut self.state, index)
                }
                ActiveView::ArtistPage => {
                    let index = self.state.ui.artist_page_album_index;
                    echo_core::intent::open_artist_album(&mut self.state, index)
                }
                _ => {
                    let index = self.state.ui.selected_playlist_index;
                    match self.state.ui.active_library_tab {
                        LibraryTab::Albums => {
                            echo_core::intent::open_album(&mut self.state, index)
                        }
                        _ => echo_core::intent::open_library_entry(&mut self.state, index),
                    }
                }
            }
        };
        if let Some(event) = event {
            self.dispatch(event);
        }
        cx.notify();
    }

    fn toggle_queue(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.active_view == ActiveView::Queue {
            // Mirrors the TUI's `q` from the queue view.
            self.state.ui.active_view = ActiveView::Library;
        } else {
            let event = echo_core::intent::open_queue(&mut self.state);
            self.dispatch(event);
        }
        cx.notify();
    }

    fn open_devices(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::open_device_picker(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn toggle_lyrics(&mut self, cx: &mut Context<Self>) {
        self.state.ui.lyrics_modal_open = !self.state.ui.lyrics_modal_open;
        cx.notify();
    }

    fn toggle_themes(&mut self, cx: &mut Context<Self>) {
        self.theme_modal_open = !self.theme_modal_open;
        cx.notify();
    }

    /// Escape / `h` / backspace: close whatever is topmost, else go back — the same ordering
    /// as the TUI's back handling, with the desktop-only modals checked first.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if echo_core::intent::prompt_active(&self.state) {
            echo_core::intent::cancel_prompt(&mut self.state);
        } else if self.context_menu.is_some() {
            self.context_menu = None;
        } else if self.state.ui.device_modal_open {
            self.state.ui.device_modal_open = false;
        } else if self.theme_modal_open {
            self.theme_modal_open = false;
        } else if self.state.ui.lyrics_modal_open {
            self.state.ui.lyrics_modal_open = false;
        } else if self.state.pop_view_history() {
            self.state.clear_pending_artist_page();
            if self.state.data.tracklist_image_url.is_some() {
                self.dispatch(AppEvent::ReloadHeaderImage);
            }
        } else if self.state.ui.active_view == ActiveView::TrackList {
            self.state.ui.active_view = if self.search_has_results() {
                ActiveView::SearchResults
            } else {
                ActiveView::Library
            };
        } else if self.state.ui.active_view == ActiveView::ArtistPage {
            if self.search_has_results() {
                self.state.ui.active_view = ActiveView::SearchResults;
                self.state.clear_pending_artist_page();
                self.dispatch(AppEvent::CancelArtistPageLoad);
            } else {
                let event = echo_core::intent::back_to_artist_list(&mut self.state);
                self.dispatch(event);
            }
        } else if self.state.ui.active_view == ActiveView::SearchResults {
            // Mirrors the TUI: leaving search results drops them entirely.
            self.state.ui.active_view = ActiveView::Library;
            self.state.data.search_results = Default::default();
            self.state.ui.search_context_query.clear();
            self.state.ui.status_message = None;
            if self.state.data.tracklist_image_url.is_some() {
                self.dispatch(AppEvent::ReloadHeaderImage);
            }
        } else if self.state.ui.active_view == ActiveView::Queue
            || self.state.ui.active_view == ActiveView::ArtistList
        {
            self.state.ui.active_view = ActiveView::Library;
            if self.state.data.tracklist_image_url.is_some() {
                self.dispatch(AppEvent::ReloadHeaderImage);
            }
        }
        cx.notify();
    }

    fn search_has_results(&self) -> bool {
        !self.state.data.search_results.tracks.is_empty()
            || !self.state.data.search_results.albums.is_empty()
            || !self.state.data.search_results.artists.is_empty()
    }

    /// Runs a context-menu item against sidebar row `index`, then closes the menu. Destructive
    /// items only stage a `*_prompt` — the confirm modal fires the actual event.
    pub(crate) fn run_menu_action(
        &mut self,
        action: MenuAction,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        if self.state.ui.active_library_tab == LibraryTab::Albums {
            match action {
                MenuAction::Open => {
                    if let Some(event) = echo_core::intent::open_album(&mut self.state, index) {
                        self.dispatch(event);
                    }
                }
                MenuAction::RemoveAlbum => {
                    if let Some(album) = self.state.data.saved_albums.get(index) {
                        self.state.ui.album_mass_delete_prompt = Some(vec![album.id.clone()]);
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        let Some(node) = self.state.data.library_view.get(index).cloned() else {
            cx.notify();
            return;
        };
        match action {
            MenuAction::Open => {
                self.state.ui.selected_playlist_index = index;
                if let Some(event) =
                    echo_core::intent::open_library_entry(&mut self.state, index)
                {
                    self.dispatch(event);
                }
            }
            MenuAction::TogglePin => {
                self.state.ui.selected_playlist_index = index;
                echo_core::intent::toggle_pin_selected(&mut self.state);
            }
            MenuAction::Rename => {
                let name = match &node {
                    echo_core::models::LibraryNode::Playlist { playlist, .. } => {
                        playlist.name.clone()
                    }
                    echo_core::models::LibraryNode::Folder(folder) => folder.name.clone(),
                };
                self.state.ui.selected_playlist_index = index;
                self.open_command(&format!("rename {name}"), window, cx);
            }
            MenuAction::RemoveFromFolder => {
                if let echo_core::models::LibraryNode::Playlist { playlist, .. } = &node {
                    echo_core::intent::remove_playlist_from_folders(
                        &mut self.state,
                        &playlist.id,
                    );
                }
            }
            MenuAction::DeletePlaylist => {
                if let echo_core::models::LibraryNode::Playlist { playlist, .. } = &node {
                    self.state.ui.playlist_delete_prompt = Some(vec![playlist.id.clone()]);
                }
            }
            MenuAction::DeleteFolder => {
                if let echo_core::models::LibraryNode::Folder(folder) = &node {
                    self.state.ui.folder_delete_prompt = Some(folder.name.clone());
                }
            }
            MenuAction::RemoveAlbum => {}
        }
        cx.notify();
    }

    // Vim-style command mode (`:`) and track filter (`/`) — a bar above the playback bar that
    // owns key handling while one of the modes is active. The command registry itself is
    // `echo_core::commands`, shared with the TUI.

    fn open_command(&mut self, prefill: &str, window: &mut Window, cx: &mut Context<Self>) {
        echo_core::commands::clear_suggestions(&mut self.state);
        self.state.ui.mode = AppMode::Command;
        self.state.ui.command_buffer = prefill.to_string();
        self.state.ui.status_message = None;
        window.focus(&self.command_focus.clone(), cx);
        cx.notify();
    }

    fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.ui.mode = AppMode::Search;
        self.state.ui.search_query.clear();
        self.state.ui.search_matches.clear();
        self.state.ui.status_message = None;
        window.focus(&self.command_focus.clone(), cx);
        cx.notify();
    }

    /// `e` in the sidebar: prefill `:rename` with the selected playlist/folder name.
    fn rename_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.ui.active_view != ActiveView::Library {
            return;
        }
        let Some(node) = self
            .state
            .data
            .library_view
            .get(self.state.ui.selected_playlist_index)
        else {
            return;
        };
        let name = match node {
            echo_core::models::LibraryNode::Playlist { playlist, .. } => playlist.name.clone(),
            echo_core::models::LibraryNode::Folder(f) => f.name.clone(),
        };
        self.open_command(&format!("rename {name}"), window, cx);
    }

    fn handle_command_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => {
                if let Some(event) = echo_core::commands::submit(&mut self.state) {
                    self.dispatch(event);
                }
                window.focus(&self.focus_handle.clone(), cx);
                // `:q` and friends land here.
                if !self.state.ui.is_running {
                    cx.quit();
                }
            }
            "escape" => {
                echo_core::commands::clear_suggestions(&mut self.state);
                self.state.ui.mode = AppMode::Normal;
                self.state.ui.command_buffer.clear();
                window.focus(&self.focus_handle.clone(), cx);
            }
            "tab" => {
                echo_core::commands::cycle_suggestion(
                    &mut self.state,
                    !event.keystroke.modifiers.shift,
                );
            }
            "backspace" => {
                echo_core::commands::clear_suggestions(&mut self.state);
                self.state.ui.command_buffer.pop();
            }
            "v" if event.keystroke.modifiers.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    echo_core::commands::clear_suggestions(&mut self.state);
                    self.state
                        .ui
                        .command_buffer
                        .extend(text.chars().filter(|c| *c != '\r' && *c != '\n'));
                }
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    echo_core::commands::clear_suggestions(&mut self.state);
                    self.state.ui.command_buffer.push_str(text);
                }
            }
        }
        cx.notify();
    }

    fn handle_filter_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => {
                self.state.ui.mode = AppMode::Normal;
                if !self.state.ui.search_matches.is_empty() {
                    self.state.ui.selected_track_index = self.state.ui.search_matches[0];
                    self.scroll_to_selected_track();
                }
                window.focus(&self.focus_handle.clone(), cx);
            }
            "escape" => {
                self.state.ui.mode = AppMode::Normal;
                self.state.ui.search_query.clear();
                self.state.ui.search_matches.clear();
                window.focus(&self.focus_handle.clone(), cx);
            }
            "backspace" => {
                self.state.ui.search_query.pop();
                echo_core::intent::update_search_matches(&mut self.state);
                self.scroll_to_selected_track();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    self.state.ui.search_query.push_str(text);
                    echo_core::intent::update_search_matches(&mut self.state);
                    self.scroll_to_selected_track();
                }
            }
        }
        cx.notify();
    }

    fn next_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        echo_core::intent::next_search_match(&mut self.state, forward);
        self.scroll_to_selected_track();
        cx.notify();
    }

    fn scroll_to_selected_track(&mut self) {
        self.tracks_scroll
            .scroll_to_item(self.state.ui.selected_track_index, ScrollStrategy::Nearest);
    }

    /// `g c`: select whatever is playing, loading its context first when needed.
    fn jump_to_current(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::jump_to_current_context(&mut self.state) {
            self.dispatch(event);
        }
        if self.state.ui.active_view == ActiveView::TrackList {
            self.tracks_scroll
                .scroll_to_item(self.state.ui.selected_track_index, ScrollStrategy::Center);
        }
        cx.notify();
    }

    fn add_to_queue(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::queue_selected_track(&self.state) {
            self.dispatch(event);
            self.state.ui.status_message = Some("Added to queue".to_string());
            self.state.ui.status_message_expiry =
                Some(std::time::Instant::now() + Duration::from_secs(3));
        }
        cx.notify();
    }

    fn toggle_pin(&mut self, cx: &mut Context<Self>) {
        echo_core::intent::toggle_pin_selected(&mut self.state);
        cx.notify();
    }

    fn refresh_view(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::refresh_view(&mut self.state) {
            self.dispatch(event);
        }
        cx.notify();
    }

    fn adjust_volume(&mut self, delta: i32, cx: &mut Context<Self>) {
        let event = echo_core::intent::adjust_volume(&mut self.state, delta);
        self.dispatch(event);
        cx.notify();
    }

    fn seek_start(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::seek_to(&mut self.state, 0) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Tab: cycle the tabs of whichever tabbed view is active. The desktop has no Browse
    /// library tab — those entries live in the sidebar — so the library just flips between
    /// Playlists and Albums.
    fn cycle_tab(&mut self, cx: &mut Context<Self>) {
        match self.state.ui.active_view {
            ActiveView::SearchResults => {
                self.state.ui.active_search_tab = match self.state.ui.active_search_tab {
                    SearchTab::Tracks => SearchTab::Albums,
                    SearchTab::Albums => SearchTab::Artists,
                    SearchTab::Artists => SearchTab::Tracks,
                };
                self.state.ui.selected_search_index = 0;
            }
            ActiveView::Library => {
                self.state.ui.active_library_tab = match self.state.ui.active_library_tab {
                    LibraryTab::Playlists => LibraryTab::Albums,
                    _ => LibraryTab::Playlists,
                };
                self.state.ui.selected_playlist_index = 0;
            }
            _ => {}
        }
        cx.notify();
    }

    fn toggle_inline_lyrics(&mut self, cx: &mut Context<Self>) {
        echo_core::intent::toggle_condensed_lyrics(&mut self.state);
        cx.notify();
    }

    /// The `:`/`/` bar. Rendered only while one of the two modes is active; carries the
    /// search key context so plain letters type instead of triggering list bindings.
    fn render_command_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.ui.active_theme;
        let fg = theme.text.gpui(WINDOW_FG());
        let muted = theme.text_muted.gpui(WINDOW_FG());
        let accent = theme.primary.gpui(WINDOW_FG());

        let is_command = self.state.ui.mode == AppMode::Command;
        let prefix: SharedString = if is_command { ":" } else { "/" }.into();
        let buffer = if is_command {
            self.state.ui.command_buffer.clone()
        } else {
            self.state.ui.search_query.clone()
        };
        let suggestions = self.state.ui.command_suggestions.clone();
        let selected_suggestion = self.state.ui.command_suggestion_index;
        let match_hint: Option<SharedString> = (!is_command
            && !self.state.ui.search_query.is_empty())
        .then(|| format!("{} matches", self.state.ui.search_matches.len()).into());

        div()
            .id("command-bar")
            .key_context(SEARCH_CONTEXT)
            .track_focus(&self.command_focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.state.ui.mode == AppMode::Command {
                    this.handle_command_key(event, window, cx);
                } else {
                    this.handle_filter_key(event, window, cx);
                }
            }))
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_1()
            .border_t_1()
            .border_color(muted.opacity(0.3))
            .when(is_command && !suggestions.is_empty(), |el| {
                el.child(div().flex().flex_row().gap_2().overflow_hidden().children(
                    suggestions.into_iter().enumerate().map(|(index, suggestion)| {
                        let selected = selected_suggestion == Some(index);
                        div()
                            .px_2()
                            .rounded_sm()
                            .text_xs()
                            .map(|el| {
                                if selected {
                                    el.bg(accent.opacity(0.25)).text_color(fg)
                                } else {
                                    el.text_color(muted)
                                }
                            })
                            .child(SharedString::from(suggestion))
                    }),
                ))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(div().text_sm().text_color(accent).child(prefix))
                    .child(
                        div()
                            .text_sm()
                            .text_color(fg)
                            .child(SharedString::from(format!("{buffer}▏"))),
                    )
                    .when_some(match_hint, |el, hint| {
                        el.child(div().flex_grow(1.0))
                            .child(div().text_xs().text_color(muted).child(hint))
                    }),
            )
    }

    /// Typing for the two setup fields. Credentials are pasted more often than typed, so
    /// ctrl-v pulls from the clipboard (with whitespace stripped).
    fn handle_setup_key(
        &mut self,
        secret: bool,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = if secret {
            &mut self.state.ui.setup_client_secret
        } else {
            &mut self.state.ui.setup_client_id
        };
        match event.keystroke.key.as_str() {
            "enter" => {
                if let Some(event) = echo_core::intent::submit_setup_credentials(&mut self.state)
                {
                    self.dispatch(event);
                }
            }
            "tab" => {
                let target = if secret {
                    self.setup_id_focus.clone()
                } else {
                    self.setup_secret_focus.clone()
                };
                window.focus(&target, cx);
            }
            "escape" => window.focus(&self.focus_handle.clone(), cx),
            "backspace" => {
                field.pop();
            }
            "v" if event.keystroke.modifiers.control => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    field.extend(text.chars().filter(|c| !c.is_whitespace()));
                }
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    field.push_str(text);
                }
            }
        }
        cx.notify();
    }

    /// Enter/escape and typing for the search box, which owns key handling while focused.
    fn handle_search_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => {
                let query = self.search_input.clone();
                if let Some(event) = echo_core::intent::global_search(&mut self.state, &query) {
                    self.dispatch(event);
                }
                window.focus(&self.focus_handle, cx);
            }
            "escape" => {
                self.search_input.clear();
                window.focus(&self.focus_handle, cx);
            }
            "backspace" => {
                self.search_input.pop();
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    self.search_input.push_str(text);
                }
            }
        }
        cx.notify();
    }

    fn focus_library(&mut self, cx: &mut Context<Self>) {
        self.state.ui.active_view = ActiveView::Library;
        cx.notify();
    }

    fn focus_tracks(&mut self, cx: &mut Context<Self>) {
        // Nothing to focus before a context has been opened.
        if !self.state.data.tracks.is_empty() || self.state.data.active_tracklist_context.is_some()
        {
            self.state.ui.active_view = ActiveView::TrackList;
            cx.notify();
        }
    }

    // Transport, all via the shared intents (optimistic flip + worker event).

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::toggle_playback(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn play_next(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::next_track(&self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn play_previous(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::previous_track(&self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::toggle_shuffle(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn cycle_repeat(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::cycle_repeat(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::toggle_mute(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn seek_relative(&mut self, seconds: i64, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::seek_by(&mut self.state, seconds) {
            self.dispatch(event);
        }
        cx.notify();
    }

    fn seek_to_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let target = (self.state.playback.duration_ms as f32 * fraction.clamp(0.0, 1.0)) as u32;
        if let Some(event) = echo_core::intent::seek_to(&mut self.state, target) {
            self.dispatch(event);
        }
        cx.notify();
    }

    fn set_volume_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let volume = (fraction.clamp(0.0, 1.0) * 100.0).round() as u8;
        let event = echo_core::intent::set_volume(&mut self.state, volume);
        self.dispatch(event);
        cx.notify();
    }

    /// Sends an intent-produced event to the worker, with the same side channel the TUI's main
    /// loop has: a LoadContextTracks with cover art also kicks off the header image fetch.
    pub(crate) fn dispatch(&mut self, event: AppEvent) {
        if let AppEvent::LoadContextTracks(ref context) = event
            && let Some(url) = context.image_url.as_ref()
        {
            self.state.data.tracklist_image_url = Some(url.clone());
            echo_core::image_tasks::spawn_header_for_url(
                url,
                self.worker_tx.clone(),
                self.state.ui.library_config.cover_img_pixels,
            );
        }
        let _ = self.app_tx.send(event);
    }

    fn render_playback_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.ui.active_theme;
        let fg = theme.text.gpui(WINDOW_FG());
        let muted = theme.text_muted.gpui(WINDOW_FG());
        let accent = theme.primary.gpui(WINDOW_FG());

        let playback = &self.state.playback;
        let title: SharedString = if playback.playing_track_title.is_empty() {
            "Nothing playing".into()
        } else {
            playback.playing_track_title.clone().into()
        };
        let artist: SharedString = playback.playing_track_artist.clone().into();

        let progress_ms = playback.display_progress_ms();
        let fraction = if playback.duration_ms > 0 {
            (progress_ms as f32 / playback.duration_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let elapsed_label: SharedString = format_time(progress_ms).into();
        let duration_label: SharedString = format_time(playback.duration_ms).into();
        let play_icon = if playback.is_playing {
            "icons/pause.svg"
        } else {
            "icons/play.svg"
        };

        let shuffle_color = if playback.is_shuffled { accent } else { muted };
        let (repeat_icon, repeat_color) = match playback.repeat_mode.as_str() {
            "Track" => ("icons/repeat-one.svg", accent),
            "Context" => ("icons/repeat.svg", accent),
            _ => ("icons/repeat.svg", muted),
        };
        let volume_fraction = (playback.volume as f32 / 100.0).clamp(0.0, 1.0);
        let mute_icon = if playback.volume == 0 {
            "icons/volume-off.svg"
        } else {
            "icons/volume-high.svg"
        };
        let queue_color = if self.state.ui.active_view == ActiveView::Queue {
            accent
        } else {
            muted
        };
        let lyrics_color = if self.state.ui.lyrics_modal_open {
            accent
        } else {
            muted
        };
        let visualizer_bands = (self.state.ui.library_config.enable_visualizer
            && self.state.playback.is_playing)
            .then(|| self.state.playback.audio_visualization.clone())
            .flatten();

        // The pixelate transfer trick from the TUI: keep showing the previous cover while the
        // current one refetches.
        let cover = self
            .state
            .playback
            .playing_track_image
            .as_ref()
            .or(self.state.playback.previous_track_image.as_ref())
            .cloned()
            .and_then(|artwork| self.images.get(&artwork));

        // The TUI's condensed-lyrics line: current lyric (accented) with the next one below,
        // riding the same playback tick that advances the seek bar.
        let inline_lyrics = self
            .state
            .ui
            .condensed_lyrics_enabled
            .then(|| {
                if let Some(lyrics) = self.state.playback.current_lyrics.as_ref() {
                    if lyrics.lines.is_empty() {
                        return ("No lyrics found.".to_string(), String::new());
                    }
                    let progress = self.state.playback.display_progress_ms();
                    let mut current = 0;
                    for (index, line) in lyrics.lines.iter().enumerate() {
                        if line.start_ms <= progress {
                            current = index;
                        } else {
                            break;
                        }
                    }
                    (
                        lyrics.lines[current].text.clone(),
                        lyrics
                            .lines
                            .get(current + 1)
                            .map(|line| line.text.clone())
                            .unwrap_or_default(),
                    )
                } else if !self.state.playback.playing_track_title.is_empty() {
                    ("No lyrics found.".to_string(), String::new())
                } else {
                    (String::new(), String::new())
                }
            })
            .filter(|(current, _)| !current.is_empty());

        let seek_bounds = self.seek_bounds.clone();
        let volume_bounds = self.volume_bounds.clone();

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .h(px(88.0))
            .px_4()
            .border_t_1()
            .border_color(muted.opacity(0.3))
            .child(
                // Left cluster: the song card stacked over the transport row, so the seek bar
                // gets the whole center width.
                div()
                    .flex_none()
                    .w(px(240.0))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(match cover {
                                Some(image) => img(image)
                                    .flex_none()
                                    .w(px(36.0))
                                    .h(px(36.0))
                                    .rounded_md()
                                    .into_any_element(),
                                None => div()
                                    .flex_none()
                                    .w(px(36.0))
                                    .h(px(36.0))
                                    .rounded_md()
                                    .bg(muted.opacity(0.15))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/music-note.svg")
                                            .w(px(16.0))
                                            .h(px(16.0))
                                            .text_color(muted),
                                    )
                                    .into_any_element(),
                            })
                            .child(
                                div()
                                    .flex_col()
                                    .flex_grow(1.0)
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .text_color(fg)
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .overflow_hidden()
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .text_color(muted)
                                            .text_xs()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .overflow_hidden()
                                            .child(artist),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(icon_button(
                                "previous",
                                "icons/previous.svg",
                                fg,
                                cx,
                                |this, cx| this.play_previous(cx),
                            ))
                            .child(
                                div()
                                    .id("play-pause")
                                    .flex_none()
                                    .w(px(36.0))
                                    .h(px(36.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .hover(|style| style.bg(accent.opacity(0.15)))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.toggle_playback(cx)
                                    }))
                                    .child(
                                        svg()
                                            .path(play_icon)
                                            .w(px(20.0))
                                            .h(px(20.0))
                                            .text_color(accent),
                                    ),
                            )
                            .child(icon_button("next", "icons/next.svg", fg, cx, |this, cx| {
                                this.play_next(cx)
                            }))
                            .child(icon_button(
                                "shuffle",
                                "icons/shuffle.svg",
                                shuffle_color,
                                cx,
                                |this, cx| this.toggle_shuffle(cx),
                            ))
                            .child(icon_button(
                                "repeat",
                                repeat_icon,
                                repeat_color,
                                cx,
                                |this, cx| this.cycle_repeat(cx),
                            )),
                    ),
            )
            .child(
                div()
                    .flex_grow(1.0)
                    .flex()
                    .flex_col()
                    .justify_center()
                    .overflow_hidden()
                    .when_some(inline_lyrics, |el, (current, next)| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .mb_1()
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(accent)
                                        .whitespace_nowrap()
                                        .max_w_full()
                                        .overflow_hidden()
                                        .child(SharedString::from(current)),
                                )
                                .when(!next.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .whitespace_nowrap()
                                            .max_w_full()
                                            .overflow_hidden()
                                            .child(SharedString::from(next)),
                                    )
                                }),
                        )
                    })
                    .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_none()
                            .text_color(muted)
                            .text_xs()
                            .child(elapsed_label),
                    )
                    .child(
                // Progress track: the canvas overlay records the track's bounds each paint, and
                // a click anywhere in the (taller) hit area seeks to that fraction.
                div()
                    .id("seek-bar")
                    .flex_grow(1.0)
                    .py_2()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, _window, cx| {
                        let bounds = this.seek_bounds.get();
                        if bounds.size.width > px(0.0) {
                            let fraction =
                                (event.position().x - bounds.origin.x) / bounds.size.width;
                            this.seek_to_fraction(fraction, cx);
                        }
                    }))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(6.0))
                            .rounded_full()
                            .bg(muted.opacity(0.25))
                            .child(
                                canvas(
                                    move |bounds, _window, _cx| seek_bounds.set(bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(accent)
                                    .w(gpui::relative(fraction)),
                            ),
                    ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(muted)
                            .text_xs()
                            .child(duration_label),
                    ),
                    ),
            )
            .when_some(visualizer_bands, |el, bands| {
                // 32 bands, 0–100, painted as bottom-anchored bars. Repaints ride the fast tick.
                el.child(div().flex_none().w(px(120.0)).h(px(40.0)).child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, _| {
                            let bands = bands.lock();
                            let count = bands.len();
                            let band_width = bounds.size.width / count as f32;
                            for (index, value) in bands.iter().enumerate() {
                                let ratio = (value / 100.0).clamp(0.0, 1.0);
                                let bar_height = bounds.size.height * ratio;
                                let origin = gpui::point(
                                    bounds.origin.x + band_width * index as f32,
                                    bounds.origin.y + bounds.size.height - bar_height,
                                );
                                let bar = Bounds {
                                    origin,
                                    size: size(band_width * 0.8, bar_height),
                                };
                                window.paint_quad(gpui::fill(bar, accent));
                            }
                        },
                    )
                    .size_full(),
                ))
            })
            .child(icon_button("lyrics", "icons/mic.svg", lyrics_color, cx, |this, cx| {
                this.toggle_lyrics(cx)
            }))
            .child(icon_button("themes", "icons/paint-board.svg", muted, cx, |this, cx| {
                this.toggle_themes(cx)
            }))
            .child(icon_button("queue", "icons/playlist.svg", queue_color, cx, |this, cx| {
                this.toggle_queue(cx)
            }))
            .child(icon_button("devices", "icons/computer.svg", muted, cx, |this, cx| {
                this.open_devices(cx)
            }))
            .child(icon_button("mute", mute_icon, muted, cx, |this, cx| {
                this.toggle_mute(cx)
            }))
            .child(
                div()
                    .id("volume-bar")
                    .flex_none()
                    .w(px(90.0))
                    .py_2()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, event: &ClickEvent, _window, cx| {
                        let bounds = this.volume_bounds.get();
                        if bounds.size.width > px(0.0) {
                            let fraction =
                                (event.position().x - bounds.origin.x) / bounds.size.width;
                            this.set_volume_fraction(fraction, cx);
                        }
                    }))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(6.0))
                            .rounded_full()
                            .bg(muted.opacity(0.25))
                            .child(
                                canvas(
                                    move |bounds, _window, _cx| volume_bounds.set(bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(fg)
                                    .w(gpui::relative(volume_fraction)),
                            ),
                    ),
            )
    }
}

/// A small round icon button for the playback bar. `icon` is an embedded SVG path (see
/// [`assets`]), tinted with `color` like any themed text.
fn icon_button(
    id: &'static str,
    icon: &'static str,
    color: Hsla,
    cx: &mut Context<EchoApp>,
    on_click: impl Fn(&mut EchoApp, &mut Context<EchoApp>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .w(px(32.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .hover(move |style| style.bg(color.opacity(0.15)))
        .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
        .child(svg().path(icon).w(px(16.0)).h(px(16.0)).text_color(color))
}

impl Render for EchoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.ui.active_theme;
        let bg = theme.background.gpui(WINDOW_BG());
        let muted = theme.text_muted.gpui(WINDOW_FG());

        // Built ahead of the chain because they borrow self mutably, like the other views.
        let command_bar = matches!(self.state.ui.mode, AppMode::Command | AppMode::Search)
            .then(|| self.render_command_bar(cx).into_any_element());
        let status_line = (self.state.ui.mode == AppMode::Normal)
            .then(|| self.state.ui.status_message.clone())
            .flatten()
            .map(|message| {
                div()
                    .flex_none()
                    .px_4()
                    .py_1()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(message))
                    .into_any_element()
            });
        let lyrics_modal = self
            .state
            .ui
            .lyrics_modal_open
            .then(|| views::lyrics_modal(self, cx).into_any_element());
        let theme_modal = self
            .theme_modal_open
            .then(|| views::theme_modal(self, cx).into_any_element());
        let device_modal = self
            .state
            .ui
            .device_modal_open
            .then(|| views::device_modal(self, cx).into_any_element());
        let context_menu = self
            .context_menu
            .is_some()
            .then(|| views::context_menu(self, cx).into_any_element());
        let prompt_modal = echo_core::intent::prompt_active(&self.state)
            .then(|| views::prompt_modal(self, cx).into_any_element());

        div()
            .key_context(LIST_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &TogglePlayback, _window, cx| {
                this.toggle_playback(cx)
            }))
            .on_action(cx.listener(|this, _: &MoveUp, _window, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _window, cx| this.move_selection(1, cx)))
            .on_action(
                cx.listener(|this, _: &PageUp, _window, cx| this.move_selection(-PAGE_ROWS, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _window, cx| this.move_selection(PAGE_ROWS, cx)),
            )
            .on_action(cx.listener(|this, _: &SelectFirst, _window, cx| this.set_selection(0, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _window, cx| this.select_last(cx)))
            .on_action(cx.listener(|this, _: &Activate, _window, cx| this.activate_selection(cx)))
            .on_action(cx.listener(|this, _: &FocusLibrary, _window, cx| this.focus_library(cx)))
            .on_action(cx.listener(|this, _: &FocusTracks, _window, cx| this.focus_tracks(cx)))
            .on_action(cx.listener(|this, _: &NextTrack, _window, cx| this.play_next(cx)))
            .on_action(cx.listener(|this, _: &PreviousTrack, _window, cx| this.play_previous(cx)))
            .on_action(cx.listener(|this, _: &ToggleShuffle, _window, cx| this.toggle_shuffle(cx)))
            .on_action(cx.listener(|this, _: &CycleRepeat, _window, cx| this.cycle_repeat(cx)))
            .on_action(cx.listener(|this, _: &ToggleMute, _window, cx| this.toggle_mute(cx)))
            .on_action(
                cx.listener(|this, _: &SeekForward, _window, cx| this.seek_relative(5, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SeekBackward, _window, cx| this.seek_relative(-5, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleQueue, _window, cx| this.toggle_queue(cx)))
            .on_action(cx.listener(|this, _: &OpenDevices, _window, cx| this.open_devices(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _window, cx| this.dismiss(cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                window.focus(&this.search_focus, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleLyrics, _window, cx| this.toggle_lyrics(cx)))
            .on_action(cx.listener(|this, _: &ToggleThemes, _window, cx| this.toggle_themes(cx)))
            .on_action(cx.listener(|this, _: &HalfPageUp, _window, cx| {
                this.move_selection(-PAGE_ROWS / 2, cx)
            }))
            .on_action(cx.listener(|this, _: &HalfPageDown, _window, cx| {
                this.move_selection(PAGE_ROWS / 2, cx)
            }))
            .on_action(cx.listener(|this, _: &SeekStart, _window, cx| this.seek_start(cx)))
            .on_action(cx.listener(|this, _: &VolumeUp, _window, cx| this.adjust_volume(1, cx)))
            .on_action(cx.listener(|this, _: &VolumeDown, _window, cx| this.adjust_volume(-1, cx)))
            .on_action(cx.listener(|this, _: &VolumeUpBig, _window, cx| this.adjust_volume(5, cx)))
            .on_action(
                cx.listener(|this, _: &VolumeDownBig, _window, cx| this.adjust_volume(-5, cx)),
            )
            .on_action(cx.listener(|this, _: &JumpToCurrent, _window, cx| this.jump_to_current(cx)))
            .on_action(
                cx.listener(|this, _: &OpenCommand, window, cx| this.open_command("", window, cx)),
            )
            .on_action(cx.listener(|this, _: &CommandSearch, window, cx| {
                this.open_command("search ", window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewPlaylistPrompt, window, cx| {
                if this.state.ui.active_view == ActiveView::Library {
                    this.open_command("newplaylist ", window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &RenamePrompt, window, cx| {
                this.rename_prompt(window, cx)
            }))
            .on_action(cx.listener(|this, _: &AddToQueue, _window, cx| this.add_to_queue(cx)))
            .on_action(cx.listener(|this, _: &TogglePin, _window, cx| this.toggle_pin(cx)))
            .on_action(cx.listener(|this, _: &CycleTab, _window, cx| this.cycle_tab(cx)))
            .on_action(cx.listener(|this, _: &Refresh, _window, cx| this.refresh_view(cx)))
            .on_action(cx.listener(|this, _: &OpenFilter, window, cx| this.open_filter(window, cx)))
            .on_action(cx.listener(|this, _: &NextMatch, _window, cx| this.next_match(true, cx)))
            .on_action(cx.listener(|this, _: &PrevMatch, _window, cx| this.next_match(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleInlineLyrics, _window, cx| {
                this.toggle_inline_lyrics(cx)
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .child(
                div()
                    .flex_grow(1.0)
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .child(views::sidebar(self, cx))
                    .child(views::main_area(self, window, cx)),
            )
            .when_some(status_line, |el, line| el.child(line))
            .when_some(command_bar, |el, bar| el.child(bar))
            .child(self.render_playback_bar(cx))
            .when_some(lyrics_modal, |el, modal| el.child(modal))
            .when_some(theme_modal, |el, modal| el.child(modal))
            .when_some(device_modal, |el, modal| el.child(modal))
            .when_some(context_menu, |el, menu| el.child(menu))
            .when_some(prompt_modal, |el, modal| el.child(modal))
    }
}

pub(crate) fn format_time(ms: u32) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn main() {
    // The worker lives on this runtime; entering it makes bootstrap::init()'s tokio::spawn work.
    // It must outlive the UI, which `run()` blocks for.
    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    let _guard = runtime.enter();

    echo_core::i18n::init();
    let boot = echo_core::bootstrap::init();

    application().with_assets(assets::Assets).run(move |cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("ctrl-q", Quit, None),
            // Everything else is scoped to the lists so plain letters can type into the
            // search box (whose context adds `search`, defeating the `!search` predicate).
            KeyBinding::new("space", TogglePlayback, LIST_KEYS),
            KeyBinding::new("up", MoveUp, LIST_KEYS),
            KeyBinding::new("k", MoveUp, LIST_KEYS),
            KeyBinding::new("down", MoveDown, LIST_KEYS),
            KeyBinding::new("j", MoveDown, LIST_KEYS),
            KeyBinding::new("pageup", PageUp, LIST_KEYS),
            KeyBinding::new("pagedown", PageDown, LIST_KEYS),
            KeyBinding::new("home", SelectFirst, LIST_KEYS),
            KeyBinding::new("end", SelectLast, LIST_KEYS),
            KeyBinding::new("enter", Activate, LIST_KEYS),
            // `z` is the TUI's Enter alias.
            KeyBinding::new("z", Activate, LIST_KEYS),
            KeyBinding::new("left", FocusLibrary, LIST_KEYS),
            KeyBinding::new("right", FocusTracks, LIST_KEYS),
            KeyBinding::new("l", FocusTracks, LIST_KEYS),
            // Vim motions, matching the TUI's navigation handler.
            KeyBinding::new("g g", SelectFirst, LIST_KEYS),
            KeyBinding::new("shift-g", SelectLast, LIST_KEYS),
            KeyBinding::new("g c", JumpToCurrent, LIST_KEYS),
            KeyBinding::new("ctrl-u", HalfPageUp, LIST_KEYS),
            KeyBinding::new("ctrl-d", HalfPageDown, LIST_KEYS),
            KeyBinding::new("ctrl-b", PageUp, LIST_KEYS),
            KeyBinding::new("ctrl-f", PageDown, LIST_KEYS),
            // `h` is "back" in the TUI, not "focus sidebar" — the left arrow keeps that role.
            KeyBinding::new("h", Dismiss, LIST_KEYS),
            KeyBinding::new("backspace", Dismiss, LIST_KEYS),
            KeyBinding::new("escape", Dismiss, LIST_KEYS),
            KeyBinding::new("tab", CycleTab, LIST_KEYS),
            // Transport, the TUI's default keymap.
            KeyBinding::new("ctrl-right", NextTrack, LIST_KEYS),
            KeyBinding::new("ctrl-left", PreviousTrack, LIST_KEYS),
            KeyBinding::new("]", NextTrack, LIST_KEYS),
            KeyBinding::new("[", PreviousTrack, LIST_KEYS),
            KeyBinding::new("s", ToggleShuffle, LIST_KEYS),
            KeyBinding::new("r", CycleRepeat, LIST_KEYS),
            KeyBinding::new("shift-m", ToggleMute, LIST_KEYS),
            KeyBinding::new("shift-right", SeekForward, LIST_KEYS),
            KeyBinding::new("shift-left", SeekBackward, LIST_KEYS),
            KeyBinding::new(".", SeekForward, LIST_KEYS),
            KeyBinding::new(",", SeekBackward, LIST_KEYS),
            KeyBinding::new("0", SeekStart, LIST_KEYS),
            KeyBinding::new("=", VolumeUp, LIST_KEYS),
            KeyBinding::new("-", VolumeDown, LIST_KEYS),
            KeyBinding::new("shift-=", VolumeUpBig, LIST_KEYS),
            KeyBinding::new("shift--", VolumeDownBig, LIST_KEYS),
            // Views, prompts and toggles.
            KeyBinding::new("q", AddToQueue, LIST_KEYS),
            KeyBinding::new("shift-q", ToggleQueue, LIST_KEYS),
            KeyBinding::new("shift-d", OpenDevices, LIST_KEYS),
            KeyBinding::new("shift-r", Refresh, LIST_KEYS),
            KeyBinding::new("m", TogglePin, LIST_KEYS),
            KeyBinding::new(":", OpenCommand, LIST_KEYS),
            KeyBinding::new("f", CommandSearch, LIST_KEYS),
            KeyBinding::new("c", NewPlaylistPrompt, LIST_KEYS),
            KeyBinding::new("e", RenamePrompt, LIST_KEYS),
            // `/` filters the loaded track list like the TUI; the global search box gets
            // ctrl-k (and stays clickable).
            KeyBinding::new("/", OpenFilter, LIST_KEYS),
            KeyBinding::new("n", NextMatch, LIST_KEYS),
            KeyBinding::new("shift-n", PrevMatch, LIST_KEYS),
            KeyBinding::new("ctrl-k", FocusSearch, LIST_KEYS),
            KeyBinding::new("shift-l", ToggleLyrics, LIST_KEYS),
            KeyBinding::new("ctrl-shift-l", ToggleInlineLyrics, LIST_KEYS),
            KeyBinding::new("t", ToggleThemes, LIST_KEYS),
        ]);
        cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1100.0), px(720.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("echo".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| EchoApp::new(boot, window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
