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
use gpui::{AnyElement, Context, SharedString, Window, div, img, prelude::*, px, uniform_list};

use crate::theme::{ToGpui, WINDOW_FG};
use crate::{EchoApp, format_time};

const SIDEBAR_WIDTH: f32 = 260.0;
const SIDEBAR_ROW_HEIGHT: f32 = 34.0;
const ROW_HEIGHT: f32 = 30.0;
const THUMB_EDGE: f32 = 26.0;

pub fn sidebar(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let tab = app.state.ui.active_library_tab;
    let count = match tab {
        LibraryTab::Albums => app.state.data.saved_albums.len(),
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
                cx.notify();
            }))
            .child(label)
    };

    div()
        .flex_none()
        .w(px(SIDEBAR_WIDTH))
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
                .child(tab_button("Playlists", LibraryTab::Playlists, tab != LibraryTab::Albums))
                .child(tab_button("Albums", LibraryTab::Albums, tab == LibraryTab::Albums))
                .child({
                    // Not a LibraryTab: mirrors the TUI's Browse → Followed Artists node.
                    let active = matches!(
                        app.state.ui.active_view,
                        ActiveView::ArtistList | ActiveView::ArtistPage
                    );
                    div()
                        .id("artists-tab")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_sm()
                        .text_color(if active { accent } else { muted })
                        .hover(|style| style.bg(accent.opacity(0.1)))
                        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                            if let Some(event) =
                                echo_core::intent::open_artist_list(&mut this.state)
                            {
                                this.dispatch(event);
                            }
                            cx.notify();
                        }))
                        .child("Artists")
                }),
        )
        .child({
            // The TUI's Browse nodes, as quick links.
            let browse_link = |id: &'static str,
                               label: &'static str,
                               open: fn(&mut echo_core::app::AppState)
                                   -> Option<echo_core::events::AppEvent>| {
                div()
                    .id(id)
                    .px_3()
                    .py_1()
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
                    .child(label)
            };
            div()
                .flex_none()
                .flex()
                .flex_col()
                .pb_1()
                .child(browse_link(
                    "top-tracks",
                    "⭐ Top Tracks",
                    echo_core::intent::open_top_tracks,
                ))
                .child(browse_link(
                    "recently-played",
                    "🕒 Recently Played",
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
                    let tab = this.state.ui.active_library_tab;
                    let selected = this.state.ui.selected_playlist_index;

                    let rows: Vec<_> = range
                        .map(|ix| {
                            // Folders carry no cover; playlists and albums get a thumb box even
                            // while (or if never) loaded, so the text column stays aligned.
                            let (label, label_color, indent_px, thumb_url, has_thumb): (
                                SharedString,
                                _,
                                f32,
                                Option<String>,
                                bool,
                            ) = if tab == LibraryTab::Albums {
                                let album = &this.state.data.saved_albums[ix];
                                let url = album
                                    .thumb_url
                                    .clone()
                                    .or_else(|| album.image_url.clone());
                                (album.name.clone().into(), fg, 0.0, url, true)
                            } else {
                                match &this.state.data.library_view[ix] {
                                    LibraryNode::Folder(f) => (
                                        format!(
                                            "{} {}",
                                            if f.is_open { "▼" } else { "▶" },
                                            f.name
                                        )
                                        .into(),
                                        accent,
                                        0.0,
                                        None,
                                        false,
                                    ),
                                    LibraryNode::Playlist { playlist, indent } => {
                                        let pin = if this
                                            .state
                                            .ui
                                            .library_config
                                            .pinned
                                            .contains(&playlist.id)
                                        {
                                            "📌 "
                                        } else {
                                            ""
                                        };
                                        let url = playlist
                                            .thumb_url
                                            .clone()
                                            .or_else(|| playlist.image_url.clone());
                                        (
                                            format!("{pin}{}", playlist.name).into(),
                                            fg,
                                            *indent as f32 * 14.0,
                                            url,
                                            true,
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
                                    Some(image) => img(image)
                                        .flex_none()
                                        .w(px(THUMB_EDGE))
                                        .h(px(THUMB_EDGE))
                                        .rounded_sm()
                                        .into_any_element(),
                                    None => div()
                                        .flex_none()
                                        .w(px(THUMB_EDGE))
                                        .h(px(THUMB_EDGE))
                                        .rounded_sm()
                                        .bg(muted.opacity(0.15))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_xs()
                                        .text_color(muted)
                                        .child("♪")
                                        .into_any_element(),
                                }
                            });

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
                                .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                                    this.state.ui.selected_playlist_index = ix;
                                    let event = match this.state.ui.active_library_tab {
                                        LibraryTab::Albums => {
                                            echo_core::intent::open_album(&mut this.state, ix)
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
                                .when_some(thumb, |el, thumb| el.child(thumb))
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
}

pub fn main_area(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());

    let search = search_bar(app, window, cx).into_any_element();
    let body = match app.state.ui.active_view {
        ActiveView::TrackList => track_list(app, cx).into_any_element(),
        ActiveView::Queue => queue_list(app, cx).into_any_element(),
        ActiveView::SearchResults => search_results(app, cx).into_any_element(),
        ActiveView::ArtistList => artist_list(app, cx).into_any_element(),
        ActiveView::ArtistPage => artist_page(app, cx).into_any_element(),
        _ => div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child(status_line(app))
            .into_any_element(),
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

    div().flex_none().px_4().pt_3().child(
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
            .child(div().text_sm().text_color(muted).child("🔍"))
            .child(if query.is_empty() && !focused {
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Search — press /")
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
                                .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                                    this.state.ui.selected_track_index = ix;
                                    if let Some(event) =
                                        echo_core::intent::play_track_at(&mut this.state, ix)
                                    {
                                        this.dispatch(event);
                                    }
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

/// A cover thumbnail box riding the core thumbnail cache, or a ♪ placeholder.
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
                .text_xs()
                .text_color(muted)
                .child("♪");
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
                            let event = echo_core::intent::back_to_artist_list(&mut this.state);
                            this.dispatch(event);
                            cx.notify();
                        }))
                        .child("←"),
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
                .bg(crate::theme::WINDOW_BG())
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

/// The Spotify Connect device picker, painted over everything else.
pub fn device_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
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
                .bg(crate::theme::WINDOW_BG())
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

fn status_line(app: &EchoApp) -> SharedString {
    match app.state.ui.mode {
        AppMode::Setup => {
            "Set up Spotify credentials in the terminal app first: run `spotify`".into()
        }
        AppMode::Authenticating => "Authenticating with Spotify…".into(),
        _ => {
            let playlists = app.state.data.playlists.len();
            let albums = app.state.data.saved_albums.len();
            format!(
                "Library loaded: {playlists} playlists, {albums} albums — pick one on the left"
            )
            .into()
        }
    }
}
