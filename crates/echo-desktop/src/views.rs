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

use echo_core::app::{ActiveView, AppMode, LibraryTab, QueueRow, SearchTab};
use echo_core::models::{ActionMenuAction, ActionMenuContext, LibraryNode};
use echo_core::thumbnails::ThumbState;
use gpui::{
    Animation, AnimationExt, AnyElement, Context, Div, Hsla, MouseButton, MouseDownEvent,
    SharedString, Stateful, Window, canvas, div, ease_out_quint, img, prelude::*, px, relative,
    svg, uniform_list,
};

use crate::backdrop::{Backdrop, ImmersiveColors};
use crate::theme::{DesktopPalette, ToGpui, WINDOW_FG};
use crate::{ControlColors, EchoApp, MenuAction, TrackMenuItem, UpdateState, format_time};

pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;
/// The add-to-playlist flyout's box. The row height is the measured height of one choice, used
/// only to keep the panel on screen — see [`playlist_submenu`].
const SUBMENU_WIDTH: f32 = 190.0;
const SUBMENU_MAX_H: f32 = 270.0;
const SUBMENU_ROW_HEIGHT: f32 = 31.0;
const THUMB_EDGE: f32 = 26.0;
// Native caption metrics: Windows titlebars are a fixed 32px, macOS gets a touch more.
const TITLEBAR_HEIGHT: f32 = if cfg!(target_os = "windows") { 32.0 } else { 34.0 };
// Zed's measured inset for the macOS traffic lights (71px, +1px window border).
const TRAFFIC_LIGHT_PADDING: f32 = 71.0;

/// The shape of a selectable list row: the fixed height `uniform_list` measures and the
/// shorter pill drawn inside it. Hover strength comes from the palette slot the caller
/// passes ([`DesktopPalette::row_hover`] for both the sidebar and the main
/// area). See [`pill_row`].
#[derive(Clone, Copy)]
pub(crate) struct PillMetrics {
    row_height: f32,
    pill_height: f32,
}

/// Sidebar rows, which hover a touch stronger than the main area.
const SIDEBAR_PILL: PillMetrics = PillMetrics {
    row_height: 34.0,
    pill_height: 30.0,
};
/// Main-area lists carrying a thumbnail column.
const LIST_PILL: PillMetrics = PillMetrics {
    row_height: 34.0,
    pill_height: 30.0,
};
/// The denser text-only lists — tracks and the queue.
const COMPACT_PILL: PillMetrics = PillMetrics {
    row_height: 30.0,
    pill_height: 26.0,
};

/// Whether row `ix` reads as selected: the cursor row, or any row inside a visual range.
/// The range shares the cursor's highlight so the selection looks like one block.
fn row_selected(ix: usize, selected: usize, visual: Option<(usize, usize)>) -> bool {
    ix == selected || visual.is_some_and(|(start, end)| ix >= start && ix <= end)
}

/// Resolves a translation in the configured language. Every user-facing desktop string goes
/// through this so `:lang` applies on the next frame, like the TUI. Missing keys fall back to
/// English inside [`echo_core::i18n::t`].
pub(crate) fn tr(state: &echo_core::app::AppState, key: &str) -> SharedString {
    SharedString::from(echo_core::i18n::t(key, &state.ui.library_config.language))
}

/// The visual range for one pane. The range is bare row indices measured against the active
/// view's cursor, so panes other than the one visual mode was entered in must ignore it.
fn visual_range_in(state: &echo_core::app::AppState, view: ActiveView) -> Option<(usize, usize)> {
    (state.ui.active_view == view)
        .then(|| state.get_visual_selection_range())
        .flatten()
}

/// A selectable list row, drawn as an inset rounded pill.
///
/// Every list in the app shares this shell. `uniform_list` measures a fixed row height, so the
/// pill takes its vertical inset from being shorter than the row rather than from a margin, which
/// would drift the list's item accounting; the horizontal inset is padding on the inert outer div,
/// leaving the pill's own padding free for per-list indents.
///
/// `build` decorates the pill — children, gaps, click handlers, drag sources. Hover is skipped on
/// the selected row: the hover tint is fainter than the selection colour, so it would otherwise
/// wash the selection out as the cursor passes over it.
fn pill_row(
    ix: usize,
    metrics: PillMetrics,
    selected: bool,
    selected_bg: Hsla,
    hover_bg: Hsla,
    build: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
) -> Div {
    let pill = div()
        .id(ix)
        .w_full()
        .h(px(metrics.pill_height))
        .px_2()
        .rounded_md()
        .flex()
        .flex_row()
        .items_center()
        .text_sm()
        .when(selected, |el| el.bg(selected_bg))
        .when(!selected, |el| el.hover(move |style| style.bg(hover_bg)))
        .cursor_pointer();

    div()
        .w_full()
        .h(px(metrics.row_height))
        .px_2()
        .flex()
        .items_center()
        .child(build(pill))
}

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

/// Drag payload for a track row being reordered within an owned playlist.
pub(crate) struct DraggedTrack {
    pub from: usize,
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

/// True for the sidebar rows drag-and-drop must not move (their position is fixed, mirroring
/// `echo_core::intent`'s own check).
fn is_fixed_library_row(id: &str) -> bool {
    id == "LIKED_SONGS" || id == "local-library"
}

/// The custom titlebar: themed like the rest of the window, draggable, with caption buttons
/// on Windows and Linux. macOS keeps its native traffic lights (the bar just insets past them)
/// and only needs the drag/double-click plumbing done in Rust; Windows gets drag, double-click
/// and snap layouts for free from the `HTCAPTION`/`HTMAXBUTTON` hit-tests.
///
/// Linux needs every one of those by hand. gpui's Linux backends leave
/// `on_hit_test_window_control` unimplemented, so `window_control_area` does nothing there and
/// the caption buttons have to call `minimize_window`/`zoom_window`/`remove_window` themselves.
/// Dragging uses the same mouse-down latch as macOS. This used to be skipped on Linux on the
/// assumption the window manager would draw its own bar, but GNOME/Mutter does not implement
/// xdg-decoration and forces client-side decorations, which left the window with no titlebar
/// and no way to move or resize it. See [`window_frame`] for the resize edges.
///
/// `immersive` swaps the bar's own colors for the backdrop's: no fill, so the backdrop shows
/// through, and its text and hover wash.
pub fn titlebar(
    app: &mut EchoApp,
    immersive: Option<&ImmersiveColors>,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let (fg, surface, hover) = match immersive {
        Some(colors) => (colors.text, gpui::transparent_black(), colors.wash),
        None => (
            theme.text.gpui(WINDOW_FG()),
            theme.surface.gpui(crate::theme::PANEL_BG()),
            DesktopPalette::resolve(theme).menu_hover,
        ),
    };
    let fullscreen = window.is_fullscreen();
    let maximized = window.is_maximized();
    let corners = client_corners(window);
    // The close button's hover fill reaches the window's top-right corner, so it rounds
    // itself too — DWM's radius on Windows 11, ours on a client-decorated Linux window.
    let close_radius = match corners {
        Some(tiling) => (!tiling.top && !tiling.right).then(|| px(CLIENT_CORNER_RADIUS)),
        None => (cfg!(target_os = "windows") && !maximized).then(|| px(WIN_CORNER_RADIUS)),
    };

    div()
        .id("titlebar")
        .window_control_area(gpui::WindowControlArea::Drag)
        .flex_none()
        .w_full()
        .h(px(TITLEBAR_HEIGHT))
        // The bar's own surface fill reaches the window's top corners, so it has to round them
        // itself — see `round_client_corners`.
        .map(|el| round_client_corners(el, corners, ClientCorners::Top))
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
        // (`app_owns_titlebar_drag`), and Linux has no titlebar hit-testing at all, so both do
        // it by hand — the Zed latch pattern: arm on mouse-down, and the first real move starts
        // the native window drag.
        .when(cfg!(any(target_os = "macos", target_os = "linux")), |el| {
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
        // The window manager's own menu (Move, Resize, Always on Top, …), which a server-side
        // titlebar would have offered on right-click.
        .when(cfg!(target_os = "linux"), |el| {
            el.on_mouse_down(MouseButton::Right, |event, window, _cx| {
                window.show_window_menu(event.position);
            })
        })
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(svg().path("icons/music-note.svg").size(px(14.0)).text_color(fg))
                .child(div().text_xs().text_color(fg).child("echo")),
        )
        .when(CAPTION_BUTTONS && !fullscreen, |el| {
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
                        hover,
                        None,
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
                        hover,
                        None,
                    ))
                    .child(caption_button(
                        "caption-close",
                        "icons/win-close.svg",
                        gpui::WindowControlArea::Close,
                        fg,
                        hover,
                        close_radius,
                    )),
            )
        })
}

/// Windows and Linux draw their own caption buttons; macOS uses the native traffic lights.
const CAPTION_BUTTONS: bool = cfg!(any(target_os = "windows", target_os = "linux"));

/// A caption button. On Windows there is no click handler: the `window_control_area` tag routes
/// the click through `WM_NCHITTEST`, and gpui + `DefWindowProc` do the minimize/maximize/close.
/// Linux ignores that tag (gpui's backends leave `on_hit_test_window_control` unimplemented),
/// so there the button drives the window directly. `occlude` is load-bearing on both — without
/// it the surrounding Drag hitbox wins the hit-test and the button is dead.
fn caption_button(
    id: &'static str,
    icon: &'static str,
    area: gpui::WindowControlArea,
    fg: gpui::Hsla,
    hover: gpui::Hsla,
    top_right_radius: Option<gpui::Pixels>,
) -> impl IntoElement {
    let close = matches!(area, gpui::WindowControlArea::Close);
    // The close button hovers Windows-red with a white glyph; the rest get a faint wash.
    let hover_bg: gpui::Hsla =
        if close { crate::theme::CLOSE_RED() } else { hover };
    div()
        .id(id)
        .group(id)
        .occlude()
        .window_control_area(area)
        .when(cfg!(target_os = "linux"), |el| {
            el.on_click(move |_event, window, cx| match area {
                gpui::WindowControlArea::Min => window.minimize_window(),
                gpui::WindowControlArea::Max => window.zoom_window(),
                // Not `remove_window`, which drops the window without running
                // `on_window_should_close` and would lose the saved window rectangle.
                gpui::WindowControlArea::Close => {
                    window.dispatch_action(Box::new(crate::Quit), cx)
                }
                gpui::WindowControlArea::Drag => {}
            })
        })
        .w(px(46.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .when_some(top_right_radius, |el, r| el.rounded_tr(r))
        .hover(|style| style.bg(hover_bg))
        .active(|style| style.bg(hover_bg))
        .child(
            svg()
                .path(icon)
                .size(px(10.0))
                .text_color(fg)
                .when(close, |el| el.group_hover(id, |style| style.text_color(gpui::white()))),
        )
}

/// Corner radius and drop-shadow depth for a client-decorated window, matching what GNOME and
/// Zed use so echo sits alongside them without looking off.
const CLIENT_CORNER_RADIUS: f32 = 10.0;
/// The radius DWM clips an unmaximized window to on Windows 11.
const WIN_CORNER_RADIUS: f32 = 8.0;
const CLIENT_SHADOW: f32 = 10.0;
/// Grab bands, sized to the transparent shadow margin so they sit beside the visible window
/// rather than over its content. The corners reach a little further in, as window managers' do.
const RESIZE_BAND: f32 = CLIENT_SHADOW;
const RESIZE_CORNER: f32 = CLIENT_SHADOW * 2.0;

/// Which corners of an element sit against the window's own, for [`round_client_corners`].
#[derive(Clone, Copy)]
pub(crate) enum ClientCorners {
    All,
    Top,
}

/// The window's tiling state when the app is drawing its own frame, `None` when the compositor
/// draws it and the corners are not ours to round. Resolve this once per render and hand it to
/// [`round_client_corners`] — the alternative is borrowing the window inside a style closure.
pub(crate) fn client_corners(window: &Window) -> Option<gpui::Tiling> {
    match window.window_decorations() {
        gpui::Decorations::Client { tiling } => Some(tiling),
        gpui::Decorations::Server => None,
    }
}

/// Rounds the corners an element shares with the window's.
///
/// gpui's content mask is a plain rectangle, so rounding a container does **not** clip what is
/// inside it: any child that paints an opaque background into a corner squares it off again.
/// That makes this a per-element job rather than one wrapper — here the root's background and
/// the titlebar's, which are the only two surfaces reaching a corner. An edge that is tiled is
/// flush against a screen or a neighbour, and stays square, as every other app's does.
pub(crate) fn round_client_corners<E: Styled>(
    el: E,
    corners: Option<gpui::Tiling>,
    which: ClientCorners,
) -> E {
    let radii = client_corner_radii(corners, which);
    el.rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .rounded_bl(radii.bottom_left)
        .rounded_br(radii.bottom_right)
}

/// The radii [`round_client_corners`] applies, for painting that bypasses styles.
pub(crate) fn client_corner_radii(
    corners: Option<gpui::Tiling>,
    which: ClientCorners,
) -> gpui::Corners<gpui::Pixels> {
    let Some(tiling) = corners else { return gpui::Corners::default() };
    let all = matches!(which, ClientCorners::All);
    let radius = |rounded: bool| if rounded { px(CLIENT_CORNER_RADIUS) } else { px(0.0) };
    gpui::Corners {
        top_left: radius(!tiling.top && !tiling.left),
        top_right: radius(!tiling.top && !tiling.right),
        bottom_left: radius(all && !tiling.bottom && !tiling.left),
        bottom_right: radius(all && !tiling.bottom && !tiling.right),
    }
}

/// Wraps the whole app in the frame a window manager would normally provide.
///
/// A compositor that does not implement xdg-decoration — GNOME/Mutter, notably, which is the
/// Ubuntu default — hands the window back as [`Decorations::Client`] and draws nothing itself:
/// no titlebar, no border, no shadow, and crucially no resize handles, leaving the window stuck
/// at its opening size with square edges. [`titlebar`] covers moving and the caption buttons;
/// this covers the frame, as a transparent margin around the app carrying the drop shadow, with
/// the resize strips laid into that margin so they grab beside the window rather than over its
/// content. Requesting client decorations already makes the surface transparent, so the rounded
/// corners show what is behind the window rather than a black notch.
///
/// A pass-through everywhere else: Windows, macOS, and any Linux WM that does draw its own
/// decorations all report [`Decorations::Server`], and then the real frame already works.
pub fn window_frame(
    root: impl IntoElement,
    window: &mut Window,
    palette: DesktopPalette,
) -> AnyElement {
    let Some(tiling) = client_corners(window) else {
        return root.into_any_element();
    };
    let shadow = px(CLIENT_SHADOW);
    // Tells the compositor the visible window is inset from the surface by the shadow margin, so
    // snapping and edge detection use the frame the user sees rather than the transparent one.
    window.set_client_inset(shadow);
    let resizable = window.is_resizable();

    // Edges are laid down first and corners on top, so the corners win where they overlap.
    let edges: [(&'static str, gpui::ResizeEdge, bool); 8] = [
        ("resize-top", gpui::ResizeEdge::Top, tiling.top),
        ("resize-bottom", gpui::ResizeEdge::Bottom, tiling.bottom),
        ("resize-left", gpui::ResizeEdge::Left, tiling.left),
        ("resize-right", gpui::ResizeEdge::Right, tiling.right),
        ("resize-top-left", gpui::ResizeEdge::TopLeft, tiling.top || tiling.left),
        ("resize-top-right", gpui::ResizeEdge::TopRight, tiling.top || tiling.right),
        ("resize-bottom-left", gpui::ResizeEdge::BottomLeft, tiling.bottom || tiling.left),
        ("resize-bottom-right", gpui::ResizeEdge::BottomRight, tiling.bottom || tiling.right),
    ];

    div()
        .relative()
        .size_full()
        // The transparent margin: room for the shadow to fall into, and where the grab strips
        // live. A tiled edge has neither, so the window still meets the screen edge exactly.
        .when(!tiling.top, |el| el.pt(shadow))
        .when(!tiling.bottom, |el| el.pb(shadow))
        .when(!tiling.left, |el| el.pl(shadow))
        .when(!tiling.right, |el| el.pr(shadow))
        .child(
            div()
                .size_full()
                .map(|el| round_client_corners(el, Some(tiling), ClientCorners::All))
                // A hairline outline stands in for the frame the compositor is not drawing, so
                // the window still reads as one against a same-coloured background behind it.
                .border_1()
                .border_color(palette.border)
                .when(!tiling.is_tiled(), |el| {
                    el.shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(2.0), gpui::hsla(0.0, 0.0, 0.0, 0.36))
                            .blur_radius(shadow / 2.0),
                    ])
                })
                .child(root),
        )
        .children(edges.into_iter().filter_map(|(id, edge, tiled)| {
            // A tiled edge is flush against a screen or neighbour and cannot be dragged.
            (resizable && !tiled).then(|| resize_handle(id, edge))
        }))
        .into_any_element()
}

/// One transparent grab strip, positioned against the outer edge of the shadow margin. `occlude`
/// keeps it above the app's own hitboxes, which matters at the corners, where the square reaches
/// past the margin into the window.
fn resize_handle(id: &'static str, edge: gpui::ResizeEdge) -> impl IntoElement {
    use gpui::ResizeEdge as E;
    let band = px(RESIZE_BAND);
    let corner = px(RESIZE_CORNER);

    div()
        .id(id)
        .occlude()
        .absolute()
        .cursor(match edge {
            E::Top | E::Bottom => gpui::CursorStyle::ResizeUpDown,
            E::Left | E::Right => gpui::CursorStyle::ResizeLeftRight,
            E::TopLeft | E::BottomRight => gpui::CursorStyle::ResizeUpLeftDownRight,
            E::TopRight | E::BottomLeft => gpui::CursorStyle::ResizeUpRightDownLeft,
        })
        .map(|el| match edge {
            E::Top => el.top_0().left_0().w_full().h(band),
            E::Bottom => el.bottom_0().left_0().w_full().h(band),
            E::Left => el.top_0().left_0().h_full().w(band),
            E::Right => el.top_0().right_0().h_full().w(band),
            E::TopLeft => el.top_0().left_0().w(corner).h(corner),
            E::TopRight => el.top_0().right_0().w(corner).h(corner),
            E::BottomLeft => el.bottom_0().left_0().w(corner).h(corner),
            E::BottomRight => el.bottom_0().right_0().w(corner).h(corner),
        })
        .on_mouse_down(MouseButton::Left, move |_event, window, _cx| {
            window.start_window_resize(edge);
        })
}

fn nav_button_cluster(app: &EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let can_back = !app.state.ui.view_history.is_empty();
    let can_forward = !app.state.ui.forward_history.is_empty();

    let history_button = |id: &'static str,
                          icon: &'static str,
                          enabled: bool,
                          cx: &mut Context<EchoApp>,
                          go: fn(&mut EchoApp, &mut Context<EchoApp>)| {
        if enabled {
            crate::icon_button(id, icon, muted, palette.wash, cx, go).into_any_element()
        } else {
            // `border` rather than `wash`: at icon-glyph size the 15% wash all but vanishes.
            crate::icon_button(id, icon, palette.border, gpui::transparent_black(), cx, |_, _| {})
                .into_any_element()
        }
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(crate::icon_button(
            "sidebar-toggle",
            "icons/sidebar-left.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| this.toggle_sidebar(cx),
        ))
        .child(history_button(
            "history-back",
            "icons/arrow-left.svg",
            can_back,
            cx,
            |this, cx| {
                this.history_back(cx);
            },
        ))
        .child(history_button(
            "history-forward",
            "icons/arrow-right.svg",
            can_forward,
            cx,
            |this, cx| {
                this.history_forward(cx);
            },
        ))
}

pub fn sidebar(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let tab = app.state.ui.active_library_tab;
    let count = match tab {
        LibraryTab::Albums => app.state.data.saved_albums.len(),
        LibraryTab::Artists => app.state.data.followed_artists.len(),
        _ => app.state.data.library_view.len(),
    };

    let nav_row = div()
        .flex()
        .flex_row()
        .items_center()
        .px_2()
        .pt_2()
        .child(nav_button_cluster(app, cx));

    let tab_button = |id: &'static str, label: SharedString, target: LibraryTab, active: bool| {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .text_color(if active { accent } else { muted })
            .hover(move |style| style.bg(palette.row_hover))
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
        .border_color(palette.border)
        .child(nav_row)
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .p_2()
                .child(tab_button(
                    "Playlists",
                    tr(&app.state, "ui.playlists"),
                    LibraryTab::Playlists,
                    tab == LibraryTab::Playlists,
                ))
                .child(tab_button(
                    "Albums",
                    tr(&app.state, "ui.albums"),
                    LibraryTab::Albums,
                    tab == LibraryTab::Albums,
                ))
                .child(tab_button(
                    "Artists",
                    tr(&app.state, "ui.artists"),
                    LibraryTab::Artists,
                    tab == LibraryTab::Artists,
                )),
        )
        .child({
            // The TUI's Browse nodes, as quick links.
            let browse_link = |id: &'static str,
                               icon: &'static str,
                               label: SharedString,
                               open: fn(&mut echo_core::app::AppState)
                                   -> Option<echo_core::events::AppEvent>| {
                div()
                    .id(id)
                    .mx_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(muted)
                    .hover(move |style| style.bg(palette.row_hover))
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
                    tr(&app.state, "desktop.top_tracks"),
                    echo_core::intent::open_top_tracks,
                ))
                .child(browse_link(
                    "recently-played",
                    "icons/clock.svg",
                    tr(&app.state, "desktop.recently_played"),
                    echo_core::intent::open_recently_played,
                ))
                .child(browse_link(
                    "top-artists",
                    "icons/mic.svg",
                    tr(&app.state, "desktop.top_artists"),
                    echo_core::intent::open_top_artists,
                ))
                .child(browse_link(
                    "whats-new",
                    "icons/sparkles.svg",
                    tr(&app.state, "desktop.whats_new"),
                    echo_core::intent::open_whats_new,
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
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = if this.state.ui.active_view == ActiveView::Library {
                        palette.row_selected
                    } else {
                        palette.row_hover
                    };
                    let panel_bg = theme.surface.gpui(crate::theme::PANEL_BG());
                    let tab = this.state.ui.active_library_tab;
                    let selected = this.state.ui.selected_playlist_index;
                    let visual = visual_range_in(&this.state, ActiveView::Library);

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
                                            .bg(palette.wash)
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

                            // Local playlists look like Spotify ones in the sidebar, so they are
                            // tagged to keep the distinction visible.
                            let local_badge: Option<SharedString> = (tab
                                == LibraryTab::Playlists
                                && matches!(
                                    &this.state.data.library_view[ix],
                                    LibraryNode::Playlist { playlist, .. }
                                        if playlist.id.starts_with("local-playlist:")
                                ))
                            .then(|| tr(&this.state, "desktop.local_badge"));

                            // Playlists can be dragged between folders, the pinned block and the
                            // loose list; every playlist-tab row is a drop target (the intent
                            // rejects invalid ones).
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

                            pill_row(ix, SIDEBAR_PILL, row_selected(ix, selected, visual), selected_bg, palette.row_hover, |row| {
                                row.pl(px(8.0 + indent_px))
                                .gap_2()
                                .text_color(label_color)
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
                                    let border = palette.menu_border;
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
                                        style.bg(palette.drag_target)
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
                                .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                    this.state.ui.selected_playlist_index = ix;
                                    let tab = this.state.ui.active_library_tab;
                                    // The first click of a double-click already opened the
                                    // entry; the second one starts it playing.
                                    if event.click_count() >= 2 {
                                        let play = match tab {
                                            LibraryTab::Albums => {
                                                echo_core::intent::play_album_at(&mut this.state, ix)
                                            }
                                            LibraryTab::Artists => None,
                                            _ => echo_core::intent::play_library_entry(
                                                &mut this.state,
                                                ix,
                                            ),
                                        };
                                        if let Some(event) = play {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    let event = match tab {
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
                                .when_some(local_badge, |el, badge| {
                                    el.child(
                                        div()
                                            .ml_auto()
                                            .flex_none()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(badge),
                                    )
                                })
                            })
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
                .hover(move |style| style.bg(palette.drag_target))
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
        ActiveView::WhatsNew => whats_new(app, cx).into_any_element(),
            _ if app.state.ui.mode != AppMode::Authenticating
                && (app.state.data.active_tracklist_context.is_some()
                    || !app.state.data.tracks.is_empty()) =>
            {
                track_list(app, cx).into_any_element()
            }
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
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let id_focused = app.setup_id_focus.is_focused(window);
    let secret_focused = app.setup_secret_focus.is_focused(window);
    let client_id = app.state.ui.setup_client_id.clone();
    let secret_masked = "•".repeat(app.state.ui.setup_client_secret.chars().count());
    let ready =
        !app.state.ui.setup_client_id.is_empty() && !app.state.ui.setup_client_secret.is_empty();
    let uri_copied = app.setup_uri_copied;

    let field = |id: &'static str,
                 label: SharedString,
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
            .border_color(if focused { accent } else { palette.border })
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
                .border_color(palette.border)
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_lg().text_color(fg).child(tr(&app.state, "desktop.setup.title")))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.setup.step1"))
                        .child(tr(&app.state, "desktop.setup.step2"))
                        .child(
                            div()
                                .id("setup-redirect-uri")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.wash)
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .cursor_pointer()
                                .hover(|style| style.border_color(accent))
                                .on_click(cx.listener(
                                    |this: &mut EchoApp, _event, _window, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            echo_core::worker::api::REDIRECT_URI.to_string(),
                                        ));
                                        this.setup_uri_copied = true;
                                        cx.notify();
                                    },
                                ))
                                .child(
                                    div()
                                        .text_color(fg)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(echo_core::worker::api::REDIRECT_URI),
                                )
                                .child(div().text_xs().text_color(if uri_copied {
                                    accent
                                } else {
                                    muted
                                }).child(tr(
                                    &app.state,
                                    if uri_copied {
                                        "desktop.setup.copied"
                                    } else {
                                        "desktop.setup.copy"
                                    },
                                ))),
                        )
                        .child(tr(&app.state, "desktop.setup.step3")),
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
                        .child(tr(&app.state, "desktop.setup.dashboard")),
                )
                .child(field(
                    "setup-client-id",
                    tr(&app.state, "desktop.setup.client_id"),
                    client_id,
                    id_focused,
                    false,
                    app.setup_id_focus.clone(),
                ))
                .child(field(
                    "setup-client-secret",
                    tr(&app.state, "desktop.setup.client_secret"),
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
                            palette.wash
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
                        .child(tr(&app.state, "desktop.setup.save")),
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
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let focused = app.search_focus.is_focused(window);
    let query = app.search_input.clone();
    let collapsed = app.sidebar_collapsed;
    let nav_cluster = collapsed.then(|| nav_button_cluster(app, cx).into_any_element());

    div()
        .flex_none()
        // pl_2/pt_2 mirror the sidebar header's metrics so the button cluster lands on the
        // same pixels whether it renders here (collapsed) or in the sidebar (expanded).
        .pl_2()
        .pr_4()
        .pt_2()
        .flex()
        .flex_row()
        .items_center()
        // With the sidebar collapsed its button cluster moves here; otherwise an invisible
        // stand-in balancing the right-side buttons (104 = immersive + themes + settings +
        // pr_4 - pl_2) so the search box centers exactly.
        .map(|el| match nav_cluster {
            Some(cluster) => el.child(cluster),
            None => el.child(div().flex_none().w(px(104.0))),
        })
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
            .border_color(if focused { accent } else { palette.border })
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
                    .child(tr(&app.state, "desktop.search_placeholder"))
                    .into_any_element()
            } else {
                div()
                    .text_sm()
                    .text_color(fg)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(SharedString::from(if focused {
                        crate::text_with_cursor(&query, app.search_cursor)
                    } else {
                        query
                    }))
                    .into_any_element()
            }),
        )
        .child(div().flex_grow(1.0))
        .child(immersive_button(muted, palette.wash, cx))
        .child(crate::icon_button(
            "themes",
            "icons/paint-board.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| this.toggle_themes(cx),
        ))
        .child(crate::icon_button(
            "settings",
            "icons/settings.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| this.toggle_settings(cx),
        ))
}

pub const SORT_OPTIONS: &[(&str, &str)] = &[
    ("desktop.sort.original", "original"),
    ("desktop.sort.title", "title"),
    ("ui.artist", "artist"),
    ("ui.album", "album"),
    ("ui.duration", "duration"),
    ("desktop.sort.added", "added"),
    ("desktop.sort.reverse", "reverse"),
];

fn sort_button(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    crate::icon_button(
        "track-sort",
        "icons/arrow-down.svg",
        muted,
        palette.wash,
        cx,
        |this, cx| {
            this.sort_menu_open = !this.sort_menu_open;
            cx.notify();
        },
    )
}

/// The sort picker, anchored under the track-list header's sort button.
pub fn sort_menu(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let active = app.state.ui.track_sort;
    let selected = app.sort_menu_index;

    div()
        .id("sort-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.sort_menu_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("sort-menu")
                .absolute()
                .right(px(16.0))
                .top(px(96.0))
                .w(px(180.0))
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(SORT_OPTIONS.iter().enumerate().map(|(ix, (label_key, arg))| {
                    let label = tr(&app.state, label_key);
                    // `reverse` flips the current order rather than naming one, so it never
                    // shows the active marker.
                    let is_active = *arg != "reverse" && crate::sort_arg(active) == *arg;
                    let is_selected = ix == selected;
                    div()
                        .id(*arg)
                        .mx_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .text_sm()
                        .text_color(if is_active { accent } else { fg })
                        .when(is_selected, |el| el.bg(palette.menu_selected))
                        .when(!is_selected, |el| {
                            el.hover(move |style| style.bg(palette.menu_hover))
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            this.apply_sort(arg, cx);
                        }))
                        .child(label)
                        .when(is_active, |el| {
                            el.child(div().flex_none().text_color(accent).child("●"))
                        })
                })),
        )
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
    let is_top_tracks = app
        .state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|context| context.id == "TOP_TRACKS");

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
                        .flex_grow(1.0)
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
                )
                .when(is_top_tracks, |el| el.child(range_switcher(app, cx)))
                .child(sort_button(app, cx)),
        )
        .child(if loading {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child(tr(&app.state, "desktop.loading"))
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
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = if this.state.ui.active_view == ActiveView::TrackList {
                        palette.row_selected
                    } else {
                        palette.row_hover
                    };
                    let selected = this.state.ui.selected_track_index;
                    let visual = visual_range_in(&this.state, ActiveView::TrackList);
                    let playing_id = this.state.playback.playing_track_id.clone();
                    let secondary = theme.secondary.gpui(WINDOW_FG());
                    // On an album page every row shares the album in the header, so the
                    // column carries no information.
                    let in_album = this
                        .state
                        .data
                        .active_tracklist_context
                        .as_ref()
                        .is_some_and(|context| context.is_album());
                    // Inside Liked Songs every track is liked, so the hearts say nothing.
                    let in_liked_songs = this
                        .state
                        .data
                        .active_tracklist_context
                        .as_ref()
                        .is_some_and(|context| context.id == "LIKED_SONGS");
                    // Rows are drag-reorderable only where a reorder can be applied: a
                    // playlist the user can modify, shown in original order.
                    let reorderable = this.state.ui.track_sort
                        == echo_core::app::TrackSort::Original
                        && this
                            .state
                            .data
                            .active_tracklist_context
                            .as_ref()
                            .is_some_and(|context| {
                                context.can_modify_playlist(this.state.data.user_id.as_ref())
                            });
                    let panel_bg = theme.surface.gpui(crate::theme::PANEL_BG());

                    range
                        .map(|ix| {
                            let track = &this.state.data.tracks[ix];
                            let is_liked = this.state.data.liked_tracks.contains(&track.id);
                            let is_playing = playing_id.as_deref() == Some(track.id.as_str());
                            let title_color = if is_playing { accent } else { fg };
                            let drag_name = SharedString::from(track.name.clone());

                            pill_row(ix, COMPACT_PILL, row_selected(ix, selected, visual), selected_bg, palette.row_hover, |row| {
                                row.gap_3()
                                .when(reorderable, |row| {
                                    let border = palette.menu_border;
                                    row.on_drag(
                                        DraggedTrack { from: ix, name: drag_name },
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
                                    .drag_over::<DraggedTrack>(move |style, _, _, _| {
                                        style.bg(palette.drag_target)
                                    })
                                    .on_drop(cx.listener(
                                        move |this: &mut EchoApp, drag: &DraggedTrack, _window, cx| {
                                            if let Some(event) =
                                                echo_core::intent::move_track_in_playlist(
                                                    &mut this.state,
                                                    drag.from,
                                                    ix,
                                                )
                                            {
                                                this.dispatch(event);
                                            }
                                            cx.notify();
                                        },
                                    ))
                                })
                                .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                    // Shift-click selects from the previously focused row to
                                    // this one — the desktop's version of `v` plus motion.
                                    if event.modifiers().shift {
                                        this.extend_selection_to(ix, cx);
                                        return;
                                    }
                                    this.state.ui.selected_track_index = ix;
                                    this.state.ui.active_view = ActiveView::TrackList;
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
                                        if let Some(ctx) = this.action_target() {
                                            this.track_menu = Some(crate::TrackMenuState {
                                                ctx,
                                                position: Some(event.position),
                                                selected: 0,
                                                submenu: None,
                                            });
                                        }
                                        cx.notify();
                                    }),
                                )
                                .when_some(row_number(&this.state, ix, selected), |row, number| {
                                    row.child(
                                        div()
                                            .flex_none()
                                            .w(px(32.0))
                                            .text_color(muted)
                                            .child(number),
                                    )
                                })
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
                                .child({
                                    let base = div()
                                        .flex_grow(1.5)
                                        .flex_basis(px(0.0))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_color(muted);
                                    if track.artists.is_empty() {
                                        // Local files and pre-credit caches carry only the
                                        // joined string: one span, go-to-artist fallback.
                                        base.id(("track-artist", ix))
                                            .text_ellipsis()
                                            .cursor_pointer()
                                            .hover(|style| style.underline())
                                            .on_click(cx.listener(move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
                                                cx.stop_propagation();
                                                if let Some(ctx) = this.state.data.tracks.get(ix).map(ActionMenuContext::from) {
                                                    this.go_to(ctx, ActionMenuAction::GoToArtist, cx);
                                                }
                                            }))
                                            .child(SharedString::from(track.artist.clone()))
                                            .into_any_element()
                                    } else {
                                        let mut cell = base.flex().flex_row().items_center();
                                        for (k, credit) in track.artists.iter().enumerate() {
                                            if k > 0 {
                                                cell = cell.child(div().flex_none().child(SharedString::from(", ")));
                                            }
                                            let name = SharedString::from(credit.name.clone());
                                            cell = cell.child(match credit.id.clone() {
                                                Some(id) => {
                                                    let artist_name = credit.name.clone();
                                                    div()
                                                        .id(SharedString::from(format!("track-artist-{ix}-{k}")))
                                                        .cursor_pointer()
                                                        .hover(|style| style.underline())
                                                        .on_click(cx.listener(move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
                                                            cx.stop_propagation();
                                                            this.open_artist(id.clone(), artist_name.clone(), cx);
                                                        }))
                                                        .child(name)
                                                        .into_any_element()
                                                }
                                                None => div().child(name).into_any_element(),
                                            });
                                        }
                                        cell.into_any_element()
                                    }
                                })
                                .when(!in_album, |row| {
                                    row.child(
                                        div()
                                            .id(("track-album", ix))
                                            .flex_grow(1.5)
                                            .flex_basis(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .text_color(muted)
                                            .cursor_pointer()
                                            .hover(|style| style.underline())
                                            .on_click(cx.listener(move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
                                                cx.stop_propagation();
                                                if let Some(ctx) = this.state.data.tracks.get(ix).map(ActionMenuContext::from) {
                                                    this.go_to(ctx, ActionMenuAction::GoToAlbum, cx);
                                                }
                                            }))
                                            .child(SharedString::from(track.album.clone())),
                                    )
                                })
                                .when(!in_liked_songs, |row| {
                                    row.child(liked_cell(
                                        "tracks",
                                        ix,
                                        track.id.clone(),
                                        is_liked,
                                        secondary,
                                        palette,
                                        cx,
                                    ))
                                })
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(48.0))
                                        .text_color(muted)
                                        .child(SharedString::from(format_time(track.duration_ms))),
                                )
                            })
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
    let row_count = app.state.queue_rows().len();

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
                .child(div().text_lg().text_color(fg).child(tr(&app.state, "ui.queue")))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(
                            tr(&app.state, "desktop.queue_upcoming").replace("{}", &count.to_string()),
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
                .child(tr(&app.state, "desktop.queue_empty"))
                .into_any_element()
        } else {
            uniform_list(
                "queue-rows",
                row_count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = palette.row_selected;
                    let secondary = theme.secondary.gpui(WINDOW_FG());
                    let selected = this.state.ui.selected_queue_index;
                    let visual = visual_range_in(&this.state, ActiveView::Queue);
                    let has_manual = !this.state.data.manual_queue.is_empty();
                    let rows = this.state.queue_rows();

                    range
                        .map(|row_ix| match &rows[row_ix] {
                            QueueRow::Header(text) => queue_header(
                                text.clone(),
                                (row_ix == 0 && has_manual).then(|| tr(&this.state, "actions.clear_queue")),
                                fg,
                                muted,
                                cx,
                            ),
                            QueueRow::Track(ix, track) => {
                            let (ix, track) = (*ix, *track);
                            let is_liked = this.state.data.liked_tracks.contains(&track.id);

                            pill_row(ix, COMPACT_PILL, row_selected(ix, selected, visual), selected_bg, palette.row_hover, |row| {
                                row.gap_3()
                                .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                    if event.modifiers().shift {
                                        this.extend_selection_to(ix, cx);
                                        return;
                                    }
                                    this.state.ui.selected_queue_index = ix;
                                    if event.click_count() >= 2
                                        && let Some(event) =
                                            echo_core::intent::play_queue_track_at(&mut this.state, ix)
                                    {
                                        this.dispatch(event);
                                    }
                                    cx.notify();
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    cx.listener(move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                                        this.state.ui.selected_queue_index = ix;
                                        this.context_menu = None;
                                        if let Some(ctx) = this.action_target() {
                                            this.track_menu = Some(crate::TrackMenuState {
                                                ctx,
                                                position: Some(event.position),
                                                selected: 0,
                                                submenu: None,
                                            });
                                        }
                                        cx.notify();
                                    }),
                                )
                                .when_some(row_number(&this.state, ix, selected), |row, number| {
                                    row.child(
                                        div()
                                            .flex_none()
                                            .w(px(32.0))
                                            .text_color(muted)
                                            .child(number),
                                    )
                                })
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
                                .child(liked_cell(
                                    "queue",
                                    ix,
                                    track.id.clone(),
                                    is_liked,
                                    secondary,
                                    palette,
                                    cx,
                                ))
                                .child(
                                    div()
                                        .flex_none()
                                        .w(px(48.0))
                                        .text_color(muted)
                                        .child(SharedString::from(format_time(track.duration_ms))),
                                )
                            })
                            .into_any_element()
                            }
                        })
                        .collect()
                }),
            )
            .track_scroll(&app.queue_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

fn queue_header(
    text: String,
    clear_label: Option<SharedString>,
    fg: Hsla,
    muted: Hsla,
    cx: &mut Context<EchoApp>,
) -> gpui::AnyElement {
    div()
        .w_full()
        .h(px(COMPACT_PILL.row_height))
        .px_4()
        .flex()
        .items_center()
        .justify_between()
        .text_xs()
        .text_color(muted)
        .child(SharedString::from(text))
        .when_some(clear_label, |el, label| {
            el.child(
                div()
                    .id("clear-queue")
                    .cursor_pointer()
                    .text_color(fg)
                    .hover(move |style| style.text_color(muted))
                    .child(label)
                    .on_click(cx.listener(|this: &mut EchoApp, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(event) = echo_core::intent::clear_queue(&mut this.state) {
                            this.dispatch(event);
                        }
                        cx.notify();
                    })),
            )
        })
        .into_any_element()
}

/// "Playing from {}" label for the playback bar, resolving the playing context id to a
/// name from whatever state already holds it (loaded track list, library lists, artist
/// page, What's New). Falls back to a generic "a playlist"/"an album" when the id is not
/// loaded anywhere; `None` when nothing tracks the context at all.
pub(crate) fn playing_context_label(state: &echo_core::app::AppState) -> Option<String> {
    let context = state.playback.playing_context.as_ref()?;
    let from_tracklist = state
        .data
        .active_tracklist_context
        .as_ref()
        .filter(|c| c.id == context.context_id)
        .map(|c| c.title.clone());
    let name = if context.is_album {
        from_tracklist
            .or_else(|| {
                state
                    .data
                    .saved_albums
                    .iter()
                    .find(|album| album.id == context.context_id)
                    .map(|album| album.name.clone())
            })
            .or_else(|| {
                state.data.artist_page_data.as_ref().and_then(|data| {
                    data.albums
                        .iter()
                        .find(|album| album.id == context.context_id)
                        .map(|album| album.name.clone())
                })
            })
            .or_else(|| {
                state
                    .data
                    .whats_new
                    .iter()
                    .find(|album| album.id == context.context_id)
                    .map(|album| album.name.clone())
            })
            .unwrap_or_else(|| tr(state, "desktop.playing_from_album").to_string())
    } else {
        state
            .data
            .playlists
            .iter()
            .find(|playlist| playlist.id == context.context_id)
            .map(|playlist| playlist.name.clone())
            .or_else(|| {
                state
                    .data
                    .local_playlists
                    .playlists
                    .iter()
                    .find(|playlist| playlist.id == context.context_id)
                    .map(|playlist| playlist.name.clone())
            })
            .or_else(|| from_tracklist)
            .unwrap_or_else(|| tr(state, "desktop.playing_from_playlist").to_string())
    };
    Some(tr(state, "desktop.playing_from").replace("{}", &name))
}

/// Index-column text for a list row, honoring `track_index_base` (negative hides the
/// column — the caller omits the div) and `relative_line_numbers`, exactly as the TUI does.
fn row_number(state: &echo_core::app::AppState, ix: usize, selected: usize) -> Option<SharedString> {
    let config = &state.ui.library_config;
    if config.track_index_base < 0 {
        return None;
    }
    Some(SharedString::from(
        echo_core::app::displayed_track_number(
            ix,
            selected,
            config.track_index_base,
            config.relative_line_numbers,
        )
        .to_string(),
    ))
}

/// A fixed-width heart slot ahead of the duration column: filled when the track is in
/// Liked Songs, faint otherwise so durations stay aligned across rows. Clicking toggles
/// the like (un-liking through the confirm prompt), same as `l` on the row.
fn liked_cell(
    section: &'static str,
    ix: usize,
    track_id: String,
    liked: bool,
    color: Hsla,
    palette: DesktopPalette,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let group = SharedString::from(format!("liked-{section}-{ix}"));
    div()
        .id(group.clone())
        .group(group.clone())
        .flex_none()
        .w(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
            cx.stop_propagation();
            if let Some(event) =
                echo_core::intent::toggle_like_track(&mut this.state, track_id.clone())
            {
                this.dispatch(event);
            }
            cx.notify();
        }))
        .child(
            // gpui svgs don't inherit text color, so the tint (and its hover swap) must
            // live on the svg itself.
            svg()
                .path("icons/heart.svg")
                .size(px(12.0))
                .text_color(if liked { color } else { palette.like_dim })
                .when(!liked, |el| {
                    el.group_hover(group, move |style| style.text_color(color))
                }),
        )
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
                .bg(DesktopPalette::resolve(&this.state.ui.active_theme).wash)
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
    let outer_palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let tab = app.state.ui.active_search_tab;
    let query = app.state.ui.search_context_query.clone();
    let results = &app.state.data.search_results;
    let (n_tracks, n_albums, n_artists, n_playlists) = (
        results.tracks.len(),
        results.albums.len(),
        results.artists.len(),
        results.playlists.len(),
    );
    let count = match tab {
        SearchTab::Tracks => n_tracks,
        SearchTab::Albums => n_albums,
        SearchTab::Artists => n_artists,
        SearchTab::Playlists => n_playlists,
    };

    let tab_button = |id: &'static str, label: String, target: SearchTab, active: bool| {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .text_color(if active { accent } else { muted })
            .hover(move |style| style.bg(outer_palette.row_hover))
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
                        .child(SharedString::from(format!(
                            "{}{query}",
                            tr(&app.state, "prompts.search")
                        ))),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(tab_button(
                            "search-tracks",
                            format!("{} ({n_tracks})", tr(&app.state, "ui.tracks")),
                            SearchTab::Tracks,
                            tab == SearchTab::Tracks,
                        ))
                        .child(tab_button(
                            "search-albums",
                            format!("{} ({n_albums})", tr(&app.state, "ui.albums")),
                            SearchTab::Albums,
                            tab == SearchTab::Albums,
                        ))
                        .child(tab_button(
                            "search-artists",
                            format!("{} ({n_artists})", tr(&app.state, "ui.artists")),
                            SearchTab::Artists,
                            tab == SearchTab::Artists,
                        ))
                        .child(tab_button(
                            "search-playlists",
                            format!("{} ({n_playlists})", tr(&app.state, "ui.playlists")),
                            SearchTab::Playlists,
                            tab == SearchTab::Playlists,
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
                .child(tr(&app.state, "desktop.no_results_tab"))
                .into_any_element()
        } else {
            uniform_list(
                "search-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = palette.row_selected;
                    let tab = this.state.ui.active_search_tab;
                    let selected = this.state.ui.selected_search_index;
                    let visual = visual_range_in(&this.state, ActiveView::SearchResults);

                    let rows: Vec<AnyElement> = range
                        .map(|ix| {
                            pill_row(ix, LIST_PILL, row_selected(ix, selected, visual), selected_bg, palette.row_hover, |row| {
                                let row = row.gap_3().on_click(cx.listener(
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
                                    let is_liked =
                                        this.state.data.liked_tracks.contains(&track.id);
                                    let secondary =
                                        this.state.ui.active_theme.secondary.gpui(WINDOW_FG());
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
                                    .child(liked_cell(
                                        "search",
                                        ix,
                                        track.id.clone(),
                                        is_liked,
                                        secondary,
                                        palette,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(48.0))
                                            .text_color(muted)
                                            .child(SharedString::from(format_time(
                                                track.duration_ms,
                                            ))),
                                    )
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
                                    // No followers column: the count is gone from the
                                    // dev-mode API, so it would always read 0.
                                    row.child(thumb).child(
                                        div()
                                            .flex_grow(1.0)
                                            .flex_basis(px(0.0))
                                            .overflow_hidden()
                                            .text_ellipsis()
                                            .whitespace_nowrap()
                                            .text_color(fg)
                                            .child(SharedString::from(artist.name)),
                                    )
                                }
                                SearchTab::Playlists => {
                                    let playlist =
                                        this.state.data.search_results.playlists[ix].clone();
                                    let thumb_url = playlist
                                        .thumb_url
                                        .clone()
                                        .or_else(|| playlist.image_url.clone());
                                    let thumb = thumb_element(
                                        this,
                                        thumb_url.as_deref(),
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
                                                .child(SharedString::from(playlist.name)),
                                        )
                                        .child(
                                            div()
                                                .flex_grow(1.0)
                                                .flex_basis(px(0.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(muted)
                                                .child(SharedString::from(playlist.owner)),
                                        )
                                }
                                }
                            })
                            .into_any_element()
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

/// The Top Tracks / Top Artists time-window selector: three small pills, the active one
/// accent-bordered. Persisting the choice and refetching go through the shared intent.
fn range_switcher(app: &EchoApp, cx: &mut Context<EchoApp>) -> Div {
    use echo_core::models::TopItemsRange;
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let active = app.state.ui.library_config.top_items_range;

    let mut row = div().flex().flex_row().items_center().gap_1();
    for (range, key) in [
        (TopItemsRange::Short, "desktop.range_short"),
        (TopItemsRange::Medium, "desktop.range_medium"),
        (TopItemsRange::Long, "desktop.range_long"),
    ] {
        let is_active = range == active;
        row = row.child(
            div()
                .id(SharedString::from(format!("range-{key}")))
                .px_2()
                .py_1()
                .rounded_full()
                .border_1()
                .text_xs()
                .when(is_active, |el| el.border_color(accent).text_color(accent))
                .when(!is_active, |el| {
                    el.border_color(palette.border).text_color(muted)
                })
                .hover(move |style| style.bg(palette.row_hover))
                .cursor_pointer()
                .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                    if let Some(event) =
                        echo_core::intent::set_top_items_range(&mut this.state, range)
                    {
                        this.dispatch(event);
                    }
                    cx.notify();
                }))
                .child(tr(&app.state, key)),
        );
    }
    row
}

/// Full-page artist list: reached through the "Top Artists" browse link (the sidebar's
/// Artists tab still covers followed artists) and by back-navigation.
fn artist_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let is_top = app.state.ui.artist_list_source == echo_core::app::ArtistListSource::Top;
    let title = if is_top {
        tr(&app.state, "desktop.top_artists")
    } else {
        tr(&app.state, "ui.artists")
    };
    let count = app.state.artist_list().len();

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
                        .flex_grow(1.0)
                        .text_lg()
                        .text_color(fg)
                        .child(title),
                )
                .when(is_top, |el| el.child(range_switcher(app, cx))),
        )
        .child(if count == 0 {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child(tr(&app.state, "desktop.loading"))
                .into_any_element()
        } else {
            uniform_list(
                "artist-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = palette.row_selected;
                    let selected = this.state.ui.selected_artist_index;
                    let visual = visual_range_in(&this.state, ActiveView::ArtistList);

                    let rows: Vec<AnyElement> = range
                        .map(|ix| {
                            let Some(artist) = this.state.artist_list().get(ix).cloned() else {
                                return div().id(ix).into_any_element();
                            };
                            let thumb = thumb_element(
                                this,
                                artist.image_url.as_deref(),
                                26.0,
                                true,
                                muted,
                            );
                            pill_row(
                                ix,
                                LIST_PILL,
                                row_selected(ix, selected, visual),
                                selected_bg,
                                palette.row_hover,
                                |row| {
                                    row.gap_3()
                                        .on_click(cx.listener(
                                            move |this: &mut EchoApp, _event, _window, cx| {
                                                if let Some(event) =
                                                    echo_core::intent::open_artist_at(
                                                        &mut this.state,
                                                        ix,
                                                    )
                                                {
                                                    this.dispatch(event);
                                                }
                                                cx.notify();
                                            },
                                        ))
                                        .when_some(
                                            row_number(&this.state, ix, selected),
                                            |row, number| {
                                                row.child(
                                                    div()
                                                        .flex_none()
                                                        .w(px(24.0))
                                                        .text_xs()
                                                        .text_color(muted)
                                                        .child(number),
                                                )
                                            },
                                        )
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
                                },
                            )
                            .into_any_element()
                        })
                        .collect();

                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
                    rows
                }),
            )
            .track_scroll(&app.artist_list_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

/// What's New: recent releases from followed artists, reached through the sidebar
/// browse link. Renders live from state, filling in as the background scan reports.
fn whats_new(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let count = app.state.data.whats_new.len();
    let progress = app.state.data.whats_new_progress;
    let scanning = progress.is_some();

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
                        .flex_grow(1.0)
                        .text_lg()
                        .text_color(fg)
                        .child(tr(&app.state, "desktop.whats_new")),
                )
                .when_some(progress, |el, (done, total)| {
                    el.child(
                        div().flex_none().text_xs().text_color(muted).child(
                            SharedString::from(
                                tr(&app.state, "desktop.whats_new_checking")
                                    .replace("{done}", &done.to_string())
                                    .replace("{total}", &total.to_string()),
                            ),
                        ),
                    )
                }),
        )
        .child(if count == 0 {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child(if scanning {
                    tr(&app.state, "desktop.loading")
                } else {
                    tr(&app.state, "desktop.whats_new_empty")
                })
                .into_any_element()
        } else {
            uniform_list(
                "whats-new-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = palette.row_selected;
                    let selected = this.state.ui.selected_whats_new_index;
                    let visual = visual_range_in(&this.state, ActiveView::WhatsNew);

                    let rows: Vec<AnyElement> = range
                        .map(|ix| {
                            let Some(album) = this.state.data.whats_new.get(ix).cloned() else {
                                return div().id(ix).into_any_element();
                            };
                            let thumb = thumb_element(
                                this,
                                album.thumb_url.as_deref().or(album.image_url.as_deref()),
                                26.0,
                                false,
                                muted,
                            );
                            let released = album
                                .release_date
                                .clone()
                                .unwrap_or_else(|| album.release_year.clone());
                            pill_row(
                                ix,
                                LIST_PILL,
                                row_selected(ix, selected, visual),
                                selected_bg,
                                palette.row_hover,
                                |row| {
                                    row.gap_3()
                                        .on_click(cx.listener(
                                            move |this: &mut EchoApp, _event, _window, cx| {
                                                if let Some(event) =
                                                    echo_core::intent::open_whats_new_album(
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
                                                .flex_grow(1.5)
                                                .flex_basis(px(0.0))
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_sm()
                                                .text_color(muted)
                                                .child(SharedString::from(album.artists)),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(SharedString::from(released)),
                                        )
                                },
                            )
                            .into_any_element()
                        })
                        .collect();

                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
                    rows
                }),
            )
            .track_scroll(&app.whats_new_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

fn artist_page(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let outer_palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());

    let Some(data) = app.state.data.artist_page_data.clone() else {
        return div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child(tr(&app.state, "desktop.loading_artist"))
            .into_any_element();
    };

    let accent = theme.primary.gpui(WINDOW_FG());
    let header_image = app
        .state
        .ui
        .active_library_header_image
        .clone()
        .and_then(|artwork| app.images.get(&artwork));
    let count = data.albums.len();
    let loading = app.state.data.artist_albums_loading && count == 0;
    let top_len = data.top_tracks.len();
    let top_loading = app.state.data.artist_top_tracks_loading;

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
                        .hover(move |style| style.bg(outer_palette.wash))
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
                        .flex_grow(1.0)
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
                )
                // Passive badge: there is no reliable write route for follow/unfollow,
                // so this only mirrors the (24h-cached) followed-artists list.
                .when(
                    app.state
                        .data
                        .followed_artists
                        .iter()
                        .any(|artist| artist.id == data.artist_id),
                    |el| {
                        el.child(
                            div()
                                .flex_none()
                                .px_3()
                                .py_1()
                                .rounded_full()
                                .border_1()
                                .text_xs()
                                .border_color(accent)
                                .text_color(accent)
                                .child(tr(&app.state, "desktop.following")),
                        )
                    },
                ),
        )
        .when(top_len > 0 || top_loading, |el| {
            el.child(
                div()
                    .flex_none()
                    .px_4()
                    .pt_2()
                    .pb_1()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(&app.state, "desktop.popular")),
            )
            .child(if top_len == 0 {
                div()
                    .flex_none()
                    .px_4()
                    .py_2()
                    .text_sm()
                    .text_color(muted)
                    .child(tr(&app.state, "desktop.loading"))
                    .into_any_element()
            } else {
                uniform_list(
                    "artist-top-track-rows",
                    top_len,
                    cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                        let theme = &this.state.ui.active_theme;
                        let fg = theme.text.gpui(WINDOW_FG());
                        let muted = theme.text_muted.gpui(WINDOW_FG());
                        let accent = theme.primary.gpui(WINDOW_FG());
                        let palette = DesktopPalette::resolve(theme);
                    let selected_bg = palette.row_selected;
                        // Combined index space: Popular rows come first, so `ix` compares
                        // against the page cursor directly.
                        let selected = this.state.ui.artist_page_album_index;
                        let playing_id = this.state.playback.playing_track_id.clone();

                        let rows: Vec<AnyElement> = range
                            .map(|ix| {
                                let Some(track) = this
                                    .state
                                    .data
                                    .artist_page_data
                                    .as_ref()
                                    .and_then(|data| data.top_tracks.get(ix))
                                    .cloned()
                                else {
                                    return div().id(ix).into_any_element();
                                };
                                let is_playing =
                                    playing_id.as_deref() == Some(track.id.as_str());
                                let is_liked =
                                    this.state.data.liked_tracks.contains(&track.id);
                                let secondary =
                                    this.state.ui.active_theme.secondary.gpui(WINDOW_FG());
                                let title_color = if is_playing { accent } else { fg };
                                let thumb = thumb_element(
                                    this,
                                    track.image_url.as_deref(),
                                    26.0,
                                    false,
                                    muted,
                                );
                                pill_row(ix, LIST_PILL, ix == selected, selected_bg, palette.row_hover, |row| {
                                    row.gap_3()
                                        .on_click(cx.listener(
                                            move |this: &mut EchoApp,
                                                  event: &gpui::ClickEvent,
                                                  _window,
                                                  cx| {
                                                this.state.ui.artist_page_album_index = ix;
                                                if event.click_count() >= 2
                                                    && let Some(event) =
                                                        echo_core::intent::play_artist_top_track(
                                                            &mut this.state,
                                                            ix,
                                                        )
                                                {
                                                    this.dispatch(event);
                                                }
                                                cx.notify();
                                            },
                                        ))
                                        .when_some(
                                            row_number(&this.state, ix, selected),
                                            |row, number| {
                                                row.child(
                                                    div()
                                                        .flex_none()
                                                        .w(px(24.0))
                                                        .text_xs()
                                                        .text_color(muted)
                                                        .child(number),
                                                )
                                            },
                                        )
                                        .child(thumb)
                                        .child(
                                            div()
                                                .flex_grow(1.0)
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(title_color)
                                                .child(SharedString::from(track.name.clone())),
                                        )
                                        .child(liked_cell(
                                            "popular",
                                            ix,
                                            track.id.clone(),
                                            is_liked,
                                            secondary,
                                            palette,
                                            cx,
                                        ))
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(SharedString::from(format_time(
                                                    track.duration_ms,
                                                ))),
                                        )
                                })
                                .into_any_element()
                            })
                            .collect();

                        echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
                        rows
                    }),
                )
                .track_scroll(&app.artist_top_tracks_scroll)
                .flex_none()
                .h(px(top_len.min(10) as f32 * LIST_PILL.row_height))
                .max_h(relative(0.4))
                .into_any_element()
            })
            .child(
                div()
                    .flex_none()
                    .px_4()
                    .pt_2()
                    .pb_1()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(&app.state, "desktop.albums_section")),
            )
        })
        .child(if loading {
            div()
                .flex_grow(1.0)
                .flex()
                .items_center()
                .justify_center()
                .text_color(muted)
                .child(tr(&app.state, "desktop.loading_albums"))
                .into_any_element()
        } else {
            uniform_list(
                "artist-album-rows",
                count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = palette.row_selected;
                    // Album rows sit after the Popular rows in the page's combined index space.
                    let selected = this.state.ui.artist_page_album_index;
                    let top_len = this.artist_page_top_len();

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
                            pill_row(ix, LIST_PILL, ix + top_len == selected, selected_bg, palette.row_hover, |row| {
                                row.gap_3()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        let index = ix + this.artist_page_top_len();
                                        if let Some(event) = echo_core::intent::activate_artist_page_row(
                                            &mut this.state,
                                            index,
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
                            })
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

/// Index of the lyric line playing at `progress_ms`: the last line that has started, or the
/// first before any has.
pub(crate) fn current_lyric_index(lines: &[echo_core::models::LyricLine], progress_ms: u32) -> usize {
    lines
        .iter()
        .take_while(|line| line.start_ms <= progress_ms)
        .count()
        .saturating_sub(1)
}

/// The colors a lyric line takes by its place in the song: the current line accented, lines
/// already sung in the text color, upcoming ones muted.
struct LyricColors {
    fg: Hsla,
    muted: Hsla,
    accent: Hsla,
}

impl LyricColors {
    fn resolve(app: &EchoApp) -> Self {
        let theme = &app.state.ui.active_theme;
        Self {
            fg: theme.text.gpui(WINDOW_FG()),
            muted: theme.text_muted.gpui(WINDOW_FG()),
            accent: theme.primary.gpui(WINDOW_FG()),
        }
    }

    fn immersive(colors: &ImmersiveColors) -> Self {
        Self {
            fg: colors.text,
            muted: colors.text_muted,
            accent: colors.accent,
        }
    }

    fn line(&self, ix: usize, current: usize) -> Hsla {
        match ix.cmp(&current) {
            std::cmp::Ordering::Equal => self.accent,
            std::cmp::Ordering::Greater => self.muted,
            std::cmp::Ordering::Less => self.fg,
        }
    }
}

/// One lyric line on a single row of `height` pixels, ellipsized if it is too long. A block
/// with the line height set to the row, not a flex row: a flex item is measured at its
/// max-content width, which defeats the ellipsis.
fn lyric_row(text: SharedString, color: Hsla, height: f32, large: bool) -> Div {
    div()
        .w_full()
        .h(px(height))
        .px_4()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .map(|el| if large { el.text_xl().text_center() } else { el.text_sm() })
        .line_height(px(height))
        .text_color(color)
        .child(text)
}

/// What stands in for the lyrics while they load or when the track has none; `None` once there
/// are lines to show.
fn lyric_status(app: &EchoApp, muted: Hsla) -> Option<AnyElement> {
    let playback = &app.state.playback;
    let key = if playback.is_fetching_lyrics {
        "desktop.loading_lyrics"
    } else if playback.current_lyrics.is_none() {
        "desktop.no_lyrics"
    } else {
        return None;
    };
    Some(
        div()
            .py_8()
            .flex()
            .justify_center()
            .text_color(muted)
            .child(tr(&app.state, key))
            .into_any_element(),
    )
}

/// The modal's lyric list: every line, scrolled so the current one (by playback position) sits
/// at the center whenever the list is long enough to allow it.
fn lyric_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let lines = app
        .state
        .playback
        .current_lyrics
        .as_ref()
        .map(|lyrics| lyrics.lines.as_slice())
        .unwrap_or_default();
    let current = current_lyric_index(lines, app.state.playback.display_progress_ms());
    app.lyrics_scroll
        .scroll_to_item(current, gpui::ScrollStrategy::Center);

    uniform_list(
        "lyric-lines",
        lines.len(),
        cx.processor(|this: &mut EchoApp, range: std::ops::Range<usize>, _window, _cx| {
            let colors = LyricColors::resolve(this);
            let lines = this
                .state
                .playback
                .current_lyrics
                .as_ref()
                .map(|lyrics| lyrics.lines.as_slice())
                .unwrap_or_default();
            let current = current_lyric_index(lines, this.state.playback.display_progress_ms());

            range
                .map(|ix| {
                    let text = lines.get(ix).map(|line| line.text.clone()).unwrap_or_default();
                    lyric_row(text.into(), colors.line(ix, current), MODAL_LYRIC_ROW, false).id(ix)
                })
                .collect()
        }),
    )
    .track_scroll(&app.lyrics_scroll)
    .flex_grow(1.0)
    .into_any_element()
}

/// The immersive lyrics: a fixed window of `rows` (odd) whole lines with the current line
/// pinned to the center row, so nothing is ever clipped and the first and last lines of a song
/// sit exactly where the middle ones do. When the line advances the column glides up one row
/// over [`LYRIC_GLIDE`]; the extra row above the window carries the line on its way out. Rows
/// fade toward the edges (see [`lyric_opacity`]), the outermost still faintly legible.
fn lyric_window(app: &EchoApp, colors: LyricColors, rows: usize, row_height: f32) -> AnyElement {
    let lines = app
        .state
        .playback
        .current_lyrics
        .as_ref()
        .map(|lyrics| lyrics.lines.as_slice())
        .unwrap_or_default();
    let current = current_lyric_index(lines, app.state.playback.display_progress_ms());
    let half = (rows / 2) as isize;
    let reach = (half + 1) as f32;
    let column: Vec<(isize, SharedString, Hsla)> = (-half - 1..=half)
        .map(|offset| {
            let ix = current.checked_add_signed(offset);
            let text = ix
                .and_then(|ix| lines.get(ix))
                .map(|line| SharedString::from(line.text.clone()))
                .unwrap_or_default();
            (offset, text, colors.line(ix.unwrap_or(current), current))
        })
        .collect();

    div()
        .relative()
        .w_full()
        .h(px(rows as f32 * row_height))
        .overflow_hidden()
        .child(
            div().absolute().w_full().with_animation(
                ("lyric-glide", current),
                Animation::new(LYRIC_GLIDE).with_easing(ease_out_quint()),
                move |el, t| {
                    el.top(px(-row_height * t))
                        .children(column.iter().map(|(offset, text, color)| {
                            let distance = (*offset as f32 + 1.0 - t).abs();
                            lyric_row(text.clone(), *color, row_height, true)
                                .opacity(lyric_opacity(distance, reach))
                        }))
                },
            ),
        )
        .into_any_element()
}

const MODAL_LYRIC_ROW: f32 = 26.0;
const LYRIC_GLIDE: std::time::Duration = std::time::Duration::from_millis(240);

/// The lyrics overlay: [`lyric_list`] in a centered panel titled with the track.
pub fn lyrics_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let body = lyric_status(app, muted).unwrap_or_else(|| lyric_list(app, cx));

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
                .border_color(DesktopPalette::resolve(&app.state.ui.active_theme).menu_border)
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
                        .child(SharedString::from(
                            tr(&app.state, "desktop.lyrics_title")
                                .replace("{}", &app.state.playback.playing_track_title),
                        )),
                )
                .child(body),
        )
}

/// The backdrop's picture over the whole window, under everything else: the keyframe before
/// `phase` with the one after it crossfaded on top. Painted straight from the texture as a fill
/// (its guard border just outside the window) rather than through `img`, which takes the
/// texture's aspect ratio as the box's and would paint a square.
pub fn backdrop_layer(
    backdrop: std::sync::Arc<Backdrop>,
    phase: f32,
    corners: Option<gpui::Tiling>,
) -> impl IntoElement {
    let radii = client_corner_radii(corners, ClientCorners::All);
    let (first, second, blend) = backdrop.frame(phase);
    let layer = |image: std::sync::Arc<gpui::RenderImage>| {
        let backdrop = backdrop.clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let image_bounds = backdrop.image_bounds(bounds);
                window.paint_image(bounds, image_bounds, radii, image, 0, false).ok();
            },
        )
        .absolute()
        .inset_0()
    };
    div()
        .absolute()
        .inset_0()
        .child(layer(first))
        .when(blend > 0.0, |el| el.child(div().absolute().inset_0().opacity(blend).child(layer(second))))
}

/// The immersive toggle. Callers pick its colors: the search bar paints it muted in the theme,
/// the immersive view accented in the cover's colors. It is the one control the immersive view
/// keeps, so it lives here rather than inline in the search bar.
fn immersive_button(color: Hsla, hover: Hsla, cx: &mut Context<EchoApp>) -> impl IntoElement {
    crate::icon_button("immersive", "icons/full-screen.svg", color, hover, cx, |this, cx| {
        this.toggle_immersive(cx)
    })
}

/// Edge of the immersive cover, in pixels, for a body of `width` x `height`: the largest square
/// that, with the caption under it, fits the height inside the margins, bounded by the
/// half-width; then scaled down to leave air around it, and capped for very tall windows.
pub(crate) fn immersive_cover_edge(width: f32, height: f32) -> f32 {
    let by_width = width * 0.5 - 2.0 * IMMERSIVE_MARGIN;
    let by_height = height - 2.0 * IMMERSIVE_MARGIN - IMMERSIVE_CAPTION_HEIGHT;
    (by_width.min(by_height) * IMMERSIVE_COVER_SCALE).clamp(0.0, IMMERSIVE_COVER_MAX)
}

/// Rows in the immersive lyric window for a panel `panel_height` tall: as many whole rows as
/// fit, made odd so one row is the exact center, and at least one.
pub(crate) fn lyric_window_rows(panel_height: f32, row_height: f32) -> usize {
    let rows = (panel_height / row_height).max(1.0) as usize;
    if rows % 2 == 0 { rows - 1 } else { rows }
}

/// Opacity of a lyric row `distance` rows from the center when the row just past the window's
/// edge is `reach` rows away: full at the center, easing out so the neighbors stay readable and
/// reaching transparent one row beyond the edge — the Apple Music fade. `distance` is
/// fractional mid-glide.
pub(crate) fn lyric_opacity(distance: f32, reach: f32) -> f32 {
    let t = (distance / reach).min(1.0);
    1.0 - t * t
}

const IMMERSIVE_MARGIN: f32 = 48.0;
/// Both halves stack a cover-high box, a gap and a caption-high block, centered; so the
/// right's lyrics box centers on the cover and its controls block tops out with the title.
/// The caption block is the title's 32px text_2xl line, gap_1, the artist's 28px text_lg line;
/// the controls block is the seek row, gap_1, the transport row, the same 64.
const IMMERSIVE_CAPTION_HEIGHT: f32 = IMMERSIVE_GAP + IMMERSIVE_CAPTION_BLOCK;
const IMMERSIVE_CAPTION_BLOCK: f32 = 32.0 + 4.0 + 28.0;
const IMMERSIVE_GAP: f32 = 24.0;
const IMMERSIVE_SEEK_ROW: f32 = 24.0;
const IMMERSIVE_TRANSPORT_ROW: f32 = IMMERSIVE_CAPTION_BLOCK - 4.0 - IMMERSIVE_SEEK_ROW;
const IMMERSIVE_COVER_SCALE: f32 = 0.75;
const IMMERSIVE_COVER_MAX: f32 = 480.0;
/// The lyric window fills up to this share of the body height, and never more than the cover.
const IMMERSIVE_LYRICS_SHARE: f32 = 0.5;
const IMMERSIVE_LYRIC_ROW: f32 = 40.0;

/// The immersive view: the cover with the track's title and artist under it, that whole block
/// centered in the left half; on the right the synced lyrics, centered on the cover, with the
/// seek bar and transport under them level with the caption; and top right, in the search
/// bar's slots, the toggle, the queue (where the theme picker sits otherwise) and settings.
/// Everything else — sidebar, navigation, search, playback bar — is gone until it is toggled
/// off. Lyric lines are centered so each shares its axis with the seek bar and play button. Every color comes from `backdrop`, derived from the cover
/// (see [`crate::backdrop`]); the backdrop's picture and window fill are the root's, so they
/// reach under the titlebar too.
pub fn immersive_view(
    app: &mut EchoApp,
    backdrop: &Backdrop,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let colors = backdrop.colors;
    let (fg, muted) = (colors.text, colors.text_muted);
    let controls = ControlColors {
        fg,
        muted,
        accent: colors.accent,
        hover: colors.wash,
        track: colors.wash,
    };
    let playback = &app.state.playback;
    let has_track = playback.playing_track_id.is_some();
    let title: SharedString = if playback.playing_track_title.is_empty() {
        tr(&app.state, "desktop.nothing_playing")
    } else {
        playback.playing_track_title.clone().into()
    };
    let artist: SharedString = playback.playing_track_artist.clone().into();
    let viewport = window.viewport_size();
    let body_height = f32::from(viewport.height) - TITLEBAR_HEIGHT;
    let edge = immersive_cover_edge(f32::from(viewport.width), body_height);
    let rows = lyric_window_rows(
        edge.min(body_height * IMMERSIVE_LYRICS_SHARE),
        IMMERSIVE_LYRIC_ROW,
    );
    let cover = playback
        .playing_track_image
        .as_ref()
        .or(playback.previous_track_image.as_ref())
        .cloned()
        .and_then(|artwork| app.images.get(&artwork));
    let cover = match cover {
        Some(image) => img(image)
            .flex_none()
            .w(px(edge))
            .h(px(edge))
            .rounded_lg()
            .into_any_element(),
        None => div()
            .flex_none()
            .w(px(edge))
            .h(px(edge))
            .rounded_lg()
            .bg(colors.wash)
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("icons/music-note.svg")
                    .w(px(edge * 0.25))
                    .h(px(edge * 0.25))
                    .text_color(muted),
            )
            .into_any_element(),
    };
    let lyrics = lyric_status(app, muted).unwrap_or_else(|| {
        lyric_window(app, LyricColors::immersive(&colors), rows, IMMERSIVE_LYRIC_ROW)
    });

    div()
        .id("immersive")
        .relative()
        .flex_grow(1.0)
        .flex()
        .flex_row()
        .overflow_hidden()
        .child(
            div()
                .w(relative(0.5))
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(IMMERSIVE_GAP))
                .child(cover)
                .child(
                    div()
                        .w(px(edge))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            crate::playing_track_link(
                                "immersive-title",
                                title,
                                fg,
                                has_track,
                                ActionMenuAction::GoToAlbum,
                                cx,
                            )
                            .text_2xl(),
                        )
                        .child(
                            crate::playing_track_link(
                                "immersive-artist",
                                artist,
                                muted,
                                has_track,
                                ActionMenuAction::GoToArtist,
                                cx,
                            )
                            .text_lg(),
                        ),
                ),
        )
        .child(
            div()
                .w(relative(0.5))
                .h_full()
                .pr(px(IMMERSIVE_MARGIN))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(IMMERSIVE_GAP))
                .child(
                    div()
                        .h(px(edge))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .child(lyrics),
                )
                .child(
                    div()
                        .h(px(IMMERSIVE_CAPTION_BLOCK))
                        .px_4()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(app.seek_bar(controls, cx).h(px(IMMERSIVE_SEEK_ROW)))
                        .child(
                            app.transport_controls(controls, cx)
                                .h(px(IMMERSIVE_TRANSPORT_ROW))
                                .justify_center(),
                        ),
                ),
        )
        // The exact pixels the search bar gives its three buttons (pr_4 from the edge), so the
        // toggle and settings never move out from under the pointer. The queue takes the theme
        // picker's slot; opening it leaves the view.
        .child(
            div()
                .absolute()
                .top_2()
                .right(px(16.0))
                .flex()
                .flex_row()
                .child(immersive_button(colors.accent, colors.wash, cx))
                .child(crate::icon_button(
                    "queue",
                    "icons/playlist.svg",
                    muted,
                    colors.wash,
                    cx,
                    |this, cx| this.toggle_queue(cx),
                ))
                .child(crate::icon_button(
                    "settings",
                    "icons/settings.svg",
                    muted,
                    colors.wash,
                    cx,
                    |this, cx| this.toggle_settings(cx),
                )),
        )
}

/// The theme picker: every loaded theme by name, the active one accented.
/// Theme names in the order the picker lists them. `EchoApp::theme_modal_index` indexes into
/// this, so keyboard selection and rendering must both go through here or the two drift.
pub fn sorted_theme_names(state: &echo_core::app::AppState) -> Vec<String> {
    let mut names: Vec<String> = state.ui.themes.keys().cloned().collect();
    names.sort();
    names
}

pub fn theme_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let names = sorted_theme_names(&app.state);
    let active = app.state.ui.library_config.active_theme.clone();
    let selected = app.theme_modal_index;

    div()
        .id("theme-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
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
                .border_color(palette.menu_border)
                .bg(surface)
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(div().pb_2().text_sm().text_color(muted).child(tr(&app.state, "desktop.theme")))
                .child(if names.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.no_themes"))
                        .into_any_element()
                } else {
                    div()
                        .id("theme-list")
                        .flex()
                        .flex_col()
                        .max_h(px(340.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.theme_modal_scroll)
                        .children(names.into_iter().enumerate().map(|(ix, name)| {
                            let is_active = active.as_deref() == Some(name.as_str());
                            let is_selected = ix == selected;
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
                                .when(is_selected, |el| el.bg(palette.menu_selected))
                                .when(!is_selected, |el| {
                                    el.hover(move |style| style.bg(palette.menu_hover))
                                })
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

/// Keyboard shortcuts, grouped, for the help overlay, as `(section i18n key, [(keys,
/// description i18n key)])` — resolved through [`tr`] at render so `:lang` applies.
///
/// Kept next to the `KeyBinding::new` block in `main()` conceptually — when you add a binding
/// there, add it here too. Commands are not listed: those come from
/// `echo_core::commands::COMMANDS` so they cannot go stale.
pub const KEY_HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "desktop.help.nav",
        &[
            ("j / k / ↓ / ↑", "desktop.help.move"),
            ("gg / G / home / end", "desktop.help.first_last"),
            ("ctrl-b / ctrl-f / pgup / pgdn", "desktop.help.page"),
            ("ctrl-u / ctrl-d", "desktop.help.half_page"),
            ("enter / z", "desktop.help.open"),
            ("h / esc", "desktop.help.back"),
            ("alt-← / alt-→", "desktop.help.history"),
            ("← / →", "desktop.help.pane_focus"),
            ("backspace", "desktop.help.to_sidebar"),
            ("ctrl-\\", "desktop.help.toggle_sidebar"),
            ("tab", "desktop.help.tabs"),
            ("gc", "desktop.help.jump_playing"),
            ("1-9", "desktop.help.count"),
            ("y", "desktop.help.confirm"),
        ],
    ),
    (
        "desktop.help.playback",
        &[
            ("space", "desktop.help.play_pause"),
            ("[ / ] / ctrl-← / ctrl-→", "desktop.help.prev_next"),
            (", / . / shift-← / shift-→", "desktop.help.seek"),
            ("0", "desktop.help.seek_start"),
            ("- / =", "desktop.help.volume"),
            ("shift-M", "desktop.help.mute"),
            ("s / r", "desktop.help.shuffle_repeat"),
            ("shift-D", "desktop.help.devices"),
            ("shift-L", "desktop.help.lyrics"),
            ("ctrl-shift-L", "desktop.help.lyrics_bar"),
            ("shift-F", "desktop.help.immersive"),
        ],
    ),
    (
        "desktop.help.library",
        &[
            ("l", "desktop.help.like"),
            ("a", "desktop.help.add_playlist"),
            ("shift-A", "desktop.help.track_actions"),
            ("q / shift-Q", "desktop.help.queue"),
            ("shift-J / shift-K", "desktop.help.move_track"),
            ("dd", "desktop.help.delete"),
            ("v", "desktop.help.select_range"),
            ("m", "desktop.help.pin"),
            ("c / e", "desktop.help.new_rename"),
            ("shift-R", "desktop.help.refresh"),
        ],
    ),
    (
        "desktop.help.finding",
        &[
            ("ctrl-k", "desktop.help.search"),
            ("/", "desktop.help.filter"),
            ("n / shift-N", "desktop.help.next_match"),
            (":", "desktop.help.command_bar"),
            ("f", "desktop.help.search_command"),
            ("t", "desktop.help.themes"),
            ("ctrl-,", "desktop.help.settings"),
            ("?", "desktop.help.this_help"),
            ("ctrl-q", "desktop.help.quit"),
        ],
    ),
];

/// The help overlay: every keybinding, plus every `:` command straight from the registry.
pub fn help_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let entry = |keys: &'static str, what: SharedString| {
        div()
            .flex()
            .flex_row()
            .items_baseline()
            .gap_2()
            .py_0p5()
            .child(
                div()
                    .flex_none()
                    .w(px(112.0))
                    .text_xs()
                    .text_color(accent)
                    .child(keys),
            )
            .child(div().text_xs().text_color(muted).child(what))
    };

    let state = &app.state;
    let section = move |title_key: &'static str, rows: &'static [(&'static str, &'static str)]| {
        div()
            .flex()
            .flex_col()
            .pb_2()
            .child(div().pb_1().text_xs().text_color(fg).child(tr(state, title_key)))
            .children(rows.iter().map(|(keys, what_key)| entry(keys, tr(state, what_key))))
    };

    div()
        .id("help-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.help_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("help-panel")
                .w(px(700.0))
                .max_h(px(600.0))
                .rounded_lg()
                .border_1()
                .border_color(DesktopPalette::resolve(&app.state.ui.active_theme).menu_border)
                .bg(surface)
                .p_4()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .child(div().text_sm().text_color(fg).child(tr(&app.state, "desktop.help.title")))
                        .child(div().text_xs().text_color(muted).child(tr(&app.state, "desktop.help.esc_close"))),
                )
                .child(
                    div()
                        .id("help-body")
                        .flex()
                        .flex_row()
                        .gap_6()
                        .max_h(px(510.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.help_scroll)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .w(px(320.0))
                                .flex_none()
                                .children(KEY_HELP.iter().map(|(title, rows)| section(title, rows))),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_grow(1.0)
                                .child(
                                    div()
                                        .pb_1()
                                        .text_xs()
                                        .text_color(fg)
                                        .child(tr(&app.state, "desktop.help.commands")),
                                )
                                .children(echo_core::commands::COMMANDS.iter().map(
                                    |(usage, description)| {
                                        div()
                                            .flex()
                                            .flex_col()
                                            .py_0p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(accent)
                                                    .child(SharedString::from(format!(":{usage}"))),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(*description),
                                            )
                                    },
                                )),
                        ),
                ),
        )
}

/// The settings sheet.
///
/// Every control here runs a `:` command through `echo_core::commands::run` rather than
/// touching the config itself, so the GUI and the command bar cannot drift apart. The three
/// audio-quality keys are the exception — they have no command — and write the config directly.
pub fn settings_modal(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let config = app.state.ui.library_config.clone();
    let path_focused = app.settings_path_focus.is_focused(window);
    let path_value = app.settings_path_input.clone();
    // Owns its language so translation never borrows `app` across the listener closures below.
    let s = {
        let lang = config.language.clone();
        move |key: &str| SharedString::from(echo_core::i18n::t(key, &lang))
    };

    // A labelled row with its control on the right. Labels and hints arrive translated.
    let row = |label: SharedString, hint: Option<SharedString>, control: AnyElement| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .py_1p5()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().text_color(fg).child(label))
                    .when_some(hint, |el, hint| {
                        el.child(div().text_xs().text_color(muted).child(hint))
                    }),
            )
            .child(control)
    };

    let heading = |label: SharedString| {
        div()
            .pt_3()
            .pb_1()
            .text_xs()
            .text_color(muted)
            .child(label)
    };

    // A segmented control: one button per option, the active one accented.
    let choices = |id: &'static str,
                   options: Vec<(SharedString, String, bool)>,
                   cx: &mut Context<EchoApp>| {
        div()
            .id(id)
            .flex()
            .flex_row()
            .gap_1()
            .children(options.into_iter().map(|(label, cmd, active)| {
                div()
                    .id(SharedString::from(format!("{id}-{label}")))
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .border_1()
                    .border_color(if active { accent } else { palette.menu_border })
                    .text_xs()
                    .text_color(if active { accent } else { muted })
                    .cursor_pointer()
                    .hover(move |style| style.bg(palette.menu_hover))
                    .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                        this.run_setting(cmd.clone(), cx);
                    }))
                    .child(label)
            }))
            .into_any_element()
    };

    let language = config.language.clone();
    let language_row = row(
        s("desktop.settings.language"),
        None,
        choices(
            "lang",
            vec![
                ("English".into(), "lang en".into(), language == "en"),
                ("简体".into(), "lang zh-CN".into(), language == "zh-CN"),
                ("繁體".into(), "lang zh-TW".into(), language == "zh-TW"),
            ],
            cx,
        ),
    );

    let visualizer_row = row(
        s("desktop.settings.visualizer"),
        Some(s("desktop.settings.visualizer_desc")),
        choices(
            "vis",
            vec![
                (s("ui.on"), "vis".into(), config.enable_visualizer),
                (s("ui.off"), "vis".into(), !config.enable_visualizer),
            ],
            cx,
        ),
    );

    let bins_row = row(
        s("desktop.settings.bins"),
        Some("5–32".into()),
        choices(
            "visbins",
            [7usize, 9, 16, 24, 32]
                .into_iter()
                .map(|n| {
                    (
                        SharedString::from(n.to_string()),
                        format!("visbins {n}"),
                        config.vis_bins == n,
                    )
                })
                .collect(),
            cx,
        ),
    );

    let pixelate_row = row(
        s("desktop.settings.pixelate"),
        Some(s("desktop.settings.pixelate_desc")),
        choices(
            "pixelate",
            [0u32, 8, 16, 32]
                .into_iter()
                .map(|n| {
                    (
                        if n == 0 {
                            s("ui.off")
                        } else {
                            SharedString::from(n.to_string())
                        },
                        format!("pixelate {n}"),
                        config.cover_img_pixels == n,
                    )
                })
                .collect(),
            cx,
        ),
    );

    let index_row = row(
        s("desktop.settings.numbering"),
        Some(s("desktop.settings.numbering_desc")),
        choices(
            "index",
            vec![
                ("1".into(), "index 1".into(), config.track_index_base == 1),
                ("0".into(), "index 0".into(), config.track_index_base == 0),
            ],
            cx,
        ),
    );

    let relative_row = row(
        s("desktop.settings.relative"),
        Some(s("desktop.settings.relative_desc")),
        choices(
            "relative",
            vec![
                (
                    s("ui.on"),
                    "relative on".into(),
                    config.relative_line_numbers,
                ),
                (
                    s("ui.off"),
                    "relative off".into(),
                    !config.relative_line_numbers,
                ),
            ],
            cx,
        ),
    );

    let sort_row = row(
        s("desktop.settings.order"),
        Some(s("desktop.settings.order_desc")),
        choices(
            "libsort",
            vec![
                (
                    // The manual drag-and-drop order.
                    s("desktop.settings.order_custom"),
                    "sort default".into(),
                    config.sort_mode == echo_core::config::SortMode::Default,
                ),
                (
                    s("desktop.settings.order_alpha"),
                    "sort alpha".into(),
                    config.sort_mode == echo_core::config::SortMode::Alphabetical,
                ),
                (
                    s("desktop.settings.order_creator"),
                    "sort creator".into(),
                    config.sort_mode == echo_core::config::SortMode::Creator,
                ),
            ],
            cx,
        ),
    );

    let bitrate_row = row(
        s("desktop.settings.quality"),
        Some(s("desktop.settings.quality_desc")),
        div()
            .flex()
            .flex_row()
            .gap_1()
            .children([96u32, 160, 320].into_iter().map(|rate| {
                let active = config.bitrate == rate;
                div()
                    .id(SharedString::from(format!("bitrate-{rate}")))
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .border_1()
                    .border_color(if active { accent } else { palette.menu_border })
                    .text_xs()
                    .text_color(if active { accent } else { muted })
                    .cursor_pointer()
                    .hover(move |style| style.bg(palette.menu_hover))
                    .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                        this.set_audio_quality(|config| config.bitrate = rate, cx);
                    }))
                    .child(SharedString::from(rate.to_string()))
            }))
            .into_any_element(),
    );

    let normalisation = config.normalisation;
    let norm_on_label = s("ui.on");
    let norm_off_label = s("ui.off");
    let normalisation_row = row(
        s("desktop.settings.normalisation"),
        Some(s("desktop.settings.normalisation_desc")),
        div()
            .flex()
            .flex_row()
            .gap_1()
            .children([true, false].into_iter().map(|on| {
                let active = normalisation == on;
                div()
                    .id(if on { "norm-on" } else { "norm-off" })
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .border_1()
                    .border_color(if active { accent } else { palette.menu_border })
                    .text_xs()
                    .text_color(if active { accent } else { muted })
                    .cursor_pointer()
                    .hover(move |style| style.bg(palette.menu_hover))
                    .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                        this.set_audio_quality(|config| config.normalisation = on, cx);
                    }))
                    .child(if on {
                        norm_on_label.clone()
                    } else {
                        norm_off_label.clone()
                    })
            }))
            .into_any_element(),
    );

    let pregain = config.normalisation_pregain;
    let step_button = |id: &'static str, label: &'static str, delta: f64, cx: &mut Context<EchoApp>| {
        div()
            .id(id)
            .w(px(22.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .text_xs()
            .text_color(muted)
            .cursor_pointer()
            .hover(move |style| style.bg(palette.menu_hover))
            .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                this.set_audio_quality(
                    |config| {
                        // Matches librespot's usable range; beyond this the limiter dominates.
                        config.normalisation_pregain = (config.normalisation_pregain + delta)
                            .clamp(-10.0, 10.0);
                    },
                    cx,
                );
            }))
            .child(label)
    };
    let pregain_row = row(
        s("desktop.settings.pregain"),
        Some(s("desktop.settings.pregain_desc")),
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(step_button("pregain-down", "−", -0.5, cx))
            .child(
                div()
                    .w(px(52.0))
                    .text_xs()
                    .text_color(fg)
                    .text_center()
                    .child(SharedString::from(format!("{pregain:+.1} dB"))),
            )
            .child(step_button("pregain-up", "+", 0.5, cx))
            .into_any_element(),
    );

    let local_field = div()
        .id("settings-localpath")
        .key_context(crate::SEARCH_CONTEXT)
        .track_focus(&app.settings_path_focus)
        .on_key_down(cx.listener(|this: &mut EchoApp, event, window, cx| {
            this.handle_settings_path_key(event, window, cx)
        }))
        .w_full()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(if path_focused {
            accent
        } else {
            palette.menu_border
        })
        .text_xs()
        .text_color(if path_value.is_empty() { muted } else { fg })
        .whitespace_nowrap()
        .overflow_hidden()
        .cursor_pointer()
        .on_click(cx.listener(|this: &mut EchoApp, _event, window, cx| {
            let handle = this.settings_path_focus.clone();
            window.focus(&handle, cx);
            cx.notify();
        }))
        .child(SharedString::from(if path_focused {
            crate::text_with_cursor(&path_value, app.settings_path_cursor)
        } else if path_value.is_empty() {
            s("desktop.settings.folder_empty").to_string()
        } else {
            path_value
        }));

    let small_button = |id: &'static str, label: SharedString, cmd: &'static str, cx: &mut Context<EchoApp>| {
        div()
            .id(id)
            .flex_none()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .text_xs()
            .text_color(muted)
            .cursor_pointer()
            .hover(move |style| style.bg(palette.menu_hover))
            .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                this.run_setting(cmd.to_string(), cx);
            }))
            .child(label)
    };

    // --- Updates ---------------------------------------------------------------------
    // The one control here that is neither a `:` command nor a config write: it runs a network
    // call, so its label and action both come from `app.update_state`.
    let version_row = row(
        s("desktop.settings.updates.version"),
        None,
        div()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(if echo_core::update::is_dev_build() {
                format!(
                    "{} {}",
                    echo_core::update::current_version(),
                    echo_core::i18n::t("desktop.settings.updates.dev", &config.language)
                )
            } else {
                echo_core::update::current_version().to_string()
            }))
            .into_any_element(),
    );

    let translate = |key: &str, value: &str| -> SharedString {
        SharedString::from(echo_core::i18n::t(key, &config.language).replace("{}", value))
    };
    let (update_label, update_hint, update_active) = match &app.update_state {
        UpdateState::Idle => (s("desktop.settings.updates.check"), None, true),
        UpdateState::Checking => (s("desktop.settings.updates.checking"), None, false),
        UpdateState::UpToDate => (s("desktop.settings.updates.uptodate"), None, true),
        UpdateState::Available(release) => (
            translate("desktop.settings.updates.install", release.version()),
            None,
            true,
        ),
        UpdateState::Downloading(percent) => (
            translate("desktop.settings.updates.downloading", &percent.to_string()),
            None,
            false,
        ),
        UpdateState::Ready(version) => (
            s("desktop.settings.updates.restart"),
            Some(translate("desktop.settings.updates.installed", version)),
            false,
        ),
        UpdateState::Failed(message) => (
            s("desktop.settings.updates.retry"),
            Some(SharedString::from(message.clone())),
            true,
        ),
        // Nothing in-app can fix this one, so the button leaves for the releases page.
        UpdateState::Blocked(message) => (
            s("desktop.settings.updates.open_releases"),
            Some(SharedString::from(message.clone())),
            true,
        ),
    };

    let update_button = div()
        .id("settings-update")
        .flex_none()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(if update_active { accent } else { palette.menu_border })
        .text_xs()
        .text_color(if update_active { accent } else { muted })
        .when(update_active, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(palette.menu_hover))
                .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                    match &this.update_state {
                        UpdateState::Available(_) => this.install_update(cx),
                        UpdateState::Blocked(_) => {
                            let _ = webbrowser::open(&echo_core::update::releases_url());
                        }
                        _ => this.check_for_updates(cx),
                    }
                }))
        })
        .child(update_label)
        .into_any_element();

    let update_row = row(
        s("desktop.settings.updates.label"),
        update_hint.or_else(|| Some(s("desktop.settings.updates.desc"))),
        update_button,
    );

    div()
        .id("settings-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.settings_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("settings-panel")
                .w(px(520.0))
                .max_h(px(560.0))
                .rounded_lg()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .p_4()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .child(div().text_sm().text_color(fg).child(s("desktop.settings.title")))
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(s("desktop.settings.subtitle")),
                        ),
                )
                .child(
                    div()
                        .id("settings-list")
                        .flex()
                        .flex_col()
                        .max_h(px(470.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.settings_scroll)
                        .child(heading(s("desktop.settings.appearance")))
                        .child(row(
                            s("desktop.theme"),
                            None,
                            div()
                                .id("settings-theme")
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.menu_border)
                                .text_xs()
                                .text_color(muted)
                                .cursor_pointer()
                                .hover(move |style| style.bg(palette.menu_hover))
                                .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                                    this.settings_open = false;
                                    this.toggle_themes(cx);
                                }))
                                .child(SharedString::from(
                                    config.active_theme.clone().unwrap_or_else(|| "echo".into()),
                                ))
                                .into_any_element(),
                        ))
                        .child(language_row)
                        .child(pixelate_row)
                        .child(heading(s("desktop.settings.library")))
                        .child(sort_row)
                        .child(index_row)
                        .child(relative_row)
                        .child(heading(s("desktop.settings.playback")))
                        .child(visualizer_row)
                        .child(bins_row)
                        .child(heading(s("desktop.settings.audio")))
                        .child(bitrate_row)
                        .child(normalisation_row)
                        .child(pregain_row)
                        .child(heading(s("desktop.settings.local")))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .py_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(s("desktop.settings.folder_desc")),
                                )
                                .child(local_field)
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_1()
                                        .child(small_button(
                                            "settings-rescan",
                                            s("desktop.settings.rescan"),
                                            "rescanlocal",
                                            cx,
                                        )),
                                ),
                        )
                        .child(heading(s("desktop.settings.updates.section")))
                        .child(version_row)
                        .child(update_row),
                ),
        )
}

/// The Spotify Connect device picker, painted over everything else.
pub fn device_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let selected_bg = palette.menu_selected;
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
        .occlude()
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
                .border_color(palette.menu_border)
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
                        .child(tr(&app.state, "desktop.devices_title")),
                )
                .child(if devices.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.devices_none"))
                        .into_any_element()
                } else {
                    div()
                        .id("device-list")
                        .flex()
                        .flex_col()
                        .max_h(px(340.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.device_modal_scroll)
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
                                .when(ix != selected, |el| {
                                    el.hover(move |style| style.bg(palette.menu_hover))
                                })
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
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let selected_bg = palette.menu_selected;
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
        // Modal: the backdrop swallows pointer events, so a scroll wheel over it can't reach the
        // track list behind the overlay.
        .occlude()
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
                .border_color(palette.menu_border)
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
                        .child(tr(&app.state, "desktop.playlist_add_title")),
                )
                .child(if choices.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.playlist_add_none"))
                        .into_any_element()
                } else {
                    div()
                        .id("playlist-add-list")
                        .flex()
                        .flex_col()
                        .max_h(px(340.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.playlist_modal_scroll)
                        .children(choices.into_iter().enumerate().map(|(ix, (name, local))| {
                            let local_label = tr(&app.state, "ui.local");
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
                                .when(ix != selected, |el| {
                                    el.hover(move |style| style.bg(palette.menu_hover))
                                })
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
                                        div().flex_none().text_xs().text_color(muted).child(local_label),
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
            AppMode::Setup => tr(&app.state, "desktop.setup.waiting"),
            _ => tr(&app.state, "desktop.setup.authenticating"),
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
                .font_family(app.mono_font.clone())
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
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let mut items: Vec<(SharedString, MenuAction, bool)> = Vec::new();
    if app.state.ui.active_library_tab == LibraryTab::Albums {
        if app.state.data.saved_albums.get(menu.index).is_some() {
            items.push((tr(&app.state, "desktop.menu.open"), MenuAction::Open, false));
            items.push((
                tr(&app.state, "desktop.menu.remove_library"),
                MenuAction::RemoveAlbum,
                true,
            ));
        }
    } else if let Some(node) = app.state.data.library_view.get(menu.index) {
        match node {
            LibraryNode::Folder(_) => {
                items.push((tr(&app.state, "desktop.menu.rename"), MenuAction::Rename, false));
                items.push((
                    tr(&app.state, "desktop.menu.delete_folder"),
                    MenuAction::DeleteFolder,
                    true,
                ));
            }
            LibraryNode::Playlist { playlist, indent } => {
                items.push((tr(&app.state, "desktop.menu.open"), MenuAction::Open, false));
                let special = playlist.id == "LIKED_SONGS" || playlist.id == "local-library";
                let local = playlist.id.starts_with("local-playlist:");
                if !special {
                    let pinned = app.state.ui.library_config.pinned.contains(&playlist.id);
                    items.push((
                        tr(
                            &app.state,
                            if pinned { "desktop.menu.unpin" } else { "desktop.menu.pin" },
                        ),
                        MenuAction::TogglePin,
                        false,
                    ));
                }
                if !special {
                    items.push((tr(&app.state, "desktop.menu.rename"), MenuAction::Rename, false));
                }
                if *indent >= 1 {
                    items.push((
                        tr(&app.state, "desktop.menu.remove_folder"),
                        MenuAction::RemoveFromFolder,
                        false,
                    ));
                }
                let own = app.state.data.user_id.as_ref() == Some(&playlist.owner_id);
                if local || (!special && own) {
                    items.push((
                        tr(&app.state, "desktop.menu.delete_playlist"),
                        MenuAction::DeletePlaylist,
                        true,
                    ));
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
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                // Clicks on the menu itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(items.into_iter().map(|(label, action, danger)| {
                    // Inset and rounded like the list rows: a full-bleed highlight would square
                    // off the panel's rounded corners on the first and last item.
                    div()
                        .id(label.clone())
                        .mx_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_sm()
                        .text_color(if danger { danger_color } else { fg })
                        .hover(move |style| style.bg(palette.menu_hover))
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
/// The open track menu's rows: `(label, item, is_destructive)`. Keyboard selection indexes
/// into this, so it is the single source of the menu's contents.
pub fn track_menu_items(app: &EchoApp) -> Vec<(SharedString, TrackMenuItem, bool)> {
    let Some(menu) = app.track_menu.as_ref() else {
        return Vec::new();
    };
    let ctx = &menu.ctx;
    let mut items: Vec<(SharedString, TrackMenuItem, bool)> = ctx
        .actions()
        .into_iter()
        .map(|action| {
            (
                echo_core::action_menu::label(&app.state, ctx, action).into(),
                TrackMenuItem::Action(action),
                false,
            )
        })
        .collect();
    // Queue rows have no playlist behind them — `active_tracklist_context` is whatever was
    // last browsed, so removing would hit the wrong playlist.
    if app.state.ui.active_view != ActiveView::Queue
        && app
            .state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.can_modify_playlist(app.state.data.user_id.as_ref()))
    {
        items.push((
            tr(&app.state, "desktop.menu.remove_playlist"),
            TrackMenuItem::RemoveFromPlaylist,
            true,
        ));
    }
    items
}

pub fn track_context_menu(
    app: &mut EchoApp,
    window: &Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let menu = app
        .track_menu
        .clone()
        .expect("track_context_menu rendered without state");
    let items = track_menu_items(app);
    let add_row = app.track_menu_add_row();
    let row_bounds = app.submenu_row_bounds.clone();
    // Built first: it reads `app` immutably and hangs off the menu as a sibling, because the
    // menu panel clips its own children.
    let submenu = menu
        .submenu
        .map(|choice| playlist_submenu(app, choice, window.viewport_size(), cx));

    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let selected = menu.selected;
    let submenu_open = menu.submenu.is_some();

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
        .when(menu.position.is_none(), |el| {
            // Keyboard-opened: no click to anchor to, so center it like the other modals.
            el.flex().items_center().justify_center()
        })
        .child(
            div()
                .id("track-menu")
                .when_some(menu.position, |el, position| {
                    el.absolute().left(position.x).top(position.y)
                })
                .w(px(210.0))
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                // Clicks on the menu itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(items.into_iter().enumerate().map(|(ix, (label, item, danger))| {
                    let is_add = add_row == Some(ix);
                    let is_selected = ix == selected;
                    // A hover-opened flyout leaves the keyboard selection where it was, so its
                    // parent row keeps the hover wash for as long as the flyout is up — the
                    // selected wash would read as a second cursor.
                    let holds_submenu = is_add && submenu_open && !is_selected;
                    let row_bounds = row_bounds.clone();
                    div()
                        .id(label.clone())
                        .relative()
                        .mx_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .text_color(if danger { danger_color } else { fg })
                        .when(is_selected, |el| el.bg(palette.menu_selected))
                        .when(holds_submenu, |el| el.bg(palette.menu_hover))
                        .when(!is_selected, |el| {
                            el.hover(move |style| style.bg(palette.menu_hover))
                        })
                        .cursor_pointer()
                        // Pointer moves drive the flyout: opening it on the add row, closing it
                        // on any other row the pointer settles on. See
                        // [`EchoApp::hover_track_menu_row`].
                        .on_mouse_move(cx.listener(
                            move |this: &mut EchoApp, event: &gpui::MouseMoveEvent, _window, cx| {
                                this.hover_track_menu_row(ix, event.position, cx);
                            },
                        ))
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            if is_add {
                                this.open_playlist_submenu(true, cx);
                            } else {
                                this.run_track_menu_action(item, cx);
                            }
                        }))
                        .when(is_add, |el| {
                            // The flyout is a sibling of the menu, so it needs this row's
                            // rectangle in window space to anchor to.
                            el.child(
                                canvas(move |bounds, _window, _cx| row_bounds.set(bounds), |_, _, _, _| {})
                                    .absolute()
                                    .size_full(),
                            )
                        })
                        .child(div().flex_grow(1.0).child(label))
                        .when(is_add, |el| {
                            el.child(div().flex_none().text_xs().text_color(muted).child("▸"))
                        })
                })),
        )
        .children(submenu)
}

/// The add-to-playlist flyout: the writable playlists, hanging off the track menu's "Add to
/// playlist" row — to its right, or to its left where the window edge leaves no room. A sibling
/// of the menu panel rather than a child of that row, which `overflow_hidden` would clip.
fn playlist_submenu(
    app: &EchoApp,
    choice: usize,
    viewport: gpui::Size<gpui::Pixels>,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());

    let row = app.submenu_row_bounds.get();
    let bounds = app.submenu_bounds.clone();
    let local_label = tr(&app.state, "ui.local");
    let empty_label = tr(&app.state, "desktop.playlist_add_none");
    let choices: Vec<(SharedString, bool)> =
        echo_core::action_menu::playlist_add_choices(&app.state)
            .into_iter()
            .map(|playlist| (SharedString::from(playlist.name), playlist.owner_id == "local"))
            .collect();

    // A right-click near the window's right edge would otherwise hang the flyout off-screen,
    // so it flips to the menu's other side; the same for a menu low enough that the list would
    // run past the bottom. Height is estimated from the row count rather than measured — it is
    // only needed to keep the panel on screen, and measuring costs a frame of jitter.
    let height = px((choices.len().max(1) as f32 * SUBMENU_ROW_HEIGHT + 10.0).min(SUBMENU_MAX_H));
    let flipped = row.right() + px(SUBMENU_WIDTH) > viewport.width - px(8.0);
    let left = if flipped { row.left() - px(SUBMENU_WIDTH) } else { row.right() };
    let top = (row.top() - px(5.0)).min(viewport.height - height - px(8.0)).max(px(8.0));

    div()
        .id("track-menu-submenu")
        .absolute()
        // `py_1` plus the border: lines the first choice up with the row it hangs off.
        .left(left)
        .top(top)
        .w(px(SUBMENU_WIDTH))
        .rounded_md()
        .border_1()
        .border_color(palette.menu_border)
        .bg(surface)
        .py_1()
        .flex()
        .flex_col()
        // Swallows clicks and wheel events so neither reaches the backdrop or the list behind.
        .occlude()
        .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
        .child(
            canvas(move |painted, _window, _cx| bounds.set(painted), |_, _, _, _| {})
                .absolute()
                .size_full(),
        )
        .child(if choices.is_empty() {
            div()
                .mx_1()
                .px_2()
                .py_1()
                .text_sm()
                .text_color(muted)
                .child(empty_label)
                .into_any_element()
        } else {
            div()
                .id("track-menu-submenu-list")
                .flex()
                .flex_col()
                .max_h(px(SUBMENU_MAX_H - 10.0))
                .overflow_y_scroll()
                .track_scroll(&app.submenu_scroll)
                .children(choices.into_iter().enumerate().map(|(ix, (name, local))| {
                    let is_selected = ix == choice;
                    let local_label = local_label.clone();
                    div()
                        .id(ix)
                        .mx_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_sm()
                        .when(is_selected, |el| el.bg(palette.menu_selected))
                        .when(!is_selected, |el| {
                            el.hover(move |style| style.bg(palette.menu_hover))
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            this.commit_playlist_submenu(ix, cx);
                        }))
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
                                div().flex_none().text_xs().text_color(muted).child(local_label),
                            )
                        })
                }))
                .into_any_element()
        })
        .into_any_element()
}

/// Confirm dialog for whichever destructive prompt is staged, resolved through the same
/// `intent::confirm_prompt`/`cancel_prompt` the TUI's y/n keys use.
pub fn prompt_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = theme.error.gpui(WINDOW_FG());

    let ui = &app.state.ui;
    let delete_label = tr(&app.state, "desktop.prompt.delete");
    let remove_label = tr(&app.state, "desktop.prompt.remove");
    let (message, confirm_label): (String, SharedString) =
        if let Some(name) = &ui.folder_delete_prompt {
            (
                tr(&app.state, "desktop.prompt.delete_folder")
                    .replace("{}", name),
                delete_label,
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
                .unwrap_or_else(|| tr(&app.state, "desktop.prompt.this_playlist").to_string());
            (
                tr(&app.state, "desktop.prompt.delete_playlist").replace("{}", &name),
                delete_label,
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
                .unwrap_or_else(|| tr(&app.state, "desktop.prompt.this_album").to_string());
            (
                tr(&app.state, "desktop.prompt.remove_album").replace("{}", &name),
                remove_label,
            )
        } else if ui.track_delete_prompt.is_some() {
            (
                tr(&app.state, "desktop.prompt.remove_tracks").to_string(),
                remove_label,
            )
        } else {
            (
                tr(&app.state, "desktop.prompt.remove_liked").to_string(),
                remove_label,
            )
        };

    let button = |id: &'static str,
                  label: SharedString,
                  color: gpui::Hsla,
                  border: gpui::Hsla,
                  hover_bg: gpui::Hsla| {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(border)
            .text_sm()
            .text_color(color)
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
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
                .border_color(palette.menu_border)
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
                        .child(button("prompt-cancel", tr(&app.state, "desktop.prompt.cancel"), muted, palette.menu_border, palette.menu_hover).on_click(cx.listener(
                            |this: &mut EchoApp, _event, _window, cx| {
                                echo_core::intent::cancel_prompt(&mut this.state);
                                cx.notify();
                            },
                        )))
                        .child(button("prompt-confirm", confirm_label, danger_color, palette.danger_border, palette.danger_wash).on_click(
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

#[cfg(test)]
mod immersive_tests {
    use super::*;
    use echo_core::models::LyricLine;

    fn line(start_ms: u32) -> LyricLine {
        LyricLine {
            start_ms,
            text: String::new(),
        }
    }

    #[test]
    fn current_lyric_is_last_started_line() {
        let lines = [line(0), line(1_000), line(2_000)];
        assert_eq!(current_lyric_index(&lines, 0), 0);
        assert_eq!(current_lyric_index(&lines, 1_500), 1);
        assert_eq!(current_lyric_index(&lines, 9_000), 2);
    }

    #[test]
    fn current_lyric_before_first_line_is_first() {
        let lines = [line(500), line(1_000)];
        assert_eq!(current_lyric_index(&lines, 100), 0);
        assert_eq!(current_lyric_index(&[], 100), 0);
    }

    #[test]
    fn cover_edge_fits_the_narrower_axis() {
        // Wide body: the half-width minus margins bounds the square (504), scaled to 378.
        assert_eq!(immersive_cover_edge(1200.0, 900.0), 378.0);
        // Short body: the height minus margins and the caption bounds it (316), scaled to 237.
        assert_eq!(immersive_cover_edge(1600.0, 500.0), 237.0);
    }

    #[test]
    fn lyric_opacity_eases_to_transparent_past_the_edge() {
        assert_eq!(lyric_opacity(0.0, 5.0), 1.0);
        assert!(lyric_opacity(1.0, 5.0) > 0.9);
        assert!(lyric_opacity(4.0, 5.0) < 0.4);
        assert!(lyric_opacity(4.0, 5.0) > 0.3);
        assert_eq!(lyric_opacity(5.0, 5.0), 0.0);
        assert_eq!(lyric_opacity(9.0, 5.0), 0.0);
    }

    #[test]
    fn lyric_window_holds_an_odd_number_of_whole_rows() {
        assert_eq!(lyric_window_rows(360.0, 40.0), 9);
        assert_eq!(lyric_window_rows(434.0, 40.0), 9);
        assert_eq!(lyric_window_rows(479.0, 40.0), 11);
        assert_eq!(lyric_window_rows(0.0, 40.0), 1);
        assert_eq!(lyric_window_rows(80.0, 40.0), 1);
    }

    #[test]
    fn cover_edge_is_capped_and_never_negative() {
        assert_eq!(immersive_cover_edge(4000.0, 3000.0), IMMERSIVE_COVER_MAX);
        assert_eq!(immersive_cover_edge(50.0, 50.0), 0.0);
    }
}
