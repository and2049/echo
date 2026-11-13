//! Horizontally scrolling rows of cards (Home shelves, the search page's sections) and the
//! scrollbar under each.

use super::*;

/// A row of cards that scrolls sideways inside a vertically scrolling page.
///
/// Locked to its own axis, so an up/down gesture over it scrolls the page instead. Mice on
/// Windows and Linux send shift+wheel as vertical motion; that scrolls the row.
pub(super) fn shelf_row(
    id: impl Into<gpui::ElementId>,
    scroll: &gpui::ScrollHandle,
    cx: &mut Context<EchoApp>,
) -> Stateful<Div> {
    let wheel_scroll = scroll.clone();
    div()
        .id(id)
        .flex()
        .gap_3()
        // Scroll the end spacing with the cards instead of clipping before the gutter.
        .px_4()
        .overflow_x_scroll()
        .restrict_scroll_to_axis()
        .track_scroll(scroll)
        .on_scroll_wheel(cx.listener(
            move |_: &mut EchoApp, event: &gpui::ScrollWheelEvent, window, cx| {
                if event.modifiers.shift {
                    let delta = event.delta.pixel_delta(window.line_height());
                    let x = wheel_scroll.offset().x
                        + if delta.x != px(0.0) { delta.x } else { delta.y };
                    wheel_scroll.set_offset(gpui::point(
                        x.clamp(-wheel_scroll.max_offset().x, px(0.0)),
                        px(0.0),
                    ));
                    cx.stop_propagation();
                    cx.notify();
                }
            },
        ))
}

/// Horizontal scrollbar under a shelf, colored like the seek bar: `border` track, `accent`
/// thumb. Dragging it scrubs the shelf.
pub(super) fn shelf_scrollbar(
    scroll: gpui::ScrollHandle,
    palette: DesktopPalette,
    accent: Hsla,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let paint_scroll = scroll.clone();
    div()
        .id("shelf-scrollbar")
        .h(px(12.0))
        .w_full()
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this: &mut EchoApp, event: &MouseDownEvent, _, cx| {
                this.begin_shelf_scrub(scroll.clone(), event.position.x);
                cx.stop_propagation();
                cx.notify();
            }),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let max = f32::from(paint_scroll.max_offset().x);
                    if max <= 0.0 {
                        return;
                    }
                    let width = f32::from(bounds.size.width);
                    let thumb = (width * width / (width + max)).max(24.0).min(width);
                    let left = -f32::from(paint_scroll.offset().x) / max * (width - thumb);
                    window.paint_quad(gpui::fill(
                        gpui::Bounds::new(
                            bounds.origin + gpui::point(px(0.0), px(4.0)),
                            gpui::size(bounds.size.width, px(4.0)),
                        ),
                        palette.border,
                    ));
                    window.paint_quad(gpui::fill(
                        gpui::Bounds::new(
                            bounds.origin + gpui::point(px(left), px(4.0)),
                            gpui::size(px(thumb), px(4.0)),
                        ),
                        accent,
                    ));
                },
            )
            .size_full(),
        )
}
