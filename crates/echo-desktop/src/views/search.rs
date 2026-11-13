use super::*;
use echo_core::{
    intent,
    search::{SearchRow, TopResult, all_tab_rows, search_row_item},
};

pub fn empty_results(app: &EchoApp) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .p_4()
        .child(
            div()
                .font_weight(gpui::FontWeight::BOLD)
                .text_lg()
                .text_color(theme.text.gpui(WINDOW_FG()))
                .child(
                    tr(&app.state, "desktop.search_no_results")
                        .replace("{query}", &app.state.ui.search_context_query),
                ),
        )
        .child(
            div()
                .text_sm()
                .text_color(theme.text_muted.gpui(WINDOW_FG()))
                .child(tr(&app.state, "desktop.search_no_results_hint")),
        )
        .into_any_element()
}

/// Recent searches under the search box while it has focus, narrowed to the entries that
/// contain what has been typed; `None` once nothing matches. Painted deferred so it lies over
/// the page. Rows act on mouse down, not click: the window's own focus handle takes focus on any
/// mouse down outside the box, so a click would have closed the dropdown before the button came
/// back up.
pub fn recent_searches(app: &EchoApp, cx: &mut Context<EchoApp>) -> Option<AnyElement> {
    let matches = echo_core::search::matching_recent_searches(
        &app.state.ui.library_config.recent_searches,
        app.search_field.read(cx).text(),
    );
    if matches.is_empty() {
        return None;
    }
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let rows: Vec<_> = matches
        .into_iter()
        .map(|(index, query)| {
            let query = query.to_string();
            let run = query.clone();
            div()
                .id(("recent-search", index))
                .mx_1()
                .px_2()
                .py_1()
                .rounded_md()
                .flex()
                .items_center()
                .gap_2()
                .text_sm()
                .text_color(fg)
                .cursor_pointer()
                .hover(move |s| s.bg(palette.menu_hover))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this: &mut EchoApp, _, window, cx| {
                        if let Some(event) = intent::global_search(&mut this.state, &run) {
                            this.dispatch(event);
                        }
                        window.focus(&this.focus_handle, cx);
                        cx.notify();
                    }),
                )
                .child(
                    svg()
                        .path("icons/clock.svg")
                        .flex_none()
                        .size(px(14.0))
                        .text_color(muted),
                )
                .child(div().flex_1().min_w_0().truncate().child(query))
                .child(
                    div()
                        .id("remove-recent")
                        .flex_none()
                        .p_1()
                        .rounded_full()
                        .hover(move |s| s.bg(palette.wash))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this: &mut EchoApp, _, window, cx| {
                                intent::remove_recent_search(&mut this.state, index);
                                cx.stop_propagation();
                                window.prevent_default();
                                cx.notify();
                            }),
                        )
                        .child(
                            svg()
                                .path("icons/win-close.svg")
                                .size(px(12.0))
                                .text_color(muted),
                        ),
                )
        })
        .collect();
    Some(
        gpui::deferred(
            div()
                .id("recent-searches")
                .absolute()
                .top(relative(1.0))
                .left_0()
                .right_0()
                .mt_1()
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(
                    div()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.recent_searches")),
                )
                .children(rows)
                .child(
                    div()
                        .id("clear-recent")
                        .mx_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_xs()
                        .text_color(muted)
                        .cursor_pointer()
                        .hover(move |s| s.bg(palette.menu_hover))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this: &mut EchoApp, _, window, cx| {
                                intent::clear_recent_searches(&mut this.state);
                                cx.stop_propagation();
                                window.prevent_default();
                                cx.notify();
                            }),
                        )
                        .child(tr(&app.state, "desktop.clear_recent_searches")),
                ),
        )
        .into_any_element(),
    )
}

fn section_title(
    app: &EchoApp,
    key: &'static str,
    tab: SearchTab,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(fg)
                .child(tr(&app.state, key)),
        )
        .child(
            div()
                .id("show-all")
                .text_sm()
                .text_color(muted)
                .cursor_pointer()
                .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
                    this.state.ui.active_search_tab = tab;
                    this.state.ui.selected_search_index = 0;
                    window.focus(&this.focus_handle, cx);
                    cx.notify();
                }))
                .child(tr(&app.state, "desktop.search_show_all")),
        )
        .into_any_element()
}

fn top_card(app: &mut EchoApp, item: TopResult, cx: &mut Context<EchoApp>) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let background = theme.background.gpui(crate::theme::WINDOW_BG());
    let subtitle = tr(&app.state, item.subtitle_key()).replace("{credit}", &item.credit);
    let cover = thumb_element(
        app,
        item.image_url.as_deref(),
        92.0,
        item.kind == SearchTab::Artists,
        muted,
    );
    let row = item.row;
    let playable = item.playable();
    div()
        .flex_none()
        .w(px(280.0))
        .max_w_full()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(fg)
                .child(tr(&app.state, "desktop.search_top_result")),
        )
        .child(
            div()
                .id("search-top-card")
                .group("search-top")
                .relative()
                .p_4()
                .h(px(238.0))
                .rounded_lg()
                .bg(surface)
                .border_1()
                .border_color(if app.state.ui.selected_search_index == 0 {
                    palette.border
                } else {
                    palette.menu_border
                })
                .flex()
                .flex_col()
                .gap_3()
                .cursor_pointer()
                .hover(move |s| s.bg(palette.menu_hover))
                .on_click(cx.listener(|this: &mut EchoApp, _, window, cx| {
                    if let Some(event) = intent::activate_search_result(&mut this.state, 0) {
                        this.dispatch(event);
                    }
                    window.focus(&this.focus_handle, cx);
                    cx.notify();
                }))
                .child(cover)
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(fg)
                        .line_clamp(2)
                        .child(item.title),
                )
                .child(div().text_sm().text_color(muted).truncate().child(subtitle))
                .when(playable, |el| {
                    el.child(
                        div()
                            .id("top-play")
                            .absolute()
                            .right(px(12.0))
                            .bottom(px(12.0))
                            .size(px(40.0))
                            .rounded_full()
                            .bg(accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .invisible()
                            .group_hover("search-top", |s| s.visible())
                            .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
                                if let Some(event) = intent::play_search_row(&mut this.state, row) {
                                    this.dispatch(event);
                                }
                                window.focus(&this.focus_handle, cx);
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .child(
                                svg()
                                    .path("icons/play.svg")
                                    .size(px(20.0))
                                    .text_color(background),
                            ),
                    )
                }),
        )
        .into_any_element()
}

fn song_row(
    app: &mut EchoApp,
    row: SearchRow,
    flat: usize,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let track = app.state.data.search_results.tracks[row.index].clone();
    let palette = DesktopPalette::resolve(&app.state.ui.active_theme);
    let fg = app.state.ui.active_theme.text.gpui(WINDOW_FG());
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    let cover = thumb_element(app, track.image_url.as_deref(), 40.0, false, muted);
    div()
        .id(("all-song", flat))
        .h(px(56.0))
        .flex_none()
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .when(app.state.ui.selected_search_index == flat, |el| {
            el.bg(palette.row_selected)
        })
        .hover(move |s| s.bg(palette.row_hover))
        .cursor_pointer()
        .on_click(cx.listener(
            move |this: &mut EchoApp, event: &gpui::ClickEvent, window, cx| {
                this.state.ui.selected_search_index = flat;
                if event.click_count() >= 2
                    && let Some(event) = intent::activate_search_result(&mut this.state, flat)
                {
                    this.dispatch(event);
                }
                window.focus(&this.focus_handle, cx);
                cx.notify();
            },
        ))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(
                move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                    this.state.ui.selected_search_index = flat;
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
        .child(cover)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().text_sm().text_color(fg).truncate().child(track.name))
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(track.artist),
                ),
        )
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(format_time(track.duration_ms)),
        )
        .into_any_element()
}

pub fn all_results(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let rows = all_tab_rows(
        &app.state.data.search_results,
        &app.state.ui.search_context_query,
    );
    app.search_all_positions = vec![(0, None); rows.len()];
    let narrow = app.main_content_width(window) < 620.0;
    let mut top_band = div().flex().gap_4().when(narrow, |el| el.flex_col());
    if let Some(row) = rows.first()
        && let Some(item) = search_row_item(&app.state.data.search_results, *row)
    {
        top_band = top_band.child(top_card(app, item, cx));
    }
    let song_rows: Vec<_> = rows
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, row)| !row.top && row.tab == SearchTab::Tracks)
        .collect();
    if !song_rows.is_empty() {
        let mut songs = div()
            .id("all-songs")
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(section_title(
                app,
                "desktop.search_songs",
                SearchTab::Tracks,
                cx,
            ));
        for (flat, row) in song_rows {
            songs = songs.child(song_row(app, row, flat, cx));
        }
        top_band = top_band.child(songs);
    }
    let mut content = div()
        .id("search-all-scroll")
        .flex_1()
        .min_h_0()
        .py_4()
        .flex()
        .flex_col()
        .gap_6()
        .overflow_y_scroll()
        .track_scroll(&app.search_all_scroll)
        .child(top_band.flex_none().px_4());
    let mut section = 1;
    for (tab, key) in [
        (SearchTab::Albums, "ui.albums"),
        (SearchTab::Artists, "ui.artists"),
        (SearchTab::Playlists, "ui.playlists"),
    ] {
        let cards: Vec<_> = rows
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, row)| !row.top && row.tab == tab)
            .collect();
        if cards.is_empty() {
            continue;
        }
        let scroll = app.search_section_scrolls.entry(tab).or_default().clone();
        let mut card_row = div()
            .id("cards")
            .flex()
            .gap_3()
            // Keep spacing inside the scrolling viewport so artwork clips at the page edge.
            .px_4()
            .overflow_x_scroll()
            .track_scroll(&scroll);
        for (flat, row) in cards {
            app.search_all_positions[flat] = (section, Some((tab, row.index)));
            let Some(item) = search_row_item(&app.state.data.search_results, row) else {
                continue;
            };
            let kind = match tab {
                SearchTab::Artists => echo_core::home::HomeItemKind::Artist,
                SearchTab::Albums => echo_core::home::HomeItemKind::Album,
                _ => echo_core::home::HomeItemKind::Playlist,
            };
            card_row = card_row.child(home_card(
                app,
                echo_core::home::HomeItem {
                    id: item.id,
                    kind,
                    title: item.title,
                    subtitle: item.credit,
                    image_url: item.image_url,
                    owner_id: None,
                    track: None,
                    release_year: None,
                },
                flat,
                false,
                Some(row),
                cx,
            ));
        }
        content = content.child(
            div()
                .id(SharedString::from(format!("search-section-{tab:?}")))
                .flex_none()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().px_4().child(section_title(app, key, tab, cx)))
                .child(card_row),
        );
        section += 1;
    }
    echo_core::thumbnails::drain_pending(&mut app.state, &app.worker_tx);
    content.into_any_element()
}

pub(super) fn search_results(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
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
        SearchTab::All => echo_core::search::all_tab_rows(results, &query).len(),
        SearchTab::Tracks => n_tracks,
        SearchTab::Albums => n_albums,
        SearchTab::Artists => n_artists,
        SearchTab::Playlists => n_playlists,
    };

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(search_header(app, cx))
        .child(if count == 0 {
            search::empty_results(app)
        } else if tab == SearchTab::All {
            search::all_results(app, window, cx)
        } else {
            uniform_list(
                "search-rows",
                count,
                cx.processor(
                    |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
                        search_rows(this, range, cx)
                    },
                ),
            )
            .track_scroll(&app.search_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

/// The query line over the tab pills.
fn search_header(app: &EchoApp, cx: &mut Context<EchoApp>) -> Div {
    let theme = &app.state.ui.active_theme;
    let outer_palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let tab = app.state.ui.active_search_tab;
    let query = &app.state.ui.search_context_query;

    let tab_button = |id: &'static str, label: String, target: SearchTab, active: bool| {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_full()
            .bg(outer_palette.wash)
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
                .flex_wrap()
                .child(tab_button(
                    "search-all",
                    tr(&app.state, "desktop.search_all").to_string(),
                    SearchTab::All,
                    tab == SearchTab::All,
                ))
                .child(tab_button(
                    "search-tracks",
                    tr(&app.state, "desktop.search_songs").to_string(),
                    SearchTab::Tracks,
                    tab == SearchTab::Tracks,
                ))
                .child(tab_button(
                    "search-albums",
                    tr(&app.state, "ui.albums").to_string(),
                    SearchTab::Albums,
                    tab == SearchTab::Albums,
                ))
                .child(tab_button(
                    "search-artists",
                    tr(&app.state, "ui.artists").to_string(),
                    SearchTab::Artists,
                    tab == SearchTab::Artists,
                ))
                .child(tab_button(
                    "search-playlists",
                    tr(&app.state, "ui.playlists").to_string(),
                    SearchTab::Playlists,
                    tab == SearchTab::Playlists,
                )),
        )
}

fn search_rows(
    this: &mut EchoApp,
    range: std::ops::Range<usize>,
    cx: &mut Context<EchoApp>,
) -> Vec<AnyElement> {
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
            pill_row(
                ix,
                LIST_PILL,
                row_selected(ix, selected, visual),
                selected_bg,
                palette.row_hover,
                |row| {
                    let row = row.gap_3().on_click(cx.listener(
                        move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                            this.state.ui.selected_search_index = ix;
                            if (tab != SearchTab::Tracks || event.click_count() >= 2)
                                && let Some(event) =
                                    echo_core::intent::activate_search_result(&mut this.state, ix)
                            {
                                this.dispatch(event);
                            }
                            cx.notify();
                        },
                    ));

                    match tab {
                        SearchTab::All => row,
                        SearchTab::Tracks => {
                            search_track_row(this, row, ix, fg, muted, palette, cx)
                        }
                        SearchTab::Albums => {
                            let album = this.state.data.search_results.albums[ix].clone();
                            let thumb =
                                thumb_element(this, album.image_url.as_deref(), 26.0, false, muted);
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
                            let artist = this.state.data.search_results.artists[ix].clone();
                            let thumb =
                                thumb_element(this, artist.image_url.as_deref(), 26.0, true, muted);
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
                            let playlist = this.state.data.search_results.playlists[ix].clone();
                            let thumb_url = playlist
                                .thumb_url
                                .clone()
                                .or_else(|| playlist.image_url.clone());
                            let thumb =
                                thumb_element(this, thumb_url.as_deref(), 26.0, false, muted);
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
                },
            )
            .into_any_element()
        })
        .collect();

    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);
    rows
}

fn search_track_row(
    this: &mut EchoApp,
    row: Stateful<Div>,
    ix: usize,
    fg: Hsla,
    muted: Hsla,
    palette: DesktopPalette,
    cx: &mut Context<EchoApp>,
) -> Stateful<Div> {
    let track = this.state.data.search_results.tracks[ix].clone();
    let is_liked = this.state.data.liked_tracks.contains(&track.id);
    let secondary = this.state.ui.active_theme.secondary.gpui(WINDOW_FG());
    row.on_mouse_down(
        MouseButton::Right,
        cx.listener(
            move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                this.state.ui.selected_search_index = ix;
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
            .child(SharedString::from(format_time(track.duration_ms))),
    )
}
