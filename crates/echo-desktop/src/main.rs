//! echo — the GPUI desktop frontend.
//!
//! [`echo_core::bootstrap::init`] spawns the worker on a tokio runtime and hands back the two
//! event channels; the app applies worker events to [`AppState`](echo_core::app::AppState) and
//! draws from it. GPUI's reactive model awaits worker events in a spawned task and calls
//! `cx.notify()`.
//!
//! The tokio runtime lives on the main function's stack and stays entered for the lifetime of
//! the UI, so worker tasks keep running on its threads while GPUI blocks in `run()`.

#![windows_subsystem = "windows"]

mod app;
mod assets;
mod backdrop;
mod dock_icon;
mod images;
mod playback_bar;
mod playlist_ui;
mod regions;
mod single_instance;
mod text_input;
mod theme;
mod tray;
mod views;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use echo_core::app::{ActiveView, AppMode, LibraryTab, SearchTab};
use echo_core::apply_worker_event::apply_worker_event;
use echo_core::config::RightPanel;
use echo_core::events::AppEvent;
use gpui::{
    App, Bounds, Context, Div, Entity, FocusHandle, Hsla, KeyBinding, Pixels, QuitMode,
    ScrollHandle, ScrollStrategy, SharedString, UniformListScrollHandle, Window, WindowBounds,
    WindowOptions, actions, canvas, div, prelude::*, px, size, svg,
};
use gpui_platform::application;
pub(crate) use playback_bar::*;
pub(crate) use text_input::*;
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
        OpenHome,
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
        ToggleImmersive,
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
        ZoomWindow,
        CloseWindow
    ]
);

/// What outlives the window: the one `EchoApp` entity, which every opened window renders, and
/// the tray icon standing in for it while no window is up.
struct Shell {
    instance_task: Option<tokio::task::JoinHandle<()>>,
    app: Entity<EchoApp>,
    tray_tx: tokio::sync::mpsc::UnboundedSender<tray::TrayEvent>,
    tray: Option<tray::Tray>,
}

impl gpui::Global for Shell {}

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
struct PanelResize {
    start_x: Pixels,
    start_width: f32,
    right: bool,
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
    EditDetails,
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
    pub(crate) search_all_scroll: ScrollHandle,
    pub(crate) search_section_scrolls: std::collections::HashMap<SearchTab, ScrollHandle>,
    pub(crate) search_all_positions: Vec<(usize, Option<(SearchTab, usize)>)>,
    pub(crate) artist_albums_scroll: UniformListScrollHandle,
    pub(crate) artist_top_tracks_scroll: UniformListScrollHandle,
    pub(crate) artist_list_scroll: UniformListScrollHandle,
    pub(crate) whats_new_scroll: UniformListScrollHandle,
    pub(crate) home_scroll: ScrollHandle,
    pub(crate) home_shelf_scrolls:
        std::collections::HashMap<echo_core::home::HomeShelfKind, ScrollHandle>,
    pub(crate) home_section_indices:
        std::collections::HashMap<echo_core::home::HomeShelfKind, usize>,
    pub(crate) lyrics_scroll: UniformListScrollHandle,
    pub(crate) queue_panel_scroll: UniformListScrollHandle,
    /// The docked queue or lyrics column beside the main area; mirrored into the core's
    /// `queue_panel_open` so the queue keeps refreshing, and saved with the library config.
    pub(crate) right_panel: Option<RightPanel>,
    /// The list a drag is scrolling and its pixels per frame; `None` while the pointer is away
    /// from a list edge. The ticking task ends itself when this clears or the drag ends.
    pub(crate) drag_scroll: Option<(DragList, f32)>,
    pub(crate) drag_scroll_ticking: bool,
    playing_bars: Cell<PlayingBars>,
    regions: regions::Regions,
    _regions_follow_app: gpui::Subscription,
    pub(crate) playlist_modal_scroll: ScrollHandle,
    pub(crate) device_modal_scroll: ScrollHandle,
    pub(crate) theme_modal_scroll: ScrollHandle,
    pub(crate) sidebar_width: f32,
    pub(crate) right_panel_width: f32,
    pub(crate) sidebar_collapsed: bool,
    pub(crate) theme_modal_open: bool,
    pub(crate) immersive: bool,
    pub(crate) backdrops: backdrop::BackdropCache,
    pub(crate) theme_modal_index: usize,
    /// Which appearance's themes the picker lists. Pinned modes list their own; under
    /// `Auto` either can be shown, to choose a theme for each.
    pub(crate) theme_modal_tab: echo_core::config::Scheme,
    /// Keeps the system-appearance observer of the current window alive; replaced when the
    /// window reopens from the tray.
    _appearance: Option<gpui::Subscription>,
    pub(crate) sort_menu_open: bool,
    pub(crate) page_menu: Option<(gpui::Point<Pixels>, usize)>,
    pub(crate) sort_menu_top: f32,
    pub(crate) sort_menu_index: usize,
    pub(crate) settings_open: bool,
    /// The settings dropdown whose menu is showing, by its control id.
    pub(crate) settings_dropdown: Option<&'static str>,
    pub(crate) settings_path_input: String,
    pub(crate) settings_path_cursor: usize,
    pub(crate) settings_path_focus: FocusHandle,
    pub(crate) settings_scroll: ScrollHandle,
    pub(crate) update_state: UpdateState,
    started_at: std::time::Instant,
    last_auto_check: Option<std::time::Instant>,
    auto_update_pending: bool,
    auto_update_installed: bool,
    update_banner_dismissed: bool,
    restarting: bool,
    launch_executable: Option<std::path::PathBuf>,
    dock_icon: dock_icon::DockIcon,
    pub(crate) help_open: bool,
    pub(crate) help_scroll: ScrollHandle,
    pub(crate) context_menu: Option<ContextMenuState>,
    pub(crate) track_menu: Option<TrackMenuState>,
    pub(crate) submenu_scroll: ScrollHandle,
    pub(crate) track_menu_scroll: ScrollHandle,
    pub(crate) submenu_row_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) submenu_bounds: Rc<Cell<Bounds<Pixels>>>,
    submenu_apex: gpui::Point<Pixels>,
    pending_count: Option<usize>,
    seek_bounds: Rc<Cell<Bounds<Pixels>>>,
    volume_bounds: Rc<Cell<Bounds<Pixels>>>,
    scrubbing: Option<Scrub>,
    /// The Home shelf whose scrollbar is being dragged; tracked at the window level like a
    /// scrub so the drag survives the pointer leaving the bar.
    shelf_scrub: Option<gpui::ScrollHandle>,
    panel_resizing: Option<PanelResize>,
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

/// The animation of the playing row's bars. A render that draws them moves this to `Drawn`
/// and each tick of the animation's task moves it one step back, so the task ends itself one
/// tick after the row leaves the screen.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlayingBars {
    /// Not on screen; no task is running.
    Hidden,
    /// Drawn since the last tick.
    Drawn,
    /// Not drawn since the last tick; the next one ends the task.
    Stale,
}

/// The lists a drag can scroll by hovering near their edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragList {
    Tracks,
    Library,
}

/// Picks a fixed-pitch family that is actually installed. gpui matches families by exact name
/// against the system font database — there is no generic "monospace" alias and no substitution
/// when the name misses, so naming one font hard-codes a platform: "Consolas" is Windows-only,
/// and on Linux it silently fell through to the proportional UI font, which shears the ECHO
/// wordmark's box-drawing rows out of alignment. Ordered platform-native first, then the
/// families a desktop Linux install is likely to have; all of these cover U+2500–U+259F.
fn resolve_mono_font(cx: &App) -> SharedString {
    const CANDIDATES: &[&str] = &[
        "Consolas",         // Windows
        "Menlo",            // macOS
        "DejaVu Sans Mono", // Debian/Ubuntu default
        "Liberation Mono",  // Fedora/RHEL default
        "Noto Sans Mono",   // GNOME
        "Ubuntu Mono",      // Ubuntu
        "JetBrains Mono",   // common developer install
        "Source Code Pro",  //
        "Courier New",      //
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
    fn new(boot: echo_core::bootstrap::Bootstrap, cx: &mut Context<Self>) -> Self {
        let echo_core::bootstrap::Bootstrap {
            mut state,
            config: _,
            app_tx,
            app_rx,
            worker_tx,
        } = boot;
        if state.ui.mode == AppMode::Normal {
            for event in echo_core::intent::open_home(&mut state) {
                let _ = app_tx.send(event);
            }
        }

        Self::spawn_worker_bridge(app_rx, cx);
        Self::spawn_periodic_tick(cx);

        let focus_handle = cx.focus_handle();

        let sidebar_width = state
            .ui
            .library_config
            .sidebar_width
            .unwrap_or(views::SIDEBAR_WIDTH);
        let sidebar_collapsed = state.ui.library_config.sidebar_collapsed.unwrap_or(false);
        let right_panel_width = views::panel_layout::preferred_width(
            state
                .ui
                .library_config
                .right_panel_width
                .unwrap_or(views::panel_layout::RIGHT_PANEL_DEFAULT),
        );
        let right_panel = state.ui.library_config.right_panel;
        state.ui.queue_panel_open = right_panel == Some(RightPanel::Queue);
        if state.ui.queue_panel_open {
            let _ = app_tx.send(AppEvent::FetchQueue);
        }

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
            search_all_scroll: ScrollHandle::new(),
            search_section_scrolls: std::collections::HashMap::new(),
            search_all_positions: Vec::new(),
            artist_albums_scroll: UniformListScrollHandle::new(),
            artist_top_tracks_scroll: UniformListScrollHandle::new(),
            artist_list_scroll: UniformListScrollHandle::new(),
            whats_new_scroll: UniformListScrollHandle::new(),
            home_scroll: ScrollHandle::new(),
            home_shelf_scrolls: std::collections::HashMap::new(),
            home_section_indices: std::collections::HashMap::new(),
            lyrics_scroll: UniformListScrollHandle::new(),
            queue_panel_scroll: UniformListScrollHandle::new(),
            right_panel,
            drag_scroll: None,
            drag_scroll_ticking: false,
            playing_bars: Cell::new(PlayingBars::Hidden),
            regions: regions::Regions::new(cx),
            _regions_follow_app: cx.observe_self(|app, cx| app.regions.notify_all(cx)),
            playlist_modal_scroll: ScrollHandle::new(),
            device_modal_scroll: ScrollHandle::new(),
            theme_modal_scroll: ScrollHandle::new(),
            sidebar_width: sidebar_width.clamp(180.0, 480.0),
            right_panel_width,
            sidebar_collapsed,
            theme_modal_open: false,
            immersive: false,
            backdrops: backdrop::BackdropCache::default(),
            theme_modal_index: 0,
            theme_modal_tab: echo_core::config::Scheme::Dark,
            _appearance: None,
            sort_menu_open: false,
            page_menu: None,
            sort_menu_top: 96.0,
            sort_menu_index: 0,
            settings_open: false,
            settings_dropdown: None,
            settings_path_input: String::new(),
            settings_path_cursor: 0,
            settings_path_focus: cx.focus_handle(),
            settings_scroll: ScrollHandle::new(),
            update_state: UpdateState::Idle,
            started_at: std::time::Instant::now(),
            last_auto_check: None,
            auto_update_pending: false,
            auto_update_installed: false,
            update_banner_dismissed: false,
            restarting: false,
            launch_executable: std::env::current_exe().ok(),
            dock_icon: Default::default(),
            help_open: false,
            help_scroll: ScrollHandle::new(),
            context_menu: None,
            track_menu: None,
            submenu_scroll: ScrollHandle::new(),
            track_menu_scroll: ScrollHandle::new(),
            submenu_row_bounds: Rc::default(),
            submenu_bounds: Rc::default(),
            submenu_apex: gpui::Point::default(),
            pending_count: None,
            seek_bounds: Rc::default(),
            volume_bounds: Rc::default(),
            scrubbing: None,
            shelf_scrub: None,
            panel_resizing: None,
            titlebar_should_move: false,
            mono_font: resolve_mono_font(cx),
            focus_handle,
            tokio: tokio::runtime::Handle::current(),
        }
    }

    /// Bridge: worker events land here and become repaints. tokio's mpsc futures don't need
    /// the tokio reactor, so awaiting on GPUI's foreground executor is fine.
    fn spawn_worker_bridge(
        mut app_rx: tokio::sync::mpsc::Receiver<echo_core::events::WorkerEvent>,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            while let Some(event) = app_rx.recv().await {
                let applied = this.update(cx, |app: &mut EchoApp, cx| {
                    app.on_worker_event(event, cx);
                });
                if applied.is_err() {
                    break; // entity dropped — app is shutting down
                }
            }
        })
        .detach();
    }

    fn on_worker_event(&mut self, event: echo_core::events::WorkerEvent, cx: &mut Context<Self>) {
        let signed_in = matches!(
            &event,
            echo_core::events::WorkerEvent::AuthenticationComplete
        );
        // Arrives ten times a second during playback and only moves the position.
        let position_only = matches!(&event, echo_core::events::WorkerEvent::Tick);
        apply_worker_event(event, &mut self.state, &self.app_tx, &self.worker_tx);
        if signed_in {
            self.open_home(cx);
        }
        // A tick that reaches the end of the track stops playback, which every
        // pane shows.
        if position_only && self.state.playback.is_playing {
            self.notify_playback_position(cx);
        } else {
            cx.notify();
        }
    }

    /// Progress tick: elapsed time is interpolated at render time, so while playing the bar
    /// needs a repaint even when no worker event arrives. The visualizer needs a much faster
    /// cadence, so the interval tightens while it is live; the immersive backdrop's drift
    /// sets its own, slower one.
    fn spawn_periodic_tick(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                let interval = this.update(cx, |app: &mut EchoApp, cx| app.periodic_tick(cx));
                let Ok(interval) = interval else {
                    break; // entity dropped — app is shutting down
                };
                cx.background_executor().timer(interval).await;
            }
        })
        .detach();
    }

    fn periodic_tick(&mut self, cx: &mut Context<Self>) -> Duration {
        self.maybe_auto_update(cx);
        let animating = self.immersive && !cx.reduce_motion();
        if animating {
            cx.notify();
        } else if self.state.playback.is_playing {
            self.notify_playback_position(cx);
        }
        // Status messages carry an expiry; the periodic tick retires them.
        if self
            .state
            .ui
            .status_message_expiry
            .is_some_and(|expiry| expiry <= std::time::Instant::now())
        {
            self.state.ui.status_message = None;
            self.state.ui.status_message_expiry = None;
            cx.notify();
        }
        let visualizing = self.state.playback.is_playing
            && self.state.ui.library_config.enable_visualizer
            && self.state.playback.audio_visualization.is_some();
        if visualizing {
            Duration::from_millis(66)
        } else if animating {
            backdrop::FRAME
        } else {
            Duration::from_millis(500)
        }
    }

    /// Binds the entity to a freshly opened window: keyboard focus, the close hook and the
    /// titlebar drag latch. Runs at launch and every time the window reopens from the tray.
    fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.titlebar_should_move = false;
        window.focus(&self.focus_handle, cx);
        // Read here, before the window's first frame, so `Auto` never flashes the wrong
        // theme; the observer then follows the system while this window is up. A window
        // reopened from the tray re-reads it, covering a switch made while it was closed.
        self.sync_system_appearance(window.appearance(), cx);
        self._appearance = Some(cx.observe_window_appearance(window, |this, window, cx| {
            this.sync_system_appearance(window.appearance(), cx)
        }));
        let this = cx.entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |this: &mut EchoApp, cx| {
                this.on_close_request(window, cx)
            })
        });
    }

    /// The one close decision, for the platform's close request and the Linux caption button
    /// alike. The rectangle is saved first because `remove_window` skips the close hook. On
    /// macOS the window just closes and the Dock keeps the app; elsewhere the tray icon takes
    /// over when the setting is on and a tray host exists, and the app quits otherwise. Always
    /// `true`: the window closes either way, the tray comes up a moment after.
    fn on_close_request(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.persist_window_bounds(window);
        if cfg!(target_os = "macos") {
            return true;
        }
        if !self.state.ui.library_config.close_to_tray {
            cx.quit();
            return true;
        }
        let lang = &self.state.ui.library_config.language;
        let labels = tray::TrayLabels {
            show: echo_core::i18n::t("desktop.tray.show", lang),
            quit: echo_core::i18n::t("desktop.help.quit", lang),
        };
        let tx = cx.global::<Shell>().tray_tx.clone();
        cx.spawn(async move |_, cx| {
            let tray = tray::Tray::create(tx, labels).await;
            cx.update(|cx| match tray {
                Ok(tray) => cx.global_mut::<Shell>().tray = Some(tray),
                Err(error) => {
                    eprintln!("tray unavailable, quitting instead: {error:#}");
                    cx.quit();
                }
            });
        })
        .detach();
        true
    }

    /// Remembers the window rectangle so the next launch reopens the same size and place. All
    /// three `WindowBounds` variants carry the restore bounds, so a window closed while
    /// maximized still remembers a sensible windowed size. Called from every way out:
    /// `on_close_request` for the platform's close request and the Linux caption button, and
    /// the `Quit` action the keybinding dispatches. The playback session is saved alongside.
    fn persist_window_bounds(&mut self, window: &Window) {
        echo_core::session::persist(&self.state);
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

    // The add-to-playlist flyout. The track menu's "Add to playlist" row opens a second panel
    // beside it rather than the full-screen picker `a` uses, so the choice stays in one gesture.

    // Vim-style command mode (`:`) and track filter (`/`) — a bar above the playback bar that
    // owns key handling while one of the modes is active. The command registry itself lives in
    // `echo_core::commands`.

    // Transport, all via the shared intents (optimistic flip + worker event).

    // Drag-to-scrub: mouse-down on a bar starts it, window-level mouse-move updates the state
    // optimistically for live feedback, and release fires the worker event once.

    // Both docks track drags at window level and persist only on release. The right dock's
    // left edge moves in the opposite direction to the library's right edge.

    /// Sends an intent-produced event to the worker with its required side channels: a
    /// LoadContextTracks with cover art also kicks off the header image fetch, and
    /// ReloadHeaderImage is handled entirely here (the worker has no handler for it).
    pub(crate) fn dispatch(&mut self, event: AppEvent) {
        if let AppEvent::GlobalSearch(query) = &event {
            self.state.ui.active_search_tab = SearchTab::All;
            self.state.ui.selected_search_index = 0;
            self.search_input = query.clone();
            self.search_cursor = query.len();
            self.search_all_scroll
                .set_offset(gpui::point(px(0.0), px(0.0)));
        }
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
}

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
        TrackSort::AddedBy => "addedby",
    }
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

    // A second launch only asks the running echo to show itself, which also brings a window
    // back from the tray.
    let instance = match single_instance::claim(&echo_core::config::echo_config_root()) {
        single_instance::Instance::Secondary => return,
        single_instance::Instance::Primary(listener) => listener,
    };

    // Windows cannot delete the image of a running process, so a previous upgrade leaves its
    // backups behind. Nothing holds them now.
    echo_core::update::sweep_backups();
    echo_core::update::remove_retired();

    echo_core::i18n::init();
    let boot = echo_core::bootstrap::init();

    // Explicit: closing the last window must not end the process, which is what lets the
    // window close into the tray (and the app stay in the Dock on macOS). Every exit is a
    // `cx.quit()`. `on_reopen` is the macOS Dock click with no window up; a no-op elsewhere.
    let app = application()
        .with_assets(assets::Assets)
        .with_quit_mode(QuitMode::Explicit);
    app.on_reopen(open_main_window);
    app.run(move |cx: &mut App| {
        // Must match the MSI shortcuts' System.AppUserModel.ID so every launch path
        // (Start Menu, desktop shortcut, raw exe) groups onto the same pinned button.
        cx.set_app_identity("com.echo.app", "echo");
        cx.on_action(|_: &Quit, cx| cx.quit());
        // The keymap is ctrl-based everywhere. These are the chords a Mac user reaches for
        // without thinking, and they are what the menu bar below shows as its accelerators —
        // gpui reads them back out of the keymap when it builds the menu.
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
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "ctrl-shift-h"
                } else {
                    "ctrl-h"
                },
                OpenHome,
                None,
            ),
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
            // `z` is an Enter alias.
            KeyBinding::new("z", Activate, LIST_KEYS),
            // `y` answers confirm prompts; Enter reaches the same handler.
            KeyBinding::new("y", ConfirmPrompt, LIST_KEYS),
            KeyBinding::new("left", FocusLibrary, LIST_KEYS),
            KeyBinding::new("right", FocusTracks, LIST_KEYS),
            // `l` likes the focused track (or acts as Enter on library rows);
            // `right` keeps pane focus for arrow navigation.
            KeyBinding::new("l", ToggleLike, LIST_KEYS),
            // Vim-style navigation motions.
            KeyBinding::new("g g", SelectFirst, LIST_KEYS),
            KeyBinding::new("shift-g", SelectLast, LIST_KEYS),
            KeyBinding::new("g c", JumpToCurrent, LIST_KEYS),
            KeyBinding::new("ctrl-u", HalfPageUp, LIST_KEYS),
            KeyBinding::new("ctrl-d", HalfPageDown, LIST_KEYS),
            KeyBinding::new("ctrl-b", PageUp, LIST_KEYS),
            KeyBinding::new("ctrl-f", PageDown, LIST_KEYS),
            // `h` is "back", not "focus sidebar" — the left arrow keeps that role.
            KeyBinding::new("h", Dismiss, LIST_KEYS),
            // Backspace hops focus back to the sidebar; `h`/escape keep going through history.
            KeyBinding::new("backspace", BackOrFocusLibrary, LIST_KEYS),
            KeyBinding::new("escape", Dismiss, LIST_KEYS),
            KeyBinding::new("tab", CycleTab, LIST_KEYS),
            // Playback transport.
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
            // `/` filters the loaded track list; the global search box gets ctrl-k (and stays
            // clickable).
            KeyBinding::new("/", OpenFilter, LIST_KEYS),
            KeyBinding::new("n", NextMatch, LIST_KEYS),
            KeyBinding::new("shift-n", PrevMatch, LIST_KEYS),
            KeyBinding::new("ctrl-k", FocusSearch, LIST_KEYS),
            KeyBinding::new("shift-l", ToggleLyrics, LIST_KEYS),
            KeyBinding::new("ctrl-shift-l", ToggleInlineLyrics, LIST_KEYS),
            KeyBinding::new("t", ToggleThemes, LIST_KEYS),
            KeyBinding::new("shift-f", ToggleImmersive, LIST_KEYS),
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
        let (tray_tx, mut tray_rx) = tokio::sync::mpsc::unbounded_channel();
        let instance_task = instance.map(|listener| listener.serve(tray_tx.clone()));
        let app = cx.new(|cx| EchoApp::new(boot, cx));
        cx.set_global(Shell {
            instance_task,
            app,
            tray_tx,
            tray: None,
        });
        cx.spawn(async move |cx| {
            while let Some(event) = tray_rx.recv().await {
                cx.update(|cx| match event {
                    tray::TrayEvent::Show => open_main_window(cx),
                    tray::TrayEvent::Quit => cx.quit(),
                });
            }
        })
        .detach();
        open_main_window(cx);
        cx.activate(true);
    });
}

/// Opens the one main window around the entity `Shell` keeps, at the rectangle saved on the
/// last close if it still lands on a display (a window saved on a monitor that is no longer
/// attached would otherwise open off-screen with no way to drag it back). Shared by launch,
/// the tray icon's Show, a second launch's knock and the macOS Dock reopen; with a window
/// already up it only brings that one forward. `activate_window` is what foregrounds on
/// Windows, where `cx.activate` is a no-op; `cx.activate` is what does it on macOS.
fn open_main_window(cx: &mut App) {
    if let Some(existing) = cx.windows().into_iter().next() {
        existing
            .update(cx, |_, window, _| window.activate_window())
            .ok();
        cx.activate(true);
        return;
    }
    let app = cx.global::<Shell>().app.clone();
    cx.global_mut::<Shell>().tray = None;
    let bounds = app
        .read(cx)
        .state
        .ui
        .library_config
        .window_bounds
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
    let root = app.clone();
    let window = cx
        .open_window(
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
            move |window, cx| {
                root.update(cx, |this, cx| this.attach(window, cx));
                root
            },
        )
        .expect("failed to open window");
    window
        .update(cx, |_, window, _| window.activate_window())
        .ok();
}
