use super::*;

/// The add-to-playlist flyout's preferred dimensions.
const SUBMENU_WIDTH: f32 = 190.0;

const SUBMENU_MAX_H: f32 = 270.0;

/// The sidebar right-click menu: items depend on what the row is, actions run through
/// [`EchoApp::run_menu_action`]. Destructive items stage a prompt for [`prompt_modal`].
pub fn context_menu(
    app: &mut EchoApp,
    window: &Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let menu = app
        .context_menu
        .clone()
        .expect("context_menu rendered without state");
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let mut items: Vec<(SharedString, MenuAction, bool)> = Vec::new();
    if app.state.ui.active_library_tab == LibraryTab::Albums {
        if app.state.data.saved_albums.get(menu.index).is_some() {
            items.push((tr(&app.state, "desktop.menu.open"), MenuAction::Open, false));
            items.push((
                tr(&app.state, "desktop.menu.remove_library"),
                MenuAction::RemoveAlbum,
                true,
            ));
        }
    } else if let Some(node) = app.state.data.library_view.get(menu.index) {
        match node {
            LibraryNode::Folder(_) => {
                items.push((
                    tr(&app.state, "desktop.menu.rename"),
                    MenuAction::Rename,
                    false,
                ));
                items.push((
                    tr(&app.state, "desktop.menu.delete_folder"),
                    MenuAction::DeleteFolder,
                    true,
                ));
            }
            LibraryNode::SpotifyFolder { .. } => {}
            LibraryNode::Playlist { playlist, indent } => {
                items.push((tr(&app.state, "desktop.menu.open"), MenuAction::Open, false));
                let special = playlist.id == "LIKED_SONGS" || playlist.id == "local-library";
                let local = playlist.id.starts_with("local-playlist:");
                if !special {
                    let pinned = app.state.ui.library_config.pinned.contains(&playlist.id);
                    items.push((
                        tr(
                            &app.state,
                            if pinned {
                                "desktop.menu.unpin"
                            } else {
                                "desktop.menu.pin"
                            },
                        ),
                        MenuAction::TogglePin,
                        false,
                    ));
                }
                if local {
                    items.push((
                        tr(&app.state, "desktop.menu.rename"),
                        MenuAction::Rename,
                        false,
                    ));
                } else if echo_core::intent::owns_playlist(&app.state, &playlist.id) {
                    items.push((
                        tr(&app.state, "desktop.edit_details"),
                        MenuAction::EditDetails,
                        false,
                    ));
                }
                if *indent >= 1 && !app.state.in_spotify_folder(&playlist.id) {
                    items.push((
                        tr(&app.state, "desktop.menu.remove_folder"),
                        MenuAction::RemoveFromFolder,
                        false,
                    ));
                }
                let own = app.state.data.user_id.as_ref() == Some(&playlist.owner_id);
                if local || (!special && own) {
                    items.push((
                        tr(&app.state, "desktop.menu.delete_playlist"),
                        MenuAction::DeletePlaylist,
                        true,
                    ));
                }
            }
        }
    }

    let index = menu.index;
    div()
        .id("context-menu-backdrop")
        .absolute()
        .inset_0()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.context_menu = None;
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this: &mut EchoApp, _event: &MouseDownEvent, _window, cx| {
                this.context_menu = None;
                cx.notify();
            }),
        )
        .child(menu_placement(
            MenuAnchor::Pointer(menu.position),
            div()
                .id("context-menu")
                .occlude()
                .w(px(210.0).min(menu_bounds(window).size.width))
                .max_h(menu_bounds(window).size.height)
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                // Clicks on the menu itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(items.into_iter().map(|(label, action, danger)| {
                    // Inset and rounded like the list rows: a full-bleed highlight would square
                    // off the panel's rounded corners on the first and last item.
                    div()
                        .id(label.clone())
                        .flex_shrink_0()
                        .mx_1()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .text_sm()
                        .text_color(if danger { danger_color } else { fg })
                        .hover(move |style| style.bg(palette.menu_hover))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut EchoApp, _event, window, cx| {
                            this.run_menu_action(action, index, window, cx);
                        }))
                        .child(label)
                })),
        ))
}

/// Right-click menu for a track row. The item set and labels come from the core action-menu
/// model (`ActionMenuContext::actions()` / `action_menu::label`); remove-from-playlist is
/// appended for modifiable playlists.
/// The open track menu's rows: `(label, item, is_destructive)`. Keyboard selection indexes
/// into this, so it is the single source of the menu's contents.
pub fn track_menu_items(app: &EchoApp) -> Vec<(SharedString, TrackMenuItem, bool)> {
    let Some(menu) = app.track_menu.as_ref() else {
        return Vec::new();
    };
    let ctx = &menu.ctx;
    let mut items: Vec<(SharedString, TrackMenuItem, bool)> = ctx
        .actions()
        .into_iter()
        .map(|action| {
            (
                echo_core::action_menu::label(&app.state, ctx, action).into(),
                TrackMenuItem::Action(action),
                false,
            )
        })
        .collect();
    // Queue rows have no playlist behind them — `active_tracklist_context` is whatever was
    // last browsed, so removing would hit the wrong playlist.
    if app.state.ui.active_view != ActiveView::Queue
        && app
            .state
            .data
            .active_tracklist_context
            .as_ref()
            .is_some_and(|context| context.can_modify_playlist(app.state.data.user_id.as_ref()))
    {
        items.push((
            tr(&app.state, "desktop.menu.remove_playlist"),
            TrackMenuItem::RemoveFromPlaylist,
            true,
        ));
    }
    items
}

pub fn track_context_menu(
    app: &mut EchoApp,
    window: &Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let menu = app
        .track_menu
        .clone()
        .expect("track_context_menu rendered without state");
    let items = track_menu_items(app);
    let add_row = app.track_menu_add_row();
    let row_bounds = app.submenu_row_bounds.clone();
    // Built first: it reads `app` immutably and hangs off the menu as a sibling, because the
    // menu panel clips its own children.
    let submenu = menu
        .submenu
        .map(|choice| playlist_submenu(app, choice, window, cx));

    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let danger_color = gpui::hsla(0.0, 0.7, 0.6, 1.0);

    let selected = menu.selected;
    let submenu_open = menu.submenu.is_some();

    div()
        .id("track-menu-backdrop")
        .absolute()
        .inset_0()
        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
            this.track_menu = None;
            this.state.ui.playlist_add_filter.clear();
            cx.notify();
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this: &mut EchoApp, _event: &MouseDownEvent, _window, cx| {
                this.track_menu = None;
                this.state.ui.playlist_add_filter.clear();
                cx.notify();
            }),
        )
        .child(menu_placement(
            menu.position
                .map_or(MenuAnchor::Center, MenuAnchor::Pointer),
            div()
                .id("track-menu")
                .occlude()
                .track_scroll(&app.track_menu_scroll)
                .w(px(210.0).min(menu_bounds(window).size.width))
                .max_h(menu_bounds(window).size.height)
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                // Clicks on the menu itself must not reach the backdrop's close handler.
                .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
                .children(
                    items
                        .into_iter()
                        .enumerate()
                        .map(|(ix, (label, item, danger))| {
                            let is_add = add_row == Some(ix);
                            let is_selected = ix == selected;
                            // A hover-opened flyout leaves the keyboard selection where it was, so its
                            // parent row keeps the hover wash for as long as the flyout is up — the
                            // selected wash would read as a second cursor.
                            let holds_submenu = is_add && submenu_open && !is_selected;
                            let row_bounds = row_bounds.clone();
                            div()
                                .id(label.clone())
                                .flex_shrink_0()
                                .relative()
                                .mx_1()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .text_color(if danger { danger_color } else { fg })
                                .when(is_selected, |el| el.bg(palette.menu_selected))
                                .when(holds_submenu, |el| el.bg(palette.menu_hover))
                                .when(!is_selected, |el| {
                                    el.hover(move |style| style.bg(palette.menu_hover))
                                })
                                .cursor_pointer()
                                // Pointer moves drive the flyout: opening it on the add row, closing it
                                // on any other row the pointer settles on. See
                                // [`EchoApp::hover_track_menu_row`].
                                .on_mouse_move(cx.listener(
                                    move |this: &mut EchoApp,
                                          event: &gpui::MouseMoveEvent,
                                          _window,
                                          cx| {
                                        this.hover_track_menu_row(ix, event.position, cx);
                                    },
                                ))
                                .on_click(cx.listener(
                                    move |this: &mut EchoApp, _event, _window, cx| {
                                        if is_add {
                                            this.open_playlist_submenu(true, cx);
                                        } else {
                                            this.run_track_menu_action(item, cx);
                                        }
                                    },
                                ))
                                .when(is_add, |el| {
                                    // The flyout is a sibling of the menu, so it needs this row's
                                    // rectangle in window space to anchor to.
                                    el.child(
                                        canvas(
                                            move |bounds, _window, _cx| row_bounds.set(bounds),
                                            |_, _, _, _| {},
                                        )
                                        .absolute()
                                        .size_full(),
                                    )
                                })
                                .child(div().flex_grow(1.0).child(label))
                                .when(is_add, |el| {
                                    el.child(
                                        div().flex_none().text_xs().text_color(muted).child("▸"),
                                    )
                                })
                        }),
                ),
        ))
        .children(submenu)
}

/// The add-to-playlist flyout: the writable playlists, hanging off the track menu's "Add to
/// playlist" row — to its right, or to its left where the window edge leaves no room. A sibling
/// of the menu panel rather than a child of that row, which `overflow_hidden` would clip.
pub(super) fn playlist_submenu(
    app: &EchoApp,
    choice: usize,
    window: &Window,
    cx: &mut Context<EchoApp>,
) -> AnyElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());

    let bounds = app.submenu_bounds.clone();
    let local_label = tr(&app.state, "ui.local");
    let empty_label = tr(&app.state, "desktop.playlist_add_none");
    let choices: Vec<(SharedString, bool)> =
        echo_core::action_menu::filtered_playlist_add_choices(&app.state)
            .into_iter()
            .map(|(_, playlist)| {
                (
                    SharedString::from(playlist.name),
                    playlist.owner_id == "local",
                )
            })
            .collect();

    let choices: Vec<_> = std::iter::once((tr(&app.state, "desktop.new_playlist"), false))
        .chain(choices)
        .collect();

    menu_placement(
        MenuAnchor::Row(app.submenu_row_bounds.clone()),
        div()
            .id("track-menu-submenu")
            .w(px(SUBMENU_WIDTH).min(menu_bounds(window).size.width))
            .max_h(px(SUBMENU_MAX_H).min(menu_bounds(window).size.height))
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .bg(surface)
            .py_1()
            .flex()
            .flex_col()
            // Swallows clicks and wheel events so neither reaches the backdrop or the list behind.
            .occlude()
            .on_click(cx.listener(|_this, _event, _window, cx| cx.stop_propagation()))
            .when(!app.state.ui.playlist_add_filter.is_empty(), |el| {
                el.child(
                    div()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .text_sm()
                        .text_color(muted)
                        .truncate()
                        .child(app.state.ui.playlist_add_filter.clone()),
                )
            })
            .child(
                canvas(
                    move |painted, _window, _cx| bounds.set(painted),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(if choices.is_empty() {
                div()
                    .mx_1()
                    .px_2()
                    .py_1()
                    .text_sm()
                    .text_color(muted)
                    .child(empty_label)
                    .into_any_element()
            } else {
                div()
                    .id("track-menu-submenu-list")
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .max_h(px(SUBMENU_MAX_H - 10.0))
                    .overflow_y_scroll()
                    .track_scroll(&app.submenu_scroll)
                    .children(choices.into_iter().enumerate().map(|(ix, (name, local))| {
                        let is_selected = ix == choice;
                        let local_label = local_label.clone();
                        div()
                            .id(ix)
                            .flex_shrink_0()
                            .mx_1()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .when(is_selected, |el| el.bg(palette.menu_selected))
                            .when(!is_selected, |el| {
                                el.hover(move |style| style.bg(palette.menu_hover))
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(
                                move |this: &mut EchoApp, _event, _window, cx| {
                                    this.commit_playlist_submenu(ix, cx);
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
    .into_any_element()
}
