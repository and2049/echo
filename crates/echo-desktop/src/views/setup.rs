use super::*;

/// First-run credentials: a BYOK (bring-your-own-key) card writing to the
/// `state.ui.setup_*` fields and submitting through the setup intent.
pub(super) fn setup_view(
    app: &mut EchoApp,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let theme = &app.state.ui.active_theme;
    let palette = DesktopPalette::resolve(theme);
    let fg = theme.text.gpui(WINDOW_FG());
    let muted = theme.text_muted.gpui(WINDOW_FG());
    let accent = theme.primary.gpui(WINDOW_FG());

    let id_focused = app.setup_id_focus.is_focused(window);
    let secret_focused = app.setup_secret_focus.is_focused(window);
    let client_id = app.state.ui.setup_client_id.clone();
    let secret_masked = "•".repeat(app.state.ui.setup_client_secret.chars().count());
    let ready =
        !app.state.ui.setup_client_id.is_empty() && !app.state.ui.setup_client_secret.is_empty();
    let uri_copied = app.setup_uri_copied;

    let field = |id: &'static str,
                 label: SharedString,
                 value: String,
                 focused: bool,
                 secret: bool,
                 handle: gpui::FocusHandle| {
        let click_handle = handle.clone();
        div()
            .id(id)
            .key_context(crate::SEARCH_CONTEXT)
            .track_focus(&handle)
            .on_key_down(cx.listener(move |this: &mut EchoApp, event, window, cx| {
                this.handle_setup_key(secret, event, window, cx)
            }))
            .w_full()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(if focused { accent } else { palette.border })
            .flex()
            .flex_col()
            .gap_1()
            .cursor_pointer()
            .on_click(cx.listener(move |_this, _event, window, cx| {
                window.focus(&click_handle, cx);
                cx.notify();
            }))
            .child(div().text_xs().text_color(muted).child(label))
            .child(
                div()
                    .text_sm()
                    .text_color(fg)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(SharedString::from(if focused {
                        format!("{value}▏")
                    } else if value.is_empty() {
                        " ".to_string()
                    } else {
                        value
                    })),
            )
    };

    div()
        .flex_grow(1.0)
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(460.0))
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(palette.border)
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .text_lg()
                        .text_color(fg)
                        .child(tr(&app.state, "desktop.setup.title")),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .text_sm()
                        .text_color(muted)
                        .child(tr(&app.state, "desktop.setup.step1"))
                        .child(tr(&app.state, "desktop.setup.step2"))
                        .child(
                            div()
                                .id("setup-redirect-uri")
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.border)
                                .bg(palette.wash)
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_2()
                                .cursor_pointer()
                                .hover(|style| style.border_color(accent))
                                .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        echo_core::worker::api::REDIRECT_URI.to_string(),
                                    ));
                                    this.setup_uri_copied = true;
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .text_color(fg)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(echo_core::worker::api::REDIRECT_URI),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(if uri_copied { accent } else { muted })
                                        .child(tr(
                                            &app.state,
                                            if uri_copied {
                                                "desktop.setup.copied"
                                            } else {
                                                "desktop.setup.copy"
                                            },
                                        )),
                                ),
                        )
                        .child(tr(&app.state, "desktop.setup.step3")),
                )
                .child(
                    div()
                        .id("open-dashboard")
                        .text_sm()
                        .text_color(accent)
                        .cursor_pointer()
                        .hover(|style| style.underline())
                        .on_click(|_event, _window, _cx| {
                            let _ = webbrowser::open("https://developer.spotify.com/dashboard");
                        })
                        .child(tr(&app.state, "desktop.setup.dashboard")),
                )
                .child(field(
                    "setup-client-id",
                    tr(&app.state, "desktop.setup.client_id"),
                    client_id,
                    id_focused,
                    false,
                    app.setup_id_focus.clone(),
                ))
                .child(field(
                    "setup-client-secret",
                    tr(&app.state, "desktop.setup.client_secret"),
                    secret_masked,
                    secret_focused,
                    true,
                    app.setup_secret_focus.clone(),
                ))
                .child(
                    div()
                        .id("setup-submit")
                        .mt_1()
                        .px_4()
                        .py_2()
                        .rounded_md()
                        .flex()
                        .justify_center()
                        .text_sm()
                        .bg(if ready { accent } else { palette.wash })
                        .text_color(if ready {
                            gpui::hsla(0.0, 0.0, 0.08, 1.0)
                        } else {
                            muted
                        })
                        .when(ready, |el| el.cursor_pointer())
                        .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                            if let Some(event) =
                                echo_core::intent::submit_setup_credentials(&mut this.state)
                            {
                                this.dispatch(event);
                            }
                            cx.notify();
                        }))
                        .child(tr(&app.state, "desktop.setup.save")),
                ),
        )
}
