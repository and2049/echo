//! echo — the GPUI desktop frontend.
//!
//! Same architecture as the `spotify` TUI: [`echo_core::bootstrap::init`] spawns the worker on a
//! tokio runtime and hands back the two event channels; the frontend applies worker events to
//! [`AppState`](echo_core::app::AppState) and draws from it. Here the 16ms poll loop is replaced
//! by GPUI's reactive model — a spawned task awaits worker events and calls `cx.notify()`.
//!
//! The tokio runtime lives on the main function's stack and stays entered for the lifetime of
//! the UI, so worker tasks keep running on its threads while GPUI blocks in `run()`.

#![windows_subsystem = "windows"]

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
    App, Bounds, Context, FocusHandle, Hsla, KeyBinding, Pixels, ScrollHandle, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, canvas,
    div, img, prelude::*, px, size, svg,
};
use gpui_platform::application;
use theme::{DesktopPalette, ToGpui, WINDOW_BG, WINDOW_FG};

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
        ToggleSidebar,
        HistoryBack,
        HistoryForward,
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
        PrevMatch,
        BackOrFocusLibrary,
        ConfirmPrompt,
        AddToPlaylist,
        OpenActionMenu,
        MarkDelete,
        ToggleSettings,
        ToggleHelp,
        EnterVisual,
        MoveTrackUp,
        MoveTrackDown,
        ToggleLike,
        Hide,
        HideOthers,
        ShowAll,
        MinimizeWindow,
        ZoomWindow
    ]
);

const LIST_CONTEXT: &str = "list";
const SEARCH_CONTEXT: &str = "search";
const LIST_KEYS: Option<&str> = Some("list && !search");

const PAGE_ROWS: isize = 10;

#[derive(Clone)]
pub(crate) struct ContextMenuState {
    pub index: usize,
    pub position: gpui::Point<Pixels>,
}

#[derive(Clone, Copy, PartialEq)]
enum Scrub {
    Seek,
    Volume,
}

#[derive(Clone, Copy)]
struct SidebarResize {
    start_x: Pixels,
    start_width: f32,
}

#[derive(Clone)]
pub(crate) struct TrackMenuState {
    pub ctx: echo_core::models::ActionMenuContext,
    pub position: Option<gpui::Point<Pixels>>,
    pub selected: usize,
    pub submenu: Option<usize>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum TrackMenuItem {
    Action(echo_core::models::ActionMenuAction),
    RemoveFromPlaylist,
}

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

#[derive(Clone)]
pub(crate) enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(echo_core::update::Release),
    Downloading(u8),
    Ready(String),
    Failed(String),
    Blocked(String),
}

impl UpdateState {
    fn from_error(error: echo_core::update::UpdateError) -> Self {
        use echo_core::update::UpdateError as E;
        let message = error.to_string();
        match error {
            E::NotWritable(..)
            | E::UnsupportedPlatform(_)
            | E::MissingAsset { .. }
            | E::DevBuild(_) => Self::Blocked(message),
            _ => Self::Failed(message),
        }
    }
}

pub(crate) struct EchoApp {
    pub(crate) state: echo_core::app::AppState,
    pub(crate) app_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    pub(crate) worker_tx: tokio::sync::mpsc::Sender<echo_core::events::WorkerEvent>,
    pub(crate) images: images::ImageCache,
    pub(crate) search_input: String,
    pub(crate) search_cursor: usize,
    pub(crate) search_focus: FocusHandle,
    command_focus: FocusHandle,
    pub(crate) setup_id_focus: FocusHandle,
    pub(crate) setup_secret_focus: FocusHandle,
    pub(crate) setup_uri_copied: bool,
    pub(crate) library_scroll: UniformListScrollHandle,
    pub(crate) tracks_scroll: UniformListScrollHandle,
    pub(crate) queue_scroll: UniformListScrollHandle,
    pub(crate) search_scroll: UniformListScrollHandle,
    pub(crate) artist_albums_scroll: UniformListScrollHandle,
    pub(crate) artist_top_tracks_scroll: UniformListScrollHandle,
    pub(crate) artist_list_scroll: UniformListScrollHandle,
    pub(crate) whats_new_scroll: UniformListScrollHandle,
    pub(crate) lyrics_scroll: UniformListScrollHandle,
    pub(crate) playlist_modal_scroll: ScrollHandle,
    pub(crate) device_modal_scroll: ScrollHandle,
    pub(crate) theme_modal_scroll: ScrollHandle,
    pub(crate) sidebar_width: f32,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) theme_modal_open: bool,
    pub(crate) theme_modal_index: usize,
    pub(crate) sort_menu_open: bool,
    pub(crate) sort_menu_index: usize,
    pub(crate) settings_open: bool,
    pub(crate) settings_path_input: String,
    pub(crate) settings_path_cursor: usize,
    pub(crate) settings_path_focus: FocusHandle,
    pub(crate) settings_scroll: ScrollHandle,
    pub(crate) update_state: UpdateState,
    pub(crate) help_open: bool,
    pub(crate) help_scroll: ScrollHandle,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) track_menu: Option<TrackMenuState>,
    pub(crate) submenu_scroll: ScrollHandle,
    pub(crate) submenu_row_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) submenu_bounds: Rc<Cell<Bounds<Pixels>>>,
    submenu_apex: gpui::Point<Pixels>,
    pending_count: Option<usize>,
    seek_bounds: Rc<Cell<Bounds<Pixels>>>,
    volume_bounds: Rc<Cell<Bounds<Pixels>>>,
    scrubbing: Option<Scrub>,
    sidebar_resizing: Option<SidebarResize>,
    titlebar_should_move: bool,
    mono_font: SharedString,
    focus_handle: FocusHandle,
    tokio: tokio::runtime::Handle,
}

fn mac_menus() -> Vec<gpui::Menu> {
    use gpui::{Menu, MenuItem};

    vec![
        Menu {
            name: "echo".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Settings…", ToggleSettings),
                MenuItem::action("Keyboard Shortcuts", ToggleHelp),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide echo", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit echo", Quit),
            ],
        },
        Menu {
            name: "Window".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Minimize", MinimizeWindow),
                MenuItem::action("Zoom", ZoomWindow),
                MenuItem::separator(),
            ],
        },
    ]
}

/// Whether a keystroke's modifiers mean "paste" for the hand-rolled text fields.
///
/// `Modifiers::secondary` is cmd on macOS and ctrl elsewhere; a Mac that only matched `control`
/// ignored cmd-v, which made the setup card — the first thing a new install shows, and a place
/// where credentials are pasted rather than typed — impossible to paste into. ctrl-v is still
/// accepted everywhere, since the desktop app mirrors the TUI's keymap.
fn is_paste_chord(modifiers: &gpui::Modifiers) -> bool {
    modifiers.secondary() || modifiers.control
}

/// Byte index of the char boundary one character before `cursor` (or `cursor` at the start).
fn prev_char_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor.min(value.len())]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Byte index of the char boundary one character after `cursor` (or `cursor` at the end).
fn next_char_boundary(value: &str, cursor: usize) -> usize {
    let cursor = cursor.min(value.len());
    value[cursor..]
        .chars()
        .next()
        .map(|c| cursor + c.len_utf8())
        .unwrap_or(cursor)
}

/// Insert `text` into `value` at the byte offset `cursor`, advancing the cursor past it. The
/// shared insertion path for both typed characters and pastes in the hand-rolled fields.
fn insert_at_cursor(value: &mut String, cursor: &mut usize, text: &str) {
    *cursor = (*cursor).min(value.len());
    value.insert_str(*cursor, text);
    *cursor += text.len();
}

/// Apply the cursor-aware editing keys shared by the hand-rolled text fields — arrow keys,
/// Home/End, Backspace, Delete, and printable character insertion — to `value`/`cursor`.
///
/// Returns `true` when the event was one of those keys (so the caller can stop). Enter, Escape,
/// Tab, and paste stay with the caller: each field submits, cancels, and filters pasted text
/// differently. Cursor positions are byte offsets kept on char boundaries so multi-byte queries
/// (e.g. CJK search terms) navigate a character at a time rather than splitting a codepoint.
fn apply_text_edit(value: &mut String, cursor: &mut usize, event: &gpui::KeyDownEvent) -> bool {
    *cursor = (*cursor).min(value.len());
    match event.keystroke.key.as_str() {
        "left" => *cursor = prev_char_boundary(value, *cursor),
        "right" => *cursor = next_char_boundary(value, *cursor),
        "home" => *cursor = 0,
        "end" => *cursor = value.len(),
        "backspace" => {
            if *cursor > 0 {
                let start = prev_char_boundary(value, *cursor);
                value.replace_range(start..*cursor, "");
                *cursor = start;
            }
        }
        "delete" => {
            if *cursor < value.len() {
                let end = next_char_boundary(value, *cursor);
                value.replace_range(*cursor..end, "");
            }
        }
        _ => {
            // Insert only genuine typed text: a modifier chord (ctrl-v, cmd-a) is not input, and
            // named keys with no glyph report no `key_char`.
            let modifiers = &event.keystroke.modifiers;
            if modifiers.control || modifiers.platform {
                return false;
            }
            match event.keystroke.key_char.as_deref() {
                Some(text) if !text.is_empty() && !text.chars().any(char::is_control) => {
                    insert_at_cursor(value, cursor, text);
                }
                _ => return false,
            }
        }
    }
    true
}

/// Render a field's text with the block caret drawn at `cursor` (a byte offset), so the hand-rolled
/// inputs show where the next edit lands instead of pinning the caret to the end.
pub(crate) fn text_with_cursor(value: &str, cursor: usize) -> String {
    let mut cursor = cursor.min(value.len());
    while cursor > 0 && !value.is_char_boundary(cursor) {
        cursor -= 1;
    }
    format!("{}▏{}", &value[..cursor], &value[cursor..])
}

/// Picks a fixed-pitch family that is actually installed. gpui matches families by exact name
/// against the system font database — there is no generic "monospace" alias and no substitution
/// when the name misses, so naming one font hard-codes a platform: "Consolas" is Windows-only,
/// and on Linux it silently fell through to the proportional UI font, which shears the ECHO
/// wordmark's box-drawing rows out of alignment. Ordered platform-native first, then the
/// families a desktop Linux install is likely to have; all of these cover U+2500–U+259F.
fn resolve_mono_font(cx: &App) -> SharedString {
    const CANDIDATES: &[&str] = &[
        "Consolas",          // Windows
        "Menlo",             // macOS
        "DejaVu Sans Mono",  // Debian/Ubuntu default
        "Liberation Mono",   // Fedora/RHEL default
        "Noto Sans Mono",    // GNOME
        "Ubuntu Mono",       // Ubuntu
        "JetBrains Mono",    // common developer install
        "Source Code Pro",   //
        "Courier New",       //
    ];

    let available = cx.text_system().all_font_names();
    CANDIDATES
        .iter()
        .find(|candidate| available.iter().any(|name| name == *candidate))
        .map(|name| SharedString::from(*name))
        // Nothing known is installed, so take anything self-describing as monospace rather
        // than falling back to a proportional face and mangling the art.
        .or_else(|| {
            available
                .iter()
                .find(|name| name.to_ascii_lowercase().contains("mono"))
                .map(|name| SharedString::from(name.clone()))
        })
        .unwrap_or_else(|| SharedString::from("monospace"))
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

        let sidebar_width = state
            .ui
            .library_config
            .sidebar_width
            .unwrap_or(views::SIDEBAR_WIDTH);
        let sidebar_collapsed = state.ui.library_config.sidebar_collapsed.unwrap_or(false);

        // Save the window rectangle on close so the next launch reopens the same size and
        // place. All three `WindowBounds` variants carry the restore bounds, so a window closed
        // while maximized still remembers a sensible windowed size.
        let this = cx.entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |this: &mut EchoApp, _cx| this.persist_window_bounds(window));
            true
        });

        Self {
            state,
            app_tx,
            worker_tx,
            images: images::ImageCache::default(),
            search_input: String::new(),
            search_cursor: 0,
            search_focus: cx.focus_handle(),
            command_focus: cx.focus_handle(),
            setup_id_focus: cx.focus_handle(),
            setup_secret_focus: cx.focus_handle(),
            setup_uri_copied: false,
            library_scroll: UniformListScrollHandle::new(),
            tracks_scroll: UniformListScrollHandle::new(),
            queue_scroll: UniformListScrollHandle::new(),
            search_scroll: UniformListScrollHandle::new(),
            artist_albums_scroll: UniformListScrollHandle::new(),
            artist_top_tracks_scroll: UniformListScrollHandle::new(),
            artist_list_scroll: UniformListScrollHandle::new(),
            whats_new_scroll: UniformListScrollHandle::new(),
            lyrics_scroll: UniformListScrollHandle::new(),
            playlist_modal_scroll: ScrollHandle::new(),
            device_modal_scroll: ScrollHandle::new(),
            theme_modal_scroll: ScrollHandle::new(),
            sidebar_width: sidebar_width.clamp(180.0, 480.0),
            sidebar_collapsed,
            theme_modal_open: false,
            theme_modal_index: 0,
            sort_menu_open: false,
            sort_menu_index: 0,
            settings_open: false,
            settings_path_input: String::new(),
            settings_path_cursor: 0,
            settings_path_focus: cx.focus_handle(),
            settings_scroll: ScrollHandle::new(),
            update_state: UpdateState::Idle,
            help_open: false,
            help_scroll: ScrollHandle::new(),
            context_menu: None,
            track_menu: None,
            submenu_scroll: ScrollHandle::new(),
            submenu_row_bounds: Rc::default(),
            submenu_bounds: Rc::default(),
            submenu_apex: gpui::Point::default(),
            pending_count: None,
            seek_bounds: Rc::default(),
            volume_bounds: Rc::default(),
            scrubbing: None,
            sidebar_resizing: None,
            titlebar_should_move: false,
            mono_font: resolve_mono_font(cx),
            focus_handle,
            tokio: tokio::runtime::Handle::current(),
        }
    }

    /// Remembers the window rectangle so the next launch reopens the same size and place. All
    /// three `WindowBounds` variants carry the restore bounds, so a window closed while
    /// maximized still remembers a sensible windowed size. Called from every way out: the
    /// platform's close request, and the `Quit` action the keybinding and the Linux caption
    /// button both dispatch.
    fn persist_window_bounds(&mut self, window: &Window) {
        let bounds = match window.window_bounds() {
            WindowBounds::Windowed(bounds)
            | WindowBounds::Maximized(bounds)
            | WindowBounds::Fullscreen(bounds) => bounds,
        };
        self.state.ui.library_config.window_bounds = Some(echo_core::config::WindowBoundsConfig {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
        });
        self.state.save_library_config();
    }

    /// Rows in whatever currently has keyboard focus: the device modal when open, else the
    /// `active_view` list — the same routing the TUI's navigation handler does.
    fn list_len(&self) -> usize {
        if self.state.ui.playlist_add_modal_open {
            return echo_core::action_menu::playlist_add_choices(&self.state).len();
        }
        if self.state.ui.device_modal_open {
            return self.state.data.devices.len();
        }
        if self.theme_modal_open {
            return views::sorted_theme_names(&self.state).len();
        }
        if let Some(menu) = self.track_menu.as_ref() {
            // The flyout takes over navigation while it is open, exactly like a nested list.
            if menu.submenu.is_some() {
                return echo_core::action_menu::playlist_add_choices(&self.state).len();
            }
            return views::track_menu_items(self).len();
        }
        if self.sort_menu_open {
            return views::SORT_OPTIONS.len();
        }
        match self.state.ui.active_view {
            ActiveView::TrackList => self.state.data.tracks.len(),
            ActiveView::Queue => self.state.data.queue.len(),
            ActiveView::SearchResults => match self.state.ui.active_search_tab {
                echo_core::app::SearchTab::Tracks => self.state.data.search_results.tracks.len(),
                echo_core::app::SearchTab::Albums => self.state.data.search_results.albums.len(),
                echo_core::app::SearchTab::Artists => self.state.data.search_results.artists.len(),
                echo_core::app::SearchTab::Playlists => {
                    self.state.data.search_results.playlists.len()
                }
            },
            ActiveView::ArtistList => self.state.artist_list().len(),
            ActiveView::WhatsNew => self.state.data.whats_new.len(),
            // Combined index space: Popular rows first, then the album rows.
            ActiveView::ArtistPage => self
                .state
                .data
                .artist_page_data
                .as_ref()
                .map_or(0, |data| data.top_tracks.len() + data.albums.len()),
            _ => match self.state.ui.active_library_tab {
                LibraryTab::Albums => self.state.data.saved_albums.len(),
                LibraryTab::Artists => self.state.data.followed_artists.len(),
                _ => self.state.data.library_view.len(),
            },
        }
    }

    /// Rows the artist page's Popular section occupies at the head of its combined index space.
    pub(crate) fn artist_page_top_len(&self) -> usize {
        self.state
            .data
            .artist_page_data
            .as_ref()
            .map_or(0, |data| data.top_tracks.len())
    }

    fn set_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        // A confirm prompt is modal: moving the list behind it would leave the prompt
        // describing a row that is no longer selected.
        if echo_core::intent::prompt_active(&self.state) {
            return;
        }
        // An armed `d` belongs to the row it was pressed on; moving off that row disarms it so
        // the second `d` can't delete something the user never pointed at.
        self.state.ui.pending_d_press = false;
        if self.state.ui.playlist_add_modal_open {
            self.state.ui.selected_playlist_modal_index = index;
            self.playlist_modal_scroll.scroll_to_item(index);
        } else if self.state.ui.device_modal_open {
            self.state.ui.selected_device_index = index;
            self.device_modal_scroll.scroll_to_item(index);
        } else if self.theme_modal_open {
            self.theme_modal_index = index;
            self.theme_modal_scroll.scroll_to_item(index);
        } else if let Some(menu) = self.track_menu.as_mut() {
            if menu.submenu.is_some() {
                menu.submenu = Some(index);
                self.submenu_scroll.scroll_to_item(index);
            } else {
                menu.selected = index;
            }
        } else if self.sort_menu_open {
            self.sort_menu_index = index;
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
                    self.artist_list_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::WhatsNew => {
                    self.state.ui.selected_whats_new_index = index;
                    self.whats_new_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::ArtistPage => {
                    self.state.ui.artist_page_album_index = index;
                    // The combined index spans two lists; scroll whichever owns the row.
                    let top_len = self.artist_page_top_len();
                    if index < top_len {
                        self.artist_top_tracks_scroll
                            .scroll_to_item(index, ScrollStrategy::Nearest);
                    } else {
                        self.artist_albums_scroll
                            .scroll_to_item(index - top_len, ScrollStrategy::Nearest);
                    }
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
        let current = if self.state.ui.playlist_add_modal_open {
            self.state.ui.selected_playlist_modal_index
        } else if self.state.ui.device_modal_open {
            self.state.ui.selected_device_index
        } else if self.theme_modal_open {
            self.theme_modal_index
        } else if let Some(menu) = self.track_menu.as_ref() {
            menu.submenu.unwrap_or(menu.selected)
        } else if self.sort_menu_open {
            self.sort_menu_index
        } else {
            match self.state.ui.active_view {
                ActiveView::TrackList => self.state.ui.selected_track_index,
                ActiveView::Queue => self.state.ui.selected_queue_index,
                ActiveView::SearchResults => self.state.ui.selected_search_index,
                ActiveView::ArtistList => self.state.ui.selected_artist_index,
                ActiveView::ArtistPage => self.state.ui.artist_page_album_index,
                ActiveView::WhatsNew => self.state.ui.selected_whats_new_index,
                _ => self.state.ui.selected_playlist_index,
            }
        };
        self.set_selection(current.saturating_add_signed(delta).min(len - 1), cx);
    }

    /// `l`, matching the TUI and README: like/unlike the focused track. On library rows it
    /// acts as Enter (the TUI's `l`-opens behavior); un-liking stages the confirm prompt.
    fn toggle_like(&mut self, cx: &mut Context<Self>) {
        if self.overlay_open() {
            return;
        }
        if self.state.ui.active_view == ActiveView::Library {
            self.activate_selection(cx);
            return;
        }
        if let Some(event) = echo_core::intent::toggle_like_selected(&mut self.state) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// `shift-j`/`shift-k`: move the selected track within an owned playlist. The intent
    /// enforces the writable-context and original-order guards.
    fn move_selected_track(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.overlay_open() || self.state.ui.active_view != ActiveView::TrackList {
            return;
        }
        let from = self.state.ui.selected_track_index;
        let to = from.saturating_add_signed(delta);
        if let Some(event) = echo_core::intent::move_track_in_playlist(&mut self.state, from, to) {
            self.dispatch(event);
            self.tracks_scroll
                .scroll_to_item(self.state.ui.selected_track_index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    fn select_last(&mut self, cx: &mut Context<Self>) {
        let len = self.list_len();
        if len > 0 {
            self.set_selection(len - 1, cx);
        }
    }

    /// Accept an open confirm prompt, mirroring the Confirm button in `views::prompt_modal`.
    /// Bound to Enter (through [`Self::activate_selection`]) and to `y` like the TUI.
    fn confirm_prompt(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if !echo_core::intent::prompt_active(&self.state) {
            return;
        }
        if let Some(event) = echo_core::intent::confirm_prompt(&mut self.state) {
            self.dispatch(event);
        }
        // A prompt staged from a visual range consumes it, so the range goes away with it.
        echo_core::intent::exit_visual(&mut self.state);
        cx.notify();
    }

    /// Enter on the focused list — the same intents the row click handlers use.
    fn activate_selection(&mut self, cx: &mut Context<Self>) {
        // A confirm prompt sits above everything else, exactly as it does in `dismiss`.
        if echo_core::intent::prompt_active(&self.state) {
            self.confirm_prompt(cx);
            return;
        }
        self.pending_count = None;
        let event = if self.state.ui.playlist_add_modal_open {
            let index = self.state.ui.selected_playlist_modal_index;
            echo_core::action_menu::commit_playlist_add(&mut self.state, index)
        } else if self.state.ui.device_modal_open {
            let index = self.state.ui.selected_device_index;
            echo_core::intent::transfer_to_device(&mut self.state, index)
        } else if self.theme_modal_open {
            if let Some(name) = views::sorted_theme_names(&self.state).get(self.theme_modal_index) {
                echo_core::intent::apply_theme(&mut self.state, name);
            }
            self.theme_modal_open = false;
            None
        } else if let Some(menu) = self.track_menu.as_ref() {
            if let Some(choice) = menu.submenu {
                self.commit_playlist_submenu(choice, cx);
            } else {
                let selected = menu.selected;
                // Enter on "Add to playlist" steps into the flyout instead of running an
                // action; every other item runs and closes the menu.
                if self.track_menu_add_row() == Some(selected) {
                    self.open_playlist_submenu(true, cx);
                } else if let Some((_, item, _)) =
                    views::track_menu_items(self).into_iter().nth(selected)
                {
                    self.run_track_menu_action(item, cx);
                }
            }
            None
        } else if self.sort_menu_open {
            if let Some((_, arg)) = views::SORT_OPTIONS.get(self.sort_menu_index) {
                self.apply_sort(arg, cx);
            }
            None
        } else {
            match self.state.ui.active_view {
                ActiveView::TrackList => {
                    let index = self.state.ui.selected_track_index;
                    echo_core::intent::play_track_at(&mut self.state, index)
                }
                ActiveView::Queue => {
                    let index = self.state.ui.selected_queue_index;
                    echo_core::intent::play_queue_track_at(&mut self.state, index)
                }
                ActiveView::SearchResults => {
                    let index = self.state.ui.selected_search_index;
                    echo_core::intent::activate_search_result(&mut self.state, index)
                }
                ActiveView::ArtistList => {
                    let index = self.state.ui.selected_artist_index;
                    echo_core::intent::open_artist_at(&mut self.state, index)
                }
                ActiveView::ArtistPage => {
                    let index = self.state.ui.artist_page_album_index;
                    echo_core::intent::activate_artist_page_row(&mut self.state, index)
                }
                ActiveView::WhatsNew => {
                    let index = self.state.ui.selected_whats_new_index;
                    echo_core::intent::open_whats_new_album(&mut self.state, index)
                }
                _ => {
                    let index = self.state.ui.selected_playlist_index;
                    match self.state.ui.active_library_tab {
                        LibraryTab::Albums => {
                            echo_core::intent::open_album(&mut self.state, index)
                        }
                        LibraryTab::Artists => {
                            echo_core::intent::open_followed_artist(&mut self.state, index)
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
        if self.theme_modal_open {
            // Open on the theme that is already applied, so j/k start from where the user is.
            let active = self.state.ui.library_config.active_theme.clone();
            self.theme_modal_index = active
                .and_then(|name| {
                    views::sorted_theme_names(&self.state)
                        .iter()
                        .position(|n| *n == name)
                })
                .unwrap_or(0);
            self.theme_modal_scroll.scroll_to_item(self.theme_modal_index);
        }
        cx.notify();
    }

    /// Escape / `h` / backspace: close whatever is topmost, else go back — the same ordering
    /// as the TUI's back handling, with the desktop-only modals checked first.
    fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        self.state.ui.pending_d_press = false;
        if echo_core::intent::prompt_active(&self.state) {
            echo_core::intent::cancel_prompt(&mut self.state);
        } else if self.in_visual() {
            // Escape drops the range before it starts walking the view history.
            echo_core::intent::exit_visual(&mut self.state);
        } else if self.context_menu.is_some() {
            self.context_menu = None;
        } else if self.track_menu.as_ref().is_some_and(|menu| menu.submenu.is_some()) {
            // The flyout is the topmost layer: escape steps back to the menu items.
            self.close_playlist_submenu(cx);
        } else if self.track_menu.is_some() {
            self.track_menu = None;
        } else if self.state.ui.playlist_add_modal_open {
            echo_core::action_menu::cancel_playlist_add(&mut self.state);
        } else if self.state.ui.device_modal_open {
            self.state.ui.device_modal_open = false;
        } else if self.theme_modal_open {
            self.theme_modal_open = false;
        } else if self.sort_menu_open {
            self.sort_menu_open = false;
        } else if self.settings_open {
            self.settings_open = false;
        } else if self.help_open {
            self.help_open = false;
        } else if self.state.ui.lyrics_modal_open {
            self.state.ui.lyrics_modal_open = false;
        } else if self.history_back(cx) {
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
                self.close_artist_page(cx);
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
        } else if self.state.ui.active_view == ActiveView::Queue {
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

    /// Runs a track-menu item against the menu's staged context, then closes the menu. Remove
    /// only stages `track_delete_prompt` — the confirm modal fires the actual event.
    pub(crate) fn run_track_menu_action(&mut self, item: TrackMenuItem, cx: &mut Context<Self>) {
        let Some(menu) = self.track_menu.take() else {
            cx.notify();
            return;
        };
        let ctx = menu.ctx;
        match item {
            TrackMenuItem::Action(action) => {
                if let Some(event) = echo_core::action_menu::run(&mut self.state, ctx, action) {
                    self.dispatch(event);
                }
            }
            TrackMenuItem::RemoveFromPlaylist => {
                if let Some(context) = self
                    .state
                    .data
                    .active_tracklist_context
                    .clone()
                    .filter(|c| c.can_modify_playlist(self.state.data.user_id.as_ref()))
                {
                    self.state.ui.track_delete_prompt = Some((context.id, vec![ctx.track_id]));
                }
            }
        }
        cx.notify();
    }

    // The add-to-playlist flyout. The track menu's "Add to playlist" row opens a second panel
    // beside it rather than the full-screen picker `a` uses, so the choice stays in one gesture.

    /// Index of the "Add to playlist" row in the open track menu, if it has one. The flyout
    /// anchors to it, and the hover/keyboard handlers key off it.
    pub(crate) fn track_menu_add_row(&self) -> Option<usize> {
        views::track_menu_items(self).iter().position(|(_, item, _)| {
            matches!(
                item,
                TrackMenuItem::Action(echo_core::models::ActionMenuAction::AddToPlaylist)
            )
        })
    }

    /// Open the flyout. `focus_row` also parks the menu's keyboard selection on the row it
    /// hangs off, which is what Enter and a click mean; a hover leaves the selection where the
    /// keyboard put it, since the view lights that row for as long as the flyout is open.
    pub(crate) fn open_playlist_submenu(&mut self, focus_row: bool, cx: &mut Context<Self>) {
        let Some(row) = self.track_menu_add_row() else {
            return;
        };
        if let Some(menu) = self.track_menu.as_mut() {
            if focus_row {
                menu.selected = row;
            }
            if menu.submenu.is_none() {
                menu.submenu = Some(0);
                self.submenu_scroll.scroll_to_item(0);
            }
        }
        cx.notify();
    }

    /// Close the flyout but keep the menu itself: what escape, the left arrow and a hover that
    /// leaves the row all mean.
    pub(crate) fn close_playlist_submenu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.track_menu.as_mut() {
            menu.submenu = None;
        }
        cx.notify();
    }

    /// Add the menu's track to flyout choice `index`, then close the whole menu. The register
    /// is what `commit_playlist_add` resolves tracks from, the same handoff
    /// `action_menu::run(AddToPlaylist)` makes before opening the modal picker.
    pub(crate) fn commit_playlist_submenu(&mut self, index: usize, cx: &mut Context<Self>) {
        // Out of range only happens with no playlists at all, where Enter should do nothing
        // rather than close the menu; it would also leave the register staged.
        if index >= echo_core::action_menu::playlist_add_choices(&self.state).len() {
            return;
        }
        let Some(menu) = self.track_menu.take() else {
            return;
        };
        self.state.ui.operation_register = vec![menu.ctx.track_id];
        if let Some(event) = echo_core::action_menu::commit_playlist_add(&mut self.state, index) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// A pointer move over track-menu row `ix`. Hovering the "Add to playlist" row opens the
    /// flyout; hovering any other row closes it — unless the pointer is inside the triangle
    /// spanned by where it left that row and the flyout's near edge, which is the diagonal a
    /// user aiming at the flyout travels through.
    pub(crate) fn hover_track_menu_row(
        &mut self,
        ix: usize,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.track_menu_add_row() != Some(ix) {
            if self.track_menu.as_ref().is_some_and(|menu| menu.submenu.is_some())
                && !self.pointer_aims_at_submenu(position)
            {
                self.close_playlist_submenu(cx);
            }
            return;
        }
        self.submenu_apex = position;
        if self.track_menu.as_ref().is_some_and(|menu| menu.submenu.is_none()) {
            self.open_playlist_submenu(false, cx);
        }
    }

    /// Whether `position` falls in the triangle from [`Self::submenu_apex`] to the flyout's
    /// near edge. Degenerate while the flyout has yet to be painted, which reads as "not
    /// aiming" and simply closes it a frame early.
    fn pointer_aims_at_submenu(&self, position: gpui::Point<Pixels>) -> bool {
        let flyout = self.submenu_bounds.get();
        if flyout.size.height <= px(0.0) {
            return false;
        }
        // The flyout sits on whichever side of the menu had room (`views::playlist_submenu`),
        // so the near edge — and which way "toward it" is — follows from where it landed.
        let flipped = flyout.left() < self.submenu_row_bounds.get().left();
        let edge = if flipped { flyout.right() } else { flyout.left() };
        // The flyout can be much taller than the menu, so the triangle alone would cover most
        // of the rows below: a move only counts as aiming while it still travels toward the
        // flyout. Straight down the menu closes it at once.
        let toward = if flipped {
            position.x < self.submenu_apex.x
        } else {
            position.x > self.submenu_apex.x
        };
        if !toward {
            return false;
        }
        // A few pixels of slack at both corners: the flyout's own border and rounding mean the
        // exact corner is not where the pointer needs to land.
        let apex = self.submenu_apex;
        let top = gpui::point(edge, flyout.top() - px(4.0));
        let bottom = gpui::point(edge, flyout.bottom() + px(4.0));
        let side = |a: gpui::Point<Pixels>, b: gpui::Point<Pixels>| {
            f32::from(b.x - a.x) * f32::from(position.y - a.y)
                - f32::from(b.y - a.y) * f32::from(position.x - a.x)
        };
        let (ab, bc, ca) = (side(apex, top), side(top, bottom), side(bottom, apex));
        (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
    }

    /// The track `a` / `shift-a` act on: the focused row where the view has one, otherwise the
    /// currently playing track. Mirrors the TUI's `A` handler.
    pub(crate) fn action_target(&self) -> Option<echo_core::models::ActionMenuContext> {
        use echo_core::models::ActionMenuContext;
        let ui = &self.state.ui;
        let data = &self.state.data;
        let row = match ui.active_view {
            ActiveView::TrackList => data.tracks.get(ui.selected_track_index),
            ActiveView::Queue => data.queue.get(ui.selected_queue_index),
            ActiveView::SearchResults
                if ui.active_search_tab == echo_core::app::SearchTab::Tracks =>
            {
                return data
                    .search_results
                    .tracks
                    .get(ui.selected_search_index)
                    .map(ActionMenuContext::from);
            }
            _ => None,
        };
        if let Some(track) = row {
            return Some(ActionMenuContext::from(track));
        }

        self.playing_track_context()
    }

    /// Menu context for the currently playing track: `action_target`'s fallback and the
    /// playback bar's click-to-navigate target.
    pub(crate) fn playing_track_context(&self) -> Option<echo_core::models::ActionMenuContext> {
        let playback = &self.state.playback;
        let track_id = playback.playing_track_id.clone()?;
        // Playback state only carries the joined "A, B, C" artist string; when the playing
        // track is in the current list or queue, prefer its structured data so go-to-artist
        // gets the first artist's own name.
        if let Some(track) = self
            .state
            .data
            .tracks
            .iter()
            .chain(self.state.data.queue.iter())
            .find(|track| track.id == track_id)
        {
            return Some(echo_core::models::ActionMenuContext::from(track));
        }
        Some(echo_core::models::ActionMenuContext {
            album_name: self
                .state
                .data
                .local_library
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map(|track| track.album.clone())
                .unwrap_or_default(),
            track_id,
            source: playback
                .playing_track_source
                .unwrap_or(echo_core::models::TrackSource::Spotify),
            track_name: playback.playing_track_title.clone(),
            local_path: playback.playing_track_local_path.clone(),
            album_id: playback.playing_track_album_id.clone(),
            artist_id: playback.playing_track_artist_id.clone(),
            artist_name: playback.playing_track_artist.clone(),
        })
    }

    /// Opens an artist page directly by id — the per-artist click path in track rows.
    pub(crate) fn open_artist(
        &mut self,
        artist_id: String,
        artist_name: String,
        cx: &mut Context<Self>,
    ) {
        self.state
            .begin_artist_page_load(artist_id.clone(), artist_name.clone(), None);
        self.dispatch(AppEvent::LoadArtistPage {
            artist_id,
            artist_name: Some(artist_name),
            artist_image_url: None,
        });
        cx.notify();
    }

    /// Runs one go-to action (album/artist) for a track context — the click-to-navigate path
    /// shared by track-row cells and the playback bar.
    pub(crate) fn go_to(
        &mut self,
        ctx: echo_core::models::ActionMenuContext,
        action: echo_core::models::ActionMenuAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(event) = echo_core::action_menu::run(&mut self.state, ctx, action) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// `shift-a` — the track action menu, centered because there is no click to anchor it to.
    fn open_action_menu(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if let Some(ctx) = self.action_target() {
            self.context_menu = None;
            self.track_menu = Some(TrackMenuState {
                ctx,
                position: None,
                selected: 0,
                submenu: None,
            });
        }
        cx.notify();
    }

    /// Whether a visual-mode range is being built.
    pub(crate) fn in_visual(&self) -> bool {
        self.state.ui.mode == AppMode::Visual
    }

    /// Shift-click on row `ix`: anchor at the row that was focused (unless a range is already
    /// open, which keeps its anchor) and extend the selection to `ix`.
    pub(crate) fn extend_selection_to(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.in_visual() {
            let anchor = match self.state.ui.active_view {
                ActiveView::TrackList => self.state.ui.selected_track_index,
                ActiveView::Queue => self.state.ui.selected_queue_index,
                ActiveView::SearchResults => self.state.ui.selected_search_index,
                ActiveView::Library => self.state.ui.selected_playlist_index,
                _ => return,
            };
            echo_core::intent::set_visual_anchor(&mut self.state, anchor);
        }
        self.set_selection(ix, cx);
        cx.notify();
    }

    /// `v` — start a range selection, or leave one already in progress.
    fn toggle_visual(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if self.in_visual() {
            echo_core::intent::exit_visual(&mut self.state);
        } else {
            echo_core::intent::enter_visual(&mut self.state);
        }
        cx.notify();
    }

    /// `a` — add the selection to a playlist, or save the album when an album row is focused.
    /// The picker commits through `action_menu::commit_playlist_add`, which resolves the
    /// tracks from the current selection.
    fn add_to_playlist(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if self.in_visual() {
            echo_core::intent::add_visual_selection_to_playlist(&mut self.state);
            cx.notify();
            return;
        }
        let album_id = match self.state.ui.active_view {
            ActiveView::SearchResults
                if self.state.ui.active_search_tab == echo_core::app::SearchTab::Albums =>
            {
                self.state
                    .data
                    .search_results
                    .albums
                    .get(self.state.ui.selected_search_index)
                    .map(|album| (album.id.clone(), album.name.clone()))
            }
            _ => None,
        };

        if let Some((id, name)) = album_id {
            let language = self.state.ui.library_config.language.clone();
            self.state.ui.status_message = Some(
                echo_core::i18n::t("messages.saved_to_library", &language).replace("{}", &name),
            );
            self.state.ui.status_message_expiry =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
            self.dispatch(AppEvent::SaveAlbums(vec![id]));
        } else {
            self.state.ui.playlist_add_modal_open = true;
            self.state.ui.selected_playlist_modal_index = 0;
        }
        cx.notify();
    }

    /// Open or close the settings sheet, seeding the folder field from the saved config.
    pub(crate) fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.settings_path_input = self
                .state
                .ui
                .library_config
                .local_music_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            self.settings_path_cursor = self.settings_path_input.len();
        }
        cx.notify();
    }

    /// Run a settings control through the `:` registry. Every command the sheet exposes already
    /// validates its own input and sets its own status message, so the rows stay declarative.
    pub(crate) fn run_setting(&mut self, cmd: String, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::commands::run(&mut self.state, &cmd) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Audio-quality keys have no `:` command, so they write the config directly. They are read
    /// when the librespot daemon starts, hence the "next launch" note in the sheet.
    pub(crate) fn set_audio_quality(
        &mut self,
        apply: impl FnOnce(&mut echo_core::config::LibraryConfig),
        cx: &mut Context<Self>,
    ) {
        apply(&mut self.state.ui.library_config);
        self.state.save_library_config();
        self.state.ui.status_message =
            Some(views::tr(&self.state, "desktop.saved_next_launch").to_string());
        self.state.ui.status_message_expiry =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
        cx.notify();
    }

    /// Ask GitHub whether a newer release exists.
    ///
    /// The second exception to the "every setting is a `:` command" rule, alongside
    /// [`Self::set_audio_quality`]: `commands::execute` is synchronous and returns an
    /// `Option<AppEvent>`, which cannot express an in-flight network call.
    pub(crate) fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.update_state,
            UpdateState::Checking | UpdateState::Downloading(_)
        ) {
            return;
        }
        self.update_state = UpdateState::Checking;
        cx.notify();

        // reqwest needs the tokio reactor, so the request runs on the runtime; the result comes
        // back over a oneshot, which is reactor-free and therefore safe to await on GPUI's
        // foreground executor (the same reasoning as the worker-event bridge above).
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tokio.spawn(async move {
            let _ = tx.send(echo_core::update::check().await);
        });

        cx.spawn(async move |this, cx| {
            let outcome = rx.await;
            let _ = this.update(cx, |app: &mut EchoApp, cx| {
                app.update_state = match outcome {
                    Ok(Ok(echo_core::update::Check::UpToDate)) => UpdateState::UpToDate,
                    Ok(Ok(echo_core::update::Check::Available(release))) => {
                        UpdateState::Available(release)
                    }
                    Ok(Err(error)) => UpdateState::from_error(error),
                    Err(_) => UpdateState::Failed(
                        views::tr(&app.state, "desktop.settings.updates.failed").to_string(),
                    ),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Download the release found by [`Self::check_for_updates`] and swap it in.
    pub(crate) fn install_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(release) = self.update_state.clone() else {
            return;
        };
        // Resolved before the download so a read-only install fails immediately.
        let plan = match echo_core::update::plan() {
            Ok(plan) => plan,
            Err(error) => {
                self.update_state = UpdateState::from_error(error);
                cx.notify();
                return;
            }
        };

        self.update_state = UpdateState::Downloading(0);
        cx.notify();

        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        self.tokio.spawn(async move {
            let staged = echo_core::update::download(plan, &release, move |percent| {
                let _ = progress_tx.send(percent);
            })
            .await;
            // The swap is blocking filesystem work, but it is a handful of renames — keeping it
            // on the runtime rather than the UI thread is enough.
            let _ = done_tx.send(staged.and_then(echo_core::update::apply));
        });

        cx.spawn(async move |this, cx| {
            // The progress sender is dropped when the download future completes, which ends
            // this loop and lets the outcome through.
            while let Some(percent) = progress_rx.recv().await {
                let applied = this.update(cx, |app: &mut EchoApp, cx| {
                    app.update_state = UpdateState::Downloading(percent);
                    cx.notify();
                });
                if applied.is_err() {
                    return; // entity dropped — app is shutting down
                }
            }
            let outcome = done_rx.await;
            let _ = this.update(cx, |app: &mut EchoApp, cx| {
                app.update_state = match outcome {
                    Ok(Ok(version)) => UpdateState::Ready(version),
                    Ok(Err(error)) => UpdateState::from_error(error),
                    Err(_) => UpdateState::Failed(
                        views::tr(&app.state, "desktop.settings.updates.failed").to_string(),
                    ),
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Typing in the settings sheet's local-folder field. Enter submits it as `:localpath`.
    fn handle_settings_path_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => {
                let path = self.settings_path_input.trim().to_string();
                if !path.is_empty() {
                    self.run_setting(format!("localpath {path}"), cx);
                }
                window.focus(&self.focus_handle.clone(), cx);
            }
            "escape" => window.focus(&self.focus_handle.clone(), cx),
            "v" if is_paste_chord(&event.keystroke.modifiers) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    insert_at_cursor(
                        &mut self.settings_path_input,
                        &mut self.settings_path_cursor,
                        text.trim(),
                    );
                }
            }
            _ => {
                apply_text_edit(
                    &mut self.settings_path_input,
                    &mut self.settings_path_cursor,
                    event,
                );
            }
        }
        cx.notify();
    }

    /// Apply a sort from the picker by running the matching `:sort` command, so the desktop
    /// and the TUI cannot drift apart on what each option does.
    pub(crate) fn apply_sort(&mut self, arg: &str, cx: &mut Context<Self>) {
        self.sort_menu_open = false;
        if let Some(event) = echo_core::commands::run(&mut self.state, &format!("sort {arg}")) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Consume the pending vim count, defaulting to a single step.
    fn take_count(&mut self) -> isize {
        self.pending_count.take().unwrap_or(1).max(1) as isize
    }

    fn overlay_open(&self) -> bool {
        echo_core::intent::prompt_active(&self.state)
            || self.context_menu.is_some()
            || self.track_menu.is_some()
            || self.state.ui.playlist_add_modal_open
            || self.state.ui.device_modal_open
            || self.theme_modal_open
            || self.sort_menu_open
            || self.settings_open
            || self.help_open
            || self.state.ui.lyrics_modal_open
    }

    /// Backspace: close the topmost overlay if one is open, else hand keyboard focus back to
    /// the library sidebar while keeping the current page visible — the TUI's quick two-pane
    /// hop, without walking the view history the way `h`/escape do.
    fn back_or_focus_library(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if self.overlay_open() {
            self.dismiss(cx);
        } else {
            self.focus_library(cx);
        }
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
            "v" if is_paste_chord(&event.keystroke.modifiers) => {
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
            "v" if is_paste_chord(&event.keystroke.modifiers) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.state
                        .ui
                        .search_query
                        .extend(text.chars().filter(|c| *c != '\r' && *c != '\n'));
                    echo_core::intent::update_search_matches(&mut self.state);
                    self.scroll_to_selected_track();
                }
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
        // In visual mode `q` queues the whole range, matching the TUI.
        if self.in_visual() {
            if let Some(event) = echo_core::intent::queue_visual_selection(&mut self.state) {
                self.dispatch(event);
            }
            cx.notify();
            return;
        }
        if let Some(event) = echo_core::intent::queue_selected_track(&self.state) {
            self.dispatch(event);
            self.state.ui.status_message =
                Some(views::tr(&self.state, "desktop.added_to_queue").to_string());
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
        self.state.ui.pending_d_press = false;
        match self.state.ui.active_view {
            ActiveView::SearchResults => {
                self.state.ui.active_search_tab = match self.state.ui.active_search_tab {
                    SearchTab::Tracks => SearchTab::Albums,
                    SearchTab::Albums => SearchTab::Artists,
                    SearchTab::Artists => SearchTab::Playlists,
                    SearchTab::Playlists => SearchTab::Tracks,
                };
                self.state.ui.selected_search_index = 0;
            }
            ActiveView::Library => {
                self.state.ui.active_library_tab = match self.state.ui.active_library_tab {
                    LibraryTab::Playlists => LibraryTab::Albums,
                    LibraryTab::Albums => LibraryTab::Artists,
                    _ => LibraryTab::Playlists,
                };
                self.state.ui.selected_playlist_index = 0;
                if self.state.ui.active_library_tab == LibraryTab::Artists
                    && self.state.data.followed_artists.is_empty()
                {
                    self.dispatch(AppEvent::FetchFollowedArtists);
                }
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
        let palette = DesktopPalette::resolve(theme);
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
            .border_color(palette.border)
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
                                    el.bg(palette.row_selected).text_color(fg)
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
            "v" if is_paste_chord(&event.keystroke.modifiers) => {
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
                self.search_cursor = 0;
                window.focus(&self.focus_handle, cx);
            }
            // Spaces are kept — unlike the credential fields, a query is words — but newlines
            // are dropped so a copied line does not smuggle one into the search term.
            "v" if is_paste_chord(&event.keystroke.modifiers) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let filtered: String =
                        text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
                    insert_at_cursor(&mut self.search_input, &mut self.search_cursor, &filtered);
                }
            }
            _ => {
                apply_text_edit(&mut self.search_input, &mut self.search_cursor, event);
            }
        }
        cx.notify();
    }

    fn focus_library(&mut self, cx: &mut Context<Self>) {
        // With a menu open the arrows belong to it: left closes the add-to-playlist flyout
        // rather than moving pane focus behind the overlay.
        if self.track_menu.is_some() {
            self.close_playlist_submenu(cx);
            return;
        }
        self.state.ui.pending_d_press = false;
        if self.sidebar_collapsed {
            // Focusing an invisible pane would strand the cursor; bring the sidebar back.
            self.set_sidebar_collapsed(false, cx);
        }
        self.state.ui.active_view = ActiveView::Library;
        cx.notify();
    }

    fn focus_tracks(&mut self, cx: &mut Context<Self>) {
        // Right steps into the flyout when the menu's "Add to playlist" row is selected.
        if self.track_menu.is_some() {
            if self.track_menu_add_row() == self.track_menu.as_ref().map(|menu| menu.selected) {
                self.open_playlist_submenu(true, cx);
            }
            return;
        }
        self.state.ui.pending_d_press = false;
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

    // Drag-to-scrub: mouse-down on a bar starts it, window-level mouse-move updates the state
    // optimistically for live feedback, and release fires the worker event once.

    fn scrub_bounds(&self, target: Scrub) -> Bounds<Pixels> {
        match target {
            Scrub::Seek => self.seek_bounds.get(),
            Scrub::Volume => self.volume_bounds.get(),
        }
    }

    fn begin_scrub(&mut self, target: Scrub, x: Pixels, cx: &mut Context<Self>) {
        self.scrubbing = Some(target);
        self.update_scrub(x, cx);
    }

    fn update_scrub(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(target) = self.scrubbing else { return };
        let bounds = self.scrub_bounds(target);
        if bounds.size.width <= px(0.0) {
            return;
        }
        let fraction = ((x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
        match target {
            Scrub::Seek => {
                if self.state.playback.duration_ms > 0
                    && self.state.playback.playing_track_id.is_some()
                {
                    let target_ms = (self.state.playback.duration_ms as f32 * fraction) as u32;
                    self.state.playback.set_optimistic_progress(target_ms);
                }
            }
            Scrub::Volume => {
                self.state.playback.volume = (fraction * 100.0).round() as u32;
            }
        }
        cx.notify();
    }

    fn finish_scrub(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(target) = self.scrubbing.take() else { return };
        let bounds = self.scrub_bounds(target);
        if bounds.size.width <= px(0.0) {
            cx.notify();
            return;
        }
        let fraction = ((x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
        match target {
            Scrub::Seek => self.seek_to_fraction(fraction, cx),
            Scrub::Volume => self.set_volume_fraction(fraction, cx),
        }
    }

    // Drag-to-resize of the library sidebar: mouse-down on its right edge starts it, window-level
    // mouse moves update the width optimistically, and release settles and saves it.

    fn begin_sidebar_resize(&mut self, x: Pixels, cx: &mut Context<Self>) {
        self.sidebar_resizing = Some(SidebarResize {
            start_x: x,
            start_width: self.sidebar_width,
        });
        cx.notify();
    }

    fn update_sidebar_resize(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(resize) = self.sidebar_resizing else { return };
        let delta = x - resize.start_x;
        self.sidebar_width = (resize.start_width + f32::from(delta)).clamp(180.0, 480.0);
        cx.notify();
    }

    fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_resizing.take().is_some() {
            // Saved on release rather than on every pointer move — a drag is one decision, not
            // a few hundred config writes.
            self.state.ui.library_config.sidebar_width = Some(self.sidebar_width);
            self.state.save_library_config();
            cx.notify();
        }
    }

    pub(crate) fn set_sidebar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.sidebar_collapsed = collapsed;
        self.state.ui.library_config.sidebar_collapsed = Some(collapsed);
        self.state.save_library_config();
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.set_sidebar_collapsed(!self.sidebar_collapsed, cx);
    }

    pub(crate) fn history_back(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.state.pop_view_history() {
            return false;
        }
        self.state.clear_pending_artist_page();
        if self.state.data.tracklist_image_url.is_some() {
            self.dispatch(AppEvent::ReloadHeaderImage);
        }
        cx.notify();
        true
    }

    pub(crate) fn history_forward(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.state.forward_view_history() {
            return false;
        }
        self.state.clear_pending_artist_page();
        if self.state.data.tracklist_image_url.is_some() {
            self.dispatch(AppEvent::ReloadHeaderImage);
        }
        cx.notify();
        true
    }


    pub(crate) fn close_artist_page(&mut self, cx: &mut Context<Self>) {
        self.state.ui.active_view = ActiveView::Library;
        self.state.clear_pending_artist_page();
        self.dispatch(AppEvent::CancelArtistPageLoad);
        cx.notify();
    }

    /// Sends an intent-produced event to the worker, with the same side channels the TUI's main
    /// loop has: a LoadContextTracks with cover art also kicks off the header image fetch, and
    /// ReloadHeaderImage is handled entirely here (the worker has no handler for it).
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
        if let AppEvent::ReloadHeaderImage = event {
            if let Some(url) = self.state.data.tracklist_image_url.as_ref() {
                echo_core::image_tasks::spawn_header_for_url(
                    url,
                    self.worker_tx.clone(),
                    self.state.ui.library_config.cover_img_pixels,
                );
            }
            return;
        }
        let _ = self.app_tx.send(event);
    }

    fn render_playback_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.ui.active_theme;
        let palette = DesktopPalette::resolve(theme);
        let fg = theme.text.gpui(WINDOW_FG());
        let muted = theme.text_muted.gpui(WINDOW_FG());
        let accent = theme.primary.gpui(WINDOW_FG());

        let playback = &self.state.playback;
        let title: SharedString = if playback.playing_track_title.is_empty() {
            views::tr(&self.state, "desktop.nothing_playing")
        } else {
            playback.playing_track_title.clone().into()
        };
        let artist: SharedString = playback.playing_track_artist.clone().into();
        let has_track = playback.playing_track_id.is_some();

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
        let volume_label: SharedString = format!("{}%", playback.volume).into();
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
        let vis_bins = self.state.ui.vis_bins.clamp(5, 32);

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
                        return (
                            views::tr(&self.state, "desktop.no_lyrics_found").to_string(),
                            String::new(),
                        );
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
                    (
                        views::tr(&self.state, "desktop.no_lyrics_found").to_string(),
                        String::new(),
                    )
                } else {
                    (String::new(), String::new())
                }
            })
            .filter(|(current, _)| !current.is_empty());

        // Inline lyrics keep the top-row center slot; "Playing from X" fills it otherwise.
        let playing_from = if inline_lyrics.is_none() && has_track {
            views::playing_context_label(&self.state)
        } else {
            None
        };

        let seek_bounds = self.seek_bounds.clone();
        let volume_bounds = self.volume_bounds.clone();

        div()
            .flex()
            .flex_col()
            .justify_center()
            .gap_1()
            .h(px(108.0))
            .px_4()
            .border_t_1()
            .border_color(palette.border)
            .child(
                // Top row: song card, condensed lyric line, visualizer. Its contents come and go;
                // its height doesn't, so nothing here can move the row below.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(px(PLAYBACK_ROW_HEIGHT))
                    .child(
                        div()
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
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
                                    .bg(palette.wash)
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
                                            .id("playing-title")
                                            .text_color(fg)
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .overflow_hidden()
                                            .when(has_track, |el| {
                                                el.cursor_pointer()
                                                    .hover(|style| style.underline())
                                                    .on_click(cx.listener(
                                                        |this, _event, _window, cx| {
                                                            if let Some(ctx) =
                                                                this.playing_track_context()
                                                            {
                                                                this.go_to(
                                                                    ctx,
                                                                    echo_core::models::ActionMenuAction::GoToAlbum,
                                                                    cx,
                                                                );
                                                            }
                                                        },
                                                    ))
                                            })
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .id("playing-artist")
                                            .text_color(muted)
                                            .text_xs()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .overflow_hidden()
                                            .when(has_track, |el| {
                                                el.cursor_pointer()
                                                    .hover(|style| style.underline())
                                                    .on_click(cx.listener(
                                                        |this, _event, _window, cx| {
                                                            if let Some(ctx) =
                                                                this.playing_track_context()
                                                            {
                                                                this.go_to(
                                                                    ctx,
                                                                    echo_core::models::ActionMenuAction::GoToArtist,
                                                                    cx,
                                                                );
                                                            }
                                                        },
                                                    ))
                                            })
                                            .child(artist),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_grow(1.0)
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            .when_some(inline_lyrics, |el, (current, next)| {
                                el.child(
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
                                })
                            })
                            .when_some(playing_from, |el, label| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .whitespace_nowrap()
                                        .max_w_full()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(SharedString::from(label)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .when_some(visualizer_bands, |el, bands| {
                                // The engine always fills 32 bands, 0–100; they are averaged down
                                // to the configured bin count (`:visbins`, same math as the TUI)
                                // and painted as bottom-anchored bars. Repaints ride the fast
                                // tick.
                                el.child(div().flex_none().w(px(120.0)).h(px(32.0)).child(
                                    canvas(
                                        |_, _, _| (),
                                        move |bounds, _, window, _| {
                                            let bands = bands.lock();
                                            let chunk = bands.len() as f32 / vis_bins as f32;
                                            let band_width =
                                                bounds.size.width / vis_bins as f32;
                                            for index in 0..vis_bins {
                                                let start = (index as f32 * chunk) as usize;
                                                let end = if index == vis_bins - 1 {
                                                    bands.len()
                                                } else {
                                                    ((index + 1) as f32 * chunk) as usize
                                                };
                                                let slice = &bands[start..end.max(start + 1)];
                                                let value = slice.iter().sum::<f32>()
                                                    / slice.len() as f32;
                                                let ratio = (value / 100.0).clamp(0.0, 1.0);
                                                let bar_height = bounds.size.height * ratio;
                                                let origin = gpui::point(
                                                    bounds.origin.x + band_width * index as f32,
                                                    bounds.origin.y + bounds.size.height
                                                        - bar_height,
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
                            }),
                    ),
            )
            .child(
                // Bottom row: transport, seek and volume on one line, so the seek bar sits on the
                // buttons' baseline no matter what the row above is doing.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(px(PLAYBACK_ROW_HEIGHT))
                    .child(
                        div()
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(icon_button(
                                "previous",
                                "icons/previous.svg",
                                fg,
                                palette.wash,
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
                                    .hover(move |style| style.bg(palette.wash))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.toggle_playback(cx)
                                    }))
                                    .child(
                                        svg()
                                            .path(play_icon)
                                            .w(px(20.0))
                                            .h(px(20.0))
                                            .text_color(fg),
                                    ),
                            )
                            .child(icon_button(
                                "next",
                                "icons/next.svg",
                                fg,
                                palette.wash,
                                cx,
                                |this, cx| this.play_next(cx),
                            ))
                            .child(icon_button(
                                "shuffle",
                                "icons/shuffle.svg",
                                shuffle_color,
                                palette.wash,
                                cx,
                                |this, cx| this.toggle_shuffle(cx),
                            ))
                            .child(icon_button(
                                "repeat",
                                repeat_icon,
                                repeat_color,
                                palette.wash,
                                cx,
                                |this, cx| this.cycle_repeat(cx),
                            ))
                            .child(icon_button(
                                "lyrics",
                                "icons/mic.svg",
                                lyrics_color,
                                palette.wash,
                                cx,
                                |this, cx| this.toggle_lyrics(cx),
                            )),
                    )
                    .child(
                        div()
                            .flex_grow(1.0)
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
                                // Progress track: the canvas overlay records the track's bounds
                                // each paint, and a click anywhere in the (taller) hit area seeks
                                // to that fraction.
                                div()
                                    .id("seek-bar")
                                    .flex_grow(1.0)
                                    .py_2()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                                            this.begin_scrub(Scrub::Seek, event.position.x, cx);
                                        }),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h(px(6.0))
                                            .rounded_full()
                                            .bg(palette.border)
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
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_1()
                            .child(icon_button(
                                "queue",
                                "icons/playlist.svg",
                                queue_color,
                                palette.wash,
                                cx,
                                |this, cx| this.toggle_queue(cx),
                            ))
                            .child(icon_button(
                                "devices",
                                "icons/computer.svg",
                                muted,
                                palette.wash,
                                cx,
                                |this, cx| this.open_devices(cx),
                            ))
                            .child(icon_button("mute", mute_icon, muted, palette.wash, cx, |this, cx| {
                                this.toggle_mute(cx)
                            }))
                            .child(
                                div()
                                    .id("volume-bar")
                                    .flex_none()
                                    .w(px(90.0))
                                    .py_2()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                                            this.begin_scrub(Scrub::Volume, event.position.x, cx);
                                        }),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h(px(6.0))
                                            .rounded_full()
                                            .bg(palette.border)
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
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(34.0))
                                    .text_xs()
                                    .text_color(fg)
                                    .child(volume_label),
                            ),
                    ),
            )
    }
}

/// The playback bar's two rows share this three-slot geometry — fixed side columns around a
/// growing center — so the seek bar lines up with the transport and volume controls below it.
const PLAYBACK_SIDE_WIDTH: f32 = 240.0;
/// Fixed so that showing or hiding the lyric line and the visualizer can't move the seek bar.
const PLAYBACK_ROW_HEIGHT: f32 = 40.0;

/// A small round icon button for the playback bar. `icon` is an embedded SVG path (see
/// [`assets`]), tinted with `color` like any themed text.
/// The `:sort` argument that produces `sort`, so the picker can mark the active option.
pub(crate) fn sort_arg(sort: echo_core::app::TrackSort) -> &'static str {
    use echo_core::app::TrackSort;
    match sort {
        TrackSort::Original => "original",
        TrackSort::Title => "title",
        TrackSort::Artist => "artist",
        TrackSort::Album => "album",
        TrackSort::Duration => "duration",
        TrackSort::Added => "added",
    }
}

pub(crate) fn icon_button(
    id: &'static str,
    icon: &'static str,
    color: Hsla,
    hover_bg: Hsla,
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
        .hover(move |style| style.bg(hover_bg))
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
        // Visual mode says so on the status line: without a mode indicator a range selection
        // looks like the list has simply highlighted several rows for no reason.
        let status_line = match self.state.ui.mode {
            AppMode::Visual => Some(
                self.state
                    .ui
                    .status_message
                    .clone()
                    .map(|message| format!("-- VISUAL --  {message}"))
                    .unwrap_or_else(|| {
                        format!(
                            "-- VISUAL --  {}",
                            views::tr(&self.state, "desktop.visual_hint")
                        )
                    }),
            ),
            AppMode::Normal => self.state.ui.status_message.clone(),
            _ => None,
        }
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
        // The audio-device failure banner is persistent (no expiry) and separate from the
        // status line, so transient statuses can't hide it. Core clears it on recovery.
        let audio_banner = self.state.ui.audio_output_error.clone().map(|message| {
            let error = self.state.ui.active_theme.error.gpui(WINDOW_FG());
            let banner_bg = DesktopPalette::resolve(&self.state.ui.active_theme).error_wash;
            div()
                .flex_none()
                .px_4()
                .py_1()
                .text_xs()
                .bg(banner_bg)
                .text_color(error)
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
        let sort_menu = self
            .sort_menu_open
            .then(|| views::sort_menu(self, cx).into_any_element());
        let settings_modal = self
            .settings_open
            .then(|| views::settings_modal(self, window, cx).into_any_element());
        let help_modal = self
            .help_open
            .then(|| views::help_modal(self, cx).into_any_element());
        let context_menu = self
            .context_menu
            .is_some()
            .then(|| views::context_menu(self, cx).into_any_element());
        let track_menu = self
            .track_menu
            .is_some()
            .then(|| views::track_context_menu(self, window, cx).into_any_element());
        let playlist_add_modal = self
            .state
            .ui
            .playlist_add_modal_open
            .then(|| views::playlist_add_modal(self, cx).into_any_element());
        let prompt_modal = echo_core::intent::prompt_active(&self.state)
            .then(|| views::prompt_modal(self, cx).into_any_element());
        let frame_palette = crate::theme::DesktopPalette::resolve(&self.state.ui.active_theme);
        // Resolved before the chain so the style closure below borrows this rather than `window`.
        let corners = views::client_corners(window);
        // Windows and macOS hide the native bar (`appears_transparent`) and always want ours.
        // Linux only gets it when the app actually owns the frame: the `Client` decorations
        // requested at open are downgraded to `Server` on an X11 session with no compositor, and
        // there the window manager draws a real titlebar that ours would sit underneath.
        let owns_frame = !cfg!(target_os = "linux") || corners.is_some();
        let titlebar = owns_frame.then(|| views::titlebar(self, window, cx).into_any_element());

        let root = div()
            .key_context(LIST_CONTEXT)
            .track_focus(&self.focus_handle)
            // Scrubs track the pointer at the window level so dragging keeps working when the
            // pointer leaves the bar; release (or a move without the button) ends them.
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _window, cx| {
                if this.sidebar_resizing.is_some() {
                    if event.pressed_button == Some(gpui::MouseButton::Left) {
                        this.update_sidebar_resize(event.position.x, cx);
                    } else {
                        this.finish_sidebar_resize(cx);
                    }
                }
                if this.scrubbing.is_some() {
                    if event.pressed_button == Some(gpui::MouseButton::Left) {
                        this.update_scrub(event.position.x, cx);
                    } else {
                        this.finish_scrub(event.position.x, cx);
                    }
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                    this.finish_sidebar_resize(cx);
                    this.finish_scrub(event.position.x, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &TogglePlayback, _window, cx| {
                this.toggle_playback(cx)
            }))
            .on_action(cx.listener(|this, _: &MoveUp, _window, cx| {
                let count = this.take_count();
                this.move_selection(-count, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDown, _window, cx| {
                let count = this.take_count();
                this.move_selection(count, cx)
            }))
            .on_action(
                cx.listener(|this, _: &PageUp, _window, cx| this.move_selection(-PAGE_ROWS, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _window, cx| this.move_selection(PAGE_ROWS, cx)),
            )
            // Takes precedence over the app-level `Quit` handler registered in `main`, which
            // cannot see the window. Both the ctrl-q binding and the Linux caption button's
            // close arrive here, so every exit remembers the window rectangle the way the
            // platform's own close request already did.
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                this.persist_window_bounds(window);
                cx.quit();
            }))
            // The macOS Window menu's two entries. Handled here rather than app-wide because
            // both need the window; `Hide`/`HideOthers`/`ShowAll` do not, so those stay in
            // `main` alongside the menu itself.
            .on_action(cx.listener(|_, _: &MinimizeWindow, window, _cx| window.minimize_window()))
            .on_action(cx.listener(|_, _: &ZoomWindow, window, _cx| window.zoom_window()))
            .on_action(cx.listener(|this, _: &SelectFirst, _window, cx| this.set_selection(0, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _window, cx| this.select_last(cx)))
            .on_action(cx.listener(|this, _: &Activate, _window, cx| this.activate_selection(cx)))
            .on_action(cx.listener(|this, _: &ConfirmPrompt, _window, cx| this.confirm_prompt(cx)))
            .on_action(cx.listener(|this, _: &AddToPlaylist, _window, cx| {
                this.add_to_playlist(cx)
            }))
            .on_action(cx.listener(|this, _: &OpenActionMenu, _window, cx| {
                this.open_action_menu(cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleSettings, _window, cx| {
                this.toggle_settings(cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleHelp, _window, cx| {
                this.help_open = !this.help_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MarkDelete, _window, cx| {
                this.pending_count = None;
                if this.in_visual() {
                    echo_core::intent::delete_visual_selection(&mut this.state);
                } else {
                    echo_core::intent::mark_selected_for_delete(&mut this.state);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EnterVisual, _window, cx| this.toggle_visual(cx)))
            .on_action(cx.listener(|this, _: &MoveTrackUp, _window, cx| {
                let count = this.take_count();
                this.move_selected_track(-count, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveTrackDown, _window, cx| {
                let count = this.take_count();
                this.move_selected_track(count, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleLike, _window, cx| this.toggle_like(cx)))
            .on_action(cx.listener(|this, _: &FocusLibrary, _window, cx| this.focus_library(cx)))
            .on_action(cx.listener(|this, _: &FocusTracks, _window, cx| this.focus_tracks(cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &HistoryBack, _window, cx| {
                // Global binding, so keep the view still while an overlay is on top.
                if !this.overlay_open() {
                    this.history_back(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryForward, _window, cx| {
                if !this.overlay_open() {
                    this.history_forward(cx);
                }
            }))
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
            // `0` extends a pending count (`10j`); bare `0` keeps its seek-to-start role.
            .on_action(cx.listener(|this, _: &SeekStart, _window, cx| {
                if let Some(count) = this.pending_count {
                    this.pending_count = Some(count.saturating_mul(10).min(9999));
                    cx.notify();
                } else {
                    this.seek_start(cx)
                }
            }))
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
            .on_action(cx.listener(|this, _: &BackOrFocusLibrary, _window, cx| {
                this.back_or_focus_library(cx)
            }))
            // Digits are unbound, so they fall through the key bindings to this listener and
            // accumulate a vim-style count for j/k. Only while the list itself has focus —
            // digits typed into the search/command inputs must not count.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if !this.focus_handle.is_focused(window) {
                    return;
                }
                let keystroke = &event.keystroke;
                if keystroke.modifiers.control
                    || keystroke.modifiers.alt
                    || keystroke.modifiers.platform
                    || keystroke.modifiers.shift
                {
                    return;
                }
                match keystroke.key.as_str() {
                    digit @ ("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
                        let digit = (digit.as_bytes()[0] - b'0') as usize;
                        let current = this.pending_count.unwrap_or(0);
                        this.pending_count =
                            Some(current.saturating_mul(10).saturating_add(digit).min(9999));
                        cx.notify();
                    }
                    // Any other unbound key cancels the pending count.
                    _ => {
                        if this.pending_count.take().is_some() {
                            cx.notify();
                        }
                    }
                }
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            // The window's backdrop: the one fill covering every corner, so it rounds all four.
            .map(|el| views::round_client_corners(el, corners, views::ClientCorners::All))
            .when_some(titlebar, |el, bar| el.child(bar))
            .child(
                div()
                    .flex_grow(1.0)
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .when(!self.sidebar_collapsed, |el| {
                        el.child(views::sidebar(self, cx))
                    })
                    .child(views::main_area(self, window, cx)),
            )
            .when_some(status_line, |el, line| el.child(line))
            .when_some(command_bar, |el, bar| el.child(bar))
            .when_some(audio_banner, |el, banner| el.child(banner))
            .child(self.render_playback_bar(cx))
            .when_some(lyrics_modal, |el, modal| el.child(modal))
            .when_some(theme_modal, |el, modal| el.child(modal))
            .when_some(device_modal, |el, modal| el.child(modal))
            .when_some(playlist_add_modal, |el, modal| el.child(modal))
            .when_some(sort_menu, |el, menu| el.child(menu))
            .when_some(settings_modal, |el, modal| el.child(modal))
            .when_some(help_modal, |el, modal| el.child(modal))
            .when_some(context_menu, |el, menu| el.child(menu))
            .when_some(track_menu, |el, menu| el.child(menu))
            .when_some(prompt_modal, |el, modal| el.child(modal));

        // Adds the border and resize edges when the compositor refuses to draw them; a
        // pass-through when it does.
        views::window_frame(root, window, frame_palette)
    }
}

pub(crate) fn format_time(ms: u32) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn main() {
    // Neither launch path has a stderr anyone reads — the Linux .desktop entry sets
    // Terminal=false and the Windows build is a windows_subsystem app — so a panic before the
    // window opens is indistinguishable from the icon doing nothing at all. First thing in
    // main, so it covers the runtime and bootstrap below as well as the UI.
    std::panic::set_hook(Box::new(|info| {
        let path = echo_core::config::debug_log_path("echo-desktop-panic.log");
        let backtrace = std::backtrace::Backtrace::force_capture();
        let _ = std::fs::write(&path, format!("{info}\n\n{backtrace}\n"));
        eprintln!("{info}");
    }));

    // The worker lives on this runtime; entering it makes bootstrap::init()'s tokio::spawn work.
    // It must outlive the UI, which `run()` blocks for.
    let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
    let _guard = runtime.enter();

    // Windows cannot delete the image of a running process, so a previous upgrade leaves its
    // backups behind. Nothing holds them now.
    echo_core::update::sweep_backups();

    echo_core::i18n::init();
    let boot = echo_core::bootstrap::init();
    let saved_bounds = boot.config.library.window_bounds;

    application().with_assets(assets::Assets).run(move |cx: &mut App| {
        // Must match the MSI shortcuts' System.AppUserModel.ID so every launch path
        // (Start Menu, desktop shortcut, raw exe) groups onto the same pinned button.
        cx.set_app_identity("com.echo.app", "echo");
        cx.on_action(|_: &Quit, cx| cx.quit());
        // The keymap mirrors the TUI's, so it is ctrl-based everywhere. These are the chords a
        // Mac user reaches for without thinking, and they are what the menu bar below shows as
        // its accelerators — gpui reads them back out of the keymap when it builds the menu.
        if cfg!(target_os = "macos") {
            cx.bind_keys([
                KeyBinding::new("cmd-q", Quit, None),
                KeyBinding::new("cmd-h", Hide, None),
                KeyBinding::new("cmd-m", MinimizeWindow, None),
            ]);
            cx.on_action(|_: &Hide, cx| cx.hide());
            cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
            cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
            cx.set_menus(mac_menus());
        }
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
            // The TUI answers confirm prompts with `y`; Enter reaches the same handler.
            KeyBinding::new("y", ConfirmPrompt, LIST_KEYS),
            KeyBinding::new("left", FocusLibrary, LIST_KEYS),
            KeyBinding::new("right", FocusTracks, LIST_KEYS),
            // `l` matches the TUI/README: like the focused track (Enter on library rows);
            // `right` keeps pane focus for arrow navigation.
            KeyBinding::new("l", ToggleLike, LIST_KEYS),
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
            // Backspace hops focus back to the sidebar (TUI habit); `h`/escape keep going
            // back through the view history.
            KeyBinding::new("backspace", BackOrFocusLibrary, LIST_KEYS),
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
            KeyBinding::new("a", AddToPlaylist, LIST_KEYS),
            KeyBinding::new("shift-a", OpenActionMenu, LIST_KEYS),
            // `dd`: the first press arms, the second stages the confirm prompt.
            KeyBinding::new("d", MarkDelete, LIST_KEYS),
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
            // The platform-conventional preferences shortcut; global so it also works while
            // the search box has focus.
            KeyBinding::new("ctrl-,", ToggleSettings, None),
            // Global like ctrl-,: chords type nothing, so they're safe while inputs focus.
            KeyBinding::new("ctrl-\\", ToggleSidebar, None),
            KeyBinding::new("alt-left", HistoryBack, None),
            KeyBinding::new("alt-right", HistoryForward, None),
            KeyBinding::new("?", ToggleHelp, LIST_KEYS),
            KeyBinding::new("v", EnterVisual, LIST_KEYS),
            KeyBinding::new("shift-k", MoveTrackUp, LIST_KEYS),
            KeyBinding::new("shift-j", MoveTrackDown, LIST_KEYS),
        ]);
        cx.on_window_closed(|cx, _window_id| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        // Restore the saved rectangle, but only if it still lands on a display — a window saved
        // on a monitor that is no longer attached would otherwise open off-screen with no way
        // to drag it back.
        let bounds = saved_bounds
            .map(|saved| Bounds {
                origin: gpui::point(px(saved.x), px(saved.y)),
                size: size(px(saved.width), px(saved.height)),
            })
            .filter(|bounds| {
                cx.displays()
                    .iter()
                    .any(|display| display.bounds().intersects(bounds))
            })
            .unwrap_or_else(|| Bounds::centered(None, size(px(1100.0), px(720.0)), cx));
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // The app draws its own themed titlebar (views::titlebar) on every platform;
                // the native one is hidden on Windows/macOS.
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("echo".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
                }),
                // Asked for explicitly on Linux so the look is the same under every window
                // manager. Leaving it to the platform gives a native bar on the compositors
                // that implement xdg-decoration and none at all on the ones that don't
                // (GNOME/Mutter) — a second titlebar on some machines, zero on others.
                // Client means the app owns moving and resizing: see views::window_frame.
                window_decorations: cfg!(target_os = "linux")
                    .then_some(gpui::WindowDecorations::Client),
                // How a Linux desktop finds the window's identity: the compositor matches this
                // against a .desktop file's basename to get the icon and the visible app name.
                // It must stay equal to install.sh's `echo.desktop` (and to its StartupWMClass,
                // which is the X11 half of the same match). Without it the window has no app_id
                // at all, so the shell falls back to a blank icon and "Unknown" — the hicolor
                // icons install.sh lays down only ever reach the app grid, never the window.
                // `set_app_identity` above does not cover this; on Linux it only names
                // notifications.
                app_id: Some("echo".to_string()),
                // macOS: the app moves the window itself via start_window_move, which also
                // avoids AppKit's titlebar-click delay. No-op elsewhere.
                app_owns_titlebar_drag: true,
                window_min_size: Some(size(px(480.0), px(360.0))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| EchoApp::new(boot, window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}

#[cfg(test)]
mod text_edit_tests {
    use super::{insert_at_cursor, next_char_boundary, prev_char_boundary, text_with_cursor};

    #[test]
    fn boundaries_step_over_multibyte_chars() {
        // "é" is two bytes, "中" is three: navigation must land on char boundaries, not bytes.
        let value = "aé中b"; // bytes: a(1) é(2) 中(3) b(1)
        assert_eq!(next_char_boundary(value, 0), 1); // past 'a'
        assert_eq!(next_char_boundary(value, 1), 3); // past 'é'
        assert_eq!(next_char_boundary(value, 3), 6); // past '中'
        assert_eq!(next_char_boundary(value, 6), 7); // past 'b'
        assert_eq!(next_char_boundary(value, 7), 7); // clamped at end

        assert_eq!(prev_char_boundary(value, 7), 6);
        assert_eq!(prev_char_boundary(value, 6), 3);
        assert_eq!(prev_char_boundary(value, 3), 1);
        assert_eq!(prev_char_boundary(value, 1), 0);
        assert_eq!(prev_char_boundary(value, 0), 0); // clamped at start
    }

    #[test]
    fn insert_advances_cursor_past_inserted_text() {
        let mut value = "cd".to_string();
        let mut cursor = 0;
        insert_at_cursor(&mut value, &mut cursor, "ab");
        assert_eq!(value, "abcd");
        assert_eq!(cursor, 2);

        // Insert a multi-byte char at the caret; the cursor moves by its byte length.
        insert_at_cursor(&mut value, &mut cursor, "é");
        assert_eq!(value, "abécd");
        assert_eq!(cursor, 4);
    }

    #[test]
    fn caret_renders_at_cursor_and_survives_stale_index() {
        assert_eq!(text_with_cursor("abc", 0), "▏abc");
        assert_eq!(text_with_cursor("abc", 1), "a▏bc");
        assert_eq!(text_with_cursor("abc", 3), "abc▏");
        // A byte index past the end is clamped rather than panicking.
        assert_eq!(text_with_cursor("abc", 9), "abc▏");
        // An index landing mid-codepoint snaps back to a boundary.
        assert_eq!(text_with_cursor("é", 1), "▏é");
    }
}
