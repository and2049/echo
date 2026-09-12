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

pub fn recent_searches(app: &EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let palette = DesktopPalette::resolve(&app.state.ui.active_theme);
    let fg = app.state.ui.active_theme.text.gpui(WINDOW_FG());
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    div()
        .id("recent-searches")
        .flex_1()
        .p_4()
        .flex()
        .flex_col()
        .gap_2()
        .overflow_y_scroll()
        .child(
            div()
                .text_lg()
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(fg)
                .child(tr(&app.state, "desktop.recent_searches")),
        )
        .children(
            app.state
                .ui
                .library_config
                .recent_searches
                .iter()
                .enumerate()
                .map(|(index, query)| {
                    let query = query.clone();
                    div()
                        .id(index)
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .text_color(fg)
                        .cursor_pointer()
                        .hover(move |s| s.bg(palette.row_hover))
                        .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
                            if let Some(event) = intent::global_search(&mut this.state, &query) {
                                this.dispatch(event);
                            }
                            window.focus(&this.focus_handle, cx);
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path("icons/clock.svg")
                                .size(px(18.0))
                                .text_color(muted),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(app.state.ui.library_config.recent_searches[index].clone()),
                        )
                        .child(
                            div()
                                .id("remove-recent")
                                .p_1()
                                .rounded_full()
                                .hover(move |s| s.bg(palette.wash))
                                .on_click(cx.listener(move |this: &mut EchoApp, _, _, cx| {
                                    intent::remove_recent_search(&mut this.state, index);
                                    cx.stop_propagation();
                                    cx.notify();
                                }))
                                .child(
                                    svg()
                                        .path("icons/win-close.svg")
                                        .size(px(16.0))
                                        .text_color(muted),
                                ),
                        )
                }),
        )
        .child(
            div()
                .id("clear-recent")
                .py_2()
                .text_sm()
                .text_color(muted)
                .cursor_pointer()
                .on_click(cx.listener(|this: &mut EchoApp, _, _, cx| {
                    intent::clear_recent_searches(&mut this.state);
                    cx.notify();
                }))
                .child(tr(&app.state, "desktop.clear_recent_searches")),
        )
        .into_any_element()
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
        .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
            if let Some(event) = intent::activate_search_result(&mut this.state, flat) {
                this.dispatch(event);
            }
            window.focus(&this.focus_handle, cx);
            cx.notify();
        }))
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
    let sidebar = if app.sidebar_collapsed {
        0.0
    } else {
        app.sidebar_width
    };
    let narrow = f32::from(window.viewport_size().width) - sidebar < 620.0;
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
        .p_4()
        .flex()
        .flex_col()
        .gap_6()
        .overflow_y_scroll()
        .track_scroll(&app.search_all_scroll)
        .child(top_band.flex_none());
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
                .child(section_title(app, key, tab, cx))
                .child(card_row),
        );
        section += 1;
    }
    echo_core::thumbnails::drain_pending(&mut app.state, &app.worker_tx);
    content.into_any_element()
}
