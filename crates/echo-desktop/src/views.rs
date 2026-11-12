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
mod modals;
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
pub(crate) use modals::*;
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
            ActiveView::SearchResults => search::search_results(app, window, cx).into_any_element(),
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
