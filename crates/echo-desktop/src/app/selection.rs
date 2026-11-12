use super::*;

/// How close to a list edge, in pixels, a drag starts scrolling it.
pub(crate) const DRAG_EDGE: f32 = 48.0;

/// Pixels per frame at the very edge.
pub(crate) const DRAG_SCROLL_MAX: f32 = 14.0;

impl EchoApp {
    /// Rows in whatever currently has keyboard focus: the device modal when open, else the
    /// `active_view` list.
    fn list_len(&self) -> usize {
        if self.state.ui.playlist_edit.is_some() {
            return 0;
        }
        if self.page_menu.is_some() {
            return echo_core::intent::playlist_page_actions(&self.state).len();
        }
        if self.state.ui.playlist_add_modal_open {
            return echo_core::action_menu::filtered_playlist_add_choices(&self.state).len() + 1;
        }
        if self.state.ui.device_modal_open {
            return self.state.data.devices.len();
        }
        if self.theme_modal_open {
            return views::theme_rows(&self.state, self.theme_modal_tab).len();
        }
        if let Some(menu) = self.track_menu.as_ref() {
            // The flyout takes over navigation while it is open, exactly like a nested list.
            if menu.submenu.is_some() {
                return echo_core::action_menu::filtered_playlist_add_choices(&self.state).len()
                    + 1;
            }
            return views::track_menu_items(self).len();
        }
        if self.sort_menu_open {
            return views::SORT_OPTIONS.len();
        }
        match self.state.ui.active_view {
            ActiveView::Home => echo_core::home::home_shelves(&self.state)
                .iter()
                .map(|shelf| shelf.items.len())
                .sum(),
            ActiveView::TrackList => self.state.data.tracks.len(),
            ActiveView::Queue => self.state.queue_view_len(),
            ActiveView::SearchResults => match self.state.ui.active_search_tab {
                SearchTab::All => echo_core::search::all_tab_rows(
                    &self.state.data.search_results,
                    &self.state.ui.search_context_query,
                )
                .len(),
                echo_core::app::SearchTab::Tracks => self.state.data.search_results.tracks.len(),
                echo_core::app::SearchTab::Albums => self.state.data.search_results.albums.len(),
                echo_core::app::SearchTab::Artists => self.state.data.search_results.artists.len(),
                echo_core::app::SearchTab::Playlists => {
                    self.state.data.search_results.playlists.len()
                }
            },
            ActiveView::ArtistList => self.state.artist_list().len(),
            ActiveView::WhatsNew => self.state.data.whats_new.len(),
            // Combined index space: Popular rows first, then the album rows and the chip's
            // Load more row.
            ActiveView::ArtistPage => {
                let filter = self.state.ui.artist_discography_filter;
                self.state.data.artist_page_data.as_ref().map_or(0, |data| {
                    data.top_tracks.len() + data.album_row_count(filter)
                })
            }
            _ => match self.state.ui.active_library_tab {
                LibraryTab::Albums => self.state.data.saved_albums.len(),
                LibraryTab::Artists => self.state.data.followed_artists.len(),
                _ => self.state.data.library_view.len(),
            },
        }
    }

    /// Switches the discography chip and lands the cursor on the first album row, past the
    /// Popular rows the combined index space puts first.
    pub(crate) fn set_discography_filter(&mut self, filter: echo_core::models::DiscographyFilter) {
        if let Some(event) = echo_core::intent::set_discography_filter(&mut self.state, filter) {
            self.dispatch(event);
        }
        self.state.ui.artist_page_album_index = self.artist_page_top_len();
        self.artist_albums_scroll
            .scroll_to_item(0, ScrollStrategy::Top);
    }

    /// Rows the artist page's Popular section occupies at the head of its combined index space.
    pub(crate) fn artist_page_top_len(&self) -> usize {
        self.state
            .data
            .artist_page_data
            .as_ref()
            .map_or(0, |data| data.top_tracks.len())
    }

    pub(crate) fn set_selection(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.state.ui.playlist_edit.is_some() {
            return;
        }
        if let Some((_, selected)) = self.page_menu.as_mut() {
            *selected = index;
            cx.notify();
            return;
        }
        // A confirm prompt is modal: moving the list behind it would leave the prompt
        // describing a row that is no longer selected.
        if echo_core::intent::prompt_active(&self.state) {
            return;
        }
        // An armed `d` belongs to the row it was pressed on; moving off that row disarms it so
        // the second `d` can't delete something the user never pointed at.
        self.state.ui.pending_d_press = false;
        echo_core::intent::clear_picks(&mut self.state);
        if self.state.ui.playlist_add_modal_open {
            self.state.ui.selected_playlist_modal_index = index;
            self.playlist_modal_scroll.scroll_to_item(index);
        } else if self.state.ui.device_modal_open {
            self.state.ui.selected_device_index = index;
            self.device_modal_scroll.scroll_to_item(index);
        } else if self.theme_modal_open {
            self.theme_modal_index = index;
            self.theme_modal_scroll.scroll_to_item(index);
        } else if let Some(menu) = self.track_menu.as_mut() {
            if menu.submenu.is_some() {
                menu.submenu = Some(index);
                self.submenu_scroll.scroll_to_item(index);
            } else {
                menu.selected = index;
                self.track_menu_scroll.scroll_to_item(index);
            }
        } else if self.sort_menu_open {
            self.sort_menu_index = index;
        } else {
            match self.state.ui.active_view {
                ActiveView::Home => {
                    self.state.ui.selected_home_index = index;
                    let shelves = echo_core::home::home_shelves(&self.state);
                    let mut start = 0;
                    for shelf in &shelves {
                        if index < start + shelf.items.len() {
                            if let Some(section) = self.home_section_indices.get(&shelf.kind) {
                                self.home_scroll.scroll_to_item(*section);
                            }
                            if let Some(scroll) = self.home_shelf_scrolls.get(&shelf.kind) {
                                scroll.scroll_to_item(index - start);
                            }
                            break;
                        }
                        start += shelf.items.len();
                    }
                }
                ActiveView::TrackList => {
                    self.state.ui.selected_track_index = index;
                    self.tracks_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::Queue => {
                    self.state.ui.selected_queue_index = index;
                    let row = match self.state.ui.queue_tab {
                        echo_core::app::QueueTab::Queue => {
                            self.state.queue_row_of(index).unwrap_or(index)
                        }
                        echo_core::app::QueueTab::Recent => index,
                    };
                    self.queue_scroll
                        .scroll_to_item(row, ScrollStrategy::Nearest);
                }
                ActiveView::SearchResults => {
                    self.state.ui.selected_search_index = index;
                    if self.state.ui.active_search_tab == SearchTab::All {
                        if let Some((section, horizontal)) = self.search_all_positions.get(index) {
                            self.search_all_scroll.scroll_to_item(*section);
                            if let Some((tab, row)) = horizontal
                                && let Some(scroll) = self.search_section_scrolls.get(tab)
                            {
                                scroll.scroll_to_item(*row);
                            }
                        }
                    } else {
                        self.search_scroll
                            .scroll_to_item(index, ScrollStrategy::Nearest);
                    }
                }
                ActiveView::ArtistList => {
                    self.state.ui.selected_artist_index = index;
                    self.artist_list_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::WhatsNew => {
                    self.state.ui.selected_whats_new_index = index;
                    self.whats_new_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
                ActiveView::ArtistPage => {
                    self.state.ui.artist_page_album_index = index;
                    // The combined index spans two lists; scroll whichever owns the row.
                    let top_len = self.artist_page_top_len();
                    if index < top_len {
                        self.artist_top_tracks_scroll
                            .scroll_to_item(index, ScrollStrategy::Nearest);
                    } else {
                        self.artist_albums_scroll
                            .scroll_to_item(index - top_len, ScrollStrategy::Nearest);
                    }
                }
                _ => {
                    self.state.ui.selected_playlist_index = index;
                    self.library_scroll
                        .scroll_to_item(index, ScrollStrategy::Nearest);
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = self.list_len();
        if len == 0 {
            return;
        }
        let current = if let Some((_, selected)) = self.page_menu {
            selected
        } else if self.state.ui.playlist_add_modal_open {
            self.state.ui.selected_playlist_modal_index
        } else if self.state.ui.device_modal_open {
            self.state.ui.selected_device_index
        } else if self.theme_modal_open {
            self.theme_modal_index
        } else if let Some(menu) = self.track_menu.as_ref() {
            menu.submenu.unwrap_or(menu.selected)
        } else if self.sort_menu_open {
            self.sort_menu_index
        } else {
            match self.state.ui.active_view {
                ActiveView::Home => self.state.ui.selected_home_index,
                ActiveView::TrackList => self.state.ui.selected_track_index,
                ActiveView::Queue => self.state.ui.selected_queue_index,
                ActiveView::SearchResults => self.state.ui.selected_search_index,
                ActiveView::ArtistList => self.state.ui.selected_artist_index,
                ActiveView::ArtistPage => self.state.ui.artist_page_album_index,
                ActiveView::WhatsNew => self.state.ui.selected_whats_new_index,
                _ => self.state.ui.selected_playlist_index,
            }
        };
        self.set_selection(current.saturating_add_signed(delta).min(len - 1), cx);
    }

    /// `l`: like/unlike the focused track. On library rows it acts as Enter; un-liking stages
    /// the confirm prompt.
    pub(crate) fn toggle_like(&mut self, cx: &mut Context<Self>) {
        if self.overlay_open() {
            return;
        }
        if self.state.ui.active_view == ActiveView::Library {
            self.activate_selection(cx);
            return;
        }
        if let Some(event) = echo_core::intent::toggle_like_selected(&mut self.state) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// `shift-j`/`shift-k`: move the selected track within an owned playlist. The intent
    /// enforces the writable-context and original-order guards.
    pub(crate) fn move_selected_track(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.overlay_open() || self.state.ui.active_view != ActiveView::TrackList {
            return;
        }
        let from = self.state.ui.selected_track_index;
        let to = from.saturating_add_signed(delta);
        if let Some(event) = echo_core::intent::move_track_in_playlist(&mut self.state, from, to) {
            self.dispatch(event);
            self.tracks_scroll
                .scroll_to_item(self.state.ui.selected_track_index, ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    pub(crate) fn select_last(&mut self, cx: &mut Context<Self>) {
        let len = self.list_len();
        if len > 0 {
            self.set_selection(len - 1, cx);
        }
    }

    /// Accept an open confirm prompt, mirroring the Confirm button in `views::prompt_modal`.
    /// Bound to Enter (through [`Self::activate_selection`]) and to `y`.
    pub(crate) fn confirm_prompt(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if !echo_core::intent::prompt_active(&self.state) {
            return;
        }
        if let Some(event) = echo_core::intent::confirm_prompt(&mut self.state) {
            self.dispatch(event);
        }
        // A prompt staged from a visual range consumes it, so the range goes away with it.
        echo_core::intent::exit_visual(&mut self.state);
        cx.notify();
    }

    /// Enter on the focused list — the same intents the row click handlers use.
    pub(crate) fn activate_selection(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.playlist_edit.is_some() {
            self.save_playlist_edit(cx);
            return;
        }
        if let Some((_, selected)) = self.page_menu {
            if let Some(action) =
                echo_core::intent::playlist_page_actions(&self.state).get(selected)
            {
                self.run_page_action(*action, cx);
            }
            return;
        }
        // A confirm prompt sits above everything else, exactly as it does in `dismiss`.
        if echo_core::intent::prompt_active(&self.state) {
            self.confirm_prompt(cx);
            return;
        }
        self.pending_count = None;
        let event = if self.state.ui.playlist_add_modal_open {
            let index = self.state.ui.selected_playlist_modal_index;
            echo_core::action_menu::commit_filtered_playlist_add(&mut self.state, index)
        } else if self.state.ui.device_modal_open {
            let index = self.state.ui.selected_device_index;
            echo_core::intent::transfer_to_device(&mut self.state, index)
        } else if self.theme_modal_open {
            if let Some(row) =
                views::theme_rows(&self.state, self.theme_modal_tab).get(self.theme_modal_index)
            {
                echo_core::intent::apply_theme(&mut self.state, &row.name);
            }
            self.theme_modal_open = false;
            None
        } else if let Some(menu) = self.track_menu.as_ref() {
            if let Some(choice) = menu.submenu {
                self.commit_playlist_submenu(choice, cx);
            } else {
                let selected = menu.selected;
                // Enter on "Add to playlist" steps into the flyout instead of running an
                // action; every other item runs and closes the menu.
                if self.track_menu_add_row() == Some(selected) {
                    self.open_playlist_submenu(true, cx);
                } else if let Some((_, item, _)) =
                    views::track_menu_items(self).into_iter().nth(selected)
                {
                    self.run_track_menu_action(item, cx);
                }
            }
            None
        } else if self.sort_menu_open {
            if let Some((_, arg)) = views::SORT_OPTIONS.get(self.sort_menu_index) {
                self.apply_sort(arg, cx);
            }
            None
        } else {
            match self.state.ui.active_view {
                ActiveView::Home => {
                    let item = echo_core::home::home_shelves(&self.state)
                        .into_iter()
                        .flat_map(|shelf| shelf.items)
                        .nth(self.state.ui.selected_home_index);
                    item.and_then(|item| echo_core::intent::open_home_item(&mut self.state, item))
                }
                ActiveView::TrackList => {
                    let index = self.state.ui.selected_track_index;
                    echo_core::intent::play_track_at(&mut self.state, index)
                }
                ActiveView::Queue => {
                    let index = self.state.ui.selected_queue_index;
                    echo_core::intent::play_queue_track_at(&mut self.state, index)
                }
                ActiveView::SearchResults => {
                    let index = self.state.ui.selected_search_index;
                    echo_core::intent::activate_search_result(&mut self.state, index)
                }
                ActiveView::ArtistList => {
                    let index = self.state.ui.selected_artist_index;
                    echo_core::intent::open_artist_at(&mut self.state, index)
                }
                ActiveView::ArtistPage => {
                    let index = self.state.ui.artist_page_album_index;
                    echo_core::intent::activate_artist_page_row(&mut self.state, index)
                }
                ActiveView::WhatsNew => {
                    let index = self.state.ui.selected_whats_new_index;
                    echo_core::intent::open_whats_new_album(&mut self.state, index)
                }
                _ => {
                    let index = self.state.ui.selected_playlist_index;
                    match self.state.ui.active_library_tab {
                        LibraryTab::Albums => echo_core::intent::open_album(&mut self.state, index),
                        LibraryTab::Artists => {
                            echo_core::intent::open_followed_artist(&mut self.state, index)
                        }
                        _ => echo_core::intent::open_library_entry(&mut self.state, index),
                    }
                }
            }
        };
        if let Some(event) = event {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Whether a visual-mode range is being built.
    pub(crate) fn in_visual(&self) -> bool {
        self.state.ui.mode == AppMode::Visual
    }

    /// Whether a range action covers more than the focused row: a visual range or picked rows.
    pub(crate) fn multi_selected(&self) -> bool {
        echo_core::intent::has_multi_selection(&self.state)
    }

    /// Called on every drag move over a list: within [`DRAG_EDGE`] of its top or bottom edge
    /// the list scrolls toward that edge, faster the closer the pointer gets, on a frame tick
    /// that runs only while a drag is alive and the pointer stays near an edge.
    pub(crate) fn drag_autoscroll(
        &mut self,
        list: DragList,
        pointer_y: Pixels,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let top = f32::from(pointer_y - bounds.origin.y);
        let bottom = f32::from(bounds.origin.y + bounds.size.height - pointer_y);
        let speed = if top < DRAG_EDGE {
            -((DRAG_EDGE - top.max(0.0)) / DRAG_EDGE) * DRAG_SCROLL_MAX
        } else if bottom < DRAG_EDGE {
            ((DRAG_EDGE - bottom.max(0.0)) / DRAG_EDGE) * DRAG_SCROLL_MAX
        } else {
            self.drag_scroll = None;
            return;
        };
        self.drag_scroll = Some((list, speed));
        if self.drag_scroll_ticking {
            return;
        }
        self.drag_scroll_ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let keep_going = this.update(cx, |app, cx| {
                    let Some((list, speed)) = app.drag_scroll else {
                        return false;
                    };
                    if !cx.has_active_drag() {
                        app.drag_scroll = None;
                        return false;
                    }
                    let handle = match list {
                        DragList::Tracks => &app.tracks_scroll,
                        DragList::Library => &app.library_scroll,
                    };
                    let base = handle.0.borrow().base_handle.clone();
                    let mut offset = base.offset();
                    let lowest = -base.max_offset().y;
                    offset.y = (offset.y - px(speed)).clamp(lowest.min(px(0.0)), px(0.0));
                    base.set_offset(offset);
                    cx.notify();
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    let _ = this.update(cx, |app, _| app.drag_scroll_ticking = false);
                    break;
                }
            }
        })
        .detach();
    }

    /// Shift-click on row `ix`: anchor at the row that was focused (unless a range is already
    /// open, which keeps its anchor) and extend the selection to `ix`.
    pub(crate) fn extend_selection_to(&mut self, ix: usize, cx: &mut Context<Self>) {
        if !self.in_visual() {
            let anchor = match self.state.ui.active_view {
                ActiveView::TrackList => self.state.ui.selected_track_index,
                ActiveView::Queue => self.state.ui.selected_queue_index,
                ActiveView::SearchResults => self.state.ui.selected_search_index,
                ActiveView::Library => self.state.ui.selected_playlist_index,
                _ => return,
            };
            echo_core::intent::set_visual_anchor(&mut self.state, anchor);
        }
        self.set_selection(ix, cx);
        cx.notify();
    }

    /// `v` — start a range selection, or leave one already in progress.
    pub(crate) fn toggle_visual(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if self.in_visual() {
            echo_core::intent::exit_visual(&mut self.state);
        } else {
            echo_core::intent::enter_visual(&mut self.state);
        }
        cx.notify();
    }

    pub(crate) fn next_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        echo_core::intent::next_search_match(&mut self.state, forward);
        self.scroll_to_selected_track();
        cx.notify();
    }

    pub(crate) fn scroll_to_selected_track(&mut self) {
        self.tracks_scroll
            .scroll_to_item(self.state.ui.selected_track_index, ScrollStrategy::Nearest);
    }

    /// `g c`: select whatever is playing, loading its context first when needed.
    pub(crate) fn jump_to_current(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::jump_to_current_context(&mut self.state) {
            self.dispatch(event);
        }
        if self.state.ui.active_view == ActiveView::TrackList {
            self.tracks_scroll
                .scroll_to_item(self.state.ui.selected_track_index, ScrollStrategy::Center);
        }
        cx.notify();
    }
}
