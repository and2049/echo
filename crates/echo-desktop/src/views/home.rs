use super::*;

fn home_item_title(
    state: &echo_core::app::AppState,
    item: &echo_core::home::HomeItem,
) -> SharedString {
    if item.id == "LIKED_SONGS" {
        tr(state, "desktop.home_liked_songs")
    } else {
        item.title.clone().into()
    }
}

fn home_play_button(
    item: echo_core::home::HomeItem,
    search_row: Option<echo_core::search::SearchRow>,
    accent: Hsla,
    fg: Hsla,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    div()
        .id("home-play")
        .absolute()
        .right(px(8.0))
        .bottom(px(8.0))
        .size(px(36.0))
        .rounded_full()
        .bg(accent)
        .flex()
        .items_center()
        .justify_center()
        .invisible()
        .group_hover("home-item", |style| style.visible())
        .cursor_pointer()
        .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
            cx.stop_propagation();
            window.focus(&this.focus_handle, cx);
            let event = if let Some(row) = search_row {
                echo_core::intent::play_search_row(&mut this.state, row)
            } else {
                echo_core::intent::play_home_item(&mut this.state, item.clone())
            };
            if let Some(event) = event {
                this.dispatch(event);
            }
            cx.notify();
        }))
        .child(svg().path("icons/play.svg").size(px(18.0)).text_color(fg))
}

pub(super) fn home_card(
    app: &mut EchoApp,
    item: echo_core::home::HomeItem,
    index: usize,
    quick: bool,
    search_row: Option<echo_core::search::SearchRow>,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    use echo_core::home::HomeItemKind;
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let background = theme.background.gpui(crate::theme::WINDOW_BG());
    let title = home_item_title(&app.state, &item);
    let subtitle = if item.kind == HomeItemKind::Artist {
        tr(&app.state, "desktop.home_artist")
    } else if let Some(year) = &item.release_year {
        year.clone().into()
    } else {
        item.subtitle.clone().into()
    };
    let selected = if search_row.is_some() {
        app.state.ui.selected_search_index == index
    } else {
        app.state.ui.active_view == ActiveView::Home && app.state.ui.selected_home_index == index
    };
    let edge = if quick { 56.0 } else { 160.0 };
    let cover = if item.id == "LIKED_SONGS" {
        div()
            .size(px(edge))
            .flex_none()
            .rounded(px(8.0))
            .bg(palette.wash)
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("icons/heart.svg")
                    .size(px(edge * 0.45))
                    .text_color(accent),
            )
            .into_any_element()
    } else {
        thumb_element(
            app,
            item.image_url.as_deref(),
            edge,
            item.kind == HomeItemKind::Artist,
            muted,
        )
    };
    let play = if search_row.is_none() || !item.id.starts_with("local-") {
        home_play_button(item.clone(), search_row, accent, background, cx).into_any_element()
    } else {
        div().absolute().into_any_element()
    };
    let base = div()
        .id(("home-item", index))
        .group("home-item")
        .relative()
        .min_w_0()
        .cursor_pointer()
        .rounded(px(8.0))
        .text_color(fg)
        .when(selected, |el| el.bg(palette.row_selected))
        .hover(move |style| style.bg(palette.row_hover))
        .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
            window.focus(&this.focus_handle, cx);
            let event = if search_row.is_some() {
                echo_core::intent::activate_search_result(&mut this.state, index)
            } else {
                this.state.ui.selected_home_index = index;
                echo_core::intent::open_home_item(&mut this.state, item.clone())
            };
            if let Some(event) = event {
                this.dispatch(event);
            }
            cx.notify();
        }));
    if quick {
        base.flex_1()
            .h(px(56.0))
            .flex()
            .items_center()
            .gap_2()
            .overflow_hidden()
            .border_1()
            .border_color(palette.border)
            .when(!selected, |el| el.bg(palette.wash))
            .child(cover)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .pr_2()
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .line_clamp(2)
                    .child(title),
            )
            .child(play)
            .into_any_element()
    } else {
        base.flex_none()
            .w(px(160.0))
            .flex()
            .flex_col()
            .gap_2()
            .child(div().relative().size(px(160.0)).child(cover).child(play))
            .child(
                div()
                    .px_1()
                    .h(px(40.0))
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .line_clamp(2)
                    .child(title),
            )
            .child(
                div()
                    .px_1()
                    .pb_2()
                    .text_xs()
                    .text_color(muted)
                    .truncate()
                    .child(subtitle),
            )
            .into_any_element()
    }
}

/// Horizontal scrollbar under a Home shelf, colored like the seek bar: `border` track,
/// `accent` thumb.
fn home_scrollbar(
    scroll: gpui::ScrollHandle,
    palette: DesktopPalette,
    accent: Hsla,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let paint_scroll = scroll.clone();
    div()
        .id("home-scrollbar")
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

pub(super) fn home_view(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    use echo_core::home::{HomeShelfKind, home_shelves, pending_shelves};
    let palette = DesktopPalette::resolve(&app.state.ui.active_theme);
    let fg = app.state.ui.active_theme.text.gpui(WINDOW_FG());
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    let accent = app.state.ui.active_theme.primary.gpui(WINDOW_FG());
    let mut shelves = home_shelves(&app.state);
    let width = app.main_content_width(window);
    let columns = if width >= 640.0 {
        4
    } else if width >= 320.0 {
        2
    } else {
        1
    };
    let pending = pending_shelves(&app.state.data.home_fetches);
    let empty = shelves
        .iter()
        .all(|shelf| shelf.kind == HomeShelfKind::QuickPicks);
    let mut content = div()
        .id("home-scroll")
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_y_scroll()
        .track_scroll(&app.home_scroll)
        // Shelves clip at the main area's edge; only non-scrolling content is inset.
        .py_4()
        .flex()
        .flex_col()
        .gap_6()
        .child(
            div()
                .flex_none()
                .px_4()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .text_size(px(28.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(fg)
                        .child(tr(&app.state, echo_core::home::local_greeting_key())),
                )
                .when(empty, |el| {
                    el.child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(tr(&app.state, "desktop.home_empty")),
                    )
                }),
        );
    let mut index = 0;
    let mut section = 1;
    app.home_section_indices.clear();
    for kind in HomeShelfKind::ALL {
        let Some(position) = shelves.iter().position(|shelf| shelf.kind == kind) else {
            if pending.contains(&kind) {
                content = content.child(
                    div()
                        .flex_none()
                        .px_4()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(18.0))
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(fg)
                                .child(tr(&app.state, kind.title_key())),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .child(tr(&app.state, "desktop.home_loading")),
                        ),
                );
                section += 1;
            }
            continue;
        };
        let shelf = shelves.remove(position);
        app.home_section_indices.insert(kind, section);
        section += 1;
        if shelf.kind == HomeShelfKind::QuickPicks {
            let mut grid = div().flex_none().px_4().flex().flex_col().gap_2();
            for chunk in shelf.items.chunks(columns) {
                let mut row = div().flex().gap_2().w_full();
                for item in chunk {
                    row = row.child(home_card(app, item.clone(), index, true, None, cx));
                    index += 1;
                }
                for _ in chunk.len()..columns {
                    row = row.child(div().flex_1().min_w_0());
                }
                grid = grid.child(row);
            }
            content = content.child(grid);
            continue;
        }
        let scroll = app
            .home_shelf_scrolls
            .entry(shelf.kind)
            .or_default()
            .clone();
        let wheel_scroll = scroll.clone();
        let mut row = div()
            .id(SharedString::from(format!("home-shelf-{:?}", shelf.kind)))
            .flex()
            .gap_3()
            // Scroll the end spacing with the cards instead of clipping before the gutter.
            .px_4()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .track_scroll(&scroll)
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
            ));
        for item in shelf.items {
            row = row.child(home_card(app, item, index, false, None, cx));
            index += 1;
        }
        content = content.child(
            div()
                .id(SharedString::from(format!("home-section-{:?}", shelf.kind)))
                .flex_none()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(fg)
                        .px_4()
                        .child(tr(&app.state, shelf.kind.title_key())),
                )
                .child(row)
                .child(home_scrollbar(scroll, palette, accent, cx)),
        );
    }
    echo_core::thumbnails::drain_pending(&mut app.state, &app.worker_tx);
    content
}
