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

/// Whether a keystroke's modifiers mean "paste" for the hand-rolled text fields.
///
/// `Modifiers::secondary` is cmd on macOS and ctrl elsewhere; a Mac that only matched `control`
/// ignored cmd-v, which made the setup card — the first thing a new install shows, and a place
/// where credentials are pasted rather than typed — impossible to paste into. ctrl-v is still
/// accepted everywhere as an alternative shortcut.
fn is_paste_chord(modifiers: &gpui::Modifiers) -> bool {
    modifiers.secondary() || modifiers.control
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

/// One bounce of the playing row's bars.
const PLAYING_BARS_LOOP: Duration = Duration::from_millis(900);
/// How often the bars repaint. Every frame re-renders the track list, so this stays well
/// under the display's refresh rate.
const PLAYING_BARS_FRAME: Duration = Duration::from_millis(33);

/// The lists a drag can scroll by hovering near their edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DragList {
    Tracks,
    Library,
}

/// How close to a list edge, in pixels, a drag starts scrolling it.
const DRAG_EDGE: f32 = 48.0;
/// Pixels per frame at the very edge.
const DRAG_SCROLL_MAX: f32 = 14.0;

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
            mut app_rx,
            worker_tx,
        } = boot;
        if state.ui.mode == AppMode::Normal {
            for event in echo_core::intent::open_home(&mut state) {
                let _ = app_tx.send(event);
            }
        }

        // Bridge: worker events land here and become repaints. tokio's mpsc futures don't need
        // the tokio reactor, so awaiting on GPUI's foreground executor is fine.
        cx.spawn(async move |this, cx| {
            while let Some(event) = app_rx.recv().await {
                let applied = this.update(cx, |app: &mut EchoApp, cx| {
                    let signed_in = matches!(
                        &event,
                        echo_core::events::WorkerEvent::AuthenticationComplete
                    );
                    // Arrives ten times a second during playback and only moves the position.
                    let position_only = matches!(&event, echo_core::events::WorkerEvent::Tick);
                    apply_worker_event(event, &mut app.state, &app.app_tx, &app.worker_tx);
                    if signed_in {
                        app.open_home(cx);
                    }
                    // A tick that reaches the end of the track stops playback, which every
                    // pane shows.
                    if position_only && app.state.playback.is_playing {
                        app.notify_playback_position(cx);
                    } else {
                        cx.notify();
                    }
                });
                if applied.is_err() {
                    break; // entity dropped — app is shutting down
                }
            }
        })
        .detach();

        // Progress tick: elapsed time is interpolated at render time, so while playing the bar
        // needs a repaint even when no worker event arrives. The visualizer needs a much faster
        // cadence, so the interval tightens while it is live; the immersive backdrop's drift
        // sets its own, slower one.
        cx.spawn(async move |this, cx| {
            loop {
                let interval = this.update(cx, |app: &mut EchoApp, cx| {
                    app.maybe_auto_update(cx);
                    let animating = app.immersive && !cx.reduce_motion();
                    if animating {
                        cx.notify();
                    } else if app.state.playback.is_playing {
                        app.notify_playback_position(cx);
                    }
                    // Status messages carry an expiry; the periodic tick retires them.
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
                    if visualizing {
                        Duration::from_millis(66)
                    } else if animating {
                        backdrop::FRAME
                    } else {
                        Duration::from_millis(500)
                    }
                });
                let Ok(interval) = interval else {
                    break; // entity dropped — app is shutting down
                };
                cx.background_executor().timer(interval).await;
            }
        })
        .detach();

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

    /// Redraws only what shows the playback position or the volume: the playback bar, the
    /// lyrics panel, and the immersive view, which is drawn by the app itself.
    fn notify_playback_position(&mut self, cx: &mut Context<Self>) {
        if self.immersive {
            cx.notify();
            return;
        }
        self.regions.notify(regions::Region::PlaybackBar, cx);
        if self.right_panel == Some(RightPanel::Lyrics) {
            self.regions.notify(regions::Region::RightPanel, cx);
        }
    }

    /// The phase, in `0..1`, to draw the playing row's bars at. Calling this from a render
    /// keeps the track list repainting every [`PLAYING_BARS_FRAME`] for as long as renders keep
    /// calling it.
    pub(crate) fn animate_playing_bars(&self, cx: &mut Context<Self>) -> f32 {
        if self.playing_bars.replace(PlayingBars::Drawn) == PlayingBars::Hidden {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(PLAYING_BARS_FRAME).await;
                    let keep_going = this.update(cx, |app, cx| match app.playing_bars.get() {
                        PlayingBars::Drawn => {
                            app.playing_bars.set(PlayingBars::Stale);
                            app.regions.notify(regions::Region::Main, cx);
                            true
                        }
                        PlayingBars::Stale | PlayingBars::Hidden => {
                            app.playing_bars.set(PlayingBars::Hidden);
                            false
                        }
                    });
                    if !matches!(keep_going, Ok(true)) {
                        break;
                    }
                }
            })
            .detach();
        }
        let elapsed = self.started_at.elapsed().as_secs_f32();
        (elapsed / PLAYING_BARS_LOOP.as_secs_f32()).fract()
    }

    /// Apply a sort from the picker by running the matching `:sort` command, so picker behavior
    /// cannot drift from the command.
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

    // Vim-style command mode (`:`) and track filter (`/`) — a bar above the playback bar that
    // owns key handling while one of the modes is active. The command registry itself lives in
    // `echo_core::commands`.

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
            // Spotify's folders keep the name Spotify gives them.
            echo_core::models::LibraryNode::SpotifyFolder { .. } => return,
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

    fn add_to_queue(&mut self, cx: &mut Context<Self>) {
        // In visual mode `q` queues the whole range.
        if self.multi_selected() {
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
        if self.theme_modal_open {
            // `m` is the mode key while the theme picker is up.
            self.cycle_theme_mode(cx);
            return;
        }
        echo_core::intent::toggle_pin_selected(&mut self.state);
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
                el.child(
                    div().flex().flex_row().gap_2().overflow_hidden().children(
                        suggestions
                            .into_iter()
                            .enumerate()
                            .map(|(index, suggestion)| {
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
                    ),
                )
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
                if let Some(event) = echo_core::intent::submit_setup_credentials(&mut self.state) {
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
                if self.search_input.is_empty() {
                    window.focus(&self.focus_handle, cx);
                } else {
                    self.search_input.clear();
                    self.search_cursor = 0;
                }
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

    // Transport, all via the shared intents (optimistic flip + worker event).

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::toggle_playback(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn play_next(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::next_track(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn play_previous(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::previous_track(&self.state);
        self.dispatch(event);
        cx.notify();
    }

    fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.duplicate_prompt.is_some() {
            if let Some(event) = echo_core::intent::confirm_duplicate_skip(&mut self.state) {
                self.dispatch(event);
            }
            cx.notify();
            return;
        }
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
        self.notify_playback_position(cx);
    }

    fn finish_scrub(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(target) = self.scrubbing.take() else {
            return;
        };
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

    pub(crate) fn begin_shelf_scrub(&mut self, scroll: gpui::ScrollHandle, x: Pixels) {
        self.shelf_scrub = Some(scroll);
        self.update_shelf_scrub(x);
    }

    /// Scrolls the dragged shelf so the thumb follows the pointer's fraction of the bar (the
    /// bar spans the shelf row's width).
    fn update_shelf_scrub(&mut self, x: Pixels) {
        let Some(scroll) = self.shelf_scrub.as_ref() else {
            return;
        };
        let bounds = scroll.bounds();
        let fraction = ((x - bounds.left()) / bounds.size.width.max(px(1.0))).clamp(0.0, 1.0);
        scroll.set_offset(gpui::point(-scroll.max_offset().x * fraction, px(0.0)));
    }

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

impl Render for EchoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Every theme change re-renders the app, so this is the one place that catches them
        // all (startup, the theme picker, a hot-reloaded theme file).
        self.dock_icon.sync(&self.state.ui.active_theme, cx);
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
        let update_banner = match &self.update_state {
            UpdateState::Ready(version) if !self.update_banner_dismissed => {
                let palette = DesktopPalette::resolve(&self.state.ui.active_theme);
                let text = views::tr(&self.state, "desktop.settings.updates.installed")
                    .replace("{}", version);
                Some(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_4()
                        .py_2()
                        .bg(palette.wash)
                        .border_t_1()
                        .border_color(palette.border)
                        .text_sm()
                        .child(div().flex_1().child(text))
                        .child(
                            div()
                                .id("restart-update")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.border)
                                .cursor_pointer()
                                .hover(move |s| s.bg(palette.row_hover))
                                .on_click(cx.listener(|this, _, _, cx| this.restart_updated(cx)))
                                .child(views::tr(
                                    &self.state,
                                    "desktop.settings.updates.restart_now",
                                )),
                        )
                        .child(
                            div()
                                .id("dismiss-update")
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.update_banner_dismissed = true;
                                    cx.notify();
                                }))
                                .child(views::tr(&self.state, "desktop.settings.updates.later")),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };
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
            .then(|| views::context_menu(self, window, cx).into_any_element());
        if self.track_menu.is_none() {
            self.track_menu_scroll.scroll_to_item(0);
        }
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
        // The immersive backdrop while that view is up: its colors replace the theme's for the
        // window fill and the titlebar, and its picture goes under everything.
        let backdrop = self.immersive.then(|| {
            let playback = &self.state.playback;
            let cover = playback
                .playing_track_image
                .as_ref()
                .or(playback.previous_track_image.as_ref());
            let fallback = backdrop::theme_palette(&self.state.ui.active_theme);
            let mode = self.state.ui.library_config.immersive_backdrop;
            let backdrop = self.backdrops.get(mode, cover, &fallback);
            if let Some(build) = self.backdrops.take_build() {
                cx.spawn(async move |this, cx| {
                    let built = cx.background_spawn(async move { build.run() }).await;
                    this.update(cx, |app: &mut EchoApp, cx| {
                        app.backdrops.install(built);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            self.backdrops.release(|image| {
                window.drop_image(image).ok();
            });
            backdrop
        });
        let phase = if cx.reduce_motion() {
            0.0
        } else {
            self.backdrops.phase()
        };
        let titlebar = owns_frame.then(|| {
            views::titlebar(self, backdrop.as_ref().map(|b| &b.colors), window, cx)
                .into_any_element()
        });

        let root = div()
            .key_context(if self.playlist_input_open() {
                "list search"
            } else {
                LIST_CONTEXT
            })
            .track_focus(&self.focus_handle)
            // Scrubs track the pointer at the window level so dragging keeps working when the
            // pointer leaves the bar; release (or a move without the button) ends them.
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if this.panel_resizing.is_some() {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.update_panel_resize(event.position.x, window, cx);
                        } else {
                            this.finish_panel_resize(cx);
                        }
                    }
                    if this.scrubbing.is_some() {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.update_scrub(event.position.x, cx);
                        } else {
                            this.finish_scrub(event.position.x, cx);
                        }
                    }
                    if this.shelf_scrub.is_some() {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.update_shelf_scrub(event.position.x);
                        } else {
                            this.shelf_scrub = None;
                        }
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                    this.finish_panel_resize(cx);
                    this.finish_scrub(event.position.x, cx);
                    this.shelf_scrub = None;
                }),
            )
            .on_action(
                cx.listener(|this, _: &TogglePlayback, _window, cx| this.toggle_playback(cx)),
            )
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
            // cannot see the window, so the ctrl-q binding remembers the window rectangle the
            // way the platform's own close request does. The Linux caption button's close
            // dispatches `CloseWindow` instead: the same tray-or-quit decision as that request.
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                this.persist_window_bounds(window);
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                if this.on_close_request(window, cx) {
                    window.remove_window();
                }
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
            .on_action(cx.listener(|this, _: &AddToPlaylist, _window, cx| this.add_to_playlist(cx)))
            .on_action(
                cx.listener(|this, _: &OpenActionMenu, _window, cx| this.open_action_menu(cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleSettings, _window, cx| this.toggle_settings(cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleHelp, _window, cx| {
                this.help_open = !this.help_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MarkDelete, _window, cx| {
                this.pending_count = None;
                if this.multi_selected() {
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
            .on_action(cx.listener(|this, _: &SeekForward, _window, cx| this.seek_relative(5, cx)))
            .on_action(
                cx.listener(|this, _: &SeekBackward, _window, cx| this.seek_relative(-5, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleQueue, _window, cx| this.toggle_queue_view(cx)))
            .on_action(cx.listener(|this, _: &OpenDevices, _window, cx| this.open_devices(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _window, cx| this.dismiss(cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                window.focus(&this.search_focus, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleLyrics, _window, cx| this.toggle_lyrics(cx)))
            .on_action(cx.listener(|this, _: &ToggleThemes, _window, cx| this.toggle_themes(cx)))
            .on_action(
                cx.listener(|this, _: &ToggleImmersive, _window, cx| this.toggle_immersive(cx)),
            )
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
            .on_action(
                cx.listener(|this, _: &RenamePrompt, window, cx| this.rename_prompt(window, cx)),
            )
            .on_action(cx.listener(|this, _: &AddToQueue, _window, cx| this.add_to_queue(cx)))
            .on_action(cx.listener(|this, _: &TogglePin, _window, cx| this.toggle_pin(cx)))
            .on_action(cx.listener(|this, _: &CycleTab, _window, cx| this.cycle_tab(cx)))
            .on_action(cx.listener(|this, _: &Refresh, _window, cx| this.refresh_view(cx)))
            .on_action(cx.listener(|this, _: &OpenHome, window, cx| {
                this.open_home(cx);
                window.focus(&this.focus_handle, cx);
            }))
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
                if this.handle_playlist_input(event, window, cx) {
                    return;
                }
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
            .bg(backdrop.as_ref().map_or(bg, |b| b.colors.background))
            // The window's backdrop: the one fill covering every corner, so it rounds all four.
            .map(|el| views::round_client_corners(el, corners, views::ClientCorners::All))
            .when_some(backdrop.as_ref(), |el, backdrop| {
                el.child(views::backdrop_layer(backdrop.clone(), phase, corners))
            })
            .when_some(titlebar, |el, bar| el.child(bar))
            .map(|el| {
                if let Some(backdrop) = &backdrop {
                    el.child(views::immersive_view(self, backdrop, window, cx))
                } else {
                    el.child(
                        div()
                            .flex_grow(1.0)
                            .flex()
                            .flex_row()
                            .overflow_hidden()
                            .when(!self.sidebar_collapsed, |el| {
                                el.child(self.regions.sidebar(self.sidebar_width))
                            })
                            .child(self.regions.main())
                            .when(self.right_panel.is_some(), |el| {
                                let width = self.effective_right_panel_width(window);
                                el.child(self.regions.right_panel(width))
                            }),
                    )
                    .when_some(status_line, |el, line| el.child(line))
                }
            })
            .when_some(command_bar, |el, bar| el.child(bar))
            .when_some(audio_banner, |el, banner| el.child(banner))
            .when_some(update_banner, |el, banner| el.child(banner))
            .when(!self.immersive, |el| el.child(self.regions.playback_bar()))
            .when_some(theme_modal, |el, modal| el.child(modal))
            .when_some(device_modal, |el, modal| el.child(modal))
            .when_some(playlist_add_modal, |el, modal| el.child(modal))
            .when_some(sort_menu, |el, menu| el.child(menu))
            .when_some(settings_modal, |el, modal| el.child(modal))
            .when_some(help_modal, |el, modal| el.child(modal))
            .when_some(context_menu, |el, menu| el.child(menu))
            .when_some(track_menu, |el, menu| el.child(menu))
            .when_some(prompt_modal, |el, modal| el.child(modal))
            .when(self.page_menu.is_some(), |el| {
                el.child(views::playlist_page_menu(self, cx))
            })
            .when(self.state.ui.playlist_edit.is_some(), |el| {
                el.child(views::playlist_edit_modal(self, cx))
            });

        // Adds the border and resize edges when the compositor refuses to draw them; a
        // pass-through when it does.
        views::window_frame(root, window, frame_palette)
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
