use super::*;

impl EchoApp {
    /// Runs a context-menu item against sidebar row `index`, then closes the menu. Destructive
    /// items only stage a `*_prompt` — the confirm modal fires the actual event.
    pub(crate) fn run_menu_action(
        &mut self,
        action: MenuAction,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.context_menu = None;
        if self.state.ui.active_library_tab == LibraryTab::Albums {
            match action {
                MenuAction::Open => {
                    if let Some(event) = echo_core::intent::open_album(&mut self.state, index) {
                        self.dispatch(event);
                    }
                }
                MenuAction::RemoveAlbum => {
                    if let Some(album) = self.state.data.saved_albums.get(index) {
                        self.state.ui.album_mass_delete_prompt = Some(vec![album.id.clone()]);
                    }
                }
                _ => {}
            }
            cx.notify();
            return;
        }

        let Some(node) = self.state.data.library_view.get(index).cloned() else {
            cx.notify();
            return;
        };
        match action {
            MenuAction::EditDetails => {
                if let echo_core::models::LibraryNode::Playlist { playlist, .. } = &node {
                    echo_core::intent::open_playlist_edit(&mut self.state, &playlist.id);
                    window.focus(&self.focus_handle, cx);
                }
            }
            MenuAction::Open => {
                self.state.ui.selected_playlist_index = index;
                if let Some(event) = echo_core::intent::open_library_entry(&mut self.state, index) {
                    self.dispatch(event);
                }
            }
            MenuAction::TogglePin => {
                self.state.ui.selected_playlist_index = index;
                echo_core::intent::toggle_pin_selected(&mut self.state);
            }
            MenuAction::Rename => {
                let name = match &node {
                    echo_core::models::LibraryNode::Playlist { playlist, .. } => {
                        playlist.name.clone()
                    }
                    echo_core::models::LibraryNode::Folder(folder) => folder.name.clone(),
                    echo_core::models::LibraryNode::SpotifyFolder { .. } => return,
                };
                self.state.ui.selected_playlist_index = index;
                self.open_command(&format!("rename {name}"), window, cx);
            }
            MenuAction::RemoveFromFolder => {
                if let echo_core::models::LibraryNode::Playlist { playlist, .. } = &node {
                    echo_core::intent::remove_playlist_from_folders(&mut self.state, &playlist.id);
                }
            }
            MenuAction::DeletePlaylist => {
                if let echo_core::models::LibraryNode::Playlist { playlist, .. } = &node {
                    self.state.ui.playlist_delete_prompt = Some(vec![playlist.id.clone()]);
                }
            }
            MenuAction::DeleteFolder => {
                if let echo_core::models::LibraryNode::Folder(folder) = &node {
                    self.state.ui.folder_delete_prompt = Some(folder.name.clone());
                }
            }
            MenuAction::RemoveAlbum => {}
        }
        cx.notify();
    }

    /// Runs a track-menu item against the menu's staged context, then closes the menu. Remove
    /// only stages `track_delete_prompt` — the confirm modal fires the actual event.
    pub(crate) fn run_track_menu_action(&mut self, item: TrackMenuItem, cx: &mut Context<Self>) {
        self.state.ui.playlist_add_filter.clear();
        let Some(menu) = self.track_menu.take() else {
            cx.notify();
            return;
        };
        let ctx = menu.ctx;
        match item {
            TrackMenuItem::Action(action) => {
                if let Some(event) = echo_core::action_menu::run(&mut self.state, ctx, action) {
                    self.dispatch(event);
                }
            }
            TrackMenuItem::RemoveFromPlaylist => {
                if let Some(context) = self
                    .state
                    .data
                    .active_tracklist_context
                    .clone()
                    .filter(|c| c.can_modify_playlist(self.state.data.user_id.as_ref()))
                {
                    self.state.ui.track_delete_prompt = Some((context.id, vec![ctx.track_id]));
                }
            }
        }
        cx.notify();
    }

    /// Index of the "Add to playlist" row in the open track menu, if it has one. The flyout
    /// anchors to it, and the hover/keyboard handlers key off it.
    pub(crate) fn track_menu_add_row(&self) -> Option<usize> {
        views::track_menu_items(self)
            .iter()
            .position(|(_, item, _)| {
                matches!(
                    item,
                    TrackMenuItem::Action(echo_core::models::ActionMenuAction::AddToPlaylist)
                )
            })
    }

    /// Open the flyout. `focus_row` also parks the menu's keyboard selection on the row it
    /// hangs off, which is what Enter and a click mean; a hover leaves the selection where the
    /// keyboard put it, since the view lights that row for as long as the flyout is open.
    pub(crate) fn open_playlist_submenu(&mut self, focus_row: bool, cx: &mut Context<Self>) {
        let Some(row) = self.track_menu_add_row() else {
            return;
        };
        if let Some(menu) = self.track_menu.as_mut() {
            if focus_row {
                menu.selected = row;
                self.track_menu_scroll.scroll_to_item(row);
            }
            if menu.submenu.is_none() {
                self.state.ui.playlist_add_filter.clear();
                menu.submenu = Some(0);
                self.submenu_scroll.scroll_to_item(0);
            }
        }
        cx.notify();
    }

    /// Close the flyout but keep the menu itself: what escape, the left arrow and a hover that
    /// leaves the row all mean.
    pub(crate) fn close_playlist_submenu(&mut self, cx: &mut Context<Self>) {
        self.state.ui.playlist_add_filter.clear();
        if let Some(menu) = self.track_menu.as_mut() {
            menu.submenu = None;
        }
        cx.notify();
    }

    /// Add the menu's track to flyout choice `index`, then close the whole menu. The register
    /// is what `commit_playlist_add` resolves tracks from, the same handoff
    /// `action_menu::run(AddToPlaylist)` makes before opening the modal picker.
    pub(crate) fn commit_playlist_submenu(&mut self, index: usize, cx: &mut Context<Self>) {
        // Out of range only happens with no playlists at all, where Enter should do nothing
        // rather than close the menu; it would also leave the register staged.
        if index > echo_core::action_menu::filtered_playlist_add_choices(&self.state).len() {
            return;
        }
        let Some(menu) = self.track_menu.take() else {
            return;
        };
        self.state.ui.operation_register = vec![menu.ctx.track_id];
        if let Some(event) =
            echo_core::action_menu::commit_filtered_playlist_add(&mut self.state, index)
        {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// A pointer move over track-menu row `ix`. Hovering the "Add to playlist" row opens the
    /// flyout; hovering any other row closes it — unless the pointer is inside the triangle
    /// spanned by where it left that row and the flyout's near edge, which is the diagonal a
    /// user aiming at the flyout travels through.
    pub(crate) fn hover_track_menu_row(
        &mut self,
        ix: usize,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.track_menu_add_row() != Some(ix) {
            if self
                .track_menu
                .as_ref()
                .is_some_and(|menu| menu.submenu.is_some())
                && !self.pointer_aims_at_submenu(position)
            {
                self.close_playlist_submenu(cx);
            }
            return;
        }
        self.submenu_apex = position;
        if self
            .track_menu
            .as_ref()
            .is_some_and(|menu| menu.submenu.is_none())
        {
            self.open_playlist_submenu(false, cx);
        }
    }

    /// Whether `position` falls in the triangle from [`Self::submenu_apex`] to the flyout's
    /// near edge. Degenerate while the flyout has yet to be painted, which reads as "not
    /// aiming" and simply closes it a frame early.
    fn pointer_aims_at_submenu(&self, position: gpui::Point<Pixels>) -> bool {
        let flyout = self.submenu_bounds.get();
        if flyout.size.height <= px(0.0) {
            return false;
        }
        // The flyout sits on whichever side of the menu had room (`views::playlist_submenu`),
        // so the near edge — and which way "toward it" is — follows from where it landed.
        let flipped = flyout.left() < self.submenu_row_bounds.get().left();
        let edge = if flipped {
            flyout.right()
        } else {
            flyout.left()
        };
        // The flyout can be much taller than the menu, so the triangle alone would cover most
        // of the rows below: a move only counts as aiming while it still travels toward the
        // flyout. Straight down the menu closes it at once.
        let toward = if flipped {
            position.x < self.submenu_apex.x
        } else {
            position.x > self.submenu_apex.x
        };
        if !toward {
            return false;
        }
        // A few pixels of slack at both corners: the flyout's own border and rounding mean the
        // exact corner is not where the pointer needs to land.
        let apex = self.submenu_apex;
        let top = gpui::point(edge, flyout.top() - px(4.0));
        let bottom = gpui::point(edge, flyout.bottom() + px(4.0));
        let side = |a: gpui::Point<Pixels>, b: gpui::Point<Pixels>| {
            f32::from(b.x - a.x) * f32::from(position.y - a.y)
                - f32::from(b.y - a.y) * f32::from(position.x - a.x)
        };
        let (ab, bc, ca) = (side(apex, top), side(top, bottom), side(bottom, apex));
        (ab >= 0.0 && bc >= 0.0 && ca >= 0.0) || (ab <= 0.0 && bc <= 0.0 && ca <= 0.0)
    }

    /// The track `a` / `shift-a` act on: the focused row where the view has one, otherwise the
    /// currently playing track.
    pub(crate) fn action_target(&self) -> Option<echo_core::models::ActionMenuContext> {
        use echo_core::models::ActionMenuContext;
        let ui = &self.state.ui;
        let data = &self.state.data;
        let row = match ui.active_view {
            ActiveView::TrackList => data.tracks.get(ui.selected_track_index),
            ActiveView::Queue => self.state.queue_view_track(ui.selected_queue_index),
            ActiveView::ArtistPage => echo_core::intent::selected_artist_top_track(&self.state),
            ActiveView::SearchResults => {
                return echo_core::intent::selected_search_track(&self.state)
                    .map(ActionMenuContext::from);
            }
            _ => None,
        };
        if let Some(track) = row {
            return Some(ActionMenuContext::from(track));
        }

        self.playing_track_context()
    }

    /// Menu context for the currently playing track: `action_target`'s fallback and the
    /// playback bar's click-to-navigate target.
    pub(crate) fn playing_track_context(&self) -> Option<echo_core::models::ActionMenuContext> {
        let playback = &self.state.playback;
        let track_id = playback.playing_track_id.clone()?;
        // Playback state only carries the joined "A, B, C" artist string; when the playing
        // track is in the current list or queue, prefer its structured data so go-to-artist
        // gets the first artist's own name.
        if let Some(track) = self
            .state
            .data
            .tracks
            .iter()
            .chain(self.state.data.queue.iter())
            .find(|track| track.id == track_id)
        {
            return Some(echo_core::models::ActionMenuContext::from(track));
        }
        Some(echo_core::models::ActionMenuContext {
            album_name: self
                .state
                .data
                .local_library
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map(|track| track.album.clone())
                .unwrap_or_default(),
            track_id,
            source: playback
                .playing_track_source
                .unwrap_or(echo_core::models::TrackSource::Spotify),
            track_name: playback.playing_track_title.clone(),
            local_path: playback.playing_track_local_path.clone(),
            album_id: playback.playing_track_album_id.clone(),
            artist_id: playback.playing_track_artist_id.clone(),
            artist_name: playback.playing_track_artist.clone(),
        })
    }

    /// `shift-a` — the track action menu, centered because there is no click to anchor it to.
    pub(crate) fn open_action_menu(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if let Some(ctx) = self.action_target() {
            self.context_menu = None;
            self.track_menu = Some(TrackMenuState {
                ctx,
                position: None,
                selected: 0,
                submenu: None,
            });
        }
        cx.notify();
    }

    /// `a` — add the selection to a playlist, or save the album when an album row is focused.
    /// The picker commits through `action_menu::commit_playlist_add`, which resolves the
    /// tracks from the current selection.
    pub(crate) fn add_to_playlist(&mut self, cx: &mut Context<Self>) {
        self.pending_count = None;
        if self.multi_selected() {
            echo_core::intent::add_visual_selection_to_playlist(&mut self.state);
            cx.notify();
            return;
        }
        let album_id = match self.state.ui.active_view {
            ActiveView::SearchResults
                if self.state.ui.active_search_tab == echo_core::app::SearchTab::Albums =>
            {
                self.state
                    .data
                    .search_results
                    .albums
                    .get(self.state.ui.selected_search_index)
                    .map(|album| (album.id.clone(), album.name.clone()))
            }
            _ => None,
        };

        if let Some((id, name)) = album_id {
            let language = self.state.ui.library_config.language.clone();
            self.state.ui.status_message = Some(
                echo_core::i18n::t("messages.saved_to_library", &language).replace("{}", &name),
            );
            self.state.ui.status_message_expiry =
                Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
            self.dispatch(AppEvent::SaveAlbums(vec![id]));
        } else {
            self.state.ui.playlist_add_modal_open = true;
            self.state.ui.playlist_add_filter.clear();
            self.state.ui.selected_playlist_modal_index = 0;
        }
        cx.notify();
    }
}
