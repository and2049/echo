use super::*;

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
