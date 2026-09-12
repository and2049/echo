use crate::EchoApp;
use echo_core::intent::{self, PlaylistEditFocus, PlaylistPageAction};
use gpui::{ClipboardItem, Context, Window};

impl EchoApp {
    pub(crate) fn playlist_input_open(&self) -> bool {
        self.state.ui.playlist_edit.is_some()
            || self.state.ui.playlist_add_modal_open
            || self
                .track_menu
                .as_ref()
                .is_some_and(|m| m.submenu.is_some())
    }

    pub(crate) fn save_playlist_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = intent::submit_playlist_edit(&mut self.state) {
            self.dispatch(event);
        }
        cx.notify();
    }

    pub(crate) fn run_page_action(&mut self, action: PlaylistPageAction, cx: &mut Context<Self>) {
        self.page_menu = None;
        if action == PlaylistPageAction::CopyLink {
            if let Some(link) = intent::page_context_link(&self.state) {
                cx.write_to_clipboard(ClipboardItem::new_string(link));
                self.state.ui.status_message =
                    Some(crate::views::tr(&self.state, "desktop.link_copied").to_string());
                self.state.ui.status_message_expiry =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
            }
        } else if let Some(event) = intent::run_playlist_page_action(&mut self.state, action) {
            self.dispatch(event);
        }
        cx.notify();
    }

    pub(crate) fn handle_playlist_input(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.playlist_input_open() {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.dismiss(cx);
            cx.stop_propagation();
            return true;
        }
        if self.state.ui.playlist_edit.is_some() {
            if key == "enter" {
                if self
                    .state
                    .ui
                    .playlist_edit
                    .as_ref()
                    .is_some_and(|d| d.focus == PlaylistEditFocus::Name)
                {
                    self.save_playlist_edit(cx);
                }
            } else if key == "tab" {
                if let Some(draft) = self.state.ui.playlist_edit.as_mut() {
                    if draft.id.starts_with("local-playlist:") {
                        cx.stop_propagation();
                        return true;
                    }
                    draft.focus = if draft.focus == PlaylistEditFocus::Name {
                        PlaylistEditFocus::Description
                    } else {
                        PlaylistEditFocus::Name
                    };
                }
            } else {
                let paste = (key == "v" && crate::is_paste_chord(&event.keystroke.modifiers))
                    .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
                    .flatten();
                if let Some(draft) = self.state.ui.playlist_edit.as_mut() {
                    let value = if draft.focus == PlaylistEditFocus::Name {
                        &mut draft.name
                    } else {
                        &mut draft.description
                    };
                    if let Some(text) = paste {
                        value.push_str(&text.replace(['\r', '\n'], " "));
                    } else {
                        let mut cursor = value.len();
                        crate::apply_text_edit(value, &mut cursor, event);
                    }
                }
            }
        } else {
            match key {
                "enter" => self.activate_selection(cx),
                "up" => self.move_selection(-1, cx),
                "down" => self.move_selection(1, cx),
                _ => {
                    let value = &mut self.state.ui.playlist_add_filter;
                    if key == "v" && crate::is_paste_chord(&event.keystroke.modifiers) {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            value.push_str(&text.replace(['\r', '\n'], " "));
                        }
                    } else {
                        let mut cursor = value.len();
                        crate::apply_text_edit(value, &mut cursor, event);
                    }
                    self.state.ui.selected_playlist_modal_index = 0;
                    if let Some(menu) = self.track_menu.as_mut() {
                        menu.submenu = Some(0);
                    }
                    self.playlist_modal_scroll.scroll_to_item(0);
                    self.submenu_scroll.scroll_to_item(0);
                }
            }
        }
        window.focus(&self.focus_handle, cx);
        cx.stop_propagation();
        cx.notify();
        true
    }
}
