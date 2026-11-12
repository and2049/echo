use super::*;

/// The artist page's discography chips, styled like [`range_switcher`].
fn discography_switcher(app: &EchoApp, cx: &mut Context<EchoApp>) -> Div {
    use echo_core::models::DiscographyFilter;
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let active = app.state.ui.artist_discography_filter;

    let mut row = div().flex().flex_row().items_center().gap_1();
    for filter in DiscographyFilter::ALL {
        let is_active = filter == active;
        row = row.child(
            div()
                .id(SharedString::from(format!("discography-{:?}", filter)))
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
                    this.set_discography_filter(filter);
                    cx.notify();
                }))
                .child(tr(&app.state, filter.label_key())),
        );
    }
    row
}

/// Follow / Following toggle in the artist header: filled while the artist is in the
/// followed list, outlined otherwise.
fn follow_button(app: &EchoApp, artist_id: &str, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let accent = theme.primary.gpui(WINDOW_FG());
    let following = app
        .state
        .data
        .followed_artists
        .iter()
        .any(|artist| artist.id == artist_id);
    let label = if following {
        tr(&app.state, "desktop.following")
    } else {
        tr(&app.state, "desktop.follow")
    };
    div()
        .id("artist-follow")
        .flex_none()
        .px_3()
        .py_1()
        .rounded_full()
        .border_1()
        .text_xs()
        .border_color(accent)
        .text_color(accent)
        .when(following, |el| el.bg(palette.wash))
        .hover(move |style| style.bg(palette.row_hover))
        .cursor_pointer()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            if let Some(event) = echo_core::intent::toggle_follow_artist(&mut this.state) {
                this.dispatch(event);
            }
            cx.notify();
        }))
        .child(label)
}

/// Full-page artist list: reached through the "Top Artists" browse link (the sidebar's
/// Artists tab still covers followed artists) and by back-navigation.
pub(super) fn artist_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
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
                .child(div().flex_grow(1.0).text_lg().text_color(fg).child(title))
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
                cx.processor(
                    move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
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
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .w(px(24.0))
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(row_number(&this.state, ix, selected)),
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
                    },
                ),
            )
            .track_scroll(&app.artist_list_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

/// What's New: recent releases from followed artists, reached through the sidebar
/// browse link. Renders live from state, filling in as the background scan reports.
pub(super) fn whats_new(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
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
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(
                                tr(&app.state, "desktop.whats_new_checking")
                                    .replace("{done}", &done.to_string())
                                    .replace("{total}", &total.to_string()),
                            )),
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
                cx.processor(
                    move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
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
                    },
                ),
            )
            .track_scroll(&app.whats_new_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

pub(super) fn artist_page(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());

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

    let filter = app.state.ui.artist_discography_filter;
    let loading = app.state.data.artist_albums_loading && data.albums.is_empty();
    let row_count = data.album_row_count(filter);
    let top_len = data.top_tracks.len();
    let top_loading = app.state.data.artist_top_tracks_loading;

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(artist_header(app, &data, cx))
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
                    cx.processor(
                        |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                            artist_top_track_rows(this, range, cx)
                        },
                    ),
                )
                .track_scroll(&app.artist_top_tracks_scroll)
                .flex_none()
                .h(px(top_len.min(10) as f32 * LIST_PILL.row_height))
                .max_h(relative(0.4))
                .into_any_element()
            })
        })
        .child(
            div()
                .flex_none()
                .px_4()
                .pt_2()
                .pb_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.albums_section")),
                )
                .child(discography_switcher(app, cx)),
        )
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
                row_count,
                cx.processor(
                    |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                        artist_album_rows(this, range, cx)
                    },
                ),
            )
            .track_scroll(&app.artist_albums_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
        .into_any_element()
}

/// The back button, portrait, name and album count, and the follow button.
fn artist_header(
    app: &mut EchoApp,
    data: &echo_core::models::ArtistPageData,
    cx: &mut Context<EchoApp>,
) -> Div {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let header_image = app
        .state
        .ui
        .active_library_header_image
        .clone()
        .and_then(|artwork| app.images.get(&artwork));
    let count = data
        .chips
        .get(&echo_core::models::DiscographyFilter::All)
        .map_or(data.albums.len(), |page| page.total);

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
                .hover(move |style| style.bg(palette.wash))
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
            el.child(cover_image(image, 72.0, 36.0))
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
        .child(follow_button(app, &data.artist_id, cx))
}

fn artist_top_track_rows(
    this: &mut EchoApp,
    range: std::ops::Range<usize>,
    cx: &mut Context<EchoApp>,
) -> Vec<AnyElement> {
    let theme = &this.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let palette = DesktopPalette::resolve(theme);
    let drag_colors = DragColors::resolve(theme);
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
            let is_playing = playing_id.as_deref() == Some(track.id.as_str());
            let is_liked = this.state.data.liked_tracks.contains(&track.id);
            let secondary = this.state.ui.active_theme.secondary.gpui(WINDOW_FG());
            let title_color = if is_playing { accent } else { fg };
            let thumb = thumb_element(this, track.image_url.as_deref(), 26.0, false, muted);
            let cover = thumb_image(this, track.image_url.as_deref(), DRAG_COVER_EDGE);
            let payload = single_drag_payload(&track, cover);
            pill_row(
                ix,
                LIST_PILL,
                ix == selected,
                selected_bg,
                palette.row_hover,
                |row| {
                    row.gap_3()
                        .on_drag(payload, move |drag, _offset, _window, cx| {
                            track_drag_preview(drag, drag_colors, cx)
                        })
                        .on_click(cx.listener(
                            move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                this.state.ui.artist_page_album_index = ix;
                                if event.click_count() >= 2
                                    && let Some(event) = echo_core::intent::play_artist_top_track(
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
                                move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                                    this.state.ui.artist_page_album_index = ix;
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
                                .w(px(24.0))
                                .text_xs()
                                .text_color(muted)
                                .child(row_number(&this.state, ix, selected)),
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
                                .child(SharedString::from(format_time(track.duration_ms))),
                        )
                },
            )
            .into_any_element()
        })
        .collect();

    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
    rows
}

fn artist_album_rows(
    this: &mut EchoApp,
    range: std::ops::Range<usize>,
    cx: &mut Context<EchoApp>,
) -> Vec<AnyElement> {
    let theme = &this.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let palette = DesktopPalette::resolve(theme);
    let selected_bg = palette.row_selected;
    // Album rows sit after the Popular rows in the page's combined index space.
    let selected = this.state.ui.artist_page_album_index;
    let top_len = this.artist_page_top_len();
    let more_label = tr(
        &this.state,
        if this.state.data.artist_albums_loading {
            "desktop.loading"
        } else {
            "desktop.load_more"
        },
    );

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
                // The Load more row after the last album.
                return pill_row(
                    ix,
                    LIST_PILL,
                    ix + top_len == selected,
                    selected_bg,
                    palette.row_hover,
                    |row| {
                        row.justify_center()
                            .text_color(muted)
                            .on_click(cx.listener(
                                move |this: &mut EchoApp, _event, _window, cx| {
                                    if let Some(event) =
                                        echo_core::intent::load_more_artist_albums(&mut this.state)
                                    {
                                        this.dispatch(event);
                                    }
                                    cx.notify();
                                },
                            ))
                            .child(more_label.clone())
                    },
                )
                .into_any_element();
            };
            let thumb_url = album.thumb_url.clone().or_else(|| album.image_url.clone());
            let thumb = thumb_element(this, thumb_url.as_deref(), 26.0, false, muted);
            let tracks_label = album
                .track_count
                .map(|n| format!("{n} tracks"))
                .unwrap_or_default();
            pill_row(
                ix,
                LIST_PILL,
                ix + top_len == selected,
                selected_bg,
                palette.row_hover,
                |row| {
                    row.gap_3()
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            let index = ix + this.artist_page_top_len();
                            if let Some(event) =
                                echo_core::intent::activate_artist_page_row(&mut this.state, index)
                            {
                                this.dispatch(event);
                            }
                            cx.notify();
                        }))
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
                },
            )
            .into_any_element()
        })
        .collect();

    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
    rows
}
