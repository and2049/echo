use super::*;

const ROW_HEIGHT: f32 = 34.0;
const CONTROL_HEIGHT: f32 = 24.0;

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
    let c = Controls::new(app);
    let Controls {
        palette,
        fg,
        surface,
        ..
    } = c;
    let config = &app.state.ui.library_config;

    let mut rows = general_section(&c, config, cx);
    rows.extend(appearance_section(&c, app, config, cx));
    rows.extend(audio_section(&c, config, cx));
    rows.extend(local_section(&c, app, window, cx));
    rows.extend(updates_section(&c, app, config, cx));

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
                        .child(c.t("desktop.settings.title")),
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
                        .child(div().flex_none().flex().flex_col().children(rows)),
                ),
        )
}

fn general_section(
    c: &Controls,
    config: &echo_core::config::LibraryConfig,
    cx: &mut Context<EchoApp>,
) -> Vec<Div> {
    let language_row = c.row(
        c.t("desktop.settings.language"),
        None,
        true,
        c.dropdown(
            "lang",
            [("English", "en"), ("简体中文", "zh-CN")]
                .into_iter()
                .map(|(label, code)| {
                    (
                        label.into(),
                        SettingChange::Command(format!("lang {code}")),
                        config.language == code,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let tray_row = c.row(
        c.t("desktop.settings.tray"),
        None,
        true,
        c.toggle(
            "tray",
            config.close_to_tray,
            SettingChange::Command(format!("tray {}", on_off(!config.close_to_tray))),
            cx,
        ),
    );

    let sort_row = c.row(
        c.t("desktop.settings.order"),
        None,
        true,
        c.dropdown(
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
                        c.t(&format!("desktop.settings.{key}")),
                        SettingChange::Command(format!("sort {name}")),
                        config.sort_mode == mode,
                    )
                })
                .collect()
            },
            true,
            cx,
        ),
    );

    let relative_row = c.row(
        c.t("desktop.settings.relative"),
        None,
        true,
        c.toggle(
            "relative",
            config.relative_line_numbers,
            SettingChange::Command(format!(
                "relative {}",
                on_off(!config.relative_line_numbers)
            )),
            cx,
        ),
    );

    let mut rows = vec![
        c.heading(c.t("desktop.settings.general"), None),
        language_row,
    ];
    if !cfg!(target_os = "macos") {
        rows.push(tray_row);
    }
    rows.extend([sort_row, relative_row]);
    rows
}

fn appearance_section(
    c: &Controls,
    app: &EchoApp,
    config: &echo_core::config::LibraryConfig,
    cx: &mut Context<EchoApp>,
) -> Vec<Div> {
    let palette = c.palette;
    let theme_mode = config
        .theme_mode
        .unwrap_or(echo_core::config::ThemeMode::Auto);
    // The theme in use, or under `Auto` the dark and the light one.
    let theme_label = {
        use echo_core::config::{Scheme, ThemeMode};
        let chosen = |scheme| echo_core::theme::selected_name(config, &app.state.ui.themes, scheme);
        match theme_mode {
            ThemeMode::Auto => format!("{} / {}", chosen(Scheme::Dark), chosen(Scheme::Light)),
            ThemeMode::Dark => chosen(Scheme::Dark),
            ThemeMode::Light => chosen(Scheme::Light),
        }
    };
    let theme_row = c.row(
        c.t("desktop.theme"),
        None,
        true,
        c.button("settings-theme", theme_label.into(), false)
            .cursor_pointer()
            .hover(move |style| style.bg(palette.menu_hover))
            .on_click(cx.listener(|this: &mut EchoApp, _event, _window, cx| {
                this.settings_open = false;
                this.toggle_themes(cx);
            }))
            .into_any_element(),
    );

    let mode_row = c.row(
        c.t("desktop.settings.mode"),
        None,
        true,
        c.segmented(
            "mode",
            echo_core::config::ThemeMode::ALL
                .into_iter()
                .map(|mode| {
                    (
                        c.t(&format!("desktop.theme_mode.{}", mode.name())),
                        format!("mode {}", mode.name()),
                        theme_mode == mode,
                    )
                })
                .collect(),
            cx,
        ),
    );

    let pixelate_row = c.row(
        c.t("desktop.settings.pixelate"),
        None,
        true,
        c.dropdown(
            "pixelate",
            with_current(&[0, 8, 16, 32], config.cover_img_pixels as usize)
                .into_iter()
                .map(|n| {
                    (
                        if n == 0 {
                            c.t("ui.off")
                        } else {
                            SharedString::from(format!("{n} px"))
                        },
                        SettingChange::Command(format!("pixelate {n}")),
                        config.cover_img_pixels as usize == n,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let backdrop_row = c.row(
        c.t("desktop.settings.backdrop"),
        None,
        true,
        c.dropdown(
            "backdrop",
            echo_core::config::BackdropMode::ALL
                .into_iter()
                .map(|mode| {
                    (
                        c.t(&format!("desktop.settings.backdrops.{}", mode.name())),
                        SettingChange::Command(format!("backdrop {}", mode.name())),
                        config.immersive_backdrop == mode,
                    )
                })
                .collect(),
            true,
            cx,
        ),
    );

    let visualizer_row = c.row(
        c.t("desktop.settings.visualizer"),
        None,
        true,
        // `:vis` flips the visualizer; it takes no argument.
        c.toggle(
            "vis",
            config.enable_visualizer,
            SettingChange::Command("vis".into()),
            cx,
        ),
    );

    let bins_row = c.row(
        c.t("desktop.settings.bins"),
        None,
        config.enable_visualizer,
        c.dropdown(
            "visbins",
            with_current(&[7, 9, 16, 24, 32], config.vis_bins)
                .into_iter()
                .map(|n| {
                    (
                        SharedString::from(n.to_string()),
                        SettingChange::Command(format!("visbins {n}")),
                        config.vis_bins == n,
                    )
                })
                .collect(),
            config.enable_visualizer,
            cx,
        ),
    );

    vec![
        c.heading(c.t("desktop.settings.appearance"), None),
        theme_row,
        mode_row,
        pixelate_row,
        backdrop_row,
        visualizer_row,
        bins_row,
    ]
}

fn audio_section(
    c: &Controls,
    config: &echo_core::config::LibraryConfig,
    cx: &mut Context<EchoApp>,
) -> Vec<Div> {
    let Controls { palette, fg, .. } = *c;
    let bitrate_row = c.row(
        c.t("desktop.settings.quality"),
        None,
        true,
        c.dropdown(
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
    let normalisation_row = c.row(
        c.t("desktop.settings.normalisation"),
        None,
        true,
        c.toggle(
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
    let pregain_row = c.row(
        c.t("desktop.settings.pregain"),
        Some(c.t("desktop.settings.pregain_desc")),
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

    vec![
        c.heading(
            c.t("desktop.settings.audio"),
            Some(c.t("desktop.settings.next_launch")),
        ),
        bitrate_row,
        normalisation_row,
        pregain_row,
    ]
}

fn local_section(
    c: &Controls,
    app: &EchoApp,
    window: &Window,
    cx: &mut Context<EchoApp>,
) -> Vec<Div> {
    let Controls {
        palette,
        fg,
        muted,
        accent,
        ..
    } = *c;
    let path_focused = app.settings_path_focus.is_focused(window);
    let path_value = app.settings_path_input.clone();
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
            c.t("desktop.settings.folder_empty").to_string()
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
            c.button(
                "settings-rescan",
                c.t(if path_unsaved {
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

    vec![c.heading(c.t("desktop.settings.local"), None), local_row]
}

// The one control here that is neither a `:` command nor a config write: it runs a network
// call, so its label and action both come from `app.update_state`.
fn updates_section(
    c: &Controls,
    app: &EchoApp,
    config: &echo_core::config::LibraryConfig,
    cx: &mut Context<EchoApp>,
) -> Vec<Div> {
    let Controls { palette, muted, .. } = *c;
    let version_row = c.row(
        c.t("desktop.settings.updates.version"),
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
    let auto_update_row = c.row(
        c.t("desktop.settings.updates.auto"),
        None,
        true,
        c.toggle(
            "autoupdate",
            config.auto_update,
            SettingChange::Command(format!("autoupdate {}", on_off(!config.auto_update))),
            cx,
        ),
    );
    // The hint only appears when there is something to report: an installed update waiting
    // for a restart, or why the last attempt failed.
    let (update_label, update_hint, update_active) = match &app.update_state {
        UpdateState::Idle => (c.t("desktop.settings.updates.check"), None, true),
        UpdateState::Checking => (c.t("desktop.settings.updates.checking"), None, false),
        UpdateState::UpToDate => (c.t("desktop.settings.updates.uptodate"), None, true),
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
            c.t("desktop.settings.updates.restart"),
            Some(translate("desktop.settings.updates.installed", version)),
            false,
        ),
        UpdateState::Failed(message) => (
            c.t("desktop.settings.updates.retry"),
            Some(SharedString::from(message.clone())),
            true,
        ),
        // Nothing in-app can fix this one, so the button leaves for the releases page.
        UpdateState::Blocked(message) => (
            c.t("desktop.settings.updates.open_releases"),
            Some(SharedString::from(message.clone())),
            true,
        ),
    };

    let update_button = c
        .button("settings-update", update_label, update_active)
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

    let update_row = c.row(
        c.t("desktop.settings.updates.label"),
        update_hint,
        true,
        update_button,
    );

    vec![
        c.heading(c.t("desktop.settings.updates.section"), None),
        version_row,
        auto_update_row,
        update_row,
    ]
}

fn on_off(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

/// The offered sizes, plus whatever `:pixelate` / `:visbins` set outside them, so the
/// button never shows blank.
fn with_current(offered: &[usize], current: usize) -> Vec<usize> {
    let mut values = offered.to_vec();
    if !values.contains(&current) {
        values.push(current);
        values.sort_unstable();
    }
    values
}

/// The colours and building blocks every settings row is made of.
#[derive(Clone, Copy)]
struct Controls<'a> {
    palette: DesktopPalette,
    fg: Hsla,
    muted: Hsla,
    surface: Hsla,
    accent: Hsla,
    open_dropdown: Option<&'static str>,
    language: &'a str,
}

impl<'a> Controls<'a> {
    fn new(app: &'a EchoApp) -> Self {
        let theme = &app.state.ui.active_theme;
        Self {
            palette: DesktopPalette::resolve(theme),
            fg: theme.text.gpui(WINDOW_FG()),
            muted: theme.text_muted.gpui(WINDOW_FG()),
            surface: theme.surface.gpui(crate::theme::PANEL_BG()),
            accent: theme.primary.gpui(WINDOW_FG()),
            open_dropdown: app.settings_dropdown,
            language: &app.state.ui.library_config.language,
        }
    }

    fn t(&self, key: &str) -> SharedString {
        SharedString::from(echo_core::i18n::t(key, self.language))
    }

    /// A labelled row with its control on the right. Labels and hints arrive translated. A
    /// disabled row is dimmed: its setting has no effect until another one is turned on.
    fn row(
        &self,
        label: SharedString,
        hint: Option<SharedString>,
        enabled: bool,
        control: AnyElement,
    ) -> Div {
        let Self { fg, muted, .. } = *self;
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
    }

    /// A section title over a rule, with an optional note that applies to the whole section.
    fn heading(&self, label: SharedString, note: Option<SharedString>) -> Div {
        let Self { palette, muted, .. } = *self;
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
    }

    /// An on/off switch.
    fn toggle(
        &self,
        id: &'static str,
        on: bool,
        change: SettingChange,
        cx: &mut Context<EchoApp>,
    ) -> AnyElement {
        let Self {
            palette,
            muted,
            surface,
            accent,
            ..
        } = *self;
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
    }

    /// A segmented control: one joined bar, the active segment filled.
    fn segmented(
        &self,
        id: &'static str,
        options: Vec<(SharedString, String, bool)>,
        cx: &mut Context<EchoApp>,
    ) -> AnyElement {
        let Self {
            palette, fg, muted, ..
        } = *self;
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
    }

    /// A dropdown: a button showing the current option, and while open a menu of all of them
    /// hanging under it. The menu is drawn deferred so the scrolling list does not clip it.
    fn dropdown(
        &self,
        id: &'static str,
        options: Vec<(SharedString, SettingChange, bool)>,
        enabled: bool,
        cx: &mut Context<EchoApp>,
    ) -> AnyElement {
        let Self {
            palette,
            fg,
            muted,
            surface,
            accent,
            ..
        } = *self;
        let current = options
            .iter()
            .find(|option| option.2)
            .map(|option| option.0.clone())
            .unwrap_or_default();
        let is_open = enabled && self.open_dropdown == Some(id);
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
    }

    /// A plain push button; the caller adds the click. `strong` accents the one that matters.
    fn button(&self, id: &'static str, label: SharedString, strong: bool) -> Stateful<Div> {
        let Self {
            palette,
            fg,
            accent,
            ..
        } = *self;
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
    }
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
