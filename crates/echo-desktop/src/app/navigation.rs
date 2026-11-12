use super::*;

impl EchoApp {
    /// Opens an artist page directly by id — the per-artist click path in track rows.
    pub(crate) fn open_artist(
        &mut self,
        artist_id: String,
        artist_name: String,
        cx: &mut Context<Self>,
    ) {
        self.state
            .begin_artist_page_load(artist_id.clone(), artist_name.clone(), None);
        self.dispatch(AppEvent::LoadArtistPage {
            artist_id,
            artist_name: Some(artist_name),
            artist_image_url: None,
        });
        cx.notify();
    }

    /// Runs one go-to action (album/artist) for a track context — the click-to-navigate path
    /// shared by track-row cells and the playback bar.
    pub(crate) fn go_to(
        &mut self,
        ctx: echo_core::models::ActionMenuContext,
        action: echo_core::models::ActionMenuAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(event) = echo_core::action_menu::run(&mut self.state, ctx, action) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Backspace: close the topmost overlay if one is open, else hand keyboard focus back to the
    /// library sidebar while keeping the current page visible, without walking the view history
    /// the way `h`/escape do.
    pub(crate) fn back_or_focus_library(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if self.overlay_open() {
            self.dismiss(cx);
        } else {
            self.focus_library(cx);
        }
    }

    pub(crate) fn refresh_view(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.active_view == ActiveView::Home {
            for event in echo_core::intent::refresh_home(&mut self.state) {
                self.dispatch(event);
            }
            cx.notify();
            return;
        }
        if let Some(event) = echo_core::intent::refresh_view(&mut self.state) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Tab: cycle the tabs of whichever tabbed view is active. The desktop has no Browse
    /// library tab — those entries live in the sidebar — so the library just flips between
    /// Playlists and Albums.
    pub(crate) fn cycle_tab(&mut self, cx: &mut Context<Self>) {
        self.state.ui.pending_d_press = false;
        if self.theme_modal_open {
            // Under `Auto` the picker has a dark and a light list; a pinned mode has one.
            if self.state.ui.library_config.theme_mode == Some(echo_core::config::ThemeMode::Auto) {
                self.show_theme_list(self.theme_modal_tab.other(), cx);
            }
            return;
        }
        match self.state.ui.active_view {
            ActiveView::SearchResults => {
                self.state.ui.active_search_tab = match self.state.ui.active_search_tab {
                    SearchTab::All => SearchTab::Tracks,
                    SearchTab::Tracks => SearchTab::Albums,
                    SearchTab::Albums => SearchTab::Artists,
                    SearchTab::Artists => SearchTab::Playlists,
                    SearchTab::Playlists => SearchTab::All,
                };
                self.state.ui.selected_search_index = 0;
            }
            ActiveView::Library => {
                self.state.ui.active_library_tab = match self.state.ui.active_library_tab {
                    LibraryTab::Playlists => LibraryTab::Albums,
                    LibraryTab::Albums => LibraryTab::Artists,
                    _ => LibraryTab::Playlists,
                };
                self.state.ui.selected_playlist_index = 0;
                if self.state.ui.active_library_tab == LibraryTab::Artists
                    && self.state.data.followed_artists.is_empty()
                {
                    self.dispatch(AppEvent::FetchFollowedArtists);
                }
            }
            ActiveView::Queue => echo_core::intent::cycle_queue_tab(&mut self.state),
            ActiveView::ArtistPage => {
                self.set_discography_filter(self.state.ui.artist_discography_filter.next())
            }
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn open_home(&mut self, cx: &mut Context<Self>) {
        self.immersive = false;
        for event in echo_core::intent::open_home(&mut self.state) {
            self.dispatch(event);
        }
        cx.notify();
    }

    pub(crate) fn focus_library(&mut self, cx: &mut Context<Self>) {
        // With a menu open the arrows belong to it: left closes the add-to-playlist flyout
        // rather than moving pane focus behind the overlay.
        if self.track_menu.is_some() {
            self.close_playlist_submenu(cx);
            return;
        }
        self.state.ui.pending_d_press = false;
        if self.sidebar_collapsed {
            // Focusing an invisible pane would strand the cursor; bring the sidebar back.
            self.set_sidebar_collapsed(false, cx);
        }
        self.state.ui.active_view = ActiveView::Library;
        cx.notify();
    }

    pub(crate) fn focus_tracks(&mut self, cx: &mut Context<Self>) {
        // Right steps into the flyout when the menu's "Add to playlist" row is selected.
        if self.track_menu.is_some() {
            if self.track_menu_add_row() == self.track_menu.as_ref().map(|menu| menu.selected) {
                self.open_playlist_submenu(true, cx);
            }
            return;
        }
        self.state.ui.pending_d_press = false;
        // Nothing to focus before a context has been opened.
        if !self.state.data.tracks.is_empty() || self.state.data.active_tracklist_context.is_some()
        {
            self.state.ui.active_view = ActiveView::TrackList;
            cx.notify();
        }
    }

    pub(crate) fn history_back(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.state.pop_view_history() {
            return false;
        }
        self.state.clear_pending_artist_page();
        if self.state.data.tracklist_image_url.is_some() {
            self.dispatch(AppEvent::ReloadHeaderImage);
        }
        cx.notify();
        true
    }

    pub(crate) fn history_forward(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.state.forward_view_history() {
            return false;
        }
        self.state.clear_pending_artist_page();
        if self.state.data.tracklist_image_url.is_some() {
            self.dispatch(AppEvent::ReloadHeaderImage);
        }
        cx.notify();
        true
    }

    pub(crate) fn close_artist_page(&mut self, cx: &mut Context<Self>) {
        self.state.ui.active_view = ActiveView::Library;
        self.state.clear_pending_artist_page();
        self.dispatch(AppEvent::CancelArtistPageLoad);
        cx.notify();
    }
}
