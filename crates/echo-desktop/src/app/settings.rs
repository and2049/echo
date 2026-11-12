use super::*;

impl EchoApp {
    /// Open or close the settings sheet, seeding the folder field from the saved config.
    pub(crate) fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        self.settings_dropdown = None;
        if self.settings_open {
            self.settings_path_input = self
                .state
                .ui
                .library_config
                .local_music_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            self.settings_path_cursor = self.settings_path_input.len();
        }
        cx.notify();
    }

    /// Run a settings control through the `:` registry. Every command the sheet exposes already
    /// validates its own input and sets its own status message, so the rows stay declarative.
    pub(crate) fn run_setting(&mut self, cmd: String, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::commands::run(&mut self.state, &cmd) {
            self.dispatch(event);
        }
        cx.notify();
    }

    /// Audio-quality keys have no `:` command, so they write the config directly. They are read
    /// when the librespot daemon starts, hence the "next launch" note in the sheet.
    pub(crate) fn set_audio_quality(
        &mut self,
        apply: impl FnOnce(&mut echo_core::config::LibraryConfig),
        cx: &mut Context<Self>,
    ) {
        apply(&mut self.state.ui.library_config);
        self.state.save_library_config();
        self.state.ui.status_message =
            Some(views::tr(&self.state, "desktop.saved_next_launch").to_string());
        self.state.ui.status_message_expiry =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(3));
        cx.notify();
    }

    /// True when the folder field holds something other than the saved folder, including when
    /// none is saved yet.
    pub(crate) fn local_folder_unsaved(&self) -> bool {
        let saved = self
            .state
            .ui
            .library_config
            .local_music_dir
            .as_ref()
            .map(|path| path.display().to_string());
        saved.as_deref() != Some(self.settings_path_input.trim())
    }

    /// Enter in the folder field, or its button: save and scan what was typed, or rescan when
    /// the field already matches the saved folder.
    pub(crate) fn apply_or_rescan_local_folder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.local_folder_unsaved() {
            let path = self.settings_path_input.trim().to_string();
            if !path.is_empty() {
                self.run_setting(format!("localpath {path}"), cx);
                // Show the folder as saved: `~` expanded, and the button back to rescan.
                if let Some(saved) = &self.state.ui.library_config.local_music_dir
                    && echo_core::commands::expand_home(&path) == *saved
                {
                    self.settings_path_input = saved.display().to_string();
                    self.settings_path_cursor = self.settings_path_input.len();
                }
            }
        } else {
            self.run_setting("rescanlocal".to_string(), cx);
        }
        window.focus(&self.focus_handle.clone(), cx);
        cx.notify();
    }

    /// Typing in the settings sheet's local-folder field. Enter submits it as `:localpath`.
    pub(crate) fn handle_settings_path_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "enter" => self.apply_or_rescan_local_folder(window, cx),
            "escape" => window.focus(&self.focus_handle.clone(), cx),
            "v" if is_paste_chord(&event.keystroke.modifiers) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    insert_at_cursor(
                        &mut self.settings_path_input,
                        &mut self.settings_path_cursor,
                        text.trim(),
                    );
                }
            }
            _ => {
                apply_text_edit(
                    &mut self.settings_path_input,
                    &mut self.settings_path_cursor,
                    event,
                );
            }
        }
        cx.notify();
    }
}
