use super::*;

/// A queue row cloned out of the state: its row index and either a header or `(track index, track)`.
type OwnedQueueRow = (usize, Result<(usize, echo_core::models::Track), String>);

pub(super) fn queue_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());

    let count = app.state.data.queue.len();
    let row_count = app.state.queue_rows().len();
    let recent = app.state.ui.queue_tab == QueueTab::Recent;
    let subtitle = if recent {
        tr(&app.state, "ui.recently_played").to_string()
    } else {
        tr(&app.state, "desktop.queue_upcoming").replace("{}", &count.to_string())
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
                .child(div().text_lg().text_color(fg).child(tr(&app.state, "ui.queue")))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(subtitle)),
                )
                .child(queue_tab_strip(app, cx)),
        )
        .child(if recent {
            recent_list(app, cx).into_any_element()
        } else if count == 0 {
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
                    let picked = picked_in(&this.state, ActiveView::Queue);
                    let has_manual = !this.state.data.manual_queue.is_empty();
                    let drag_colors = DragColors::resolve(theme);
                    // Cloned out first: the rows borrow the state the drag payload below mutates.
                    let rows: Vec<OwnedQueueRow> = {
                        let rows = this.state.queue_rows();
                        range
                            .map(|row_ix| {
                                let row = match &rows[row_ix] {
                                    QueueRow::Header(text) => Err(text.clone()),
                                    QueueRow::Track(ix, track) => Ok((*ix, (*track).clone())),
                                };
                                (row_ix, row)
                            })
                            .collect()
                    };

                    rows.into_iter()
                        .map(|(row_ix, row)| match row {
                            Err(text) => queue_header(
                                text,
                                (row_ix == 0 && has_manual).then(|| tr(&this.state, "actions.clear_queue")),
                                COMPACT_PILL,
                                fg,
                                muted,
                                cx,
                            ),
                            Ok((ix, track)) => {
                            let is_liked = this.state.data.liked_tracks.contains(&track.id);
                            let payload = drag_payload(this, ActiveView::Queue, ix, &track);

                            pill_row(ix, COMPACT_PILL, row_marked(ix, selected, visual, &picked), selected_bg, palette.row_hover, |row| {
                                row.gap_3()
                                .on_drag(payload, move |drag, _offset, _window, cx| {
                                    track_drag_preview(drag, drag_colors, cx)
                                })
                                .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                    if event.modifiers().shift {
                                        this.extend_selection_to(ix, cx);
                                        return;
                                    }
                                    if is_pick_click(event) {
                                        echo_core::intent::toggle_picked_row(&mut this.state, ix);
                                        cx.notify();
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
                                .child(div()
                                            .flex_none()
                                            .w(px(32.0))
                                            .text_color(muted)
                                            .child(row_number(&this.state, ix, selected)),)
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

fn queue_tab_strip(app: &EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let palette = DesktopPalette::resolve(theme);
    let active = app.state.ui.queue_tab;
    let tabs = [
        (
            QueueTab::Queue,
            "queue-tab-queue",
            tr(&app.state, "ui.queue"),
        ),
        (
            QueueTab::Recent,
            "queue-tab-recent",
            tr(&app.state, "ui.recent"),
        ),
    ];
    div()
        .mt_2()
        .flex()
        .gap_2()
        .children(tabs.into_iter().map(|(tab, id, label)| {
            let is_active = tab == active;
            div()
                .id(id)
                .cursor_pointer()
                .px_3()
                .py_1()
                .rounded_md()
                .text_sm()
                .text_color(if is_active { fg } else { muted })
                .when(is_active, |el| el.bg(palette.row_selected))
                .when(!is_active, |el| {
                    el.hover(move |style| style.bg(palette.row_hover))
                })
                .child(label)
                .on_click(cx.listener(
                    move |this: &mut EchoApp, _: &gpui::ClickEvent, _window, cx| {
                        if this.state.ui.queue_tab != tab {
                            echo_core::intent::cycle_queue_tab(&mut this.state);
                        }
                        cx.notify();
                    },
                ))
        }))
}

fn recent_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    let row_count = app.state.data.recent_plays.len();
    if row_count == 0 {
        return div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child(tr(&app.state, "desktop.recent_empty"))
            .into_any_element();
    }
    uniform_list(
        "recent-rows",
        row_count,
        cx.processor(
            move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                let theme = &this.state.ui.active_theme;
                let fg = theme.text.gpui(WINDOW_FG());
                let muted = theme.text_muted.gpui(WINDOW_FG());
                let palette = DesktopPalette::resolve(theme);
                let secondary = theme.secondary.gpui(WINDOW_FG());
                let selected = this.state.ui.selected_queue_index;
                let visual = visual_range_in(&this.state, ActiveView::Queue);
                let picked = picked_in(&this.state, ActiveView::Queue);
                let drag_colors = DragColors::resolve(theme);
                let now = std::time::SystemTime::now().into();
                // Cloned out first: the records borrow the state the drag payload below mutates.
                let rows: Vec<(usize, echo_core::models::Track, String)> = range
                    .filter_map(|ix| {
                        let record = this.state.data.recent_plays.get(ix)?;
                        let played = echo_core::context_details::format_added_at_with(
                            now,
                            &record.played_at,
                            &|key| tr(&this.state, key).to_string(),
                        );
                        Some((ix, record.track.clone(), played))
                    })
                    .collect();

                rows.into_iter()
                    .map(|(ix, track, played)| {
                        let is_liked = this.state.data.liked_tracks.contains(&track.id);
                        let payload = drag_payload(this, ActiveView::Queue, ix, &track);

                        pill_row(
                            ix,
                            COMPACT_PILL,
                            row_marked(ix, selected, visual, &picked),
                            palette.row_selected,
                            palette.row_hover,
                            |row| {
                                row.gap_3()
                                    .on_drag(payload, move |drag, _offset, _window, cx| {
                                        track_drag_preview(drag, drag_colors, cx)
                                    })
                                    .on_click(cx.listener(
                                        move |this: &mut EchoApp,
                                              event: &gpui::ClickEvent,
                                              _window,
                                              cx| {
                                            if event.modifiers().shift {
                                                this.extend_selection_to(ix, cx);
                                                return;
                                            }
                                            if is_pick_click(event) {
                                                echo_core::intent::toggle_picked_row(
                                                    &mut this.state,
                                                    ix,
                                                );
                                                cx.notify();
                                                return;
                                            }
                                            this.state.ui.selected_queue_index = ix;
                                            if event.click_count() >= 2
                                                && let Some(event) =
                                                    echo_core::intent::play_queue_track_at(
                                                        &mut this.state,
                                                        ix,
                                                    )
                                            {
                                                this.dispatch(event);
                                            }
                                            cx.notify();
                                        },
                                    ))
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(
                                            move |this: &mut EchoApp,
                                                  event: &MouseDownEvent,
                                                  _window,
                                                  cx| {
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
                                            },
                                        ),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(32.0))
                                            .text_color(muted)
                                            .child(row_number(&this.state, ix, selected)),
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
                                    .child(liked_cell(
                                        "recent",
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
                                            .w(px(112.0))
                                            .text_xs()
                                            .text_color(muted)
                                            .child(SharedString::from(played)),
                                    )
                            },
                        )
                        .into_any_element()
                    })
                    .collect()
            },
        ),
    )
    .track_scroll(&app.queue_scroll)
    .flex_grow(1.0)
    .into_any_element()
}

/// A section label inside a queue list. `uniform_list` takes its row pitch from row 0, which is
/// always a header, so the header has to be as tall as the track rows of the list it sits in:
/// `metrics` is that list's. In rows taller than the compact ones the label drops to the bottom
/// edge, next to the tracks it names.
fn queue_header(
    text: String,
    clear_label: Option<SharedString>,
    metrics: PillMetrics,
    fg: Hsla,
    muted: Hsla,
    cx: &mut Context<EchoApp>,
) -> gpui::AnyElement {
    div()
        .w_full()
        .h(px(metrics.row_height))
        .px_4()
        .flex()
        .map(|el| {
            if metrics.row_height > COMPACT_PILL.row_height {
                el.items_end().pb_1()
            } else {
                el.items_center()
            }
        })
        .justify_between()
        .gap_2()
        .text_xs()
        .text_color(muted)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(SharedString::from(text)),
        )
        .when_some(clear_label, |el, label| {
            el.child(
                div()
                    .id("clear-queue")
                    .flex_none()
                    .max_w(relative(0.6))
                    .truncate()
                    .cursor_pointer()
                    .text_color(fg)
                    .hover(move |style| style.text_color(muted))
                    .child(label)
                    .on_click(cx.listener(
                        |this: &mut EchoApp, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(event) = echo_core::intent::clear_queue(&mut this.state) {
                                this.dispatch(event);
                            }
                            cx.notify();
                        },
                    )),
            )
        })
        .into_any_element()
}

/// The queue panel: the playing track on top, then the same rows as the full queue view in a
/// narrower two-line layout. Double-click plays a row, right-click opens its track menu; the
/// keyboard-driven selection stays with the full view.
pub(super) fn queue_panel_body(
    app: &mut EchoApp,
    width: f32,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let show_duration = panel_layout::show_queue_duration(width);
    let theme = &app.state.ui.active_theme;
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let palette = DesktopPalette::resolve(theme);
    let playback = &app.state.playback;
    let now_playing = playback.playing_track_id.is_some().then(|| {
        (
            SharedString::from(playback.playing_track_title.clone()),
            SharedString::from(playback.playing_track_artist.clone()),
            playback
                .playing_track_image
                .as_ref()
                .or(playback.previous_track_image.as_ref())
                .cloned()
                .and_then(|artwork| app.images.get(&artwork)),
        )
    });
    let row_count = app.state.queue_rows().len();

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .when_some(now_playing, |el, (title, artist, cover)| {
            el.child(
                div()
                    .flex_none()
                    .px_4()
                    .pb_1()
                    .text_xs()
                    .text_color(muted)
                    .child(tr(&app.state, "desktop.now_playing")),
            )
            .child(
                div()
                    .flex_none()
                    .mx_2()
                    .px_2()
                    .h(px(TRACK_PILL.pill_height))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(match cover {
                        Some(image) => cover_image(image, 32.0, 4.0).into_any_element(),
                        None => div()
                            .flex_none()
                            .size(px(32.0))
                            .rounded_sm()
                            .bg(palette.wash)
                            .into_any_element(),
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex_grow(1.0)
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(accent)
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .child(artist),
                            ),
                    ),
            )
        })
        .child(if row_count == 0 {
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
                "queue-panel-rows",
                row_count,
                cx.processor(move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                    let theme = &this.state.ui.active_theme;
                    let fg = theme.text.gpui(WINDOW_FG());
                    let muted = theme.text_muted.gpui(WINDOW_FG());
                    let palette = DesktopPalette::resolve(theme);
                    let has_manual = !this.state.data.manual_queue.is_empty();
                    // Cloned out first: the rows borrow the state the thumbnails below mutate.
                    let rows: Vec<(usize, Option<String>, Option<echo_core::models::Track>)> = {
                        let rows = this.state.queue_rows();
                        range
                            .map(|row_ix| match &rows[row_ix] {
                                QueueRow::Header(text) => (row_ix, Some(text.clone()), None),
                                QueueRow::Track(ix, track) => (*ix, None, Some((*track).clone())),
                            })
                            .collect()
                    };

                    rows.into_iter()
                        .map(|(ix, header, track)| {
                            if let Some(text) = header {
                                return queue_header(
                                    text,
                                    (ix == 0 && has_manual).then(|| tr(&this.state, "actions.clear_queue")),
                                    TRACK_PILL,
                                    fg,
                                    muted,
                                    cx,
                                );
                            }
                            let Some(track) = track else {
                                return div().into_any_element();
                            };
                            let thumb = thumb_element(this, track.image_url.as_deref(), 32.0, false, muted);
                            let ctx = ActionMenuContext::from(&track);
                            pill_row(ix, TRACK_PILL, false, palette.row_selected, palette.row_hover, |row| {
                                row.gap_3()
                                    .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
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
                                            this.context_menu = None;
                                            this.track_menu = Some(crate::TrackMenuState {
                                                ctx: ctx.clone(),
                                                position: Some(event.position),
                                                selected: 0,
                                                submenu: None,
                                            });
                                            cx.notify();
                                        }),
                                    )
                                    .child(thumb)
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_grow(1.0)
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(fg)
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .overflow_hidden()
                                                    .child(SharedString::from(track.name.clone())),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .overflow_hidden()
                                                    .child(SharedString::from(track.artist.clone())),
                                            ),
                                    )
                                    .when(show_duration, |row| {
                                        row.child(
                                            div()
                                                .flex_none()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(SharedString::from(format_time(track.duration_ms))),
                                        )
                                    })
                            })
                            .into_any_element()
                        })
                        .collect()
                }),
            )
            .track_scroll(&app.queue_panel_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
        .into_any_element()
}
