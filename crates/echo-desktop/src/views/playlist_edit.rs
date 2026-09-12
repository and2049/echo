use super::*;
use echo_core::intent::{self, PlaylistEditFocus, PlaylistPageAction};

pub fn page_buttons(app: &EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let palette = DesktopPalette::resolve(&app.state.ui.active_theme);
    let muted = app.state.ui.active_theme.text_muted.gpui(WINDOW_FG());
    let accent = app.state.ui.active_theme.primary.gpui(WINDOW_FG());
    let actions = intent::playlist_page_actions(&app.state);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .when(!actions.is_empty(), |el| {
            el.child(
                div()
                    .id("page-more")
                    .p_2()
                    .rounded_md()
                    .cursor_pointer()
                    .hover(move |s| s.bg(palette.wash))
                    .on_click(cx.listener(
                        |this: &mut EchoApp, event: &gpui::ClickEvent, window, cx| {
                            let size = window.viewport_size();
                            let position = event.position();
                            this.page_menu = Some((
                                gpui::point(
                                    position.x.min(size.width - px(238.0)).max(px(8.0)),
                                    position.y.min(size.height - px(190.0)).max(px(8.0)),
                                ),
                                0,
                            ));
                            window.focus(&this.focus_handle, cx);
                            cx.notify();
                        },
                    ))
                    .child(
                        svg()
                            .path("icons/more.svg")
                            .size(px(22.0))
                            .text_color(muted),
                    ),
            )
        })
        .when(actions.contains(&PlaylistPageAction::ToggleSaved), |el| {
            el.child(crate::icon_button(
                "page-save",
                "icons/heart.svg",
                if intent::page_context_saved(&app.state) {
                    accent
                } else {
                    palette.like_dim
                },
                palette.wash,
                cx,
                |this, cx| {
                    if let Some(event) = intent::run_playlist_page_action(
                        &mut this.state,
                        PlaylistPageAction::ToggleSaved,
                    ) {
                        this.dispatch(event);
                    }
                    cx.notify();
                },
            ))
        })
        .into_any_element()
}

pub fn playlist_edit_modal(app: &EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let draft = app.state.ui.playlist_edit.as_ref().unwrap();
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    let field = |focus, key, value: &str| {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().text_color(muted).child(tr(&app.state, key)))
            .child(
                div()
                    .id(key)
                    .px_3()
                    .py_2()
                    .min_h(px(if focus == PlaylistEditFocus::Description {
                        72.0
                    } else {
                        36.0
                    }))
                    .rounded_md()
                    .border_1()
                    .border_color(if draft.focus == focus {
                        accent
                    } else {
                        palette.menu_border
                    })
                    .bg(palette.wash)
                    .text_sm()
                    .text_color(fg)
                    .overflow_hidden()
                    .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
                        if let Some(d) = this.state.ui.playlist_edit.as_mut() {
                            d.focus = focus;
                        }
                        window.focus(&this.focus_handle, cx);
                        cx.notify();
                    }))
                    .child(div().truncate().child(value.to_string())),
            )
    };
    let button = |id: &'static str, key| {
        div()
            .id(id)
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(palette.menu_border)
            .text_sm()
            .cursor_pointer()
            .hover(move |s| s.bg(palette.menu_hover))
            .child(tr(&app.state, key))
    };
    div()
        .id("playlist-edit-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .on_click(cx.listener(|this: &mut EchoApp, _, _, cx| {
            intent::cancel_playlist_edit(&mut this.state);
            cx.notify();
        }))
        .child(
            div()
                .id("playlist-edit-panel")
                .w(px(440.0))
                .max_w_full()
                .p_4()
                .rounded_lg()
                .bg(surface)
                .border_1()
                .border_color(palette.menu_border)
                .text_color(fg)
                .flex()
                .flex_col()
                .gap_3()
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .child(
                    div()
                        .text_lg()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(tr(
                            &app.state,
                            if draft.id.starts_with("local-playlist:") {
                                "desktop.menu.rename"
                            } else {
                                "desktop.edit_details"
                            },
                        )),
                )
                .child(field(PlaylistEditFocus::Name, "desktop.name", &draft.name))
                .when(!draft.id.starts_with("local-playlist:"), |el| {
                    el.child(field(
                        PlaylistEditFocus::Description,
                        "desktop.description",
                        &draft.description,
                    ))
                    .child(
                        div().flex().gap_2().children(
                            [(true, "desktop.public"), (false, "desktop.private")]
                                .into_iter()
                                .map(|(public, key)| {
                                    button(key, key)
                                        .rounded_full()
                                        .when(draft.public == public, |el| {
                                            el.bg(palette.menu_selected).text_color(accent)
                                        })
                                        .on_click(cx.listener(
                                            move |this: &mut EchoApp, _, _, cx| {
                                                if let Some(d) =
                                                    this.state.ui.playlist_edit.as_mut()
                                                {
                                                    d.public = public;
                                                }
                                                cx.notify();
                                            },
                                        ))
                                }),
                        ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(
                            button("edit-cancel", "desktop.cancel").on_click(cx.listener(
                                |this: &mut EchoApp, _, _, cx| {
                                    intent::cancel_playlist_edit(&mut this.state);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            button("edit-save", "desktop.save")
                                .text_color(if draft.name.trim().is_empty() {
                                    muted
                                } else {
                                    accent
                                })
                                .on_click(cx.listener(|this: &mut EchoApp, _, _, cx| {
                                    this.save_playlist_edit(cx)
                                })),
                        ),
                ),
        )
        .into_any_element()
}

pub fn playlist_page_menu(app: &EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let (position, selected) = app.page_menu.unwrap();
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let surface = theme.surface.gpui(crate::theme::PANEL_BG());
    div()
        .id("page-menu-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .on_click(cx.listener(|this: &mut EchoApp, _, _, cx| {
            this.page_menu = None;
            cx.notify();
        }))
        .child(
            div()
                .id("page-menu")
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(230.0))
                .rounded_md()
                .border_1()
                .border_color(palette.menu_border)
                .bg(surface)
                .py_1()
                .flex()
                .flex_col()
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .children(
                    intent::playlist_page_actions(&app.state)
                        .into_iter()
                        .enumerate()
                        .map(|(index, action)| {
                            let key = match action {
                                PlaylistPageAction::Edit => "desktop.edit_details",
                                PlaylistPageAction::Rename => "desktop.menu.rename",
                                PlaylistPageAction::Queue => "desktop.add_to_queue",
                                PlaylistPageAction::CopyLink => "desktop.copy_link",
                                PlaylistPageAction::Delete => "desktop.menu.delete_playlist",
                                PlaylistPageAction::ToggleSaved => {
                                    if intent::page_context_saved(&app.state) {
                                        "desktop.remove_from_library"
                                    } else {
                                        "desktop.save_to_library"
                                    }
                                }
                            };
                            div()
                                .id(index)
                                .mx_1()
                                .px_2()
                                .py_2()
                                .rounded_md()
                                .text_sm()
                                .text_color(fg)
                                .when(index == selected, |el| el.bg(palette.menu_selected))
                                .when(action == PlaylistPageAction::Delete, |el| {
                                    el.border_1().border_color(palette.danger_border)
                                })
                                .hover(move |s| s.bg(palette.menu_hover))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this: &mut EchoApp, _, window, cx| {
                                    this.run_page_action(action, cx);
                                    if this.state.ui.mode == AppMode::Command {
                                        window.focus(&this.command_focus, cx);
                                    } else {
                                        window.focus(&this.focus_handle, cx);
                                    }
                                }))
                                .child(tr(&app.state, key))
                        }),
                ),
        )
        .into_any_element()
}
