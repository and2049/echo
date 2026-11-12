use super::*;

pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;

const THUMB_EDGE: f32 = 26.0;

/// Sidebar rows, which hover a touch stronger than the main area.
const SIDEBAR_PILL: PillMetrics = PillMetrics {
    row_height: 34.0,
    pill_height: 30.0,
};

/// True for the sidebar rows drag-and-drop must not move (their position is fixed, mirroring
/// `echo_core::intent`'s own check).
fn is_fixed_library_row(id: &str) -> bool {
    id == "LIKED_SONGS" || id == "local-library"
}

pub fn sidebar(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let tab = app.state.ui.active_library_tab;
    let count = match tab {
        LibraryTab::Albums => app.state.data.saved_albums.len(),
        LibraryTab::Artists => app.state.data.followed_artists.len(),
        _ => app.state.data.library_view.len(),
    };

    let nav_row = div()
        .flex()
        .flex_row()
        .items_center()
        .px_2()
        .pt_2()
        .child(nav_button_cluster(app, cx));

    let tab_button = |id: &'static str, label: SharedString, target: LibraryTab, active: bool| {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .text_sm()
            .text_color(if active { accent } else { muted })
            .hover(move |style| style.bg(palette.row_hover))
            .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                this.state.ui.active_library_tab = target;
                this.state.ui.selected_playlist_index = 0;
                if target == LibraryTab::Artists && this.state.data.followed_artists.is_empty() {
                    this.dispatch(echo_core::events::AppEvent::FetchFollowedArtists);
                }
                cx.notify();
            }))
            .child(label)
    };

    div()
        .relative()
        .flex_none()
        .w(px(app.sidebar_width))
        .h_full()
        .flex()
        .flex_col()
        .child(nav_row)
        .child(div().id("home-link").mx_2().mt_2().px_2().py_2().rounded_md().flex().items_center().gap_2().text_sm().text_color(if app.state.ui.active_view == ActiveView::Home { accent } else { muted }).when(app.state.ui.active_view == ActiveView::Home, |el| el.bg(palette.row_selected)).hover(move |style| style.bg(palette.row_hover)).cursor_pointer().on_click(cx.listener(|this: &mut EchoApp, _, window, cx| { this.open_home(cx); window.focus(&this.focus_handle, cx); })).child(svg().path("icons/home.svg").size(px(18.0)).text_color(muted)).child(tr(&app.state, "desktop.home")))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .p_2()
                .child(tab_button(
                    "Playlists",
                    tr(&app.state, "ui.playlists"),
                    LibraryTab::Playlists,
                    tab == LibraryTab::Playlists,
                ))
                .child(tab_button(
                    "Albums",
                    tr(&app.state, "ui.albums"),
                    LibraryTab::Albums,
                    tab == LibraryTab::Albums,
                ))
                .child(tab_button(
                    "Artists",
                    tr(&app.state, "ui.artists"),
                    LibraryTab::Artists,
                    tab == LibraryTab::Artists,
                )),
        )
        .child({
            // Browse nodes presented as quick links.
            let browse_link = |id: &'static str,
                               icon: &'static str,
                               label: SharedString,
                               open: fn(&mut echo_core::app::AppState)
                                   -> Option<echo_core::events::AppEvent>| {
                div()
                    .id(id)
                    .mx_2()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(muted)
                    .hover(move |style| style.bg(palette.row_hover))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut EchoApp, _event, _window, cx| {
                        if let Some(event) = open(&mut this.state) {
                            this.dispatch(event);
                        }
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .path(icon)
                            .flex_none()
                            .w(px(14.0))
                            .h(px(14.0))
                            .text_color(muted),
                    )
                    .child(label)
            };
            div()
                .flex_none()
                .flex()
                .flex_col()
                .pb_1()
                .child(browse_link(
                    "top-tracks",
                    "icons/star.svg",
                    tr(&app.state, "desktop.top_tracks"),
                    echo_core::intent::open_top_tracks,
                ))
                .child(browse_link(
                    "recently-played",
                    "icons/clock.svg",
                    tr(&app.state, "desktop.recently_played"),
                    echo_core::intent::open_recently_played,
                ))
                .child(browse_link(
                    "top-artists",
                    "icons/mic.svg",
                    tr(&app.state, "desktop.top_artists"),
                    echo_core::intent::open_top_artists,
                ))
                .child(browse_link(
                    "whats-new",
                    "icons/sparkles.svg",
                    tr(&app.state, "desktop.whats_new"),
                    echo_core::intent::open_whats_new,
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
                    let palette = DesktopPalette::resolve(theme);
                    let selected_bg = if this.state.ui.active_view == ActiveView::Library {
                        palette.row_selected
                    } else {
                        palette.row_hover
                    };
                    let drag_colors = DragColors::resolve(theme);
                    let tab = this.state.ui.active_library_tab;
                    let selected = this.state.ui.selected_playlist_index;
                    let visual = visual_range_in(&this.state, ActiveView::Library);

                    let rows: Vec<_> = range
                        .map(|ix| {
                            // Folders carry no cover; playlists, albums and artists get a
                            // thumb box even while (or if never) loaded, so the text column
                            // stays aligned. Folders get a chevron icon, pinned playlists a
                            // pin icon; artist thumbs are circular.
                            let (label, label_color, indent_px, thumb_url, has_thumb, chevron, pinned, round_thumb): (
                                SharedString,
                                _,
                                f32,
                                Option<String>,
                                bool,
                                Option<&'static str>,
                                bool,
                                bool,
                            ) = if tab == LibraryTab::Albums {
                                let album = &this.state.data.saved_albums[ix];
                                let url = album
                                    .thumb_url
                                    .clone()
                                    .or_else(|| album.image_url.clone());
                                (album.name.clone().into(), fg, 0.0, url, true, None, false, false)
                            } else if tab == LibraryTab::Artists {
                                let artist = &this.state.data.followed_artists[ix];
                                (
                                    artist.name.clone().into(),
                                    fg,
                                    0.0,
                                    artist.image_url.clone(),
                                    true,
                                    None,
                                    false,
                                    true,
                                )
                            } else {
                                match &this.state.data.library_view[ix] {
                                    LibraryNode::Folder(f) => (
                                        f.name.clone().into(),
                                        accent,
                                        0.0,
                                        None,
                                        false,
                                        Some(if f.is_open {
                                            "icons/arrow-down.svg"
                                        } else {
                                            "icons/arrow-right.svg"
                                        }),
                                        false,
                                        false,
                                    ),
                                    LibraryNode::SpotifyFolder { name, is_open, indent, .. } => (
                                        name.clone().into(),
                                        accent,
                                        *indent as f32 * 14.0,
                                        None,
                                        false,
                                        Some(if *is_open {
                                            "icons/arrow-down.svg"
                                        } else {
                                            "icons/arrow-right.svg"
                                        }),
                                        false,
                                        false,
                                    ),
                                    LibraryNode::Playlist { playlist, indent } => {
                                        let pinned = this
                                            .state
                                            .ui
                                            .library_config
                                            .pinned
                                            .contains(&playlist.id);
                                        let url = playlist
                                            .thumb_url
                                            .clone()
                                            .or_else(|| playlist.image_url.clone());
                                        (
                                            playlist.name.clone().into(),
                                            fg,
                                            *indent as f32 * 14.0,
                                            url,
                                            true,
                                            None,
                                            pinned,
                                            false,
                                        )
                                    }
                                }
                            };

                            // Liked Songs has no cover, so its box holds the heart icon, which
                            // takes the theme's accent. (An emoji in the name would not: macOS
                            // draws those in full colour.)
                            let liked_row = tab == LibraryTab::Playlists
                                && matches!(
                                    &this.state.data.library_view[ix],
                                    LibraryNode::Playlist { playlist, .. }
                                        if playlist.id == "LIKED_SONGS"
                                );
                            let thumb: Option<AnyElement> = has_thumb.then(|| {
                                let artwork = thumb_url.as_deref().and_then(|url| {
                                    this.state.ui.thumbnails.request(url, ThumbTier::Small);
                                    match this.state.ui.thumbnails.get(url, ThumbTier::Small) {
                                        Some(ThumbState::Ready { artwork }) => {
                                            Some(artwork.clone())
                                        }
                                        _ => None,
                                    }
                                });
                                match artwork.and_then(|artwork| this.images.get(&artwork)) {
                                    Some(image) => {
                                        let radius = if round_thumb { THUMB_EDGE / 2.0 } else { 4.0 };
                                        cover_image(image, THUMB_EDGE, radius).into_any_element()
                                    }
                                    None => {
                                        let el = div()
                                            .flex_none()
                                            .w(px(THUMB_EDGE))
                                            .h(px(THUMB_EDGE))
                                            .bg(palette.wash)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                svg()
                                                    .path(if liked_row {
                                                        "icons/heart.svg"
                                                    } else {
                                                        "icons/music-note.svg"
                                                    })
                                                    .w(px(12.0))
                                                    .h(px(12.0))
                                                    .text_color(if liked_row { accent } else { muted }),
                                            );
                                        if round_thumb { el.rounded_full() } else { el.rounded_sm() }
                                            .into_any_element()
                                    }
                                }
                            });

                            // Local playlists and echo's own folders look like Spotify's in the
                            // sidebar, so they are tagged to keep the distinction visible.
                            let local_badge: Option<SharedString> = (tab
                                == LibraryTab::Playlists
                                && match &this.state.data.library_view[ix] {
                                    LibraryNode::Playlist { playlist, .. } => {
                                        playlist.id.starts_with("local-playlist:")
                                    }
                                    LibraryNode::Folder(_) => true,
                                    LibraryNode::SpotifyFolder { .. } => false,
                                })
                            .then(|| tr(&this.state, "desktop.local_badge"));

                            // Playlists can be dragged between folders, the pinned block and the
                            // loose list, and folders among themselves; every playlist-tab row
                            // is a drop target (the intent rejects invalid ones).
                            let drag_source: Option<(String, SharedString)> =
                                if tab == LibraryTab::Playlists {
                                    match &this.state.data.library_view[ix] {
                                        LibraryNode::Playlist { playlist, .. }
                                            if !is_fixed_library_row(&playlist.id) =>
                                        {
                                            Some((
                                                playlist.id.clone(),
                                                playlist.name.clone().into(),
                                            ))
                                        }
                                        LibraryNode::Playlist { .. } => None,
                                        LibraryNode::Folder(folder) => Some((
                                            echo_core::models::echo_folder_key(&folder.name),
                                            folder.name.clone().into(),
                                        )),
                                        LibraryNode::SpotifyFolder { id, name, .. } => Some((
                                            echo_core::models::spotify_folder_key(id),
                                            name.clone().into(),
                                        )),
                                    }
                                } else {
                                    None
                                };

                            pill_row(ix, SIDEBAR_PILL, row_selected(ix, selected, visual), selected_bg, palette.row_hover, |row| {
                                row.pl(px(8.0 + indent_px))
                                .gap_2()
                                .text_color(label_color)
                                .when(tab != LibraryTab::Artists, |el| {
                                    // Artists have no row actions, so no context menu.
                                    el.on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                                            this.state.ui.selected_playlist_index = ix;
                                            this.track_menu = None;
                                            this.state.ui.playlist_add_filter.clear();
                                            // Spotify's folders are read-only, so they have no
                                            // row actions to offer.
                                            if this.state.ui.active_library_tab
                                                == LibraryTab::Playlists
                                                && matches!(
                                                    this.state.data.library_view.get(ix),
                                                    Some(LibraryNode::SpotifyFolder { .. })
                                                )
                                            {
                                                this.context_menu = None;
                                                cx.notify();
                                                return;
                                            }
                                            this.context_menu = Some(crate::ContextMenuState {
                                                index: ix,
                                                position: event.position,
                                            });
                                            cx.notify();
                                        }),
                                    )
                                })
                                .when_some(drag_source, |el, (id, name)| {
                                    el.on_drag(
                                        DraggedPlaylist { id, name },
                                        move |drag, _offset, _window, cx| {
                                            let name = drag.name.clone();
                                            cx.new(|_| DragPreview::label(name, drag_colors))
                                        },
                                    )
                                })
                                .when(tab == LibraryTab::Playlists, |el| {
                                    el.drag_over::<DraggedPlaylist>(move |style, _, _, _| {
                                        style.bg(palette.drag_target)
                                    })
                                    .on_drop(cx.listener(
                                        move |this: &mut EchoApp, drag: &DraggedPlaylist, _window, cx| {
                                            if echo_core::intent::move_library_row(
                                                &mut this.state,
                                                &drag.id,
                                                ix,
                                            ) {
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .drag_over::<DraggedTracks>(move |style, _, _, _| {
                                        style.bg(palette.drag_target)
                                    })
                                    .on_drop(cx.listener(
                                        move |this: &mut EchoApp, drag: &DraggedTracks, _window, cx| {
                                            for event in echo_core::intent::drop_tracks_on_library_row(
                                                &mut this.state,
                                                ix,
                                                drag.tracks.clone(),
                                            ) {
                                                this.dispatch(event);
                                            }
                                            cx.notify();
                                        },
                                    ))
                                })
                                .on_click(cx.listener(move |this: &mut EchoApp, event: &gpui::ClickEvent, _window, cx| {
                                    this.state.ui.selected_playlist_index = ix;
                                    let tab = this.state.ui.active_library_tab;
                                    // The first click of a double-click already opened the
                                    // entry; the second one starts it playing.
                                    if event.click_count() >= 2 {
                                        let play = match tab {
                                            LibraryTab::Albums => {
                                                echo_core::intent::play_album_at(&mut this.state, ix)
                                            }
                                            LibraryTab::Artists => None,
                                            _ => echo_core::intent::play_library_entry(
                                                &mut this.state,
                                                ix,
                                            ),
                                        };
                                        if let Some(event) = play {
                                            this.dispatch(event);
                                        }
                                        cx.notify();
                                        return;
                                    }
                                    let event = match tab {
                                        LibraryTab::Albums => {
                                            echo_core::intent::open_album(&mut this.state, ix)
                                        }
                                        LibraryTab::Artists => {
                                            echo_core::intent::open_followed_artist(
                                                &mut this.state,
                                                ix,
                                            )
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
                                .when_some(chevron, |el, icon| {
                                    el.child(
                                        svg()
                                            .path(icon)
                                            .flex_none()
                                            .w(px(12.0))
                                            .h(px(12.0))
                                            .text_color(accent),
                                    )
                                })
                                .when_some(thumb, |el, thumb| el.child(thumb))
                                .when(pinned, |el| {
                                    el.child(
                                        svg()
                                            .path("icons/pin.svg")
                                            .flex_none()
                                            .w(px(12.0))
                                            .h(px(12.0))
                                            .text_color(muted),
                                    )
                                })
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .child(label),
                                )
                                .when_some(local_badge, |el, badge| {
                                    el.child(
                                        div()
                                            .ml_auto()
                                            .flex_none()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(badge),
                                    )
                                })
                            })
                        })
                        .collect();

                    // Kick off fetches for whatever the rows above just requested.
                    echo_core::thumbnails::drain_pending(&mut this.state, &this.worker_tx);

                    rows
                }),
            )
            .track_scroll(&app.library_scroll)
            .on_drag_move(cx.listener(
                |this: &mut EchoApp, event: &gpui::DragMoveEvent<DraggedTracks>, _window, cx| {
                    this.drag_autoscroll(crate::DragList::Library, event.event.position.y, event.bounds, cx);
                },
            ))
            .on_drag_move(cx.listener(
                |this: &mut EchoApp, event: &gpui::DragMoveEvent<DraggedPlaylist>, _window, cx| {
                    this.drag_autoscroll(crate::DragList::Library, event.event.position.y, event.bounds, cx);
                },
            ))
            .flex_grow(1.0),
        )
        .child(
            div()
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .w(px(5.0))
                .cursor_col_resize()
                .hover(move |style| style.bg(palette.drag_target))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this: &mut EchoApp, event: &MouseDownEvent, _window, cx| {
                        this.begin_sidebar_resize(event.position.x, cx);
                        cx.stop_propagation();
                    }),
                ),
        )
}
