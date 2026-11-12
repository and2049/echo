use super::*;

pub const SORT_OPTIONS: &[(&str, &str)] = &[
    ("desktop.sort.original", "original"),
    ("desktop.sort.title", "title"),
    ("ui.artist", "artist"),
    ("ui.album", "album"),
    ("ui.duration", "duration"),
    ("desktop.sort.added", "added"),
    ("desktop.context.added_by", "addedby"),
    ("desktop.sort.reverse", "reverse"),
];

fn sort_button(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    div()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this: &mut EchoApp, event: &MouseDownEvent, window, _cx| {
                this.sort_menu_top = (f32::from(event.position.y) + 16.0)
                    .min((f32::from(window.viewport_size().height) - 230.0).max(0.0));
            }),
        )
        .child(crate::icon_button(
            "track-sort",
            "icons/arrow-down.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| {
                this.sort_menu_open = !this.sort_menu_open;
                cx.notify();
            },
        ))
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
                .top(px(app.sort_menu_top))
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
                .children(
                    SORT_OPTIONS
                        .iter()
                        .enumerate()
                        .map(|(ix, (label_key, arg))| {
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
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        this.apply_sort(arg, cx);
                                    },
                                ))
                                .child(label)
                                .when(is_active, |el| {
                                    el.child(div().flex_none().text_color(accent).child("●"))
                                })
                        }),
                ),
        )
}

#[derive(Clone, Copy)]
struct TrackColumns {
    album: bool,
    added_by: bool,
    date: bool,
    heart: bool,
}

fn track_columns(state: &echo_core::app::AppState) -> TrackColumns {
    let context = state.data.active_tracklist_context.as_ref();
    TrackColumns {
        album: context.is_none_or(|context| !context.is_album()),
        added_by: context.is_some_and(|context| {
            context.kind == echo_core::models::TrackListContextKind::Playlist
        }) && state
            .data
            .active_context_details
            .as_ref()
            .is_some_and(|details| !details.collaborators.is_empty()),
        date: context.is_some_and(|context| {
            context.kind == echo_core::models::TrackListContextKind::Playlist
        }),
        heart: context.is_none_or(|context| context.id != "LIKED_SONGS"),
    }
}

fn context_hero(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    use echo_core::context_details::header_summary;
    let state = &app.state;
    let context = state.data.active_tracklist_context.as_ref().unwrap();
    let details = state
        .data
        .active_context_details
        .clone()
        .unwrap_or_default();
    let theme = &state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let background = theme.background.gpui(crate::theme::WINDOW_BG());
    let label = tr(state, details.label_key(context));
    let title = SharedString::from(context.title.clone());
    let summary = SharedString::from(header_summary(
        context,
        &details,
        &state.data.tracks,
        &|key| tr(state, key).to_string(),
    ));
    let playing = echo_core::intent::active_context_is_playing(state) && state.playback.is_playing;
    let shuffled =
        echo_core::intent::active_context_is_playing(state) && state.playback.is_shuffled;
    let cover = state
        .ui
        .active_library_header_image
        .clone()
        .and_then(|artwork| app.images.get(&artwork));
    let image_url = context.image_url.clone();
    let cover = match cover {
        Some(image) => cover_image(image, 160.0, 6.0).into_any_element(),
        None => thumb_element(app, image_url.as_deref(), 160.0, false, muted),
    };
    div()
        .flex_none()
        .px_4()
        .pt_4()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div().flex().items_end().gap_4().child(cover).child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(label.to_uppercase())),
                    )
                    .child(
                        div()
                            .text_size(px(32.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .line_clamp(2)
                            .text_color(fg)
                            .child(title),
                    )
                    .when_some(details.description, |el, description| {
                        el.child(
                            div()
                                .text_sm()
                                .text_color(muted)
                                .truncate()
                                .child(SharedString::from(description)),
                        )
                    })
                    .child(div().text_xs().text_color(muted).truncate().child(summary)),
            ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .pb_3()
                .child(
                    div()
                        .id("context-play")
                        .size(px(48.0))
                        .rounded_full()
                        .bg(accent)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(cx.listener(|this: &mut EchoApp, _, _, cx| {
                            if let Some(event) =
                                echo_core::intent::play_context_from_header(&mut this.state)
                            {
                                this.dispatch(event);
                            }
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path(if playing {
                                    "icons/pause.svg"
                                } else {
                                    "icons/play.svg"
                                })
                                .size(px(24.0))
                                .text_color(background),
                        ),
                )
                .child(crate::icon_button(
                    "context-shuffle",
                    "icons/shuffle.svg",
                    if shuffled { accent } else { muted },
                    palette.wash,
                    cx,
                    |this, cx| {
                        for event in echo_core::intent::toggle_context_shuffle(&mut this.state) {
                            this.dispatch(event);
                        }
                        cx.notify();
                    },
                ))
                .child(playlist_edit::page_buttons(app, cx))
                .child(div().flex_1())
                .child(sort_button(app, cx)),
        )
}

fn track_column_headers(
    app: &EchoApp,
    columns: TrackColumns,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    use echo_core::app::TrackSort;
    let palette = DesktopPalette::resolve(&app.state.ui.active_theme);
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    let cell = |sort, label: SharedString, width: Option<f32>, grow: f32| {
        div()
            .id(SharedString::from(format!("column-{sort:?}")))
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .cursor_pointer()
            .when_some(width, |el, width| el.flex_none().w(px(width)))
            .when(width.is_none(), |el| el.flex_grow(grow).flex_basis(px(0.0)))
            .on_click(cx.listener(move |this: &mut EchoApp, _, _, cx| {
                echo_core::intent::sort_by_column(&mut this.state, sort);
                cx.notify();
            }))
            .child(div().truncate().child(label))
            .when(sort == TrackSort::Duration, |el| {
                el.child(
                    svg()
                        .path("icons/clock.svg")
                        .size(px(14.0))
                        .text_color(muted),
                )
            })
            .when(sort == app.state.ui.track_sort, |el| {
                el.child(
                    svg()
                        .path(if app.state.ui.track_sort_ascending {
                            "icons/arrow-up.svg"
                        } else {
                            "icons/arrow-down.svg"
                        })
                        .size(px(10.0))
                        .text_color(muted),
                )
            })
    };
    div()
        .flex_none()
        .mx_4()
        .h(px(32.0))
        .flex()
        .items_center()
        .gap_3()
        .text_xs()
        .text_color(muted)
        .border_b_1()
        .border_color(palette.border)
        .child(cell(TrackSort::Original, "#".into(), Some(32.0), 0.0))
        .child(cell(
            TrackSort::Title,
            tr(&app.state, "desktop.context.title"),
            None,
            2.0,
        ))
        .when(columns.album, |el| {
            el.child(cell(
                TrackSort::Album,
                tr(&app.state, "desktop.context.album_column"),
                None,
                1.5,
            ))
        })
        .when(columns.added_by, |el| {
            el.child(cell(
                TrackSort::AddedBy,
                tr(&app.state, "desktop.context.added_by"),
                None,
                1.0,
            ))
        })
        .when(columns.date, |el| {
            el.child(cell(
                TrackSort::Added,
                tr(&app.state, "desktop.context.date_added"),
                None,
                1.0,
            ))
        })
        .when(columns.heart, |el| el.child(div().flex_none().w(px(14.0))))
        .child(cell(TrackSort::Duration, "".into(), Some(48.0), 0.0))
}

pub(super) fn playing_bars(phase: f32, accent: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            for bar in 0..3 {
                let height = 4.0
                    + 12.0
                        * (phase * std::f32::consts::TAU + bar as f32 * 1.8)
                            .sin()
                            .abs();
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(
                        bounds.origin
                            + gpui::point(px(bar as f32 * 6.0), bounds.size.height - px(height)),
                        gpui::size(px(3.0), px(height)),
                    ),
                    accent,
                ));
            }
        },
    )
    .size(px(18.0))
}

fn track_index_cell(
    app: &EchoApp,
    ix: usize,
    selected: usize,
    playing: bool,
    accent: Hsla,
    muted: Hsla,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let number = row_number(&app.state, ix, selected);
    let animate = playing && app.state.playback.is_playing && !cx.reduce_motion();
    let phase = if animate {
        app.animate_playing_bars(cx)
    } else {
        0.25
    };
    let bars = playing_bars(phase, accent).into_any_element();
    div()
        .id(("track-index", ix))
        .relative()
        .flex_none()
        .w(px(32.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .on_click(cx.listener(move |this: &mut EchoApp, _, _, cx| {
            cx.stop_propagation();
            if let Some(event) = echo_core::intent::play_track_at(&mut this.state, ix) {
                this.dispatch(event);
            }
            cx.notify();
        }))
        .child(
            div()
                .group_hover("track-row", |style| style.invisible())
                .text_color(muted)
                .child(if playing {
                    bars
                } else {
                    div().child(number).into_any_element()
                }),
        )
        .child(
            svg()
                .absolute()
                .path("icons/play.svg")
                .size(px(16.0))
                .text_color(muted)
                .invisible()
                .group_hover("track-row", |style| style.visible()),
        )
}

pub(super) fn track_list(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    let wide = app.main_content_width(window) >= 640.0;
    let hero = wide
        && app
            .state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.requires_worker_load());
    let columns = track_columns(&app.state);

    let count = app.state.data.tracks.len();
    let loading = count == 0 && app.state.data.active_context_details.is_none();

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(if hero {
            context_hero(app, cx).into_any_element()
        } else {
            track_page_header(app, wide, cx).into_any_element()
        })
        .child(track_column_headers(app, columns, cx))
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
                cx.processor(
                    move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                        track_rows(this, range, columns, cx)
                    },
                ),
            )
            .track_scroll(&app.tracks_scroll)
            .on_drag_move(cx.listener(
                |this: &mut EchoApp, event: &gpui::DragMoveEvent<DraggedTracks>, _window, cx| {
                    this.drag_autoscroll(
                        crate::DragList::Tracks,
                        event.event.position.y,
                        event.bounds,
                        cx,
                    );
                },
            ))
            .on_drop(
                cx.listener(|this: &mut EchoApp, drag: &DraggedTracks, _window, cx| {
                    drop_tracks_at_end(this, drag, cx);
                }),
            )
            .flex_grow(1.0)
            .into_any_element()
        })
}

/// The compact page header: cover, title and author, and the page's buttons.
fn track_page_header(app: &mut EchoApp, wide: bool, cx: &mut Context<EchoApp>) -> Div {
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
    let is_top_tracks = app
        .state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|context| context.id == "TOP_TRACKS");

    div()
        .flex_none()
        .px_4()
        .py_3()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .when_some(header_image.filter(|_| wide), |el, image| {
            el.child(cover_image(image, 72.0, 6.0))
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
                        .truncate()
                        .child(SharedString::from(context_title)),
                )
                .when(wide, |el| {
                    el.child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .truncate()
                            .child(SharedString::from(context_author)),
                    )
                }),
        )
        .when(is_top_tracks, |el| el.child(range_switcher(app, cx)))
        .child(playlist_edit::page_buttons(app, cx))
        .child(sort_button(app, cx))
}

fn track_rows(
    this: &mut EchoApp,
    range: std::ops::Range<usize>,
    columns: TrackColumns,
    cx: &mut Context<EchoApp>,
) -> Vec<Div> {
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
    let picked = picked_in(&this.state, ActiveView::TrackList);
    let context_id = this
        .state
        .data
        .active_tracklist_context
        .as_ref()
        .map(|context| context.id.clone());
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
    let reorderable = this.state.ui.track_sort == echo_core::app::TrackSort::Original
        && this.state.ui.track_sort_ascending
        && this
            .state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.can_modify_playlist(this.state.data.user_id.as_ref()));
    let drag_colors = DragColors::resolve(theme);

    let rows = range
        .map(|ix| {
            let track = this.state.data.tracks[ix].clone();
            let thumb = thumb_element(this, track.image_url.as_deref(), 32.0, false, muted);
            let is_liked = this.state.data.liked_tracks.contains(&track.id);
            let is_playing = playing_id.as_deref() == Some(track.id.as_str());
            let title_color = if is_playing { accent } else { fg };
            let payload = drag_payload(this, ActiveView::TrackList, ix, &track);
            let over_context = context_id.clone();
            let drop_context = context_id.clone();

            pill_row(
                ix,
                TRACK_PILL,
                row_marked(ix, selected, visual, &picked),
                selected_bg,
                palette.row_hover,
                |row| {
                    row.gap_3()
                        .group("track-row")
                        .on_drag(payload, move |drag, _offset, _window, cx| {
                            track_drag_preview(drag, drag_colors, cx)
                        })
                        .when(reorderable, |row| {
                            row.drag_over::<DraggedTracks>(move |style, drag, _, _| {
                                if drag.source_context == over_context {
                                    style.bg(palette.drag_target)
                                } else {
                                    style.border_t_2().border_color(accent)
                                }
                            })
                            .on_drop(
                                cx.listener(
                                    move |this: &mut EchoApp, drag: &DraggedTracks, _window, cx| {
                                        let same_list = drag.source_context == drop_context;
                                        if same_list && drag.tracks.len() > 1 {
                                            return;
                                        }
                                        if let Some(event) =
                                            echo_core::intent::drop_tracks_in_tracklist(
                                                &mut this.state,
                                                drag.tracks.clone(),
                                                same_list.then_some(drag.from_row),
                                                Some(ix),
                                            )
                                        {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                    },
                                ),
                            )
                        })
                        .on_click(cx.listener(
                            move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                click_track_row(this, ix, event, cx);
                            },
                        ))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(
                                move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                                    open_track_row_menu(this, ix, event.position, cx);
                                },
                            ),
                        )
                        .child(track_index_cell(
                            this, ix, selected, is_playing, accent, muted, cx,
                        ))
                        .child(
                            div()
                                .flex_grow(2.0)
                                .flex_basis(px(0.0))
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(thumb)
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_color(title_color)
                                                .child(SharedString::from(track.name.clone())),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap_1()
                                                .text_xs()
                                                .overflow_hidden()
                                                .when(track.explicit, |el| {
                                                    el.child(
                                                        div()
                                                            .flex_none()
                                                            .rounded_sm()
                                                            .border_1()
                                                            .border_color(muted)
                                                            .px(px(2.0))
                                                            .text_size(px(10.0))
                                                            .text_color(muted)
                                                            .child(tr(
                                                                &this.state,
                                                                "desktop.context.explicit",
                                                            )),
                                                    )
                                                })
                                                .child(track_artist_cell(ix, &track, muted, cx)),
                                        ),
                                ),
                        )
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
                                    .on_click(cx.listener(
                                        move |this: &mut EchoApp,
                                              _event: &gpui::ClickEvent,
                                              _window,
                                              cx| {
                                            cx.stop_propagation();
                                            if let Some(ctx) = this
                                                .state
                                                .data
                                                .tracks
                                                .get(ix)
                                                .map(ActionMenuContext::from)
                                            {
                                                this.go_to(ctx, ActionMenuAction::GoToAlbum, cx);
                                            }
                                        },
                                    ))
                                    .child(SharedString::from(track.album.clone())),
                            )
                        })
                        .when(columns.added_by, |row| {
                            row.child(
                                div()
                                    .flex_grow(1.0)
                                    .flex_basis(px(0.0))
                                    .min_w_0()
                                    .truncate()
                                    .text_color(muted)
                                    .child(SharedString::from(
                                        track.added_by.clone().unwrap_or_default(),
                                    )),
                            )
                        })
                        .when(columns.date, |row| {
                            row.child(
                                div()
                                    .flex_grow(1.0)
                                    .flex_basis(px(0.0))
                                    .min_w_0()
                                    .truncate()
                                    .text_color(muted)
                                    .child(SharedString::from(
                                        echo_core::context_details::format_added_at_with(
                                            std::time::SystemTime::now().into(),
                                            track.added_at.as_deref().unwrap_or_default(),
                                            &|key| tr(&this.state, key).to_string(),
                                        ),
                                    )),
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
                },
            )
        })
        .collect();
    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
    rows
}

fn click_track_row(
    this: &mut EchoApp,
    ix: usize,
    event: &gpui::ClickEvent,
    cx: &mut Context<EchoApp>,
) {
    // Shift-click selects from the previously focused row to
    // this one — the desktop's version of `v` plus motion.
    if event.modifiers().shift {
        this.extend_selection_to(ix, cx);
        return;
    }
    this.state.ui.active_view = ActiveView::TrackList;
    if is_pick_click(event) {
        echo_core::intent::toggle_picked_row(&mut this.state, ix);
        cx.notify();
        return;
    }
    this.state.ui.selected_track_index = ix;
    if event.click_count() >= 2
        && let Some(event) = echo_core::intent::play_track_at(&mut this.state, ix)
    {
        this.dispatch(event);
    }
    cx.notify();
}

fn open_track_row_menu(
    this: &mut EchoApp,
    ix: usize,
    position: gpui::Point<gpui::Pixels>,
    cx: &mut Context<EchoApp>,
) {
    this.state.ui.selected_track_index = ix;
    this.context_menu = None;
    if let Some(ctx) = this.action_target() {
        this.track_menu = Some(crate::TrackMenuState {
            ctx,
            position: Some(position),
            selected: 0,
            submenu: None,
        });
    }
    cx.notify();
}

/// The row's artist credits, each one linking to its artist page.
fn track_artist_cell(
    ix: usize,
    track: &echo_core::models::Track,
    muted: Hsla,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let base = div()
        .min_w_0()
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
            .on_click(cx.listener(
                move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
                    cx.stop_propagation();
                    if let Some(ctx) = this.state.data.tracks.get(ix).map(ActionMenuContext::from) {
                        this.go_to(ctx, ActionMenuAction::GoToArtist, cx);
                    }
                },
            ))
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
                        .on_click(cx.listener(
                            move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
                                cx.stop_propagation();
                                this.open_artist(id.clone(), artist_name.clone(), cx);
                            },
                        ))
                        .child(name)
                        .into_any_element()
                }
                None => div().child(name).into_any_element(),
            });
        }
        cell.into_any_element()
    }
}

fn drop_tracks_at_end(this: &mut EchoApp, drag: &DraggedTracks, cx: &mut Context<EchoApp>) {
    // A drop past the last row appends; drops on rows never reach here.
    let same_list = drag.source_context.is_some()
        && drag.source_context
            == this
                .state
                .data
                .active_tracklist_context
                .as_ref()
                .map(|c| c.id.clone());
    if same_list && drag.tracks.len() > 1 {
        return;
    }
    if let Some(event) = echo_core::intent::drop_tracks_in_tracklist(
        &mut this.state,
        drag.tracks.clone(),
        same_list.then_some(drag.from_row),
        None,
    ) {
        this.dispatch(event);
    }
    cx.notify();
}
