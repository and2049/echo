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

use echo_core::app::{ActiveView, AppMode, LibraryTab};
use echo_core::models::LibraryNode;
use echo_core::thumbnails::ThumbState;
use gpui::{AnyElement, Context, SharedString, div, img, prelude::*, px, uniform_list};

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
                .child(tab_button("Albums", LibraryTab::Albums, tab == LibraryTab::Albums)),
        )
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

pub fn main_area(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());

    let body = if app.state.ui.active_view == ActiveView::TrackList {
        track_list(app, cx).into_any_element()
    } else {
        div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child(status_line(app))
            .into_any_element()
    };

    div()
        .flex_grow(1.0)
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(body)
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
