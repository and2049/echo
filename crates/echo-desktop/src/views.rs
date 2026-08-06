//! Library sidebar and track list — the first real views of the desktop app.
//!
//! Both are thin projections of [`AppState`]: rows come straight from `library_view` /
//! `saved_albums` / `tracks`, and activating a row goes through [`echo_core::intent`], the same
//! functions the TUI's Enter key uses.
//!
//! Thumbnails ride the core's [`echo_core::thumbnails`] cache. Rendering a row requests its
//! cover; `drain_pending` (called after each row batch) spawns the fetches, and the resulting
//! worker events repaint. Unlike the TUI this ignores the `library_thumbnails` config toggle —
//! that flag exists because thumbnails cost real estate and glitch on some terminals, neither of
//! which applies here.

use echo_core::app::{ActiveView, AppMode, LibraryTab, SearchTab};
use echo_core::models::LibraryNode;
use echo_core::thumbnails::ThumbState;
use gpui::{
    AnyElement, Context, MouseButton, MouseDownEvent, SharedString, Window, div, img, prelude::*,
    px, relative, svg, uniform_list,
};

use crate::theme::{ToGpui, WINDOW_FG};
use crate::{EchoApp, MenuAction, TrackMenuItem, format_time};

pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;
const SIDEBAR_ROW_HEIGHT: f32 = 34.0;
const ROW_HEIGHT: f32 = 30.0;
const THUMB_EDGE: f32 = 26.0;
// Native caption metrics: Windows titlebars are a fixed 32px, macOS gets a touch more.
const TITLEBAR_HEIGHT: f32 = if cfg!(target_os = "windows") { 32.0 } else { 34.0 };
// Zed's measured inset for the macOS traffic lights (71px, +1px window border).
const TRAFFIC_LIGHT_PADDING: f32 = 71.0;

/// The TUI's start-screen wordmark, shown in the main area while nothing is selected.
const ECHO_LOGO: [&str; 6] = [
    "███████╗ ██████╗██╗  ██╗ ██████╗               ██████╗ ███████╗",
    "██╔════╝██╔════╝██║  ██║██╔═══██╗              ██╔══██╗██╔════╝",
    "█████╗  ██║     ███████║██║   ██║    █████╗    ██████╔╝███████╗",
    "██╔══╝  ██║     ██╔══██║██║   ██║    ╚════╝    ██╔══██╗╚════██║",
    "███████╗╚██████╗██║  ██║╚██████╔╝              ██║  ██║███████║",
    "╚══════╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝               ╚═╝  ╚═╝╚══════╝",
];

/// Drag payload for a sidebar playlist row.
pub(crate) struct DraggedPlaylist {
    pub id: String,
    pub name: SharedString,
}

/// The chip that follows the cursor while a playlist is dragged.
pub(crate) struct DragPreview {
    name: SharedString,
    fg: gpui::Hsla,
    bg: gpui::Hsla,
    border: gpui::Hsla,
}

impl gpui::Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(self.bg)
            .border_1()
            .border_color(self.border)
            .text_sm()
            .text_color(self.fg)
            .child(self.name.clone())
    }
}

/// True for the sidebar rows drag-and-drop must not move (their position is fixed or comes
/// from the local store, mirroring `echo_core::intent`'s own check).
fn is_fixed_library_row(id: &str) -> bool {
    id == "LIKED_SONGS" || id == "local-library" || id.starts_with("local-playlist:")
}

/// The custom titlebar: themed like the rest of the window, draggable, with caption buttons
/// on Windows. macOS keeps its native traffic lights (the bar just insets past them) and
/// only needs the drag/double-click plumbing done in Rust; Windows gets drag, double-click
/// and snap layouts for free from the `HTCAPTION`/`HTMAXBUTTON` hit-tests. On Linux the
/// window manager draws server-side decorations, so this view isn't rendered at all.
pub fn titlebar(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let fullscreen = window.is_fullscreen();
    let maximized = window.is_maximized();

    div()
        .id("titlebar")
        .window_control_area(gpui::WindowControlArea::Drag)
        .flex_none()
        .w_full()
        .h(px(TITLEBAR_HEIGHT))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .bg(surface)
        .map(|el| {
            if fullscreen {
                el.pl_3()
            } else if cfg!(target_os = "macos") {
                el.pl(px(TRAFFIC_LIGHT_PADDING))
            } else {
                el.pl_3()
            }
        })
        // AppKit neither drags nor double-click-zooms a transparent titlebar for us
        // (`app_owns_titlebar_drag`), so do both by hand — the Zed latch pattern: arm on
        // mouse-down, and the first real move starts the native window drag.
        .when(cfg!(target_os = "macos"), |el| {
            el.on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _window, _cx| this.titlebar_should_move = true),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _window, _cx| this.titlebar_should_move = false),
            )
            .on_mouse_move(cx.listener(|this, _, window, _cx| {
                if this.titlebar_should_move {
                    this.titlebar_should_move = false;
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _cx| {
                if event.click_count() == 2 {
                    window.titlebar_double_click();
                }
            })
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(svg().path("icons/music-note.svg").size(px(14.0)).text_color(accent))
                .child(div().text_xs().text_color(muted).child("echo")),
        )
        .when(cfg!(target_os = "windows") && !fullscreen, |el| {
            el.child(
                div()
                    .flex()
                    .flex_row()
                    .h_full()
                    .child(caption_button(
                        "caption-min",
                        "icons/win-minimize.svg",
                        gpui::WindowControlArea::Min,
                        fg,
                    ))
                    .child(caption_button(
                        "caption-max",
                        if maximized {
                            "icons/win-restore.svg"
                        } else {
                            "icons/win-maximize.svg"
                        },
                        gpui::WindowControlArea::Max,
                        fg,
                    ))
                    .child(caption_button(
                        "caption-close",
                        "icons/win-close.svg",
                        gpui::WindowControlArea::Close,
                        fg,
                    )),
            )
        })
}

/// A Windows caption button. No click handler: the `window_control_area` tag routes the
/// click through `WM_NCHITTEST`, and gpui + `DefWindowProc` do the minimize/maximize/close.
/// `occlude` is load-bearing — without it the surrounding Drag hitbox wins the hit-test and
/// the button is dead.
fn caption_button(
    id: &'static str,
    icon: &'static str,
    area: gpui::WindowControlArea,
    fg: gpui::Hsla,
) -> impl IntoElement {
    let close = matches!(area, gpui::WindowControlArea::Close);
    // The close button hovers Windows-red with a white glyph; the rest get a faint wash.
    let hover_bg: gpui::Hsla =
        if close { gpui::rgb(0xE81123).into() } else { fg.opacity(0.08) };
    let active_bg = if close { hover_bg.opacity(0.8) } else { fg.opacity(0.12) };
    div()
        .id(id)
        .group(id)
        .occlude()
        .window_control_area(area)
        .w(px(46.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .hover(|style| style.bg(hover_bg))
        .active(|style| style.bg(active_bg))
        .child(
            svg()
                .path(icon)
                .size(px(10.0))
                .text_color(fg)
                .when(close, |el| el.group_hover(id, |style| style.text_color(gpui::white()))),
        )
}

pub fn sidebar(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let tab = app.state.ui.active_library_tab;
    let count = match tab {
        LibraryTab::Albums => app.state.data.saved_albums.len(),
        LibraryTab::Artists => app.state.data.followed_artists.len(),
        _ => app.state.data.library_view.len(),
    };

    let tab_button = |label: &'static str, target: LibraryTab, active: bool| {
        div()
            .id(label)
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .text_color(if active { accent } else { muted })
            .hover(|style| style.bg(accent.opacity(0.1)))
            .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                this.state.ui.active_library_tab = target;
                this.state.ui.selected_playlist_index = 0;
                if target == LibraryTab::Artists && this.state.data.followed_artists.is_empty() {
                    this.dispatch(echo_core::events::AppEvent::FetchFollowedArtists);
                }
                cx.notify();
            }))
            .child(label)
    };

    div()
        .relative()
        .flex_none()
        .w(px(app.sidebar_width))
        .h_full()
        .flex()
        .flex_col()
        .border_r_1()
        .border_color(muted.opacity(0.3))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .p_2()
                .child(tab_button(
                    "Playlists",
                    LibraryTab::Playlists,
                    tab == LibraryTab::Playlists,
                ))
                .child(tab_button("Albums", LibraryTab::Albums, tab == LibraryTab::Albums))
                .child(tab_button("Artists", LibraryTab::Artists, tab == LibraryTab::Artists)),
        )
        .child({
            // The TUI's Browse nodes, as quick links.
            let browse_link = |id: &'static str,
                               icon: &'static str,
                               label: &'static str,
                               open: fn(&mut echo_core::app::AppState)
                                   -> Option<echo_core::events::AppEvent>| {
                div()
                    .id(id)
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(muted)
                    .hover(|style| style.bg(muted.opacity(0.1)))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                        if let Some(event) = open(&mut this.state) {
                            this.dispatch(event);
                        }
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path(icon)
                            .flex_none()
                            .w(px(14.0))
                            .h(px(14.0))
                            .text_color(muted),
                    )
                    .child(label)
            };
            div()
                .flex_none()
                .flex()
                .flex_col()
                .pb_1()
                .child(browse_link(
                    "top-tracks",
                    "icons/star.svg",
                    "Top Tracks",
                    echo_core::intent::open_top_tracks,
                ))
                .child(browse_link(
                    "recently-played",
                    "icons/clock.svg",
                    "Recently Played",
                    echo_core::intent::open_recently_played,
                ))
        })
        .child(
            uniform_list(
                "library-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let accent = theme.primary.gpui(WINDOW_FG());
                    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
                    let panel_bg = theme.surface.gpui(crate::theme::PANEL_BG());
                    let tab = this.state.ui.active_library_tab;
                    let selected = this.state.ui.selected_playlist_index;

                    let rows: Vec<_> = range
                        .map(|ix| {
                            // Folders carry no cover; playlists, albums and artists get a
                            // thumb box even while (or if never) loaded, so the text column
                            // stays aligned. Folders get a chevron icon, pinned playlists a
                            // pin icon; artist thumbs are circular.
                            let (label, label_color, indent_px, thumb_url, has_thumb, chevron, pinned, round_thumb): (
                                SharedString,
                                _,
                                f32,
                                Option<String>,
                                bool,
                                Option<&'static str>,
                                bool,
                                bool,
                            ) = if tab == LibraryTab::Albums {
                                let album = &this.state.data.saved_albums[ix];
                                let url = album
                                    .thumb_url
                                    .clone()
                                    .or_else(|| album.image_url.clone());
                                (album.name.clone().into(), fg, 0.0, url, true, None, false, false)
                            } else if tab == LibraryTab::Artists {
                                let artist = &this.state.data.followed_artists[ix];
                                (
                                    artist.name.clone().into(),
                                    fg,
                                    0.0,
                                    artist.image_url.clone(),
                                    true,
                                    None,
                                    false,
                                    true,
                                )
                            } else {
                                match &this.state.data.library_view[ix] {
                                    LibraryNode::Folder(f) => (
                                        f.name.clone().into(),
                                        accent,
                                        0.0,
                                        None,
                                        false,
                                        Some(if f.is_open {
                                            "icons/arrow-down.svg"
                                        } else {
                                            "icons/arrow-right.svg"
                                        }),
                                        false,
                                        false,
                                    ),
                                    LibraryNode::Playlist { playlist, indent } => {
                                        let pinned = this
                                            .state
                                            .ui
                                            .library_config
                                            .pinned
                                            .contains(&playlist.id);
                                        let url = playlist
                                            .thumb_url
                                            .clone()
                                            .or_else(|| playlist.image_url.clone());
                                        (
                                            playlist.name.clone().into(),
                                            fg,
                                            *indent as f32 * 14.0,
                                            url,
                                            true,
                                            None,
                                            pinned,
                                            false,
                                        )
                                    }
                                }
                            };

                            let thumb: Option<AnyElement> = has_thumb.then(|| {
                                let artwork = thumb_url.as_deref().and_then(|url| {
                                    this.state.ui.thumbnails.request(url);
                                    match this.state.ui.thumbnails.get(url) {
                                        Some(ThumbState::Ready { artwork }) => {
                                            Some(artwork.clone())
                                        }
                                        _ => None,
                                    }
                                });
                                match artwork.and_then(|artwork| this.images.get(&artwork)) {
                                    Some(image) => {
                                        let el = img(image)
                                            .flex_none()
                                            .w(px(THUMB_EDGE))
                                            .h(px(THUMB_EDGE));
                                        if round_thumb { el.rounded_full() } else { el.rounded_sm() }
                                            .into_any_element()
                                    }
                                    None => {
                                        let el = div()
                                            .flex_none()
                                            .w(px(THUMB_EDGE))
                                            .h(px(THUMB_EDGE))
                                            .bg(muted.opacity(0.15))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                svg()
                                                    .path("icons/music-note.svg")
                                                    .w(px(12.0))
                                                    .h(px(12.0))
                                                    .text_color(muted),
                                            );
                                        if round_thumb { el.rounded_full() } else { el.rounded_sm() }
                                            .into_any_element()
                                    }
                                }
                            });

                            // Regular Spotify playlists can be dragged between folders, the
                            // pinned block and the loose list; every playlist-tab row is a
                            // drop target (the intent rejects invalid ones).
                            let drag_source: Option<(String, SharedString)> =
                                if tab == LibraryTab::Playlists {
                                    match &this.state.data.library_view[ix] {
                                        LibraryNode::Playlist { playlist, .. }
                                            if !is_fixed_library_row(&playlist.id) =>
                                        {
                                            Some((
                                                playlist.id.clone(),
                                                playlist.name.clone().into(),
                                            ))
                                        }
                                        _ => None,
                                    }
                                } else {
                                    None
                                };

                            div()
                                .id(ix)
                                .w_full()
                                .h(px(SIDEBAR_ROW_HEIGHT))
                                .px_3()
                                .pl(px(12.0 + indent_px))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .text_color(label_color)
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.1)))
                                .cursor_pointer()
                                .when(tab != LibraryTab::Artists, |el| {
                                    // Artists have no row actions, so no context menu.
                                    el.on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                                            this.state.ui.selected_playlist_index = ix;
                                            this.track_menu = None;
                                            this.context_menu = Some(crate::ContextMenuState {
                                                index: ix,
                                                position: event.position,
                                            });
                                            cx.notify();
                                        }),
                                    )
                                })
                                .when_some(drag_source, |el, (id, name)| {
                                    let border = muted.opacity(0.4);
                                    el.on_drag(
                                        DraggedPlaylist { id, name },
                                        move |drag, _offset, _window, cx| {
                                            let name = drag.name.clone();
                                            cx.new(|_| DragPreview {
                                                name,
                                                fg,
                                                bg: panel_bg,
                                                border,
                                            })
                                        },
                                    )
                                })
                                .when(tab == LibraryTab::Playlists, |el| {
                                    el.drag_over::<DraggedPlaylist>(move |style, _, _, _| {
                                        style.bg(accent.opacity(0.2))
                                    })
                                    .on_drop(cx.listener(
                                        move |this: &mut EchoApp, drag: &DraggedPlaylist, _window, cx| {
                                            if echo_core::intent::move_library_playlist(
                                                &mut this.state,
                                                &drag.id,
                                                ix,
                                            ) {
                                                cx.notify();
                                            }
                                        },
                                    ))
                                })
                                .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                                    this.state.ui.selected_playlist_index = ix;
                                    let event = match this.state.ui.active_library_tab {
                                        LibraryTab::Albums => {
                                            echo_core::intent::open_album(&mut this.state, ix)
                                        }
                                        LibraryTab::Artists => {
                                            echo_core::intent::open_followed_artist(
                                                &mut this.state,
                                                ix,
                                            )
                                        }
                                        _ => echo_core::intent::open_library_entry(
                                            &mut this.state,
                                            ix,
                                        ),
                                    };
                                    if let Some(event) = event {
                                        this.dispatch(event);
                                    }
                                    cx.notify();
                                }))
                                .when_some(chevron, |el, icon| {
                                    el.child(
                                        svg()
                                            .path(icon)
                                            .flex_none()
                                            .w(px(12.0))
                                            .h(px(12.0))
                                            .text_color(accent),
                                    )
                                })
                                .when_some(thumb, |el, thumb| el.child(thumb))
                                .when(pinned, |el| {
                                    el.child(
                                        svg()
                                            .path("icons/pin.svg")
                                            .flex_none()
                                            .w(px(12.0))
                                            .h(px(12.0))
                                            .text_color(muted),
                                    )
                                })
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(label),
                                )
                        })
                        .collect();

                    // Kick off fetches for whatever the rows above just requested.
                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);

                    rows
                }),
            )
            .track_scroll(&app.library_scroll)
            .flex_grow(1.0),
        )
        .child(
            div()
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .w(px(5.0))
                .cursor_col_resize()
                .hover(|style| style.bg(accent.opacity(0.2)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                        this.begin_sidebar_resize(event.position.x, cx);
                        cx.stop_propagation();
                    }),
                ),
        )
}

pub fn main_area(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let search = search_bar(app, window, cx).into_any_element();
    let body = if app.state.ui.mode == AppMode::Setup {
        setup_view(app, window, cx).into_any_element()
    } else {
        match app.state.ui.active_view {
        ActiveView::TrackList => track_list(app, cx).into_any_element(),
        ActiveView::Queue => queue_list(app, cx).into_any_element(),
        ActiveView::SearchResults => search_results(app, cx).into_any_element(),
        ActiveView::ArtistList => artist_list(app, cx).into_any_element(),
        ActiveView::ArtistPage => artist_page(app, cx).into_any_element(),
            _ => library_placeholder(app),
        }
    };

    div()
        .flex_grow(1.0)
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(search)
        .child(body)
}

/// First-run credentials: a BYOK (bring-your-own-key) card matching the TUI's setup screen,
/// writing to the same `state.ui.setup_*` fields and submitting through the same intent.
fn setup_view(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let id_focused = app.setup_id_focus.is_focused(window);
    let secret_focused = app.setup_secret_focus.is_focused(window);
    let client_id = app.state.ui.setup_client_id.clone();
    let secret_masked = "•".repeat(app.state.ui.setup_client_secret.chars().count());
    let ready =
        !app.state.ui.setup_client_id.is_empty() && !app.state.ui.setup_client_secret.is_empty();

    let field = |id: &'static str,
                 label: &'static str,
                 value: String,
                 focused: bool,
                 secret: bool,
                 handle: gpui::FocusHandle| {
        let click_handle = handle.clone();
        div()
            .id(id)
            .key_context(crate::SEARCH_CONTEXT)
            .track_focus(&handle)
            .on_key_down(cx.listener(move |this: &mut EchoApp, event, window, cx| {
                this.handle_setup_key(secret, event, window, cx)
            }))
            .w_full()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(if focused { accent } else { muted.opacity(0.3) })
            .flex()
            .flex_col()
            .gap_1()
            .cursor_pointer()
            .on_click(cx.listener(move |_this, _event, window, cx| {
                window.focus(&click_handle, cx);
                cx.notify();
            }))
            .child(div().text_xs().text_color(muted).child(label))
            .child(
                div()
                    .text_sm()
                    .text_color(fg)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(SharedString::from(if focused {
                        format!("{value}▏")
                    } else if value.is_empty() {
                        " ".to_string()
                    } else {
                        value
                    })),
            )
    };

    div()
        .flex_grow(1.0)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(460.0))
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(muted.opacity(0.3))
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_lg().text_color(fg).child("Connect to Spotify"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .text_color(muted)
                        .child("1. Create an app in the Spotify Developer Dashboard.")
                        .child("2. Add http://127.0.0.1:8888/callback as a Redirect URI.")
                        .child("3. Paste the app's Client ID and Secret below (ctrl-v)."),
                )
                .child(
                    div()
                        .id("open-dashboard")
                        .text_sm()
                        .text_color(accent)
                        .cursor_pointer()
                        .hover(|style| style.underline())
                        .on_click(|_event, _window, _cx| {
                            let _ = webbrowser::open("https://developer.spotify.com/dashboard");
                        })
                        .child("Open the Spotify Developer Dashboard ↗"),
                )
                .child(field(
                    "setup-client-id",
                    "Client ID",
                    client_id,
                    id_focused,
                    false,
                    app.setup_id_focus.clone(),
                ))
                .child(field(
                    "setup-client-secret",
                    "Client Secret",
                    secret_masked,
                    secret_focused,
                    true,
                    app.setup_secret_focus.clone(),
                ))
                .child(
                    div()
                        .id("setup-submit")
                        .mt_1()
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .flex()
                        .justify_center()
                        .text_sm()
                        .bg(if ready {
                            accent
                        } else {
                            muted.opacity(0.15)
                        })
                        .text_color(if ready {
                            gpui::hsla(0.0, 0.0, 0.08, 1.0)
                        } else {
                            muted
                        })
                        .when(ready, |el| el.cursor_pointer())
                        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                            if let Some(event) =
                                echo_core::intent::submit_setup_credentials(&mut this.state)
                            {
                                this.dispatch(event);
                            }
                            cx.notify();
                        }))
                        .child("Save & Connect"),
                ),
        )
}

/// The global search box. A minimal hand-rolled input: gpui ships no text field, and query
/// entry needs no more than append/backspace/submit.
fn search_bar(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let focused = app.search_focus.is_focused(window);
    let query = app.search_input.clone();

    div()
        .flex_none()
        .px_4()
        .pt_3()
        .flex()
        .flex_row()
        .items_center()
        // Invisible stand-in for the theme button so the search box centers exactly.
        .child(div().flex_none().w(px(32.0)))
        .child(div().flex_grow(1.0))
        .child(
        div()
            .id("search-box")
            .key_context(crate::SEARCH_CONTEXT)
            .track_focus(&app.search_focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                this.handle_search_key(event, window, cx)
            }))
            .w(px(360.0))
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(if focused { accent } else { muted.opacity(0.3) })
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .cursor_pointer()
            .on_click(cx.listener(|this: &mut EchoApp, _event, window, cx| {
                window.focus(&this.search_focus, cx);
                cx.notify();
            }))
            .child(
                svg()
                    .path("icons/search.svg")
                    .flex_none()
                    .w(px(14.0))
                    .h(px(14.0))
                    .text_color(muted),
            )
            .child(if query.is_empty() && !focused {
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Search — ctrl-k")
                    .into_any_element()
            } else {
                div()
                    .text_sm()
                    .text_color(fg)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(SharedString::from(if focused {
                        format!("{query}▏")
                    } else {
                        query
                    }))
                    .into_any_element()
            }),
        )
        .child(div().flex_grow(1.0))
        .child(crate::icon_button(
            "themes",
            "icons/paint-board.svg",
            muted,
            cx,
            |this, cx| this.toggle_themes(cx),
        ))
}

fn track_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());

    let (context_title, context_author) = app
        .state
        .data
        .active_tracklist_context
        .as_ref()
        .map(|context| (context.title.clone(), context.subtitle.clone()))
        .unwrap_or_default();

    let header_image = app
        .state
        .ui
        .active_library_header_image
        .clone()
        .and_then(|artwork| app.images.get(&artwork));

    let count = app.state.data.tracks.len();
    // No explicit loading flag in core: an empty list under an active context is "loading".
    let loading = count == 0;

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .px_4()
                .py_3()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .when_some(header_image, |el, image| {
                    el.child(
                        img(image)
                            .flex_none()
                            .w(px(72.0))
                            .h(px(72.0))
                            .rounded_md(),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(
                            div()
                                .text_lg()
                                .text_color(fg)
                                .child(SharedString::from(context_title)),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(context_author)),
                        ),
                ),
        )
        .child(if loading {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("Loading…")
                .into_any_element()
        } else {
            uniform_list(
                "track-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let accent = theme.primary.gpui(WINDOW_FG());
                    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
                    let selected = this.state.ui.selected_track_index;
                    let playing_id = this.state.playback.playing_track_id.clone();

                    range
                        .map(|ix| {
                            let track = &this.state.data.tracks[ix];
                            let is_playing = playing_id.as_deref() == Some(track.id.as_str());
                            let title_color = if is_playing { accent } else { fg };

                            div()
                                .id(ix)
                                .w_full()
                                .h(px(ROW_HEIGHT))
                                .px_4()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.08)))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                    this.state.ui.selected_track_index = ix;
                                    if event.click_count() >= 2
                                        && let Some(event) =
                                            echo_core::intent::play_track_at(&mut this.state, ix)
                                    {
                                        this.dispatch(event);
                                    }
                                    cx.notify();
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                                        this.state.ui.selected_track_index = ix;
                                        this.context_menu = None;
                                        this.track_menu = Some(crate::TrackMenuState {
                                            index: ix,
                                            position: event.position,
                                        });
                                        cx.notify();
                                    }),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(32.0))
                                        .text_color(muted)
                                        .child(SharedString::from(format!("{}", ix + 1))),
                                )
                                .child(
                                    div()
                                        .flex_grow(2.0)
                                        .flex_basis(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(title_color)
                                        .child(SharedString::from(track.name.clone())),
                                )
                                .child(
                                    div()
                                        .flex_grow(1.5)
                                        .flex_basis(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(muted)
                                        .child(SharedString::from(track.artist.clone())),
                                )
                                .child(
                                    div()
                                        .flex_grow(1.5)
                                        .flex_basis(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(muted)
                                        .child(SharedString::from(track.album.clone())),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(48.0))
                                        .text_color(muted)
                                        .child(SharedString::from(format_time(track.duration_ms))),
                                )
                        })
                        .collect()
                }),
            )
            .track_scroll(&app.tracks_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

fn queue_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());

    let count = app.state.data.queue.len();

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .child(div().text_lg().text_color(fg).child("Queue"))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(format!("{count} upcoming"))),
                ),
        )
        .child(if count == 0 {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("The queue is empty")
                .into_any_element()
        } else {
            uniform_list(
                "queue-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
                    let selected = this.state.ui.selected_queue_index;

                    range
                        .map(|ix| {
                            let track = &this.state.data.queue[ix];

                            // Browse-only rows: the Spotify API can't jump into the queue, so a
                            // click just moves the selection.
                            div()
                                .id(ix)
                                .w_full()
                                .h(px(ROW_HEIGHT))
                                .px_4()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.08)))
                                .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                                    this.state.ui.selected_queue_index = ix;
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(32.0))
                                        .text_color(muted)
                                        .child(SharedString::from(format!("{}", ix + 1))),
                                )
                                .child(
                                    div()
                                        .flex_grow(2.0)
                                        .flex_basis(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(fg)
                                        .child(SharedString::from(track.name.clone())),
                                )
                                .child(
                                    div()
                                        .flex_grow(1.5)
                                        .flex_basis(px(0.0))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(muted)
                                        .child(SharedString::from(track.artist.clone())),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(48.0))
                                        .text_color(muted)
                                        .child(SharedString::from(format_time(track.duration_ms))),
                                )
                        })
                        .collect()
                }),
            )
            .track_scroll(&app.queue_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

/// A cover thumbnail box riding the core thumbnail cache, or a music-note placeholder.
fn thumb_element(
    this: &mut EchoApp,
    url: Option<&str>,
    edge: f32,
    round: bool,
    muted: gpui::Hsla,
) -> AnyElement {
    let artwork = url.and_then(|url| {
        this.state.ui.thumbnails.request(url);
        match this.state.ui.thumbnails.get(url) {
            Some(ThumbState::Ready { artwork }) => Some(artwork.clone()),
            _ => None,
        }
    });
    match artwork.and_then(|artwork| this.images.get(&artwork)) {
        Some(image) => {
            let el = img(image).flex_none().w(px(edge)).h(px(edge));
            if round { el.rounded_full() } else { el.rounded_sm() }.into_any_element()
        }
        None => {
            let el = div()
                .flex_none()
                .w(px(edge))
                .h(px(edge))
                .bg(muted.opacity(0.15))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path("icons/music-note.svg")
                        .w(px((edge * 0.45).max(10.0)))
                        .h(px((edge * 0.45).max(10.0)))
                        .text_color(muted),
                );
            if round { el.rounded_full() } else { el.rounded_sm() }.into_any_element()
        }
    }
}

fn search_results(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let tab = app.state.ui.active_search_tab;
    let query = app.state.ui.search_context_query.clone();
    let results = &app.state.data.search_results;
    let (n_tracks, n_albums, n_artists) = (
        results.tracks.len(),
        results.albums.len(),
        results.artists.len(),
    );
    let count = match tab {
        SearchTab::Tracks => n_tracks,
        SearchTab::Albums => n_albums,
        SearchTab::Artists => n_artists,
    };

    let tab_button = |id: &'static str, label: String, target: SearchTab, active: bool| {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .text_color(if active { accent } else { muted })
            .hover(|style| style.bg(accent.opacity(0.1)))
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                this.state.ui.active_search_tab = target;
                this.state.ui.selected_search_index = 0;
                cx.notify();
            }))
            .child(SharedString::from(label))
    };

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_lg()
                        .text_color(fg)
                        .child(SharedString::from(format!("Search: {query}"))),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(tab_button(
                            "search-tracks",
                            format!("Tracks ({n_tracks})"),
                            SearchTab::Tracks,
                            tab == SearchTab::Tracks,
                        ))
                        .child(tab_button(
                            "search-albums",
                            format!("Albums ({n_albums})"),
                            SearchTab::Albums,
                            tab == SearchTab::Albums,
                        ))
                        .child(tab_button(
                            "search-artists",
                            format!("Artists ({n_artists})"),
                            SearchTab::Artists,
                            tab == SearchTab::Artists,
                        )),
                ),
        )
        .child(if count == 0 {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("No results on this tab")
                .into_any_element()
        } else {
            uniform_list(
                "search-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
                    let tab = this.state.ui.active_search_tab;
                    let selected = this.state.ui.selected_search_index;

                    let rows: Vec<AnyElement> = range
                        .map(|ix| {
                            let row = div()
                                .id(ix)
                                .w_full()
                                .h(px(SIDEBAR_ROW_HEIGHT))
                                .px_4()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.08)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if let Some(event) =
                                            echo_core::intent::activate_search_result(
                                                &mut this.state,
                                                ix,
                                            )
                                        {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                    },
                                ));

                            match tab {
                                SearchTab::Tracks => {
                                    let track =
                                        this.state.data.search_results.tracks[ix].clone();
                                    row.child(
                                        div()
                                            .flex_grow(2.0)
                                            .flex_basis(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .text_color(fg)
                                            .child(SharedString::from(track.name)),
                                    )
                                    .child(
                                        div()
                                            .flex_grow(1.5)
                                            .flex_basis(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .text_color(muted)
                                            .child(SharedString::from(track.artist)),
                                    )
                                    .child(
                                        div()
                                            .flex_grow(1.5)
                                            .flex_basis(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .text_color(muted)
                                            .child(SharedString::from(track.album)),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(48.0))
                                            .text_color(muted)
                                            .child(SharedString::from(format_time(
                                                track.duration_ms,
                                            ))),
                                    )
                                    .into_any_element()
                                }
                                SearchTab::Albums => {
                                    let album =
                                        this.state.data.search_results.albums[ix].clone();
                                    let thumb = thumb_element(
                                        this,
                                        album.image_url.as_deref(),
                                        26.0,
                                        false,
                                        muted,
                                    );
                                    row.child(thumb)
                                        .child(
                                            div()
                                                .flex_grow(2.0)
                                                .flex_basis(px(0.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(fg)
                                                .child(SharedString::from(album.name)),
                                        )
                                        .child(
                                            div()
                                                .flex_grow(1.0)
                                                .flex_basis(px(0.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(muted)
                                                .child(SharedString::from(album.artist)),
                                        )
                                        .into_any_element()
                                }
                                SearchTab::Artists => {
                                    let artist =
                                        this.state.data.search_results.artists[ix].clone();
                                    let thumb = thumb_element(
                                        this,
                                        artist.image_url.as_deref(),
                                        26.0,
                                        true,
                                        muted,
                                    );
                                    row.child(thumb)
                                        .child(
                                            div()
                                                .flex_grow(1.0)
                                                .flex_basis(px(0.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(fg)
                                                .child(SharedString::from(artist.name)),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(SharedString::from(format!(
                                                    "{} followers",
                                                    artist.followers
                                                ))),
                                        )
                                        .into_any_element()
                                }
                            }
                        })
                        .collect();

                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
                    rows
                }),
            )
            .track_scroll(&app.search_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

fn artist_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());

    let count = app.state.data.followed_artists.len();

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .px_4()
                .py_3()
                .flex()
                .flex_col()
                .child(div().text_lg().text_color(fg).child("Followed artists"))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(format!("{count} artists"))),
                ),
        )
        .child(if count == 0 {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("Loading artists…")
                .into_any_element()
        } else {
            uniform_list(
                "artist-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
                    let selected = this.state.ui.selected_artist_index;

                    let rows: Vec<AnyElement> = range
                        .map(|ix| {
                            let artist = this.state.data.followed_artists[ix].clone();
                            let thumb = thumb_element(
                                this,
                                artist.image_url.as_deref(),
                                26.0,
                                true,
                                muted,
                            );
                            div()
                                .id(ix)
                                .w_full()
                                .h(px(SIDEBAR_ROW_HEIGHT))
                                .px_4()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.08)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if let Some(event) =
                                            echo_core::intent::open_followed_artist(
                                                &mut this.state,
                                                ix,
                                            )
                                        {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(thumb)
                                .child(
                                    div()
                                        .flex_grow(1.0)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(fg)
                                        .child(SharedString::from(artist.name)),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(SharedString::from(format!(
                                            "{} followers",
                                            artist.followers
                                        ))),
                                )
                                .into_any_element()
                        })
                        .collect();

                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
                    rows
                }),
            )
            .track_scroll(&app.artists_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

fn artist_page(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());

    let Some(data) = app.state.data.artist_page_data.clone() else {
        return div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child("Loading artist…")
            .into_any_element();
    };

    let header_image = app
        .state
        .ui
        .active_library_header_image
        .clone()
        .and_then(|artwork| app.images.get(&artwork));
    let count = data.albums.len();
    let loading = app.state.data.artist_albums_loading && count == 0;

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .px_4()
                .py_3()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .id("artist-back")
                        .flex_none()
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .text_color(muted)
                        .hover(|style| style.bg(muted.opacity(0.15)))
                        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                            this.close_artist_page(cx);
                        }))
                        .child(
                            svg()
                                .path("icons/arrow-left.svg")
                                .w(px(16.0))
                                .h(px(16.0))
                                .text_color(muted),
                        ),
                )
                .when_some(header_image, |el, image| {
                    el.child(
                        img(image)
                            .flex_none()
                            .w(px(72.0))
                            .h(px(72.0))
                            .rounded_full(),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(
                            div()
                                .text_lg()
                                .text_color(fg)
                                .child(SharedString::from(data.artist_name.clone())),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(format!("{count} albums"))),
                        ),
                ),
        )
        .child(if loading {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child("Loading albums…")
                .into_any_element()
        } else {
            uniform_list(
                "artist-album-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
                    let selected = this.state.ui.artist_page_album_index;

                    let rows: Vec<AnyElement> = range
                        .map(|ix| {
                            let Some(album) = this
                                .state
                                .data
                                .artist_page_data
                                .as_ref()
                                .and_then(|data| data.albums.get(ix))
                                .cloned()
                            else {
                                return div().id(ix).into_any_element();
                            };
                            let thumb_url =
                                album.thumb_url.clone().or_else(|| album.image_url.clone());
                            let thumb =
                                thumb_element(this, thumb_url.as_deref(), 26.0, false, muted);
                            let tracks_label = album
                                .track_count
                                .map(|n| format!("{n} tracks"))
                                .unwrap_or_default();
                            div()
                                .id(ix)
                                .w_full()
                                .h(px(SIDEBAR_ROW_HEIGHT))
                                .px_4()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.08)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if let Some(event) = echo_core::intent::open_artist_album(
                                            &mut this.state,
                                            ix,
                                        ) {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(thumb)
                                .child(
                                    div()
                                        .flex_grow(1.0)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(fg)
                                        .child(SharedString::from(album.name)),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(SharedString::from(album.release_year)),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(70.0))
                                        .text_xs()
                                        .text_color(muted)
                                        .child(SharedString::from(tracks_label)),
                                )
                                .into_any_element()
                        })
                        .collect();

                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
                    rows
                }),
            )
            .track_scroll(&app.artist_albums_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
        .into_any_element()
}

/// The lyrics overlay: the current line (by playback position) highlighted and kept centered.
pub fn lyrics_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());

    let body = if app.state.playback.is_fetching_lyrics {
        div()
            .py_8()
            .flex()
            .justify_center()
            .text_color(muted)
            .child("Loading lyrics…")
            .into_any_element()
    } else if let Some(lyrics) = app.state.playback.current_lyrics.clone() {
        let progress_ms = app.state.playback.display_progress_ms();
        let current = lyrics
            .lines
            .iter()
            .take_while(|line| line.start_ms <= progress_ms)
            .count()
            .saturating_sub(1);
        let count = lyrics.lines.len();
        app.lyrics_scroll
            .scroll_to_item(current, gpui::ScrollStrategy::Center);

        uniform_list(
            "lyric-lines",
            count,
            cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, _cx| {
                let theme = &this.state.ui.active_theme;
                let fg = theme.text.gpui(WINDOW_FG());
                let muted = theme.text_muted.gpui(WINDOW_FG());
                let accent = theme.primary.gpui(WINDOW_FG());
                let progress_ms = this.state.playback.display_progress_ms();
                let lines = this
                    .state
                    .playback
                    .current_lyrics
                    .as_ref()
                    .map(|lyrics| lyrics.lines.clone())
                    .unwrap_or_default();
                let current = lines
                    .iter()
                    .take_while(|line| line.start_ms <= progress_ms)
                    .count()
                    .saturating_sub(1);

                range
                    .map(|ix| {
                        let text = lines.get(ix).map(|line| line.text.clone()).unwrap_or_default();
                        let color = if ix == current {
                            accent
                        } else if ix > current {
                            muted
                        } else {
                            fg
                        };
                        div()
                            .id(ix)
                            .w_full()
                            .h(px(26.0))
                            .px_4()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_sm()
                            .text_color(color)
                            .child(SharedString::from(text))
                    })
                    .collect()
            }),
        )
        .track_scroll(&app.lyrics_scroll)
        .flex_grow(1.0)
        .into_any_element()
    } else {
        div()
            .py_8()
            .flex()
            .justify_center()
            .text_color(muted)
            .child("No lyrics for this track")
            .into_any_element()
    };

    div()
        .id("lyrics-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.state.ui.lyrics_modal_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("lyrics-panel")
                .w(px(520.0))
                .h(px(480.0))
                .rounded_lg()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .p_3()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .pb_2()
                        .text_sm()
                        .text_color(fg)
                        .child(SharedString::from(format!(
                            "Lyrics — {}",
                            app.state.playback.playing_track_title.clone()
                        ))),
                )
                .child(body),
        )
}

/// The theme picker: every loaded theme by name, the active one accented.
pub fn theme_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let mut names: Vec<String> = app.state.ui.themes.keys().cloned().collect();
    names.sort();
    let active = app.state.ui.library_config.active_theme.clone();

    div()
        .id("theme-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.theme_modal_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("theme-panel")
                .w(px(320.0))
                .max_h(px(420.0))
                .rounded_lg()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(div().pb_2().text_sm().text_color(muted).child("Theme"))
                .child(if names.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child("No themes found")
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(names.into_iter().enumerate().map(|(ix, name)| {
                            let is_active = active.as_deref() == Some(name.as_str());
                            let color = if is_active { accent } else { fg };
                            let clicked = name.clone();
                            div()
                                .id(ix)
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .text_sm()
                                .text_color(color)
                                .hover(|style| style.bg(muted.opacity(0.1)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        echo_core::intent::apply_theme(&mut this.state, &clicked);
                                        this.theme_modal_open = false;
                                        cx.notify();
                                    },
                                ))
                                .child(SharedString::from(name))
                                .when(is_active, |el| {
                                    el.child(div().flex_none().text_color(accent).child("●"))
                                })
                        }))
                        .into_any_element()
                }),
        )
}

/// The Spotify Connect device picker, painted over everything else.
pub fn device_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
    let selected = app.state.ui.selected_device_index;

    let devices = app.state.data.devices.clone();

    div()
        .id("device-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.state.ui.device_modal_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("device-panel")
                .w(px(400.0))
                .max_h(px(420.0))
                .rounded_lg()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .overflow_hidden()
                // Clicks on the panel itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .pb_2()
                        .text_sm()
                        .text_color(muted)
                        .child("Connect to a device"),
                )
                .child(if devices.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child("No devices found — is Spotify open anywhere?")
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(devices.into_iter().enumerate().map(|(ix, device)| {
                            let name_color = if device.is_active { accent } else { fg };
                            div()
                                .id(ix)
                                .px_2()
                                .py_2()
                                .rounded_md()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.1)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if let Some(event) = echo_core::intent::transfer_to_device(
                                            &mut this.state,
                                            ix,
                                        ) {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(
                                    div()
                                        .flex_grow(1.0)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(name_color)
                                        .child(SharedString::from(device.name.clone())),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(SharedString::from(device.device_type.clone())),
                                )
                                .when(device.is_active, |el| {
                                    el.child(div().flex_none().text_color(accent).child("●"))
                                })
                        }))
                        .into_any_element()
                }),
        )
}

/// Add-to-playlist picker over the choices both frontends share (own Spotify playlists plus
/// local ones); Enter and row clicks resolve through `action_menu::commit_playlist_add`.
pub fn playlist_add_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let selected_bg = theme.highlight_bg.gpui(WINDOW_FG()).opacity(0.2);
    let selected = app.state.ui.selected_playlist_modal_index;

    // Materialized before the element closures capture anything from `app`.
    let choices: Vec<(SharedString, bool)> =
        echo_core::action_menu::playlist_add_choices(&app.state)
            .into_iter()
            .map(|playlist| {
                let local = playlist.owner_id == "local";
                (SharedString::from(playlist.name), local)
            })
            .collect();

    div()
        .id("playlist-add-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            echo_core::action_menu::cancel_playlist_add(&mut this.state);
            cx.notify();
        }))
        .child(
            div()
                .id("playlist-add-panel")
                .w(px(400.0))
                .max_h(px(420.0))
                .rounded_lg()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .overflow_hidden()
                // Clicks on the panel itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .pb_2()
                        .text_sm()
                        .text_color(muted)
                        .child("Add to playlist"),
                )
                .child(if choices.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child("No playlists you can edit")
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .children(choices.into_iter().enumerate().map(|(ix, (name, local))| {
                            div()
                                .id(ix)
                                .px_2()
                                .py_2()
                                .rounded_md()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .when(ix == selected, |el| el.bg(selected_bg))
                                .hover(|style| style.bg(muted.opacity(0.1)))
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if let Some(event) =
                                            echo_core::action_menu::commit_playlist_add(
                                                &mut this.state,
                                                ix,
                                            )
                                        {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                    },
                                ))
                                .child(
                                    div()
                                        .flex_grow(1.0)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(fg)
                                        .child(name),
                                )
                                .when(local, |el| {
                                    el.child(
                                        div().flex_none().text_xs().text_color(muted).child("Local"),
                                    )
                                })
                        }))
                        .into_any_element()
                }),
        )
}

/// The main-area empty state: the TUI's ECHO wordmark with a vertical secondary→primary
/// gradient, or plain status text while authenticating.
fn library_placeholder(app: &EchoApp) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let muted = theme.text_muted.gpui(WINDOW_FG());

    if matches!(app.state.ui.mode, AppMode::Setup | AppMode::Authenticating) {
        let message: SharedString = match app.state.ui.mode {
            AppMode::Setup => "Waiting for Spotify credentials…".into(),
            _ => "Authenticating with Spotify… complete the login in your browser".into(),
        };
        return div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child(message)
            .into_any_element();
    }

    let secondary = theme.secondary.gpui(WINDOW_FG());
    let primary = theme.primary.gpui(WINDOW_FG());
    let playlists = app.state.data.playlists.len();
    let albums = app.state.data.saved_albums.len();
    let counts: SharedString = format!("{playlists} playlists · {albums} albums").into();

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .font_family("Consolas")
                .text_sm()
                .line_height(relative(1.0))
                .children(ECHO_LOGO.iter().enumerate().map(|(index, line)| {
                    let t = index as f32 / (ECHO_LOGO.len() - 1) as f32;
                    div()
                        .whitespace_nowrap()
                        .text_color(lerp_hsla(secondary, primary, t))
                        .child(*line)
                })),
        )
        .child(div().text_xs().text_color(muted).child(counts))
        .into_any_element()
}

fn lerp_hsla(a: gpui::Hsla, b: gpui::Hsla, t: f32) -> gpui::Hsla {
    let a: gpui::Rgba = a.into();
    let b: gpui::Rgba = b.into();
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
    .into()
}

/// The sidebar right-click menu: items depend on what the row is, actions run through
/// [`EchoApp::run_menu_action`]. Destructive items stage a prompt for [`prompt_modal`].
pub fn context_menu(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let menu = app
        .context_menu
        .clone()
        .expect("context_menu rendered without state");
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let mut items: Vec<(SharedString, MenuAction, bool)> = Vec::new();
    if app.state.ui.active_library_tab == LibraryTab::Albums {
        if app.state.data.saved_albums.get(menu.index).is_some() {
            items.push(("Open".into(), MenuAction::Open, false));
            items.push((
                "Remove from library".into(),
                MenuAction::RemoveAlbum,
                true,
            ));
        }
    } else if let Some(node) = app.state.data.library_view.get(menu.index) {
        match node {
            LibraryNode::Folder(_) => {
                items.push(("Rename".into(), MenuAction::Rename, false));
                items.push(("Delete folder".into(), MenuAction::DeleteFolder, true));
            }
            LibraryNode::Playlist { playlist, indent } => {
                items.push(("Open".into(), MenuAction::Open, false));
                let special = playlist.id == "LIKED_SONGS" || playlist.id == "local-library";
                let local = playlist.id.starts_with("local-playlist:");
                if !special && !local {
                    let pinned = app.state.ui.library_config.pinned.contains(&playlist.id);
                    items.push((
                        if pinned { "Unpin" } else { "Pin" }.into(),
                        MenuAction::TogglePin,
                        false,
                    ));
                }
                if !special {
                    items.push(("Rename".into(), MenuAction::Rename, false));
                }
                if *indent >= 1 {
                    items.push((
                        "Remove from folder".into(),
                        MenuAction::RemoveFromFolder,
                        false,
                    ));
                }
                let own = app.state.data.user_id.as_ref() == Some(&playlist.owner_id);
                if local || (!special && own) {
                    items.push(("Delete playlist".into(), MenuAction::DeletePlaylist, true));
                }
            }
        }
    }

    let index = menu.index;
    div()
        .id("context-menu-backdrop")
        .absolute()
        .inset_0()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.context_menu = None;
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this: &mut EchoApp, _event: &MouseDownEvent, _window, cx| {
                this.context_menu = None;
                cx.notify();
            }),
        )
        .child(
            div()
                .id("context-menu")
                .absolute()
                .left(menu.position.x)
                .top(menu.position.y)
                .w(px(210.0))
                .rounded_md()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                // Clicks on the menu itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(items.into_iter().map(|(label, action, danger)| {
                    div()
                        .id(label.clone())
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(if danger { danger_color } else { fg })
                        .hover(move |style| style.bg(accent.opacity(0.12)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, window, cx| {
                            this.run_menu_action(action, index, window, cx);
                        }))
                        .child(label)
                })),
        )
}

/// Right-click menu for a track row. The item set and labels come from the shared action-menu
/// model (`ActionMenuContext::actions()` / `action_menu::label`), so it always matches the
/// TUI's `A` popup; remove-from-playlist is appended for modifiable playlists, where the TUI
/// uses `dd` instead.
pub fn track_context_menu(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let menu = app
        .track_menu
        .clone()
        .expect("track_context_menu rendered without state");
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let mut items: Vec<(SharedString, TrackMenuItem, bool)> = Vec::new();
    if let Some(track) = app.state.data.tracks.get(menu.index) {
        let ctx = echo_core::models::ActionMenuContext::from(track);
        for action in ctx.actions() {
            items.push((
                echo_core::action_menu::label(&app.state, &ctx, action).into(),
                TrackMenuItem::Action(action),
                false,
            ));
        }
        if app
            .state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.can_modify_playlist(app.state.data.user_id.as_ref()))
        {
            items.push((
                "Remove from playlist".into(),
                TrackMenuItem::RemoveFromPlaylist,
                true,
            ));
        }
    }

    let index = menu.index;
    div()
        .id("track-menu-backdrop")
        .absolute()
        .inset_0()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.track_menu = None;
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this: &mut EchoApp, _event: &MouseDownEvent, _window, cx| {
                this.track_menu = None;
                cx.notify();
            }),
        )
        .child(
            div()
                .id("track-menu")
                .absolute()
                .left(menu.position.x)
                .top(menu.position.y)
                .w(px(210.0))
                .rounded_md()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                // Clicks on the menu itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(items.into_iter().map(|(label, item, danger)| {
                    div()
                        .id(label.clone())
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(if danger { danger_color } else { fg })
                        .hover(move |style| style.bg(accent.opacity(0.12)))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            this.run_track_menu_action(item, index, cx);
                        }))
                        .child(label)
                })),
        )
}

/// Confirm dialog for whichever destructive prompt is staged, resolved through the same
/// `intent::confirm_prompt`/`cancel_prompt` the TUI's y/n keys use.
pub fn prompt_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let ui = &app.state.ui;
    let (message, confirm_label): (String, &'static str) =
        if let Some(name) = &ui.folder_delete_prompt {
            (
                format!("Delete folder “{name}”? Its playlists return to the library."),
                "Delete",
            )
        } else if let Some(ids) = &ui.playlist_delete_prompt {
            let name = ids
                .first()
                .and_then(|id| {
                    app.state.data.library_view.iter().find_map(|node| match node {
                        LibraryNode::Playlist { playlist, .. } if &playlist.id == id => {
                            Some(playlist.name.clone())
                        }
                        _ => None,
                    })
                })
                .unwrap_or_else(|| "this playlist".to_string());
            (
                format!("Delete “{name}”? This removes it from your library."),
                "Delete",
            )
        } else if let Some(ids) = &ui.album_mass_delete_prompt {
            let name = ids
                .first()
                .and_then(|id| {
                    app.state
                        .data
                        .saved_albums
                        .iter()
                        .find(|album| &album.id == id)
                        .map(|album| album.name.clone())
                })
                .unwrap_or_else(|| "this album".to_string());
            (format!("Remove “{name}” from your saved albums?"), "Remove")
        } else if ui.track_delete_prompt.is_some() {
            (
                "Remove the selected tracks from this playlist?".to_string(),
                "Remove",
            )
        } else {
            ("Remove this track from Liked Songs?".to_string(), "Remove")
        };

    let button = |id: &'static str, label: &'static str, color: gpui::Hsla| {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(color.opacity(0.5))
            .text_sm()
            .text_color(color)
            .cursor_pointer()
            .hover(move |style| style.bg(color.opacity(0.12)))
            .child(label)
    };

    div()
        .id("prompt-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            echo_core::intent::cancel_prompt(&mut this.state);
            cx.notify();
        }))
        .child(
            div()
                .id("prompt-panel")
                .w(px(380.0))
                .rounded_lg()
                .border_1()
                .border_color(muted.opacity(0.4))
                .bg(surface)
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(div().text_sm().text_color(fg).child(SharedString::from(message)))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .child(button("prompt-cancel", "Cancel", muted).on_click(cx.listener(
                            |this: &mut EchoApp, _event, _window, cx| {
                                echo_core::intent::cancel_prompt(&mut this.state);
                                cx.notify();
                            },
                        )))
                        .child(button("prompt-confirm", confirm_label, danger_color).on_click(
                            cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                                if let Some(event) =
                                    echo_core::intent::confirm_prompt(&mut this.state)
                                {
                                    this.dispatch(event);
                                }
                                cx.notify();
                            }),
                        )),
                ),
        )
}
