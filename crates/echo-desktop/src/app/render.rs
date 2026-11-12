use super::*;

impl Render for EchoApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Every theme change re-renders the app, so this is the one place that catches them
        // all (startup, the theme picker, a hot-reloaded theme file).
        self.dock_icon.sync(&self.state.ui.active_theme, cx);
        let theme = &self.state.ui.active_theme;
        let bg = theme.background.gpui(WINDOW_BG());
        let muted = theme.text_muted.gpui(WINDOW_FG());

        // Built ahead of the chain because they borrow self mutably, like the other views.
        let command_bar = matches!(self.state.ui.mode, AppMode::Command | AppMode::Search)
            .then(|| self.render_command_bar(cx).into_any_element());
        // Visual mode says so on the status line: without a mode indicator a range selection
        // looks like the list has simply highlighted several rows for no reason.
        let status_line = match self.state.ui.mode {
            AppMode::Visual => Some(
                self.state
                    .ui
                    .status_message
                    .clone()
                    .map(|message| format!("-- VISUAL --  {message}"))
                    .unwrap_or_else(|| {
                        format!(
                            "-- VISUAL --  {}",
                            views::tr(&self.state, "desktop.visual_hint")
                        )
                    }),
            ),
            AppMode::Normal => self.state.ui.status_message.clone(),
            _ => None,
        }
        .map(|message| {
            div()
                .flex_none()
                .px_4()
                .py_1()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(message))
                .into_any_element()
        });
        // The audio-device failure banner is persistent (no expiry) and separate from the
        // status line, so transient statuses can't hide it. Core clears it on recovery.
        let audio_banner = self.state.ui.audio_output_error.clone().map(|message| {
            let error = self.state.ui.active_theme.error.gpui(WINDOW_FG());
            let banner_bg = DesktopPalette::resolve(&self.state.ui.active_theme).error_wash;
            div()
                .flex_none()
                .px_4()
                .py_1()
                .text_xs()
                .bg(banner_bg)
                .text_color(error)
                .child(SharedString::from(message))
                .into_any_element()
        });
        let update_banner = match &self.update_state {
            UpdateState::Ready(version) if !self.update_banner_dismissed => {
                let palette = DesktopPalette::resolve(&self.state.ui.active_theme);
                let text = views::tr(&self.state, "desktop.settings.updates.installed")
                    .replace("{}", version);
                Some(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_4()
                        .py_2()
                        .bg(palette.wash)
                        .border_t_1()
                        .border_color(palette.border)
                        .text_sm()
                        .child(div().flex_1().child(text))
                        .child(
                            div()
                                .id("restart-update")
                                .px_3()
                                .py_1()
                                .rounded_md()
                                .border_1()
                                .border_color(palette.border)
                                .cursor_pointer()
                                .hover(move |s| s.bg(palette.row_hover))
                                .on_click(cx.listener(|this, _, _, cx| this.restart_updated(cx)))
                                .child(views::tr(
                                    &self.state,
                                    "desktop.settings.updates.restart_now",
                                )),
                        )
                        .child(
                            div()
                                .id("dismiss-update")
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.update_banner_dismissed = true;
                                    cx.notify();
                                }))
                                .child(views::tr(&self.state, "desktop.settings.updates.later")),
                        )
                        .into_any_element(),
                )
            }
            _ => None,
        };
        let theme_modal = self
            .theme_modal_open
            .then(|| views::theme_modal(self, cx).into_any_element());
        let device_modal = self
            .state
            .ui
            .device_modal_open
            .then(|| views::device_modal(self, cx).into_any_element());
        let sort_menu = self
            .sort_menu_open
            .then(|| views::sort_menu(self, cx).into_any_element());
        let settings_modal = self
            .settings_open
            .then(|| views::settings_modal(self, window, cx).into_any_element());
        let help_modal = self
            .help_open
            .then(|| views::help_modal(self, cx).into_any_element());
        let context_menu = self
            .context_menu
            .is_some()
            .then(|| views::context_menu(self, window, cx).into_any_element());
        if self.track_menu.is_none() {
            self.track_menu_scroll.scroll_to_item(0);
        }
        let track_menu = self
            .track_menu
            .is_some()
            .then(|| views::track_context_menu(self, window, cx).into_any_element());
        let playlist_add_modal = self
            .state
            .ui
            .playlist_add_modal_open
            .then(|| views::playlist_add_modal(self, cx).into_any_element());
        let prompt_modal = echo_core::intent::prompt_active(&self.state)
            .then(|| views::prompt_modal(self, cx).into_any_element());
        let frame_palette = crate::theme::DesktopPalette::resolve(&self.state.ui.active_theme);
        // Resolved before the chain so the style closure below borrows this rather than `window`.
        let corners = views::client_corners(window);
        // Windows and macOS hide the native bar (`appears_transparent`) and always want ours.
        // Linux only gets it when the app actually owns the frame: the `Client` decorations
        // requested at open are downgraded to `Server` on an X11 session with no compositor, and
        // there the window manager draws a real titlebar that ours would sit underneath.
        let owns_frame = !cfg!(target_os = "linux") || corners.is_some();
        // The immersive backdrop while that view is up: its colors replace the theme's for the
        // window fill and the titlebar, and its picture goes under everything.
        let backdrop = self.immersive.then(|| {
            let playback = &self.state.playback;
            let cover = playback
                .playing_track_image
                .as_ref()
                .or(playback.previous_track_image.as_ref());
            let fallback = backdrop::theme_palette(&self.state.ui.active_theme);
            let mode = self.state.ui.library_config.immersive_backdrop;
            let backdrop = self.backdrops.get(mode, cover, &fallback);
            if let Some(build) = self.backdrops.take_build() {
                cx.spawn(async move |this, cx| {
                    let built = cx.background_spawn(async move { build.run() }).await;
                    this.update(cx, |app: &mut EchoApp, cx| {
                        app.backdrops.install(built);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            self.backdrops.release(|image| {
                window.drop_image(image).ok();
            });
            backdrop
        });
        let phase = if cx.reduce_motion() {
            0.0
        } else {
            self.backdrops.phase()
        };
        let titlebar = owns_frame.then(|| {
            views::titlebar(self, backdrop.as_ref().map(|b| &b.colors), window, cx)
                .into_any_element()
        });

        let root = div()
            .key_context(if self.playlist_input_open() {
                "list search"
            } else {
                LIST_CONTEXT
            })
            .track_focus(&self.focus_handle)
            // Scrubs track the pointer at the window level so dragging keeps working when the
            // pointer leaves the bar; release (or a move without the button) ends them.
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    if this.panel_resizing.is_some() {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.update_panel_resize(event.position.x, window, cx);
                        } else {
                            this.finish_panel_resize(cx);
                        }
                    }
                    if this.scrubbing.is_some() {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.update_scrub(event.position.x, cx);
                        } else {
                            this.finish_scrub(event.position.x, cx);
                        }
                    }
                    if this.shelf_scrub.is_some() {
                        if event.pressed_button == Some(gpui::MouseButton::Left) {
                            this.update_shelf_scrub(event.position.x);
                        } else {
                            this.shelf_scrub = None;
                        }
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _window, cx| {
                    this.finish_panel_resize(cx);
                    this.finish_scrub(event.position.x, cx);
                    this.shelf_scrub = None;
                }),
            )
            .on_action(
                cx.listener(|this, _: &TogglePlayback, _window, cx| this.toggle_playback(cx)),
            )
            .on_action(cx.listener(|this, _: &MoveUp, _window, cx| {
                let count = this.take_count();
                this.move_selection(-count, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveDown, _window, cx| {
                let count = this.take_count();
                this.move_selection(count, cx)
            }))
            .on_action(
                cx.listener(|this, _: &PageUp, _window, cx| this.move_selection(-PAGE_ROWS, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _window, cx| this.move_selection(PAGE_ROWS, cx)),
            )
            // Takes precedence over the app-level `Quit` handler registered in `main`, which
            // cannot see the window, so the ctrl-q binding remembers the window rectangle the
            // way the platform's own close request does. The Linux caption button's close
            // dispatches `CloseWindow` instead: the same tray-or-quit decision as that request.
            .on_action(cx.listener(|this, _: &Quit, window, cx| {
                this.persist_window_bounds(window);
                cx.quit();
            }))
            .on_action(cx.listener(|this, _: &CloseWindow, window, cx| {
                if this.on_close_request(window, cx) {
                    window.remove_window();
                }
            }))
            // The macOS Window menu's two entries. Handled here rather than app-wide because
            // both need the window; `Hide`/`HideOthers`/`ShowAll` do not, so those stay in
            // `main` alongside the menu itself.
            .on_action(cx.listener(|_, _: &MinimizeWindow, window, _cx| window.minimize_window()))
            .on_action(cx.listener(|_, _: &ZoomWindow, window, _cx| window.zoom_window()))
            .on_action(cx.listener(|this, _: &SelectFirst, _window, cx| this.set_selection(0, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _window, cx| this.select_last(cx)))
            .on_action(cx.listener(|this, _: &Activate, _window, cx| this.activate_selection(cx)))
            .on_action(cx.listener(|this, _: &ConfirmPrompt, _window, cx| this.confirm_prompt(cx)))
            .on_action(cx.listener(|this, _: &AddToPlaylist, _window, cx| this.add_to_playlist(cx)))
            .on_action(
                cx.listener(|this, _: &OpenActionMenu, _window, cx| this.open_action_menu(cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleSettings, _window, cx| this.toggle_settings(cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleHelp, _window, cx| {
                this.help_open = !this.help_open;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MarkDelete, _window, cx| {
                this.pending_count = None;
                if this.multi_selected() {
                    echo_core::intent::delete_visual_selection(&mut this.state);
                } else {
                    echo_core::intent::mark_selected_for_delete(&mut this.state);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EnterVisual, _window, cx| this.toggle_visual(cx)))
            .on_action(cx.listener(|this, _: &MoveTrackUp, _window, cx| {
                let count = this.take_count();
                this.move_selected_track(-count, cx)
            }))
            .on_action(cx.listener(|this, _: &MoveTrackDown, _window, cx| {
                let count = this.take_count();
                this.move_selected_track(count, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleLike, _window, cx| this.toggle_like(cx)))
            .on_action(cx.listener(|this, _: &FocusLibrary, _window, cx| this.focus_library(cx)))
            .on_action(cx.listener(|this, _: &FocusTracks, _window, cx| this.focus_tracks(cx)))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _window, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &HistoryBack, _window, cx| {
                // Global binding, so keep the view still while an overlay is on top.
                if !this.overlay_open() {
                    this.history_back(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &HistoryForward, _window, cx| {
                if !this.overlay_open() {
                    this.history_forward(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &NextTrack, _window, cx| this.play_next(cx)))
            .on_action(cx.listener(|this, _: &PreviousTrack, _window, cx| this.play_previous(cx)))
            .on_action(cx.listener(|this, _: &ToggleShuffle, _window, cx| this.toggle_shuffle(cx)))
            .on_action(cx.listener(|this, _: &CycleRepeat, _window, cx| this.cycle_repeat(cx)))
            .on_action(cx.listener(|this, _: &ToggleMute, _window, cx| this.toggle_mute(cx)))
            .on_action(cx.listener(|this, _: &SeekForward, _window, cx| this.seek_relative(5, cx)))
            .on_action(
                cx.listener(|this, _: &SeekBackward, _window, cx| this.seek_relative(-5, cx)),
            )
            .on_action(cx.listener(|this, _: &ToggleQueue, _window, cx| this.toggle_queue_view(cx)))
            .on_action(cx.listener(|this, _: &OpenDevices, _window, cx| this.open_devices(cx)))
            .on_action(cx.listener(|this, _: &Dismiss, _window, cx| this.dismiss(cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                window.focus(&this.search_focus, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleLyrics, _window, cx| this.toggle_lyrics(cx)))
            .on_action(cx.listener(|this, _: &ToggleThemes, _window, cx| this.toggle_themes(cx)))
            .on_action(
                cx.listener(|this, _: &ToggleImmersive, _window, cx| this.toggle_immersive(cx)),
            )
            .on_action(cx.listener(|this, _: &HalfPageUp, _window, cx| {
                this.move_selection(-PAGE_ROWS / 2, cx)
            }))
            .on_action(cx.listener(|this, _: &HalfPageDown, _window, cx| {
                this.move_selection(PAGE_ROWS / 2, cx)
            }))
            // `0` extends a pending count (`10j`); bare `0` keeps its seek-to-start role.
            .on_action(cx.listener(|this, _: &SeekStart, _window, cx| {
                if let Some(count) = this.pending_count {
                    this.pending_count = Some(count.saturating_mul(10).min(9999));
                    cx.notify();
                } else {
                    this.seek_start(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &VolumeUp, _window, cx| this.adjust_volume(1, cx)))
            .on_action(cx.listener(|this, _: &VolumeDown, _window, cx| this.adjust_volume(-1, cx)))
            .on_action(cx.listener(|this, _: &VolumeUpBig, _window, cx| this.adjust_volume(5, cx)))
            .on_action(
                cx.listener(|this, _: &VolumeDownBig, _window, cx| this.adjust_volume(-5, cx)),
            )
            .on_action(cx.listener(|this, _: &JumpToCurrent, _window, cx| this.jump_to_current(cx)))
            .on_action(
                cx.listener(|this, _: &OpenCommand, window, cx| this.open_command("", window, cx)),
            )
            .on_action(cx.listener(|this, _: &CommandSearch, window, cx| {
                this.open_command("search ", window, cx)
            }))
            .on_action(cx.listener(|this, _: &NewPlaylistPrompt, window, cx| {
                if this.state.ui.active_view == ActiveView::Library {
                    this.open_command("newplaylist ", window, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &RenamePrompt, window, cx| this.rename_prompt(window, cx)),
            )
            .on_action(cx.listener(|this, _: &AddToQueue, _window, cx| this.add_to_queue(cx)))
            .on_action(cx.listener(|this, _: &TogglePin, _window, cx| this.toggle_pin(cx)))
            .on_action(cx.listener(|this, _: &CycleTab, _window, cx| this.cycle_tab(cx)))
            .on_action(cx.listener(|this, _: &Refresh, _window, cx| this.refresh_view(cx)))
            .on_action(cx.listener(|this, _: &OpenHome, window, cx| {
                this.open_home(cx);
                window.focus(&this.focus_handle, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenFilter, window, cx| this.open_filter(window, cx)))
            .on_action(cx.listener(|this, _: &NextMatch, _window, cx| this.next_match(true, cx)))
            .on_action(cx.listener(|this, _: &PrevMatch, _window, cx| this.next_match(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleInlineLyrics, _window, cx| {
                this.toggle_inline_lyrics(cx)
            }))
            .on_action(cx.listener(|this, _: &BackOrFocusLibrary, _window, cx| {
                this.back_or_focus_library(cx)
            }))
            // Digits are unbound, so they fall through the key bindings to this listener and
            // accumulate a vim-style count for j/k. Only while the list itself has focus —
            // digits typed into the search/command inputs must not count.
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.handle_playlist_input(event, window, cx) {
                    return;
                }
                if !this.focus_handle.is_focused(window) {
                    return;
                }
                let keystroke = &event.keystroke;
                if keystroke.modifiers.control
                    || keystroke.modifiers.alt
                    || keystroke.modifiers.platform
                    || keystroke.modifiers.shift
                {
                    return;
                }
                match keystroke.key.as_str() {
                    digit @ ("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
                        let digit = (digit.as_bytes()[0] - b'0') as usize;
                        let current = this.pending_count.unwrap_or(0);
                        this.pending_count =
                            Some(current.saturating_mul(10).saturating_add(digit).min(9999));
                        cx.notify();
                    }
                    // Any other unbound key cancels the pending count.
                    _ => {
                        if this.pending_count.take().is_some() {
                            cx.notify();
                        }
                    }
                }
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(backdrop.as_ref().map_or(bg, |b| b.colors.background))
            // The window's backdrop: the one fill covering every corner, so it rounds all four.
            .map(|el| views::round_client_corners(el, corners, views::ClientCorners::All))
            .when_some(backdrop.as_ref(), |el, backdrop| {
                el.child(views::backdrop_layer(backdrop.clone(), phase, corners))
            })
            .when_some(titlebar, |el, bar| el.child(bar))
            .map(|el| {
                if let Some(backdrop) = &backdrop {
                    el.child(views::immersive_view(self, backdrop, window, cx))
                } else {
                    el.child(
                        div()
                            .flex_grow(1.0)
                            .flex()
                            .flex_row()
                            .overflow_hidden()
                            .when(!self.sidebar_collapsed, |el| {
                                el.child(self.regions.sidebar(self.sidebar_width))
                            })
                            .child(self.regions.main())
                            .when(self.right_panel.is_some(), |el| {
                                let width = self.effective_right_panel_width(window);
                                el.child(self.regions.right_panel(width))
                            }),
                    )
                    .when_some(status_line, |el, line| el.child(line))
                }
            })
            .when_some(command_bar, |el, bar| el.child(bar))
            .when_some(audio_banner, |el, banner| el.child(banner))
            .when_some(update_banner, |el, banner| el.child(banner))
            .when(!self.immersive, |el| el.child(self.regions.playback_bar()))
            .when_some(theme_modal, |el, modal| el.child(modal))
            .when_some(device_modal, |el, modal| el.child(modal))
            .when_some(playlist_add_modal, |el, modal| el.child(modal))
            .when_some(sort_menu, |el, menu| el.child(menu))
            .when_some(settings_modal, |el, modal| el.child(modal))
            .when_some(help_modal, |el, modal| el.child(modal))
            .when_some(context_menu, |el, menu| el.child(menu))
            .when_some(track_menu, |el, menu| el.child(menu))
            .when_some(prompt_modal, |el, modal| el.child(modal))
            .when(self.page_menu.is_some(), |el| {
                el.child(views::playlist_page_menu(self, cx))
            })
            .when(self.state.ui.playlist_edit.is_some(), |el| {
                el.child(views::playlist_edit_modal(self, cx))
            });

        // Adds the border and resize edges when the compositor refuses to draw them; a
        // pass-through when it does.
        views::window_frame(root, window, frame_palette)
    }
}
