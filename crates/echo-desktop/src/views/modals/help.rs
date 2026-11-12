use super::*;

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
