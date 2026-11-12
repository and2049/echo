use super::*;

// Native caption metrics: Windows titlebars are a fixed 32px, macOS gets a touch more.
pub(super) const TITLEBAR_HEIGHT: f32 = if cfg!(target_os = "windows") {
    32.0
} else {
    34.0
};

// Zed's measured inset for the macOS traffic lights (71px, +1px window border).
const TRAFFIC_LIGHT_PADDING: f32 = 71.0;

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
        Some(tiling) => (!tiling.top && !tiling.right).then(|| px(CLIENT_CONTENT_RADIUS)),
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
                .child(
                    svg()
                        .path("icons/music-note.svg")
                        .size(px(14.0))
                        .text_color(fg),
                )
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
    let hover_bg: gpui::Hsla = if close {
        crate::theme::CLOSE_RED()
    } else {
        hover
    };
    div()
        .id(id)
        .group(id)
        .occlude()
        .window_control_area(area)
        .when(cfg!(target_os = "linux"), |el| {
            el.on_click(move |_event, window, cx| match area {
                gpui::WindowControlArea::Min => window.minimize_window(),
                gpui::WindowControlArea::Max => window.zoom_window(),
                // Not `remove_window` directly: that skips `on_window_should_close`, so the
                // action persists the rectangle and decides tray-or-quit first.
                gpui::WindowControlArea::Close => {
                    window.dispatch_action(Box::new(crate::CloseWindow), cx)
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
                .when(close, |el| {
                    el.group_hover(id, |style| style.text_color(gpui::white()))
                }),
        )
}

/// Corner radius and drop-shadow depth for a client-decorated window, matching what GNOME and
/// Zed use so echo sits alongside them without looking off.
const CLIENT_CORNER_RADIUS: f32 = 10.0;

const CLIENT_BORDER_WIDTH: f32 = 1.0;

// Content sits inside the frame border: reduce its radius by the same inset so both
// arcs share a center instead of leaving a sliver between their antialiased edges.
const CLIENT_CONTENT_RADIUS: f32 = CLIENT_CORNER_RADIUS - CLIENT_BORDER_WIDTH;

/// The radius DWM clips an unmaximized window to on Windows 11.
const WIN_CORNER_RADIUS: f32 = 8.0;

pub(super) const CLIENT_SHADOW: f32 = 10.0;

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

/// Width of the visible app, excluding client-drawn shadow/resize margins.
pub(crate) fn client_content_width(window: &Window) -> f32 {
    let inset = client_corners(window).map_or(0.0, |tiling| {
        f32::from(!tiling.left) * CLIENT_SHADOW + f32::from(!tiling.right) * CLIENT_SHADOW
    });
    (f32::from(window.viewport_size().width) - inset).max(0.0)
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
    corner_radii(corners, which, CLIENT_CONTENT_RADIUS)
}

fn corner_radii(
    corners: Option<gpui::Tiling>,
    which: ClientCorners,
    radius: f32,
) -> gpui::Corners<gpui::Pixels> {
    let Some(tiling) = corners else {
        return gpui::Corners::default();
    };
    let all = matches!(which, ClientCorners::All);
    let radius = |rounded: bool| {
        if rounded { px(radius) } else { px(0.0) }
    };
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
        (
            "resize-top-left",
            gpui::ResizeEdge::TopLeft,
            tiling.top || tiling.left,
        ),
        (
            "resize-top-right",
            gpui::ResizeEdge::TopRight,
            tiling.top || tiling.right,
        ),
        (
            "resize-bottom-left",
            gpui::ResizeEdge::BottomLeft,
            tiling.bottom || tiling.left,
        ),
        (
            "resize-bottom-right",
            gpui::ResizeEdge::BottomRight,
            tiling.bottom || tiling.right,
        ),
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
                .map(|el| {
                    let radii =
                        corner_radii(Some(tiling), ClientCorners::All, CLIENT_CORNER_RADIUS);
                    el.rounded_tl(radii.top_left)
                        .rounded_tr(radii.top_right)
                        .rounded_bl(radii.bottom_left)
                        .rounded_br(radii.bottom_right)
                })
                // A hairline outline stands in for the frame the compositor is not drawing, so
                // the window still reads as one against a same-coloured background behind it.
                .border(px(CLIENT_BORDER_WIDTH))
                .border_color(palette.border)
                .when(!tiling.is_tiled(), |el| {
                    el.shadow(vec![
                        gpui::BoxShadow::new(px(0.0), px(2.0), gpui::hsla(0.0, 0.0, 0.0, 0.36))
                            .blur_radius(shadow / 2.0),
                    ])
                })
                .child(root),
        )
        .children(
            edges
                .into_iter()
                .filter(|&(_id, _edge, tiled)| resizable && !tiled)
                .map(|(id, edge, _tiled)| resize_handle(id, edge)),
        )
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
