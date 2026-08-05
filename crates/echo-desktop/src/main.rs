//! Echo — the GPUI desktop frontend.
//!
//! Same architecture as the `spotify` TUI: [`echo_core::bootstrap::init`] spawns the worker on a
//! tokio runtime and hands back the two event channels; the frontend applies worker events to
//! [`AppState`](echo_core::app::AppState) and draws from it. Here the 16ms poll loop is replaced
//! by GPUI's reactive model — a spawned task awaits worker events and calls `cx.notify()`.
//!
//! The tokio runtime lives on the main function's stack and stays entered for the lifetime of
//! the UI, so worker tasks keep running on its threads while GPUI blocks in `run()`.

mod images;
mod theme;
mod views;

use std::time::Duration;

use echo_core::app::{ActiveView, LibraryTab};
use echo_core::apply_worker_event::apply_worker_event;
use echo_core::events::AppEvent;
use gpui::{
    App, Bounds, Context, FocusHandle, KeyBinding, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, div, img, prelude::*,
    px, size,
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
        SelectFirst,
        SelectLast,
        Activate,
        FocusLibrary,
        FocusTracks
    ]
);

/// Keyboard page distance; the TUI uses a similar fixed stride.
const PAGE_ROWS: isize = 10;

pub(crate) struct EchoApp {
    pub(crate) state: echo_core::app::AppState,
    pub(crate) app_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    pub(crate) worker_tx: tokio::sync::mpsc::Sender<echo_core::events::WorkerEvent>,
    pub(crate) images: images::ImageCache,
    pub(crate) library_scroll: UniformListScrollHandle,
    pub(crate) tracks_scroll: UniformListScrollHandle,
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
        // needs a repaint even when no worker event arrives.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                let alive = this.update(cx, |app: &mut EchoApp, cx| {
                    if app.state.playback.is_playing {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
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
            library_scroll: UniformListScrollHandle::new(),
            tracks_scroll: UniformListScrollHandle::new(),
            focus_handle,
        }
    }

    /// Rows in whichever list `active_view` gives keyboard focus.
    fn list_len(&self) -> usize {
        match self.state.ui.active_view {
            ActiveView::TrackList => self.state.data.tracks.len(),
            _ => match self.state.ui.active_library_tab {
                LibraryTab::Albums => self.state.data.saved_albums.len(),
                _ => self.state.data.library_view.len(),
            },
        }
    }

    fn set_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.state.ui.active_view == ActiveView::TrackList {
            self.state.ui.selected_track_index = index;
            self.tracks_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        } else {
            self.state.ui.selected_playlist_index = index;
            self.library_scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let current = if self.state.ui.active_view == ActiveView::TrackList {
            self.state.ui.selected_track_index
        } else {
            self.state.ui.selected_playlist_index
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
        let event = if self.state.ui.active_view == ActiveView::TrackList {
            let index = self.state.ui.selected_track_index;
            echo_core::intent::play_track_at(&mut self.state, index)
        } else {
            let index = self.state.ui.selected_playlist_index;
            match self.state.ui.active_library_tab {
                LibraryTab::Albums => echo_core::intent::open_album(&mut self.state, index),
                _ => echo_core::intent::open_library_entry(&mut self.state, index),
            }
        };
        if let Some(event) = event {
            self.dispatch(event);
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

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        let desired = !self.state.playback.is_playing;
        // Optimistic flip; the worker's next SyncPlaybackState corrects any divergence.
        self.state.playback.is_playing = desired;
        let _ = self.app_tx.send(AppEvent::TogglePlayback(desired));
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
        let time_label: SharedString = format!(
            "{} / {}",
            format_time(progress_ms),
            format_time(playback.duration_ms)
        )
        .into();
        let play_glyph: SharedString = if playback.is_playing { "⏸" } else { "▶" }.into();

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

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_4()
            .h(px(72.0))
            .px_4()
            .border_t_1()
            .border_color(muted.opacity(0.3))
            .child(
                div()
                    .id("play-pause")
                    .flex_none()
                    .w(px(40.0))
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_color(accent)
                    .text_xl()
                    .hover(|style| style.bg(accent.opacity(0.15)))
                    .on_click(cx.listener(|this, _event, _window, cx| this.toggle_playback(cx)))
                    .child(play_glyph),
            )
            .child(match cover {
                Some(image) => img(image)
                    .flex_none()
                    .w(px(48.0))
                    .h(px(48.0))
                    .rounded_md()
                    .into_any_element(),
                None => div()
                    .flex_none()
                    .w(px(48.0))
                    .h(px(48.0))
                    .rounded_md()
                    .bg(muted.opacity(0.15))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(muted)
                    .child("♪")
                    .into_any_element(),
            })
            .child(
                div()
                    .flex_col()
                    .flex_none()
                    .w(px(260.0))
                    .overflow_hidden()
                    .child(div().text_color(fg).text_sm().child(title))
                    .child(div().text_color(muted).text_xs().child(artist)),
            )
            .child(
                // Progress track with a filled portion.
                div()
                    .flex_grow(1.0)
                    .h(px(6.0))
                    .rounded_full()
                    .bg(muted.opacity(0.25))
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(accent)
                            .w(gpui::relative(fraction)),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(muted)
                    .text_xs()
                    .child(time_label),
            )
    }
}

impl Render for EchoApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.ui.active_theme;
        let bg = theme.background.gpui(WINDOW_BG());

        div()
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
                    .child(views::main_area(self, cx)),
            )
            .child(self.render_playback_bar(cx))
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

    application().run(move |cx: &mut App| {
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([
            KeyBinding::new("ctrl-q", Quit, None),
            KeyBinding::new("space", TogglePlayback, None),
            // Arrows plus the TUI's vim keys; no text inputs exist yet to conflict with.
            KeyBinding::new("up", MoveUp, None),
            KeyBinding::new("k", MoveUp, None),
            KeyBinding::new("down", MoveDown, None),
            KeyBinding::new("j", MoveDown, None),
            KeyBinding::new("pageup", PageUp, None),
            KeyBinding::new("pagedown", PageDown, None),
            KeyBinding::new("home", SelectFirst, None),
            KeyBinding::new("end", SelectLast, None),
            KeyBinding::new("enter", Activate, None),
            KeyBinding::new("left", FocusLibrary, None),
            KeyBinding::new("h", FocusLibrary, None),
            KeyBinding::new("right", FocusTracks, None),
            KeyBinding::new("l", FocusTracks, None),
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
