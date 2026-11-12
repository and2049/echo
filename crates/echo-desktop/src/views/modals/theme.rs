use super::*;

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
