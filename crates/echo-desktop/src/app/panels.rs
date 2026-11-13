use super::*;

impl EchoApp {
    /// Opening the queue leaves the immersive view, which has nowhere to show it.
    /// `shift-Q`: the full-page, keyboard-navigable queue view.
    pub(crate) fn toggle_queue_view(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.active_view == ActiveView::Queue {
            // `q` from the queue view returns to the library.
            self.state.ui.active_view = ActiveView::Library;
        } else {
            let event = echo_core::intent::open_queue(&mut self.state);
            self.dispatch(event);
            self.immersive = false;
        }
        cx.notify();
    }

    /// Opens, closes or swaps the docked panel. Opening the queue fetches it once; the core's
    /// `queue_visible` keeps it fresh from there.
    pub(crate) fn set_right_panel(&mut self, panel: Option<RightPanel>, cx: &mut Context<Self>) {
        if self.panel_resizing.is_some_and(|resize| resize.right) {
            self.finish_panel_resize(cx);
        }
        self.right_panel = panel;
        self.state.ui.library_config.right_panel = panel;
        self.state.save_library_config();
        let queue_open = panel == Some(RightPanel::Queue);
        if queue_open && !self.state.ui.queue_panel_open {
            self.dispatch(AppEvent::FetchQueue);
        }
        self.state.ui.queue_panel_open = queue_open;
        cx.notify();
    }

    pub(crate) fn toggle_right_panel(&mut self, panel: RightPanel, cx: &mut Context<Self>) {
        let next = (self.right_panel != Some(panel)).then_some(panel);
        self.set_right_panel(next, cx);
    }

    pub(crate) fn open_devices(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::open_device_picker(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn toggle_lyrics(&mut self, cx: &mut Context<Self>) {
        self.toggle_right_panel(RightPanel::Lyrics, cx);
    }

    pub(crate) fn toggle_immersive(&mut self, cx: &mut Context<Self>) {
        self.immersive = !self.immersive;
        cx.notify();
    }

    /// Escape / `h` / backspace: close whatever is topmost, else go back, checking modals first.
    pub(crate) fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.playlist_edit.is_some() {
            echo_core::intent::cancel_playlist_edit(&mut self.state);
            cx.notify();
            return;
        }
        if self.page_menu.take().is_some() {
            cx.notify();
            return;
        }
        self.pending_count = None;
        self.state.ui.pending_d_press = false;
        if echo_core::intent::prompt_active(&self.state) {
            echo_core::intent::cancel_prompt(&mut self.state);
        } else if self.multi_selected() {
            // Escape drops the range before it starts walking the view history.
            echo_core::intent::exit_visual(&mut self.state);
        } else if self.context_menu.is_some() {
            self.context_menu = None;
        } else if self
            .track_menu
            .as_ref()
            .is_some_and(|menu| menu.submenu.is_some())
        {
            // The flyout is the topmost layer: escape steps back to the menu items.
            self.close_playlist_submenu(cx);
        } else if self.track_menu.is_some() {
            self.track_menu = None;
            self.state.ui.playlist_add_filter.clear();
        } else if self.state.ui.playlist_add_modal_open {
            echo_core::action_menu::cancel_playlist_add(&mut self.state);
        } else if self.state.ui.device_modal_open {
            self.state.ui.device_modal_open = false;
        } else if self.theme_modal_open {
            self.theme_modal_open = false;
        } else if self.sort_menu_open {
            self.sort_menu_open = false;
        } else if self.settings_open {
            // An open dropdown is the topmost layer of the sheet.
            if self.settings_dropdown.take().is_none() {
                self.settings_open = false;
            }
        } else if self.help_open {
            self.help_open = false;
        } else if self.immersive {
            self.immersive = false;
        } else if self.history_back(cx) {
        } else if self.state.ui.active_view == ActiveView::TrackList {
            self.state.ui.active_view = if self.search_has_results() {
                ActiveView::SearchResults
            } else {
                ActiveView::Library
            };
        } else if self.state.ui.active_view == ActiveView::ArtistPage {
            if self.search_has_results() {
                self.state.ui.active_view = ActiveView::SearchResults;
                self.state.clear_pending_artist_page();
                self.dispatch(AppEvent::CancelArtistPageLoad);
            } else {
                self.close_artist_page(cx);
            }
        } else if self.state.ui.active_view == ActiveView::SearchResults {
            // Leaving search results drops them entirely.
            self.state.ui.active_view = ActiveView::Library;
            self.state.data.search_results = Default::default();
            self.state.ui.search_context_query.clear();
            self.state.ui.status_message = None;
            if self.state.data.tracklist_image_url.is_some() {
                self.dispatch(AppEvent::ReloadHeaderImage);
            }
        } else if self.state.ui.active_view == ActiveView::Queue {
            self.state.ui.active_view = ActiveView::Library;
            if self.state.data.tracklist_image_url.is_some() {
                self.dispatch(AppEvent::ReloadHeaderImage);
            }
        }
        cx.notify();
    }

    fn search_has_results(&self) -> bool {
        !self.state.data.search_results.tracks.is_empty()
            || !self.state.data.search_results.albums.is_empty()
            || !self.state.data.search_results.artists.is_empty()
    }

    pub(crate) fn overlay_open(&self) -> bool {
        if self.state.ui.playlist_edit.is_some() || self.page_menu.is_some() {
            return true;
        }
        echo_core::intent::prompt_active(&self.state)
            || self.context_menu.is_some()
            || self.track_menu.is_some()
            || self.state.ui.playlist_add_modal_open
            || self.state.ui.device_modal_open
            || self.theme_modal_open
            || self.sort_menu_open
            || self.settings_open
            || self.help_open
    }

    pub(crate) fn toggle_inline_lyrics(&mut self, cx: &mut Context<Self>) {
        echo_core::intent::toggle_condensed_lyrics(&mut self.state);
        cx.notify();
    }

    fn width_beside_library(&self, window: &Window) -> f32 {
        (views::client_content_width(window)
            - if self.sidebar_collapsed {
                0.0
            } else {
                self.sidebar_width
            })
        .max(0.0)
    }

    pub(crate) fn effective_right_panel_width(&self, window: &Window) -> f32 {
        views::panel_layout::fitted_width(self.right_panel_width, self.width_beside_library(window))
    }

    pub(crate) fn main_content_width(&self, window: &Window) -> f32 {
        (self.width_beside_library(window)
            - if self.right_panel.is_some() {
                self.effective_right_panel_width(window)
            } else {
                0.0
            })
        .max(0.0)
    }

    /// Which pane edge, if any, is being dragged.
    pub(crate) fn resizing_edge(&self) -> Option<views::ResizeEdge> {
        self.panel_resizing.map(|resize| {
            if resize.right {
                views::ResizeEdge::RightPanel
            } else {
                views::ResizeEdge::Sidebar
            }
        })
    }

    pub(crate) fn begin_sidebar_resize(&mut self, x: Pixels, cx: &mut Context<Self>) {
        self.panel_resizing = Some(PanelResize {
            start_x: x,
            start_width: self.sidebar_width,
            right: false,
        });
        cx.notify();
    }

    pub(crate) fn begin_right_panel_resize(
        &mut self,
        x: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.panel_resizing = Some(PanelResize {
            start_x: x,
            start_width: self.effective_right_panel_width(window),
            right: true,
        });
        cx.notify();
    }

    pub(crate) fn update_panel_resize(
        &mut self,
        x: Pixels,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(resize) = self.panel_resizing else {
            return;
        };
        let delta = x - resize.start_x;
        if resize.right {
            self.right_panel_width = views::panel_layout::fitted_width(
                resize.start_width - f32::from(delta),
                self.width_beside_library(window),
            );
        } else {
            self.sidebar_width = (resize.start_width + f32::from(delta)).clamp(180.0, 480.0);
        }
        cx.notify();
    }

    pub(crate) fn finish_panel_resize(&mut self, cx: &mut Context<Self>) {
        if let Some(resize) = self.panel_resizing.take() {
            // Saved on release rather than on every pointer move — a drag is one decision, not
            // a few hundred config writes.
            if resize.right {
                self.state.ui.library_config.right_panel_width = Some(self.right_panel_width);
            } else {
                self.state.ui.library_config.sidebar_width = Some(self.sidebar_width);
            }
            self.state.save_library_config();
            cx.notify();
        }
    }

    pub(crate) fn set_sidebar_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        self.sidebar_collapsed = collapsed;
        self.state.ui.library_config.sidebar_collapsed = Some(collapsed);
        self.state.save_library_config();
        cx.notify();
    }

    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.set_sidebar_collapsed(!self.sidebar_collapsed, cx);
    }
}
