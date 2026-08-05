//! Echo — the GPUI desktop frontend.
//!
//! Same architecture as the `spotify` TUI: [`echo_core::bootstrap::init`] spawns the worker on a
//! tokio runtime and hands back the two event channels; the frontend applies worker events to
//! [`AppState`](echo_core::app::AppState) and draws from it. Here the 16ms poll loop is replaced
//! by GPUI's reactive model — a spawned task awaits worker events and calls `cx.notify()`.
//!
//! The tokio runtime lives on the main function's stack and stays entered for the lifetime of
//! the UI, so worker tasks keep running on its threads while GPUI blocks in `run()`.

mod theme;

use std::time::Duration;

use echo_core::apply_worker_event::apply_worker_event;
use echo_core::events::AppEvent;
use gpui::{
    App, Bounds, Context, FocusHandle, KeyBinding, SharedString, Window, WindowBounds,
    WindowOptions, actions, div, prelude::*, px, size,
};
use gpui_platform::application;
use theme::{ToGpui, WINDOW_BG, WINDOW_FG};

actions!(echo, [Quit, TogglePlayback]);

struct EchoApp {
    state: echo_core::app::AppState,
    app_tx: tokio::sync::mpsc::UnboundedSender<AppEvent>,
    worker_tx: tokio::sync::mpsc::Sender<echo_core::events::WorkerEvent>,
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
            focus_handle,
        }
    }

    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        let desired = !self.state.playback.is_playing;
        // Optimistic flip; the worker's next SyncPlaybackState corrects any divergence.
        self.state.playback.is_playing = desired;
        let _ = self.app_tx.send(AppEvent::TogglePlayback(desired));
        cx.notify();
    }

    fn render_playback_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let muted = theme.text_muted.gpui(WINDOW_FG());

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &TogglePlayback, _window, cx| {
                this.toggle_playback(cx)
            }))
            .flex()
            .flex_col()
            .size_full()
            .bg(bg)
            .child(
                // Placeholder body — the library and track list land here in the next phase.
                div()
                    .flex_grow(1.0)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(muted)
                    .child(status_line(&self.state)),
            )
            .child(self.render_playback_bar(cx))
    }
}

/// A one-line summary of where the app is, until real views land.
fn status_line(state: &echo_core::app::AppState) -> SharedString {
    use echo_core::app::AppMode;
    match state.ui.mode {
        AppMode::Setup => "Set up Spotify credentials in the terminal app first: run `spotify`".into(),
        AppMode::Authenticating => "Authenticating with Spotify…".into(),
        _ => {
            let playlists = state.data.playlists.len();
            let albums = state.data.saved_albums.len();
            format!("Library loaded: {playlists} playlists, {albums} albums").into()
        }
    }
}

fn format_time(ms: u32) -> String {
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
