use super::*;

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
