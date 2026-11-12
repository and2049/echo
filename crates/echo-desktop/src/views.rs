//! Library sidebar and track list — the first real views of the desktop app.
//!
//! Both are thin projections of [`AppState`]: rows come straight from `library_view` /
//! `saved_albums` / `tracks`, and keyboard or pointer activation goes through
//! [`echo_core::intent`].
//!
//! Thumbnails ride the core's [`echo_core::thumbnails`] cache. Rendering a row requests its
//! cover; `drain_pending` (called after each row batch) spawns the fetches, and the resulting
//! worker events repaint. This view always renders thumbnails because they fit the available
//! space and render reliably.

use echo_core::app::{ActiveView, AppMode, LibraryTab, QueueRow, QueueTab, SearchTab};
use echo_core::config::RightPanel;
use echo_core::models::{ActionMenuAction, ActionMenuContext, LibraryNode};
use echo_core::thumbnails::{ThumbState, ThumbTier, tier_for_edge};
mod artist;
mod chrome;
mod drag;
mod home;
mod immersive;
mod lyrics;
mod menu_placement;
mod menus;
pub(crate) mod panel_layout;
mod playlist_edit;
mod queue;
mod search;
mod setup;
mod sidebar;
mod track_list;
use artist::*;
pub(crate) use chrome::*;
pub(crate) use drag::*;
use gpui::{
    Animation, AnimationExt, AnyElement, App, Context, Div, Entity, Hsla, MouseButton,
    MouseDownEvent, SharedString, Stateful, Window, canvas, div, ease_out_quint, prelude::*, px,
    relative, svg, uniform_list,
};
use home::*;
pub(crate) use immersive::*;
pub(crate) use lyrics::*;
use menu_placement::{MenuAnchor, menu_bounds, menu_placement};
pub(crate) use menus::*;
pub use playlist_edit::{playlist_edit_modal, playlist_page_menu};
use queue::*;
use setup::*;
pub(crate) use sidebar::*;
pub(crate) use track_list::*;

use crate::backdrop::{Backdrop, ImmersiveColors};
use crate::theme::{DesktopPalette, ToGpui, WINDOW_FG};
use crate::{ControlColors, EchoApp, MenuAction, TrackMenuItem, UpdateState, format_time};

/// The shape of a selectable list row: the fixed height `uniform_list` measures and the
/// shorter pill drawn inside it. Hover strength comes from the palette slot the caller
/// passes ([`DesktopPalette::row_hover`] for both the sidebar and the main
/// area). See [`pill_row`].
#[derive(Clone, Copy)]
pub(crate) struct PillMetrics {
    row_height: f32,
    pill_height: f32,
}

/// Main-area lists carrying a thumbnail column.
const LIST_PILL: PillMetrics = PillMetrics {
    row_height: 34.0,
    pill_height: 30.0,
};
/// The denser text-only lists — tracks and the queue.
const COMPACT_PILL: PillMetrics = PillMetrics {
    row_height: 30.0,
    pill_height: 26.0,
};

const TRACK_PILL: PillMetrics = PillMetrics {
    row_height: 48.0,
    pill_height: 44.0,
};

/// Whether row `ix` reads as selected: the cursor row, or any row inside a visual range.
/// The range shares the cursor's highlight so the selection looks like one block.
fn row_selected(ix: usize, selected: usize, visual: Option<(usize, usize)>) -> bool {
    ix == selected || visual.is_some_and(|(start, end)| ix >= start && ix <= end)
}

fn row_marked(
    ix: usize,
    selected: usize,
    visual: Option<(usize, usize)>,
    picked: &std::collections::BTreeSet<usize>,
) -> bool {
    row_selected(ix, selected, visual) || picked.contains(&ix)
}

/// Resolves a translation in the configured language. Every user-facing desktop string goes
/// through this so `:lang` applies on the next frame. Missing keys fall back to English inside
/// [`echo_core::i18n::t`].
pub(crate) fn tr(state: &echo_core::app::AppState, key: &str) -> SharedString {
    SharedString::from(echo_core::i18n::t(key, &state.ui.library_config.language))
}

/// The visual range for one pane. The range is bare row indices measured against the active
/// view's cursor, so panes other than the one visual mode was entered in must ignore it.
fn visual_range_in(state: &echo_core::app::AppState, view: ActiveView) -> Option<(usize, usize)> {
    (state.ui.active_view == view)
        .then(|| state.get_visual_selection_range())
        .flatten()
}

/// A selectable list row, drawn as an inset rounded pill.
///
/// Every list in the app shares this shell. `uniform_list` measures a fixed row height, so the
/// pill takes its vertical inset from being shorter than the row rather than from a margin, which
/// would drift the list's item accounting; the horizontal inset is padding on the inert outer div,
/// leaving the pill's own padding free for per-list indents.
///
/// `build` decorates the pill — children, gaps, click handlers, drag sources. Hover is skipped on
/// the selected row: the hover tint is fainter than the selection colour, so it would otherwise
/// wash the selection out as the cursor passes over it.
fn pill_row(
    ix: usize,
    metrics: PillMetrics,
    selected: bool,
    selected_bg: Hsla,
    hover_bg: Hsla,
    build: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
) -> Div {
    let pill = div()
        .id(ix)
        .w_full()
        .h(px(metrics.pill_height))
        .px_2()
        .rounded_md()
        .flex()
        .flex_row()
        .items_center()
        .text_sm()
        .when(selected, |el| el.bg(selected_bg))
        .when(!selected, |el| el.hover(move |style| style.bg(hover_bg)))
        .cursor_pointer();

    div()
        .w_full()
        .h(px(metrics.row_height))
        .px_2()
        .flex()
        .items_center()
        .child(build(pill))
}

/// The start-screen wordmark, shown in the main area while nothing is selected.
const ECHO_LOGO: [&str; 6] = [
    "███████╗ ██████╗██╗  ██╗ ██████╗               ██████╗ ███████╗",
    "██╔════╝██╔════╝██║  ██║██╔═══██╗              ██╔══██╗██╔════╝",
    "█████╗  ██║     ███████║██║   ██║    █████╗    ██████╔╝███████╗",
    "██╔══╝  ██║     ██╔══██║██║   ██║    ╚════╝    ██╔══██╗╚════██║",
    "███████╗╚██████╗██║  ██║╚██████╔╝              ██║  ██║███████║",
    "╚══════╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝               ╚═╝  ╚═╝╚══════╝",
];

/// The picked rows of `view`, when it is the one on screen.
fn picked_in(
    state: &echo_core::app::AppState,
    view: ActiveView,
) -> std::collections::BTreeSet<usize> {
    if state.ui.active_view == view {
        state.ui.picked_rows.clone()
    } else {
        std::collections::BTreeSet::new()
    }
}

fn is_pick_click(event: &gpui::ClickEvent) -> bool {
    let modifiers = event.modifiers();
    modifiers.secondary() || modifiers.control
}

fn nav_button_cluster(app: &EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let can_back = !app.state.ui.view_history.is_empty();
    let can_forward = !app.state.ui.forward_history.is_empty();

    let history_button = |id: &'static str,
                          icon: &'static str,
                          enabled: bool,
                          cx: &mut Context<EchoApp>,
                          go: fn(&mut EchoApp, &mut Context<EchoApp>)| {
        if enabled {
            crate::icon_button(id, icon, muted, palette.wash, cx, go).into_any_element()
        } else {
            // `border` rather than `wash`: at icon-glyph size the 15% wash all but vanishes.
            crate::icon_button(
                id,
                icon,
                palette.border,
                gpui::transparent_black(),
                cx,
                |_, _| {},
            )
            .into_any_element()
        }
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(crate::icon_button(
            "sidebar-toggle",
            "icons/sidebar-left.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| this.toggle_sidebar(cx),
        ))
        .child(history_button(
            "history-back",
            "icons/arrow-left.svg",
            can_back,
            cx,
            |this, cx| {
                this.history_back(cx);
            },
        ))
        .child(history_button(
            "history-forward",
            "icons/arrow-right.svg",
            can_forward,
            cx,
            |this, cx| {
                this.history_forward(cx);
            },
        ))
}

pub fn main_area(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let search = search_bar(app, window, cx).into_any_element();
    let body = if app.state.ui.mode == AppMode::Setup {
        setup_view(app, window, cx).into_any_element()
    } else {
        match app.state.ui.active_view {
            ActiveView::Home => home_view(app, window, cx).into_any_element(),
            ActiveView::TrackList => track_list(app, window, cx).into_any_element(),
            ActiveView::Queue => queue_list(app, cx).into_any_element(),
            ActiveView::SearchResults => search_results(app, window, cx).into_any_element(),
            ActiveView::ArtistList => artist_list(app, cx).into_any_element(),
            ActiveView::ArtistPage => artist_page(app, cx).into_any_element(),
            ActiveView::WhatsNew => whats_new(app, cx).into_any_element(),
            _ if app.state.ui.mode != AppMode::Authenticating
                && (app.state.data.active_tracklist_context.is_some()
                    || !app.state.data.tracks.is_empty()) =>
            {
                track_list(app, window, cx).into_any_element()
            }
            _ if app.state.ui.mode == AppMode::Authenticating => library_placeholder(app),
            _ => home_view(app, window, cx).into_any_element(),
        }
    };

    let palette = DesktopPalette::resolve(&app.state.ui.active_theme);
    let background = app
        .state
        .ui
        .active_theme
        .background
        .gpui(crate::theme::WINDOW_BG());
    let body = if app.sidebar_collapsed {
        body
    } else {
        rounded_content_frame(body, background, palette.border).into_any_element()
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

/// Join the library edge to the search header with an inner top-left corner. GPUI's
/// overflow mask is rectangular, so paint the corner cutout after the scrolling content.
/// The border belongs to this frame, not the sidebar/header, avoiding a square junction.
fn rounded_content_frame(body: AnyElement, background: Hsla, border: Hsla) -> impl IntoElement {
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .pt(px(1.0))
        .pl(px(1.0))
        .child(body)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let radius = px(12.0).min(bounds.size.width).min(bounds.size.height);
                    let point = |x, y| bounds.origin + gpui::point(x, y);
                    // A circular quarter-arc, approximated by a cubic Bezier.
                    let control = radius * (1.0 - 0.552_284_8);
                    let mut mask = gpui::PathBuilder::fill();
                    mask.move_to(point(px(0.0), px(0.0)));
                    mask.line_to(point(radius, px(0.0)));
                    mask.cubic_bezier_to(
                        point(px(0.0), radius),
                        point(control, px(0.0)),
                        point(px(0.0), control),
                    );
                    mask.close();
                    if let Ok(path) = mask.build() {
                        window.paint_path(path, background);
                    }

                    let half = px(0.5);
                    let control = radius - (radius - half) * 0.552_284_8;
                    let mut edge = gpui::PathBuilder::stroke(px(1.0));
                    edge.move_to(point(bounds.size.width, half));
                    edge.line_to(point(radius, half));
                    edge.cubic_bezier_to(
                        point(half, radius),
                        point(control, half),
                        point(half, control),
                    );
                    edge.line_to(point(half, bounds.size.height));
                    if let Ok(path) = edge.build() {
                        window.paint_path(path, border);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
}

/// The global search box. A minimal hand-rolled input: gpui ships no text field, and query
/// entry needs no more than append/backspace/submit.
fn search_bar(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let focused = app.search_focus.is_focused(window);
    let query = app.search_input.clone();
    let collapsed = app.sidebar_collapsed;
    let nav_cluster = collapsed.then(|| nav_button_cluster(app, cx).into_any_element());
    let recent = focused.then(|| search::recent_searches(app, cx)).flatten();

    div()
        .flex_none()
        // Left/top spacing mirrors the sidebar header so its button cluster stays aligned.
        // Equal bottom spacing keeps scrolling content clear of the search box.
        .pl_2()
        .pr_4()
        .py_2()
        .when(collapsed, |el| el.border_b_1().border_color(palette.border))
        .flex()
        .flex_row()
        .items_center()
        // With the sidebar collapsed its button cluster moves here; otherwise an invisible
        // stand-in balancing the right-side buttons (104 = immersive + themes + settings +
        // pr_4 - pl_2) so the search box centers exactly.
        .map(|el| match nav_cluster {
            Some(cluster) => el.child(cluster),
            None => el.child(div().flex_none().w(px(104.0))),
        })
        .child(div().flex_grow(1.0))
        .child(
            div()
                .id("search-box")
                .key_context(crate::SEARCH_CONTEXT)
                .track_focus(&app.search_focus)
                .relative()
                .children(recent)
                .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                    this.handle_search_key(event, window, cx)
                }))
                .w(px(360.0))
                .px_3()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(if focused { accent } else { palette.border })
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .on_click(cx.listener(|this: &mut EchoApp, _event, window, cx| {
                    window.focus(&this.search_focus, cx);
                    cx.notify();
                }))
                .child(
                    svg()
                        .path("icons/search.svg")
                        .flex_none()
                        .w(px(14.0))
                        .h(px(14.0))
                        .text_color(muted),
                )
                .child(if query.is_empty() {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.search_placeholder"))
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(fg)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .child(SharedString::from(if focused {
                            crate::text_with_cursor(&query, app.search_cursor)
                        } else {
                            query.clone()
                        }))
                        .into_any_element()
                })
                .when(!query.is_empty(), |el| {
                    el.child(
                        div()
                            .id("clear-search")
                            .flex_none()
                            .p_1()
                            .rounded_full()
                            .hover(move |s| s.bg(palette.wash))
                            .on_click(cx.listener(|this: &mut EchoApp, _, window, cx| {
                                this.search_input.clear();
                                this.search_cursor = 0;
                                window.focus(&this.search_focus, cx);
                                cx.stop_propagation();
                                cx.notify();
                            }))
                            .child(
                                svg()
                                    .path("icons/win-close.svg")
                                    .size(px(14.0))
                                    .text_color(muted),
                            ),
                    )
                }),
        )
        .child(div().flex_grow(1.0))
        .child(immersive_button(muted, palette.wash, cx))
        .child(crate::icon_button(
            "themes",
            "icons/paint-board.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| this.toggle_themes(cx),
        ))
        .child(crate::icon_button(
            "settings",
            "icons/settings.svg",
            muted,
            palette.wash,
            cx,
            |this, cx| this.toggle_settings(cx),
        ))
}

/// "Playing from {}" label for the playback bar, resolving the playing context id to a
/// name from whatever state already holds it (loaded track list, library lists, artist
/// page, What's New). Falls back to a generic "a playlist"/"an album" when the id is not
/// loaded anywhere; `None` when nothing tracks the context at all.
pub(crate) fn playing_context_label(state: &echo_core::app::AppState) -> Option<String> {
    let context = state.playback.playing_context.as_ref()?;
    let from_tracklist = state
        .data
        .active_tracklist_context
        .as_ref()
        .filter(|c| c.id == context.context_id)
        .map(|c| c.title.clone());
    let name = if context.is_album {
        from_tracklist
            .or_else(|| {
                state
                    .data
                    .saved_albums
                    .iter()
                    .find(|album| album.id == context.context_id)
                    .map(|album| album.name.clone())
            })
            .or_else(|| {
                state.data.artist_page_data.as_ref().and_then(|data| {
                    data.albums
                        .iter()
                        .find(|album| album.id == context.context_id)
                        .map(|album| album.name.clone())
                })
            })
            .or_else(|| {
                state
                    .data
                    .whats_new
                    .iter()
                    .find(|album| album.id == context.context_id)
                    .map(|album| album.name.clone())
            })
            .unwrap_or_else(|| tr(state, "desktop.playing_from_album").to_string())
    } else {
        state
            .data
            .playlists
            .iter()
            .find(|playlist| playlist.id == context.context_id)
            .map(|playlist| playlist.name.clone())
            .or_else(|| {
                state
                    .data
                    .local_playlists
                    .playlists
                    .iter()
                    .find(|playlist| playlist.id == context.context_id)
                    .map(|playlist| playlist.name.clone())
            })
            .or(from_tracklist)
            .unwrap_or_else(|| tr(state, "desktop.playing_from_playlist").to_string())
    };
    Some(tr(state, "desktop.playing_from").replace("{}", &name))
}

/// Index-column text for a list row: numbered from 1, or vim-style distances from the
/// selected row under `relative_line_numbers`.
fn row_number(state: &echo_core::app::AppState, ix: usize, selected: usize) -> SharedString {
    SharedString::from(
        echo_core::app::displayed_track_number(
            ix,
            selected,
            state.ui.library_config.relative_line_numbers,
        )
        .to_string(),
    )
}

/// A fixed-width heart slot ahead of the duration column: filled when the track is in
/// Liked Songs, faint otherwise so durations stay aligned across rows. Clicking toggles
/// the like (un-liking through the confirm prompt), same as `l` on the row.
fn liked_cell(
    section: &'static str,
    ix: usize,
    track_id: String,
    liked: bool,
    color: Hsla,
    palette: DesktopPalette,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let group = SharedString::from(format!("liked-{section}-{ix}"));
    div()
        .id(group.clone())
        .group(group.clone())
        .flex_none()
        .w(px(14.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(cx.listener(
            move |this: &mut EchoApp, _event: &gpui::ClickEvent, _window, cx| {
                cx.stop_propagation();
                if let Some(event) =
                    echo_core::intent::toggle_like_track(&mut this.state, track_id.clone())
                {
                    this.dispatch(event);
                }
                cx.notify();
            },
        ))
        .child(
            // gpui svgs don't inherit text color, so the tint (and its hover swap) must
            // live on the svg itself.
            svg()
                .path("icons/heart.svg")
                .size(px(12.0))
                .text_color(if liked { color } else { palette.like_dim })
                .when(!liked, |el| {
                    el.group_hover(group, move |style| style.text_color(color))
                }),
        )
}

/// A cover thumbnail box riding the core thumbnail cache, or a music-note placeholder.
/// A square, centre-cropped picture with `radius` corners. Painted on a canvas rather than
/// through `img`: gpui stamps the picture's aspect ratio onto the box and taffy then lets a
/// portrait picture grow taller than the height it was given, which turned artist portraits
/// into ovals that ran into the row below.
pub(crate) fn cover_image(
    image: std::sync::Arc<gpui::RenderImage>,
    edge: f32,
    radius: f32,
) -> impl IntoElement {
    let radii = gpui::Corners::all(px(radius));
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let image_bounds = gpui::ObjectFit::Cover.get_bounds(bounds, image.size(0));
            window
                .paint_image(bounds, image_bounds, radii, image, 0, false)
                .ok();
        },
    )
    .flex_none()
    .size(px(edge))
}

/// The cached thumbnail for `url` at `edge`, requesting it when missing.
fn thumb_image(
    this: &mut EchoApp,
    url: Option<&str>,
    edge: f32,
) -> Option<std::sync::Arc<gpui::RenderImage>> {
    let url = url?;
    let tier = tier_for_edge(edge);
    this.state.ui.thumbnails.request(url, tier);
    let artwork = match this.state.ui.thumbnails.get(url, tier) {
        Some(ThumbState::Ready { artwork }) => artwork.clone(),
        _ => return None,
    };
    this.images.get(&artwork)
}

fn thumb_element(
    this: &mut EchoApp,
    url: Option<&str>,
    edge: f32,
    round: bool,
    muted: gpui::Hsla,
) -> AnyElement {
    match thumb_image(this, url, edge) {
        Some(image) => {
            let radius = if round {
                edge / 2.0
            } else if edge >= 56.0 {
                8.0
            } else {
                4.0
            };
            cover_image(image, edge, radius).into_any_element()
        }
        None => {
            let el = div()
                .flex_none()
                .w(px(edge))
                .h(px(edge))
                .bg(DesktopPalette::resolve(&this.state.ui.active_theme).wash)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path("icons/music-note.svg")
                        .w(px((edge * 0.45).max(10.0)))
                        .h(px((edge * 0.45).max(10.0)))
                        .text_color(muted),
                );
            if round {
                el.rounded_full()
            } else if edge >= 56.0 {
                el.rounded(px(8.0))
            } else {
                el.rounded_sm()
            }
            .into_any_element()
        }
    }
}

fn search_results(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let outer_palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

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
                ),
        )
        .child(if count == 0 {
            search::empty_results(app)
        } else if tab == SearchTab::All {
            search::all_results(app, window, cx)
        } else {
            uniform_list(
                "search-rows",
                count,
                cx.processor(
                    move |this: &mut EchoApp, range: std::ops::Range<usize>, _window, cx| {
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
                                            move |this: &mut EchoApp,
                                                  event: &gpui::ClickEvent,
                                                  _window,
                                                  cx| {
                                                this.state.ui.selected_search_index = ix;
                                                if (tab != SearchTab::Tracks
                                                    || event.click_count() >= 2)
                                                    && let Some(event) =
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
                                            SearchTab::All => row,
                                            SearchTab::Tracks => {
                                                let track = this.state.data.search_results.tracks
                                                    [ix]
                                                    .clone();
                                                let is_liked = this
                                                    .state
                                                    .data
                                                    .liked_tracks
                                                    .contains(&track.id);
                                                let secondary = this
                                                    .state
                                                    .ui
                                                    .active_theme
                                                    .secondary
                                                    .gpui(WINDOW_FG());
                                                row.on_mouse_down(
                                                    MouseButton::Right,
                                                    cx.listener(move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
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
                                                    }),
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
                                                        .child(SharedString::from(format_time(
                                                            track.duration_ms,
                                                        ))),
                                                )
                                            }
                                            SearchTab::Albums => {
                                                let album = this.state.data.search_results.albums
                                                    [ix]
                                                    .clone();
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
                                                            .child(SharedString::from(
                                                                album.artist,
                                                            )),
                                                    )
                                            }
                                            SearchTab::Artists => {
                                                let artist = this.state.data.search_results.artists
                                                    [ix]
                                                    .clone();
                                                let thumb = thumb_element(
                                                    this,
                                                    artist.image_url.as_deref(),
                                                    26.0,
                                                    true,
                                                    muted,
                                                );
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
                                                let playlist =
                                                    this.state.data.search_results.playlists[ix]
                                                        .clone();
                                                let thumb_url = playlist
                                                    .thumb_url
                                                    .clone()
                                                    .or_else(|| playlist.image_url.clone());
                                                let thumb = thumb_element(
                                                    this,
                                                    thumb_url.as_deref(),
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
                                                            .child(SharedString::from(
                                                                playlist.name,
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_grow(1.0)
                                                            .flex_basis(px(0.0))
                                                            .overflow_hidden()
                                                            .text_ellipsis()
                                                            .whitespace_nowrap()
                                                            .text_color(muted)
                                                            .child(SharedString::from(
                                                                playlist.owner,
                                                            )),
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
                    },
                ),
            )
            .track_scroll(&app.search_scroll)
            .flex_grow(1.0)
            .into_any_element()
        })
}

/// The Top Tracks / Top Artists time-window selector: three small pills, the active one
/// accent-bordered. Persisting the choice and refetching go through the shared intent.
fn range_switcher(app: &EchoApp, cx: &mut Context<EchoApp>) -> Div {
    use echo_core::models::TopItemsRange;
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let active = app.state.ui.library_config.top_items_range;

    let mut row = div().flex().flex_row().items_center().gap_1();
    for (range, key) in [
        (TopItemsRange::Short, "desktop.range_short"),
        (TopItemsRange::Medium, "desktop.range_medium"),
        (TopItemsRange::Long, "desktop.range_long"),
    ] {
        let is_active = range == active;
        row = row.child(
            div()
                .id(SharedString::from(format!("range-{key}")))
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
                    if let Some(event) =
                        echo_core::intent::set_top_items_range(&mut this.state, range)
                    {
                        this.dispatch(event);
                    }
                    cx.notify();
                }))
                .child(tr(&app.state, key)),
        );
    }
    row
}

/// The docked column to the right of the main area: the queue or the lyrics, whichever the
/// playback bar's buttons (or `shift-L`) opened. Persistent UI rather than an overlay, so
/// escape leaves it alone and it comes back on the next launch.
pub fn right_panel(
    app: &mut EchoApp,
    panel: RightPanel,
    window: &Window,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let width = app.effective_right_panel_width(window);
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let title = match panel {
        RightPanel::Queue => tr(&app.state, "ui.queue"),
        RightPanel::Lyrics => tr(&app.state, "desktop.lyrics_panel"),
    };
    let subtitle: Option<SharedString> = match panel {
        RightPanel::Queue => None,
        RightPanel::Lyrics => (!app.state.playback.playing_track_title.is_empty())
            .then(|| app.state.playback.playing_track_title.clone().into()),
    };
    let body = match panel {
        RightPanel::Queue => queue_panel_body(app, width, cx),
        RightPanel::Lyrics => lyric_status(app, muted).unwrap_or_else(|| lyric_list(app, cx)),
    };

    div()
        .id("right-panel")
        .relative()
        .flex_none()
        .w(px(width))
        .h_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_l_1()
        .border_color(palette.border)
        .child(
            div()
                .flex_none()
                .pl_4()
                .pr_2()
                .py_2()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_grow(1.0)
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(fg)
                                .truncate()
                                .child(title),
                        )
                        .when_some(subtitle, |el, subtitle| {
                            el.child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .child(subtitle),
                            )
                        }),
                )
                .child(crate::icon_button(
                    "right-panel-close",
                    "icons/win-close.svg",
                    muted,
                    palette.wash,
                    cx,
                    |this, cx| this.set_right_panel(None, cx),
                )),
        )
        .child(body)
        .child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(5.0))
                .cursor_col_resize()
                .hover(move |style| style.bg(palette.drag_target))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this: &mut EchoApp, event: &MouseDownEvent, window, cx| {
                        this.begin_right_panel_resize(event.position.x, window, cx);
                        cx.stop_propagation();
                    }),
                ),
        )
        .into_any_element()
}

/// The theme picker: every loaded theme by name, the active one accented.
/// Theme names in the order the picker lists them. `EchoApp::theme_modal_index` indexes into
/// this, so keyboard selection and rendering must both go through here or the two drift.
/// The picker's rows for one appearance: its themes, each with its sibling in the other.
pub fn theme_rows(
    state: &echo_core::app::AppState,
    scheme: echo_core::config::Scheme,
) -> Vec<echo_core::theme::ThemeRow> {
    echo_core::theme::themes_for(&state.ui.themes, scheme)
}

/// The theme picker.
///
/// A theme is chosen per appearance. The header sets the mode (auto / dark / light); the
/// list holds the themes of one appearance, so a pinned mode shows just its own and `Auto`
/// adds a tab for each. A dark/light pair is one row naming both, the listed one first in
/// weight; a theme with no sibling is only in its own appearance's list.
pub fn theme_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    use echo_core::config::{Scheme, ThemeMode};

    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let config = &app.state.ui.library_config;
    let mode = config.theme_mode.unwrap_or(ThemeMode::Auto);
    let tab = app.theme_modal_tab;
    let rows = theme_rows(&app.state, tab);
    let chosen = echo_core::theme::selected_name(config, &app.state.ui.themes, tab);
    let selected = app.theme_modal_index;

    // Background, accent and second accent of each listed theme. Choosing does not preview,
    // and under `Auto` one of the two lists is never on screen, so this is how they compare.
    let swatches: Vec<[gpui::Hsla; 3]> = rows
        .iter()
        .map(|row| {
            let resolved = app
                .state
                .ui
                .themes
                .get(&row.name)
                .map(echo_core::theme::ResolvedTheme::from_theme);
            match resolved {
                Some(t) => [
                    t.background.gpui(crate::theme::WINDOW_BG()),
                    t.primary.gpui(WINDOW_FG()),
                    t.secondary.gpui(WINDOW_FG()),
                ],
                None => [surface; 3],
            }
        })
        .collect();

    let mode_label =
        |mode: ThemeMode| tr(&app.state, &format!("desktop.theme_mode.{}", mode.name()));
    let scheme_label =
        |scheme: Scheme| tr(&app.state, &format!("desktop.theme_mode.{}", scheme.name()));

    // One joined bar, the same control as the Mode row in Settings.
    let mode_control = div()
        .flex_none()
        .h(px(24.0))
        .flex()
        .flex_row()
        .rounded_md()
        .border_1()
        .border_color(palette.menu_border)
        .overflow_hidden()
        .children(ThemeMode::ALL.into_iter().enumerate().map(|(ix, option)| {
            let active = option == mode;
            div()
                .id(SharedString::from(format!("theme-mode-{}", option.name())))
                .h_full()
                .px_3()
                .flex()
                .items_center()
                .text_xs()
                .text_color(if active { fg } else { muted })
                .when(ix > 0, |el| {
                    el.border_l_1().border_color(palette.menu_border)
                })
                .when(active, |el| el.bg(palette.menu_selected))
                .when(!active, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(palette.menu_hover))
                })
                .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                    this.set_theme_mode(option, cx);
                }))
                .child(mode_label(option))
        }));

    // Under `Auto` only: one tab per appearance, the listed one underlined.
    let tabs = (mode == ThemeMode::Auto).then(|| {
        div()
            .flex_none()
            .flex()
            .flex_row()
            .gap_4()
            .px_2()
            .pt_1()
            .children([Scheme::Dark, Scheme::Light].into_iter().map(|scheme| {
                let active = scheme == tab;
                div()
                    .id(SharedString::from(format!("theme-tab-{}", scheme.name())))
                    .pb_1()
                    .border_b_2()
                    .border_color(if active {
                        accent
                    } else {
                        gpui::transparent_black()
                    })
                    .text_sm()
                    .text_color(if active { fg } else { muted })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                        this.show_theme_list(scheme, cx);
                    }))
                    .child(scheme_label(scheme))
            }))
    });

    div()
        .id("theme-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.theme_modal_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("theme-panel")
                .w(px(340.0))
                .max_h(px(480.0))
                .rounded_lg()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .flex_none()
                        .px_1()
                        .pb_3()
                        .border_b_1()
                        .border_color(palette.menu_border)
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_base()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(tr(&app.state, "desktop.theme")),
                        )
                        .child(mode_control),
                )
                .when_some(tabs, |el, tabs| el.child(tabs))
                .child(if rows.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.no_themes"))
                        .into_any_element()
                } else {
                    div()
                        .id("theme-list")
                        .flex()
                        .flex_col()
                        // Sized by its rows, shrinking (and scrolling) once the panel is full.
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&app.theme_modal_scroll)
                        .children(rows.into_iter().zip(swatches).enumerate().map(
                            |(ix, (row, swatch))| {
                                let is_chosen = row.name == chosen;
                                let is_selected = ix == selected;
                                let clicked = row.name.clone();
                                div()
                                    .id(ix)
                                    .px_2()
                                    .h(px(30.0))
                                    .flex_none()
                                    .rounded_md()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_3()
                                    .text_sm()
                                    .when(is_selected, |el| el.bg(palette.menu_selected))
                                    .when(!is_selected, |el| {
                                        el.hover(move |style| style.bg(palette.menu_hover))
                                    })
                                    .cursor_pointer()
                                    .on_click(cx.listener(
                                        move |this: &mut EchoApp, _event, _window, cx| {
                                            echo_core::intent::apply_theme(
                                                &mut this.state,
                                                &clicked,
                                            );
                                            this.theme_modal_open = false;
                                            cx.notify();
                                        },
                                    ))
                                    .child(
                                        // The theme's background, accent and second accent
                                        // as one joined chip.
                                        div()
                                            .flex_none()
                                            .flex()
                                            .flex_row()
                                            .rounded_sm()
                                            .border_1()
                                            .border_color(palette.menu_border)
                                            .overflow_hidden()
                                            .children(swatch.into_iter().map(|color| {
                                                div().w(px(12.0)).h(px(14.0)).bg(color)
                                            })),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .flex_row()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_color(if is_chosen { accent } else { fg })
                                                    .child(SharedString::from(row.name)),
                                            )
                                            .when_some(row.sibling, |el, sibling| {
                                                el.child(div().text_color(muted).child(
                                                    SharedString::from(format!("/ {sibling}")),
                                                ))
                                            }),
                                    )
                                    .when(is_chosen, |el| {
                                        el.child(div().flex_none().text_color(accent).child("✓"))
                                    })
                            },
                        ))
                        .into_any_element()
                }),
        )
}

/// Keyboard shortcuts, grouped, for the help overlay, as `(section i18n key, [(keys,
/// description i18n key)])` — resolved through [`tr`] at render so `:lang` applies.
///
/// Kept next to the `KeyBinding::new` block in `main()` conceptually — when you add a binding
/// there, add it here too. Commands are not listed: those come from
/// `echo_core::commands::COMMANDS` so they cannot go stale.
pub const KEY_HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "desktop.help.nav",
        &[
            ("j / k / ↓ / ↑", "desktop.help.move"),
            (
                if cfg!(target_os = "macos") {
                    "ctrl-shift-h"
                } else {
                    "ctrl-h"
                },
                "desktop.home",
            ),
            ("gg / G / home / end", "desktop.help.first_last"),
            ("ctrl-b / ctrl-f / pgup / pgdn", "desktop.help.page"),
            ("ctrl-u / ctrl-d", "desktop.help.half_page"),
            ("enter / z", "desktop.help.open"),
            ("h / esc", "desktop.help.back"),
            ("alt-← / alt-→", "desktop.help.history"),
            ("← / →", "desktop.help.pane_focus"),
            ("backspace", "desktop.help.to_sidebar"),
            ("ctrl-\\", "desktop.help.toggle_sidebar"),
            ("tab", "desktop.help.tabs"),
            ("gc", "desktop.help.jump_playing"),
            ("1-9", "desktop.help.count"),
            ("y", "desktop.help.confirm"),
        ],
    ),
    (
        "desktop.help.playback",
        &[
            ("space", "desktop.help.play_pause"),
            ("[ / ] / ctrl-← / ctrl-→", "desktop.help.prev_next"),
            (", / . / shift-← / shift-→", "desktop.help.seek"),
            ("0", "desktop.help.seek_start"),
            ("- / =", "desktop.help.volume"),
            ("shift-M", "desktop.help.mute"),
            ("s / r", "desktop.help.shuffle_repeat"),
            ("shift-D", "desktop.help.devices"),
            ("shift-L", "desktop.help.lyrics"),
            ("ctrl-shift-L", "desktop.help.lyrics_bar"),
            ("shift-F", "desktop.help.immersive"),
        ],
    ),
    (
        "desktop.help.library",
        &[
            ("l", "desktop.help.like"),
            ("a", "desktop.help.add_playlist"),
            ("shift-A", "desktop.help.track_actions"),
            ("q / shift-Q", "desktop.help.queue"),
            ("shift-J / shift-K", "desktop.help.move_track"),
            ("dd", "desktop.help.delete"),
            ("v", "desktop.help.select_range"),
            ("m", "desktop.help.pin"),
            ("c / e", "desktop.help.new_rename"),
            ("shift-R", "desktop.help.refresh"),
        ],
    ),
    (
        "desktop.help.finding",
        &[
            ("ctrl-k", "desktop.help.search"),
            ("/", "desktop.help.filter"),
            ("n / shift-N", "desktop.help.next_match"),
            (":", "desktop.help.command_bar"),
            ("f", "desktop.help.search_command"),
            ("t", "desktop.help.themes"),
            ("ctrl-,", "desktop.help.settings"),
            ("?", "desktop.help.this_help"),
            ("ctrl-q", "desktop.help.quit"),
        ],
    ),
];

/// The help overlay: every keybinding, plus every `:` command straight from the registry.
pub fn help_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let entry = |keys: &'static str, what: SharedString| {
        div()
            .flex()
            .flex_row()
            .items_baseline()
            .gap_2()
            .py_0p5()
            .child(
                div()
                    .flex_none()
                    .w(px(112.0))
                    .text_xs()
                    .text_color(accent)
                    .child(keys),
            )
            .child(div().text_xs().text_color(muted).child(what))
    };

    let state = &app.state;
    let section = move |title_key: &'static str, rows: &'static [(&'static str, &'static str)]| {
        div()
            .flex()
            .flex_col()
            .pb_2()
            .child(
                div()
                    .pb_1()
                    .text_xs()
                    .text_color(fg)
                    .child(tr(state, title_key)),
            )
            .children(
                rows.iter()
                    .map(|(keys, what_key)| entry(keys, tr(state, what_key))),
            )
    };

    div()
        .id("help-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.help_open = false;
            cx.notify();
        }))
        .child(
            div()
                .id("help-panel")
                .w(px(700.0))
                .max_h(px(600.0))
                .rounded_lg()
                .border_1()
                .border_color(DesktopPalette::resolve(&app.state.ui.active_theme).menu_border)
                .bg(surface)
                .p_4()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .pb_2()
                        .child(
                            div()
                                .text_sm()
                                .text_color(fg)
                                .child(tr(&app.state, "desktop.help.title")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(tr(&app.state, "desktop.help.esc_close")),
                        ),
                )
                .child(
                    div()
                        .id("help-body")
                        .flex()
                        .flex_row()
                        .gap_6()
                        .max_h(px(510.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.help_scroll)
                        .child(
                            div().flex().flex_col().w(px(320.0)).flex_none().children(
                                KEY_HELP.iter().map(|(title, rows)| section(title, rows)),
                            ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_grow(1.0)
                                .child(
                                    div()
                                        .pb_1()
                                        .text_xs()
                                        .text_color(fg)
                                        .child(tr(&app.state, "desktop.help.commands")),
                                )
                                .children(echo_core::commands::COMMANDS.iter().map(
                                    |(usage, description)| {
                                        div()
                                            .flex()
                                            .flex_col()
                                            .py_0p5()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(accent)
                                                    .child(SharedString::from(format!(":{usage}"))),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(*description),
                                            )
                                    },
                                )),
                        ),
                ),
        )
}

/// The settings sheet.
///
/// Booleans are switches, a choice among several is a dropdown (the appearance mode, three
/// short options switched often, is a segmented bar), and rows carry a hint only where the
/// label cannot say it. What each control does is a `SettingChange`.
pub fn settings_modal(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let config = app.state.ui.library_config.clone();
    let path_focused = app.settings_path_focus.is_focused(window);
    let path_value = app.settings_path_input.clone();
    // Owns its language so translation never borrows `app` across the listener closures below.
    let s = {
        let lang = config.language.clone();
        move |key: &str| SharedString::from(echo_core::i18n::t(key, &lang))
    };

    const ROW_HEIGHT: f32 = 34.0;
    const CONTROL_HEIGHT: f32 = 24.0;
    let open_dropdown = app.settings_dropdown;

    // A labelled row with its control on the right. Labels and hints arrive translated. A
    // disabled row is dimmed: its setting has no effect until another one is turned on.
    let row =
        |label: SharedString, hint: Option<SharedString>, enabled: bool, control: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_4()
                .min_h(px(ROW_HEIGHT))
                .py_0p5()
                .when(!enabled, |el| el.opacity(0.4))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(div().text_sm().text_color(fg).child(label))
                        .when_some(hint, |el, hint| {
                            el.child(div().text_xs().text_color(muted).child(hint))
                        }),
                )
                .child(control)
        };

    // A section title over a rule, with an optional note that applies to the whole section.
    let heading = |label: SharedString, note: Option<SharedString>| {
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_4()
            .mt_4()
            .mb_1()
            .pb_1()
            .border_b_1()
            .border_color(palette.menu_border)
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(muted)
                    .child(label),
            )
            .when_some(note, |el, note| {
                el.child(div().text_xs().text_color(muted).child(note))
            })
    };

    // An on/off switch.
    let toggle = |id: &'static str, on: bool, change: SettingChange, cx: &mut Context<EchoApp>| {
        div()
            .id(id)
            .flex_none()
            .w(px(34.0))
            .h(px(20.0))
            .p(px(2.0))
            .rounded_full()
            .flex()
            .flex_row()
            .items_center()
            .when(on, |el| el.justify_end())
            .bg(if on { accent } else { palette.menu_border })
            .cursor_pointer()
            .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                change.apply(this, cx);
            }))
            .child(
                div()
                    .size(px(16.0))
                    .rounded_full()
                    .bg(if on { surface } else { muted }),
            )
            .into_any_element()
    };

    // A segmented control: one joined bar, the active segment filled.
    let segmented = |id: &'static str,
                     options: Vec<(SharedString, String, bool)>,
                     cx: &mut Context<EchoApp>| {
        div()
            .id(id)
            .flex_none()
            .h(px(CONTROL_HEIGHT))
            .flex()
            .flex_row()
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .overflow_hidden()
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(ix, (label, cmd, active))| {
                        div()
                            .id(SharedString::from(format!("{id}-{ix}")))
                            .h_full()
                            .px_3()
                            .flex()
                            .items_center()
                            .text_xs()
                            .text_color(if active { fg } else { muted })
                            .when(ix > 0, |el| {
                                el.border_l_1().border_color(palette.menu_border)
                            })
                            .when(active, |el| el.bg(palette.menu_selected))
                            .when(!active, |el| {
                                el.cursor_pointer()
                                    .hover(move |style| style.bg(palette.menu_hover))
                            })
                            .on_click(cx.listener(
                                move |this: &mut EchoApp, _event, _window, cx| {
                                    this.run_setting(cmd.clone(), cx);
                                },
                            ))
                            .child(label)
                    }),
            )
            .into_any_element()
    };

    // A dropdown: a button showing the current option, and while open a menu of all of them
    // hanging under it. The menu is drawn deferred so the scrolling list does not clip it.
    let dropdown = |id: &'static str,
                    options: Vec<(SharedString, SettingChange, bool)>,
                    enabled: bool,
                    cx: &mut Context<EchoApp>| {
        let current = options
            .iter()
            .find(|option| option.2)
            .map(|option| option.0.clone())
            .unwrap_or_default();
        let is_open = enabled && open_dropdown == Some(id);
        let menu = is_open.then(|| {
            let items: Vec<_> = options
                .into_iter()
                .enumerate()
                .map(|(ix, (label, change, active))| {
                    div()
                        .id(SharedString::from(format!("{id}-option-{ix}")))
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .text_xs()
                        .text_color(if active { accent } else { fg })
                        .when(active, |el| el.bg(palette.menu_selected))
                        .cursor_pointer()
                        .hover(move |style| style.bg(palette.menu_hover))
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            this.settings_dropdown = None;
                            change.apply(this, cx);
                            cx.stop_propagation();
                        }))
                        .child(label)
                })
                .collect();
            div().absolute().top_full().right_0().child(
                gpui::deferred(
                    gpui::anchored()
                        .anchor(gpui::Anchor::TopRight)
                        .snap_to_window()
                        .child(
                            div()
                                .id(SharedString::from(format!("{id}-menu")))
                                .occlude()
                                .mt_1()
                                .min_w(px(DROPDOWN_WIDTH))
                                .p_1()
                                .flex()
                                .flex_col()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.menu_border)
                                .bg(surface)
                                .shadow_md()
                                .on_click(
                                    cx.listener(|_this, _event, _window, cx| cx.stop_propagation()),
                                )
                                .children(items),
                        ),
                )
                .with_priority(1),
            )
        });
        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id(id)
                    .h(px(CONTROL_HEIGHT))
                    .min_w(px(DROPDOWN_WIDTH))
                    .pl_2()
                    .pr_1p5()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .border_color(if is_open { accent } else { palette.menu_border })
                    .text_xs()
                    .text_color(fg)
                    .when(enabled, |el| {
                        el.cursor_pointer()
                            .hover(move |style| style.bg(palette.menu_hover))
                            .on_click(cx.listener(
                                move |this: &mut EchoApp, _event, _window, cx| {
                                    this.settings_dropdown =
                                        (this.settings_dropdown != Some(id)).then_some(id);
                                    // The panel behind closes any open dropdown on a click.
                                    cx.stop_propagation();
                                    cx.notify();
                                },
                            ))
                    })
                    .child(current)
                    .child(
                        svg()
                            .path("icons/arrow-down.svg")
                            .size(px(8.0))
                            .text_color(muted),
                    ),
            )
            .when_some(menu, |el, menu| el.child(menu))
            .into_any_element()
    };

    // A plain push button; the caller adds the click. `strong` accents the one that matters.
    let button = |id: &'static str, label: SharedString, strong: bool| {
        div()
            .id(id)
            .flex_none()
            .h(px(CONTROL_HEIGHT))
            .px_2()
            .flex()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(if strong { accent } else { palette.menu_border })
            .text_xs()
            .text_color(if strong { accent } else { fg })
            .child(label)
    };

    let command = |cmd: String| SettingChange::Command(cmd);
    let on_off = |on: bool| if on { "on" } else { "off" };

    // --- General ---------------------------------------------------------------------
    let language_row = row(
        s("desktop.settings.language"),
        None,
        true,
        dropdown(
            "lang",
            [("English", "en"), ("简体中文", "zh-CN")]
                .into_iter()
                .map(|(label, code)| {
                    (
                        label.into(),
                        command(format!("lang {code}")),
                        config.language == code,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let tray_row = row(
        s("desktop.settings.tray"),
        None,
        true,
        toggle(
            "tray",
            config.close_to_tray,
            command(format!("tray {}", on_off(!config.close_to_tray))),
            cx,
        ),
    );

    let sort_row = row(
        s("desktop.settings.order"),
        None,
        true,
        dropdown(
            "libsort",
            {
                use echo_core::config::SortMode;
                [
                    // The manual drag-and-drop order.
                    ("order_custom", "default", SortMode::Default),
                    ("order_alpha", "alpha", SortMode::Alphabetical),
                    ("order_creator", "creator", SortMode::Creator),
                ]
                .into_iter()
                .map(|(key, name, mode)| {
                    (
                        s(&format!("desktop.settings.{key}")),
                        command(format!("sort {name}")),
                        config.sort_mode == mode,
                    )
                })
                .collect()
            },
            true,
            cx,
        ),
    );

    let relative_row = row(
        s("desktop.settings.relative"),
        None,
        true,
        toggle(
            "relative",
            config.relative_line_numbers,
            command(format!(
                "relative {}",
                on_off(!config.relative_line_numbers)
            )),
            cx,
        ),
    );

    // --- Appearance ------------------------------------------------------------------
    let theme_mode = config
        .theme_mode
        .unwrap_or(echo_core::config::ThemeMode::Auto);
    // The theme in use, or under `Auto` the dark and the light one.
    let theme_label = {
        use echo_core::config::{Scheme, ThemeMode};
        let chosen =
            |scheme| echo_core::theme::selected_name(&config, &app.state.ui.themes, scheme);
        match theme_mode {
            ThemeMode::Auto => format!("{} / {}", chosen(Scheme::Dark), chosen(Scheme::Light)),
            ThemeMode::Dark => chosen(Scheme::Dark),
            ThemeMode::Light => chosen(Scheme::Light),
        }
    };
    let theme_row = row(
        s("desktop.theme"),
        None,
        true,
        button("settings-theme", theme_label.into(), false)
            .cursor_pointer()
            .hover(move |style| style.bg(palette.menu_hover))
            .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                this.settings_open = false;
                this.toggle_themes(cx);
            }))
            .into_any_element(),
    );

    let mode_row = row(
        s("desktop.settings.mode"),
        None,
        true,
        segmented(
            "mode",
            echo_core::config::ThemeMode::ALL
                .into_iter()
                .map(|mode| {
                    (
                        s(&format!("desktop.theme_mode.{}", mode.name())),
                        format!("mode {}", mode.name()),
                        theme_mode == mode,
                    )
                })
                .collect(),
            cx,
        ),
    );

    // The offered sizes, plus whatever `:pixelate` / `:visbins` set outside them, so the
    // button never shows blank.
    let with_current = |offered: &[usize], current: usize| {
        let mut values = offered.to_vec();
        if !values.contains(&current) {
            values.push(current);
            values.sort_unstable();
        }
        values
    };

    let pixelate_row = row(
        s("desktop.settings.pixelate"),
        None,
        true,
        dropdown(
            "pixelate",
            with_current(&[0, 8, 16, 32], config.cover_img_pixels as usize)
                .into_iter()
                .map(|n| {
                    (
                        if n == 0 {
                            s("ui.off")
                        } else {
                            SharedString::from(format!("{n} px"))
                        },
                        command(format!("pixelate {n}")),
                        config.cover_img_pixels as usize == n,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let backdrop_row = row(
        s("desktop.settings.backdrop"),
        None,
        true,
        dropdown(
            "backdrop",
            echo_core::config::BackdropMode::ALL
                .into_iter()
                .map(|mode| {
                    (
                        s(&format!("desktop.settings.backdrops.{}", mode.name())),
                        command(format!("backdrop {}", mode.name())),
                        config.immersive_backdrop == mode,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let visualizer_row = row(
        s("desktop.settings.visualizer"),
        None,
        true,
        // `:vis` flips the visualizer; it takes no argument.
        toggle("vis", config.enable_visualizer, command("vis".into()), cx),
    );

    let bins_row = row(
        s("desktop.settings.bins"),
        None,
        config.enable_visualizer,
        dropdown(
            "visbins",
            with_current(&[7, 9, 16, 24, 32], config.vis_bins)
                .into_iter()
                .map(|n| {
                    (
                        SharedString::from(n.to_string()),
                        command(format!("visbins {n}")),
                        config.vis_bins == n,
                    )
                })
                .collect(),
            config.enable_visualizer,
            cx,
        ),
    );

    // --- Audio quality ---------------------------------------------------------------
    let bitrate_row = row(
        s("desktop.settings.quality"),
        None,
        true,
        dropdown(
            "bitrate",
            [96u32, 160, 320]
                .into_iter()
                .map(|rate| {
                    (
                        SharedString::from(format!("{rate} kbps")),
                        SettingChange::Bitrate(rate),
                        config.bitrate == rate,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let normalisation = config.normalisation;
    let normalisation_row = row(
        s("desktop.settings.normalisation"),
        None,
        true,
        toggle(
            "normalisation",
            normalisation,
            SettingChange::Normalisation(!normalisation),
            cx,
        ),
    );

    let pregain = config.normalisation_pregain;
    let step_button =
        |id: &'static str, label: &'static str, delta: f64, cx: &mut Context<EchoApp>| {
            div()
                .id(id)
                .size(px(CONTROL_HEIGHT))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .text_xs()
                .text_color(fg)
                .when(normalisation, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(palette.menu_hover))
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                            this.set_audio_quality(
                                |config| {
                                    // Matches librespot's usable range; beyond this the
                                    // limiter dominates.
                                    config.normalisation_pregain =
                                        (config.normalisation_pregain + delta).clamp(-10.0, 10.0);
                                },
                                cx,
                            );
                        }))
                })
                .child(label)
        };
    let pregain_row = row(
        s("desktop.settings.pregain"),
        Some(s("desktop.settings.pregain_desc")),
        normalisation,
        div()
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(step_button("pregain-down", "−", -0.5, cx))
            .child(
                div()
                    .w(px(56.0))
                    .text_xs()
                    .text_color(fg)
                    .text_center()
                    .child(SharedString::from(format!("{pregain:+.1} dB"))),
            )
            .child(step_button("pregain-up", "+", 0.5, cx))
            .into_any_element(),
    );

    // --- Local music -----------------------------------------------------------------
    let path_unsaved = app.local_folder_unsaved();
    let local_field = div()
        .id("settings-localpath")
        .key_context(crate::SEARCH_CONTEXT)
        .track_focus(&app.settings_path_focus)
        .on_key_down(cx.listener(|this: &mut EchoApp, event, window, cx| {
            this.handle_settings_path_key(event, window, cx)
        }))
        .flex_1()
        .min_w_0()
        .h(px(CONTROL_HEIGHT))
        .px_2()
        .flex()
        .items_center()
        .rounded_md()
        .border_1()
        .border_color(if path_focused {
            accent
        } else {
            palette.menu_border
        })
        .text_xs()
        .text_color(if path_value.is_empty() { muted } else { fg })
        .whitespace_nowrap()
        .overflow_hidden()
        .cursor_pointer()
        .on_click(cx.listener(|this: &mut EchoApp, _event, window, cx| {
            let handle = this.settings_path_focus.clone();
            window.focus(&handle, cx);
            cx.notify();
        }))
        .child(SharedString::from(if path_focused {
            crate::text_with_cursor(&path_value, app.settings_path_cursor)
        } else if path_value.is_empty() {
            s("desktop.settings.folder_empty").to_string()
        } else {
            path_value
        }));
    let local_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .min_h(px(ROW_HEIGHT))
        .child(local_field)
        .child(
            // With no folder saved, or a different one typed, the button applies the field;
            // otherwise there is nothing to set and it rescans.
            button(
                "settings-rescan",
                s(if path_unsaved {
                    "desktop.settings.set_folder"
                } else {
                    "desktop.settings.rescan"
                }),
                false,
            )
            .cursor_pointer()
            .hover(move |style| style.bg(palette.menu_hover))
            .on_click(cx.listener(|this: &mut EchoApp, _event, window, cx| {
                this.apply_or_rescan_local_folder(window, cx);
            })),
        );

    // --- Updates ---------------------------------------------------------------------
    // The one control here that is neither a `:` command nor a config write: it runs a network
    // call, so its label and action both come from `app.update_state`.
    let version_row = row(
        s("desktop.settings.updates.version"),
        None,
        true,
        div()
            .text_xs()
            .text_color(muted)
            .child(SharedString::from(if echo_core::update::is_dev_build() {
                format!(
                    "{} {}",
                    echo_core::update::current_version(),
                    echo_core::i18n::t("desktop.settings.updates.dev", &config.language)
                )
            } else {
                echo_core::update::current_version().to_string()
            }))
            .into_any_element(),
    );

    let translate = |key: &str, value: &str| -> SharedString {
        SharedString::from(echo_core::i18n::t(key, &config.language).replace("{}", value))
    };
    let auto_update_row = row(
        s("desktop.settings.updates.auto"),
        None,
        true,
        toggle(
            "autoupdate",
            config.auto_update,
            command(format!("autoupdate {}", on_off(!config.auto_update))),
            cx,
        ),
    );
    // The hint only appears when there is something to report: an installed update waiting
    // for a restart, or why the last attempt failed.
    let (update_label, update_hint, update_active) = match &app.update_state {
        UpdateState::Idle => (s("desktop.settings.updates.check"), None, true),
        UpdateState::Checking => (s("desktop.settings.updates.checking"), None, false),
        UpdateState::UpToDate => (s("desktop.settings.updates.uptodate"), None, true),
        UpdateState::Available(release) => (
            translate("desktop.settings.updates.install", release.version()),
            None,
            true,
        ),
        UpdateState::Downloading(percent) => (
            translate("desktop.settings.updates.downloading", &percent.to_string()),
            None,
            false,
        ),
        UpdateState::Ready(version) => (
            s("desktop.settings.updates.restart"),
            Some(translate("desktop.settings.updates.installed", version)),
            false,
        ),
        UpdateState::Failed(message) => (
            s("desktop.settings.updates.retry"),
            Some(SharedString::from(message.clone())),
            true,
        ),
        // Nothing in-app can fix this one, so the button leaves for the releases page.
        UpdateState::Blocked(message) => (
            s("desktop.settings.updates.open_releases"),
            Some(SharedString::from(message.clone())),
            true,
        ),
    };

    let update_button = button("settings-update", update_label, update_active)
        .when(!update_active, |el| el.text_color(muted))
        .when(update_active, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(palette.menu_hover))
                .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                    match &this.update_state {
                        UpdateState::Available(_) => this.install_update(cx),
                        UpdateState::Blocked(_) => {
                            let _ = webbrowser::open(&echo_core::update::releases_url());
                        }
                        _ => this.check_for_updates(cx),
                    }
                }))
        })
        .into_any_element();

    let update_row = row(
        s("desktop.settings.updates.label"),
        update_hint,
        true,
        update_button,
    );

    div()
        .id("settings-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            // A click outside first closes an open dropdown, then the sheet.
            if this.settings_dropdown.take().is_none() {
                this.settings_open = false;
            }
            cx.notify();
        }))
        .child(
            div()
                .id("settings-panel")
                .w(px(480.0))
                // Never taller than the window: the list below takes what is left and scrolls.
                .max_h(px(580.0).min(window.viewport_size().height - px(32.0)))
                .rounded_lg()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .p_4()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                    if this.settings_dropdown.take().is_some() {
                        cx.notify();
                    }
                    cx.stop_propagation();
                }))
                .child(
                    // The rule is where scrolled rows disappear, so they never run up
                    // under the title.
                    div()
                        .flex_none()
                        .pb_3()
                        .border_b_1()
                        .border_color(palette.menu_border)
                        .text_base()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(fg)
                        .child(s("desktop.settings.title")),
                )
                .child(
                    div()
                        .id("settings-list")
                        .flex()
                        .flex_col()
                        // Sized by its rows, shrinking (and scrolling) once the panel is full.
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&app.settings_scroll)
                        // One child holding every row: the scroll extent is measured across the list's children and
                        // skips the first one's top margin, which left the last row short of fully scrolling in.
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .child(heading(s("desktop.settings.general"), None))
                                .child(language_row)
                                .when(!cfg!(target_os = "macos"), |el| el.child(tray_row))
                                .child(sort_row)
                                .child(relative_row)
                                .child(heading(s("desktop.settings.appearance"), None))
                                .child(theme_row)
                                .child(mode_row)
                                .child(pixelate_row)
                                .child(backdrop_row)
                                .child(visualizer_row)
                                .child(bins_row)
                                .child(heading(
                                    s("desktop.settings.audio"),
                                    Some(s("desktop.settings.next_launch")),
                                ))
                                .child(bitrate_row)
                                .child(normalisation_row)
                                .child(pregain_row)
                                .child(heading(s("desktop.settings.local"), None))
                                .child(local_row)
                                .child(heading(s("desktop.settings.updates.section"), None))
                                .child(version_row)
                                .child(auto_update_row)
                                .child(update_row),
                        ),
                ),
        )
}

/// What a settings control does when used: nearly always a `:` command, so the sheet and the
/// command bar cannot drift apart. The audio-quality keys have no command and write the
/// config directly.
#[derive(Clone)]
enum SettingChange {
    Command(String),
    Bitrate(u32),
    Normalisation(bool),
}

impl SettingChange {
    fn apply(&self, app: &mut EchoApp, cx: &mut Context<EchoApp>) {
        match *self {
            Self::Command(ref cmd) => app.run_setting(cmd.clone(), cx),
            Self::Bitrate(rate) => app.set_audio_quality(|config| config.bitrate = rate, cx),
            Self::Normalisation(on) => {
                app.set_audio_quality(|config| config.normalisation = on, cx)
            }
        }
    }
}

/// Every settings dropdown is this wide at least, so their right and left edges line up.
const DROPDOWN_WIDTH: f32 = 124.0;

/// The Spotify Connect device picker, painted over everything else.
pub fn device_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let selected_bg = palette.menu_selected;
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
        .occlude()
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
                .border_color(palette.menu_border)
                .bg(surface)
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
                        .child(tr(&app.state, "desktop.devices_title")),
                )
                .child(if devices.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.devices_none"))
                        .into_any_element()
                } else {
                    div()
                        .id("device-list")
                        .flex()
                        .flex_col()
                        .max_h(px(340.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.device_modal_scroll)
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
                                .when(ix != selected, |el| {
                                    el.hover(move |style| style.bg(palette.menu_hover))
                                })
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

/// Add-to-playlist picker over the core choices (own Spotify playlists plus local ones); Enter
/// and row clicks resolve through `action_menu::commit_playlist_add`.
pub fn playlist_add_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let selected_bg = palette.menu_selected;
    let selected = app.state.ui.selected_playlist_modal_index;

    // Materialized before the element closures capture anything from `app`.
    let choices: Vec<(SharedString, bool)> =
        echo_core::action_menu::filtered_playlist_add_choices(&app.state)
            .into_iter()
            .map(|(_, playlist)| {
                let local = playlist.owner_id == "local";
                (SharedString::from(playlist.name), local)
            })
            .collect();

    let choices: Vec<_> = std::iter::once((tr(&app.state, "desktop.new_playlist"), false))
        .chain(choices)
        .collect();
    div()
        .id("playlist-add-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        // Modal: the backdrop swallows pointer events, so a scroll wheel over it can't reach the
        // track list behind the overlay.
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            echo_core::action_menu::cancel_playlist_add(&mut this.state);
            cx.notify();
        }))
        .child(
            div()
                .id("playlist-add-panel")
                .w(px(400.0))
                .max_h(px(420.0))
                .rounded_lg()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
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
                        .child(tr(&app.state, "desktop.playlist_add_title")),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .bg(palette.wash)
                        .text_sm()
                        .text_color(muted)
                        .child(
                            svg()
                                .path("icons/search.svg")
                                .size(px(14.0))
                                .text_color(muted),
                        )
                        .child(div().truncate().child(
                            if app.state.ui.playlist_add_filter.is_empty() {
                                tr(&app.state, "desktop.filter_playlists")
                            } else {
                                app.state.ui.playlist_add_filter.clone().into()
                            },
                        )),
                )
                .child(if choices.is_empty() {
                    div()
                        .py_4()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.playlist_add_none"))
                        .into_any_element()
                } else {
                    div()
                        .id("playlist-add-list")
                        .flex()
                        .flex_col()
                        .max_h(px(340.0))
                        .overflow_y_scroll()
                        .track_scroll(&app.playlist_modal_scroll)
                        .children(choices.into_iter().enumerate().map(|(ix, (name, local))| {
                            let local_label = tr(&app.state, "ui.local");
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
                                .when(ix != selected, |el| {
                                    el.hover(move |style| style.bg(palette.menu_hover))
                                })
                                .cursor_pointer()
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if let Some(event) =
                                            echo_core::action_menu::commit_filtered_playlist_add(
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
                                        .flex_grow(1.0)
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(fg)
                                        .child(name),
                                )
                                .when(local, |el| {
                                    el.child(
                                        div()
                                            .flex_none()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(local_label),
                                    )
                                })
                        }))
                        .into_any_element()
                }),
        )
}

/// The main-area empty state: the ECHO wordmark with a vertical secondary→primary gradient, or
/// plain status text while authenticating.
fn library_placeholder(app: &EchoApp) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let muted = theme.text_muted.gpui(WINDOW_FG());

    if matches!(app.state.ui.mode, AppMode::Setup | AppMode::Authenticating) {
        let message: SharedString = match app.state.ui.mode {
            AppMode::Setup => tr(&app.state, "desktop.setup.waiting"),
            _ => tr(&app.state, "desktop.setup.authenticating"),
        };
        return div()
            .flex_grow(1.0)
            .flex()
            .items_center()
            .justify_center()
            .text_color(muted)
            .child(message)
            .into_any_element();
    }

    let secondary = theme.secondary.gpui(WINDOW_FG());
    let primary = theme.primary.gpui(WINDOW_FG());
    let playlists = app.state.data.playlists.len();
    let albums = app.state.data.saved_albums.len();
    let counts: SharedString = format!("{playlists} playlists · {albums} albums").into();

    div()
        .flex_grow(1.0)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .font_family(app.mono_font.clone())
                .text_sm()
                .line_height(relative(1.0))
                .children(ECHO_LOGO.iter().enumerate().map(|(index, line)| {
                    let t = index as f32 / (ECHO_LOGO.len() - 1) as f32;
                    div()
                        .whitespace_nowrap()
                        .text_color(lerp_hsla(secondary, primary, t))
                        .child(*line)
                })),
        )
        .child(div().text_xs().text_color(muted).child(counts))
        .into_any_element()
}

fn lerp_hsla(a: gpui::Hsla, b: gpui::Hsla, t: f32) -> gpui::Hsla {
    let a: gpui::Rgba = a.into();
    let b: gpui::Rgba = b.into();
    gpui::Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
    .into()
}

/// Confirm dialog for whichever destructive prompt is staged, resolved through
/// `intent::confirm_prompt`/`cancel_prompt`.
pub fn prompt_modal(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = theme.error.gpui(WINDOW_FG());

    let ui = &app.state.ui;
    let delete_label = tr(&app.state, "desktop.prompt.delete");
    let remove_label = tr(&app.state, "desktop.prompt.remove");
    let (message, confirm_label): (String, SharedString) =
        if let Some(prompt) = &ui.duplicate_prompt {
            let message = if prompt.duplicates.len() == 1 {
                let song = prompt
                    .tracks
                    .iter()
                    .find(|t| prompt.duplicates.contains(&t.id))
                    .map(|t| t.name.as_str())
                    .unwrap_or_default();
                tr(&app.state, "desktop.duplicate_one").replace("{song}", song)
            } else {
                tr(&app.state, "desktop.duplicate_many")
                    .replace("{n}", &prompt.duplicates.len().to_string())
            };
            (
                message.replace("{playlist}", &prompt.playlist_name),
                tr(&app.state, "desktop.add_anyway"),
            )
        } else if let Some(name) = &ui.folder_delete_prompt {
            (
                tr(&app.state, "desktop.prompt.delete_folder").replace("{}", name),
                delete_label,
            )
        } else if let Some(ids) = &ui.playlist_delete_prompt {
            let name = ids
                .first()
                .and_then(|id| {
                    app.state
                        .data
                        .library_view
                        .iter()
                        .find_map(|node| match node {
                            LibraryNode::Playlist { playlist, .. } if &playlist.id == id => {
                                Some(playlist.name.clone())
                            }
                            _ => None,
                        })
                })
                .unwrap_or_else(|| tr(&app.state, "desktop.prompt.this_playlist").to_string());
            (
                tr(&app.state, "desktop.prompt.delete_playlist").replace("{}", &name),
                delete_label,
            )
        } else if let Some(ids) = &ui.album_mass_delete_prompt {
            let name = ids
                .first()
                .and_then(|id| {
                    app.state
                        .data
                        .saved_albums
                        .iter()
                        .find(|album| &album.id == id)
                        .map(|album| album.name.clone())
                })
                .unwrap_or_else(|| tr(&app.state, "desktop.prompt.this_album").to_string());
            (
                tr(&app.state, "desktop.prompt.remove_album").replace("{}", &name),
                remove_label,
            )
        } else if ui.track_delete_prompt.is_some() {
            (
                tr(&app.state, "desktop.prompt.remove_tracks").to_string(),
                remove_label,
            )
        } else {
            (
                tr(&app.state, "desktop.prompt.remove_liked").to_string(),
                remove_label,
            )
        };

    let button = |id: &'static str,
                  label: SharedString,
                  color: gpui::Hsla,
                  border: gpui::Hsla,
                  hover_bg: gpui::Hsla| {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(border)
            .text_sm()
            .text_color(color)
            .cursor_pointer()
            .hover(move |style| style.bg(hover_bg))
            .child(label)
    };

    div()
        .id("prompt-backdrop")
        .absolute()
        .inset_0()
        .bg(gpui::hsla(0.0, 0.0, 0.0, 0.55))
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            echo_core::intent::cancel_prompt(&mut this.state);
            cx.notify();
        }))
        .child(
            div()
                .id("prompt-panel")
                .w(px(380.0))
                .rounded_lg()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .child(
                    div()
                        .text_sm()
                        .text_color(fg)
                        .child(SharedString::from(message)),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .child(
                            button(
                                "prompt-cancel",
                                tr(&app.state, "desktop.prompt.cancel"),
                                muted,
                                palette.menu_border,
                                palette.menu_hover,
                            )
                            .on_click(cx.listener(
                                |this: &mut EchoApp, _event, _window, cx| {
                                    echo_core::intent::cancel_prompt(&mut this.state);
                                    cx.notify();
                                },
                            )),
                        )
                        .when(
                            ui.duplicate_prompt.as_ref().is_some_and(|p| {
                                p.tracks.iter().any(|t| !p.duplicates.contains(&t.id))
                            }),
                            |el| {
                                el.child(
                                    button(
                                        "duplicate-skip",
                                        tr(&app.state, "desktop.skip_duplicates"),
                                        fg,
                                        palette.menu_border,
                                        palette.menu_hover,
                                    )
                                    .on_click(cx.listener(
                                        |this: &mut EchoApp, _, _, cx| {
                                            if let Some(event) =
                                                echo_core::intent::confirm_duplicate_skip(
                                                    &mut this.state,
                                                )
                                            {
                                                this.dispatch(event);
                                            }
                                            cx.notify();
                                        },
                                    )),
                                )
                            },
                        )
                        .child(
                            button(
                                "prompt-confirm",
                                confirm_label,
                                danger_color,
                                palette.danger_border,
                                palette.danger_wash,
                            )
                            .on_click(cx.listener(
                                |this: &mut EchoApp, _event, _window, cx| {
                                    if let Some(event) =
                                        echo_core::intent::confirm_prompt(&mut this.state)
                                    {
                                        this.dispatch(event);
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                ),
        )
}
