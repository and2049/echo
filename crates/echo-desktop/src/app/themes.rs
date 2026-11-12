use super::*;

impl EchoApp {
    pub(crate) fn toggle_themes(&mut self, cx: &mut Context<Self>) {
        self.theme_modal_open = !self.theme_modal_open;
        if self.theme_modal_open {
            // Open on the appearance in use and the theme chosen for it, so j/k start from
            // where the user is.
            self.show_theme_list(self.active_scheme(), cx);
        }
        cx.notify();
    }

    /// Tells the core which appearance the system is in; under `Auto` that swaps the theme.
    pub(crate) fn sync_system_appearance(
        &mut self,
        appearance: gpui::WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        use echo_core::config::Scheme;
        use gpui::WindowAppearance::*;
        let scheme = match appearance {
            Dark | VibrantDark => Scheme::Dark,
            Light | VibrantLight => Scheme::Light,
        };
        if echo_core::intent::set_system_scheme(&mut self.state, scheme) {
            // An open picker under `Auto` keeps the list the user is looking at.
            cx.notify();
        }
    }

    /// Shows one appearance's themes in the picker, with the cursor on the one chosen for it.
    pub(crate) fn show_theme_list(
        &mut self,
        scheme: echo_core::config::Scheme,
        cx: &mut Context<Self>,
    ) {
        self.theme_modal_tab = scheme;
        let chosen = echo_core::theme::selected_name(
            &self.state.ui.library_config,
            &self.state.ui.themes,
            scheme,
        );
        self.theme_modal_index = views::theme_rows(&self.state, scheme)
            .iter()
            .position(|row| row.name == chosen)
            .unwrap_or(0);
        self.theme_modal_scroll
            .scroll_to_item(self.theme_modal_index);
        cx.notify();
    }

    /// The appearance in effect right now: the pinned one, or the system's under `Auto`.
    fn active_scheme(&self) -> echo_core::config::Scheme {
        echo_core::theme::active_scheme(&self.state.ui.library_config, self.state.ui.system_scheme)
    }

    /// The picker's mode control: sets dark / light / auto and lists the appearance now on
    /// screen.
    pub(crate) fn set_theme_mode(
        &mut self,
        mode: echo_core::config::ThemeMode,
        cx: &mut Context<Self>,
    ) {
        echo_core::intent::set_theme_mode(&mut self.state, mode);
        self.show_theme_list(self.active_scheme(), cx);
    }

    /// `m` in the picker: auto → dark → light → auto.
    pub(crate) fn cycle_theme_mode(&mut self, cx: &mut Context<Self>) {
        use echo_core::config::ThemeMode;
        let next = match self.state.ui.library_config.theme_mode {
            Some(ThemeMode::Dark) => ThemeMode::Light,
            Some(ThemeMode::Light) => ThemeMode::Auto,
            _ => ThemeMode::Dark,
        };
        self.set_theme_mode(next, cx);
    }
}
