use super::*;

/// One bounce of the playing row's bars.
pub(crate) const PLAYING_BARS_LOOP: Duration = Duration::from_millis(900);

/// How often the bars repaint. Every frame re-renders the track list, so this stays well
/// under the display's refresh rate.
pub(crate) const PLAYING_BARS_FRAME: Duration = Duration::from_millis(33);

impl EchoApp {
    /// Redraws only what shows the playback position or the volume: the playback bar, the
    /// lyrics panel, and the immersive view, which is drawn by the app itself.
    pub(crate) fn notify_playback_position(&mut self, cx: &mut Context<Self>) {
        if self.immersive {
            cx.notify();
            return;
        }
        self.regions.notify(regions::Region::PlaybackBar, cx);
        if self.right_panel == Some(RightPanel::Lyrics) {
            self.regions.notify(regions::Region::RightPanel, cx);
        }
    }

    /// The phase, in `0..1`, to draw the playing row's bars at. Calling this from a render
    /// keeps the track list repainting every [`PLAYING_BARS_FRAME`] for as long as renders keep
    /// calling it.
    pub(crate) fn animate_playing_bars(&self, cx: &mut Context<Self>) -> f32 {
        if self.playing_bars.replace(PlayingBars::Drawn) == PlayingBars::Hidden {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(PLAYING_BARS_FRAME).await;
                    let keep_going = this.update(cx, |app, cx| match app.playing_bars.get() {
                        PlayingBars::Drawn => {
                            app.playing_bars.set(PlayingBars::Stale);
                            app.regions.notify(regions::Region::Main, cx);
                            true
                        }
                        PlayingBars::Stale | PlayingBars::Hidden => {
                            app.playing_bars.set(PlayingBars::Hidden);
                            false
                        }
                    });
                    if !matches!(keep_going, Ok(true)) {
                        break;
                    }
                }
            })
            .detach();
        }
        let elapsed = self.started_at.elapsed().as_secs_f32();
        (elapsed / PLAYING_BARS_LOOP.as_secs_f32()).fract()
    }

    pub(crate) fn add_to_queue(&mut self, cx: &mut Context<Self>) {
        // In visual mode `q` queues the whole range.
        if self.multi_selected() {
            if let Some(event) = echo_core::intent::queue_visual_selection(&mut self.state) {
                self.dispatch(event);
            }
            cx.notify();
            return;
        }
        if let Some(event) = echo_core::intent::queue_selected_track(&self.state) {
            self.dispatch(event);
            self.state.ui.status_message =
                Some(views::tr(&self.state, "desktop.added_to_queue").to_string());
            self.state.ui.status_message_expiry =
                Some(std::time::Instant::now() + Duration::from_secs(3));
        }
        cx.notify();
    }

    pub(crate) fn toggle_pin(&mut self, cx: &mut Context<Self>) {
        if self.theme_modal_open {
            // `m` is the mode key while the theme picker is up.
            self.cycle_theme_mode(cx);
            return;
        }
        echo_core::intent::toggle_pin_selected(&mut self.state);
        cx.notify();
    }

    pub(crate) fn adjust_volume(&mut self, delta: i32, cx: &mut Context<Self>) {
        let event = echo_core::intent::adjust_volume(&mut self.state, delta);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn seek_start(&mut self, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::seek_to(&mut self.state, 0) {
            self.dispatch(event);
        }
        cx.notify();
    }

    pub(crate) fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::toggle_playback(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn play_next(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::next_track(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn play_previous(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::previous_track(&self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn toggle_shuffle(&mut self, cx: &mut Context<Self>) {
        if self.state.ui.duplicate_prompt.is_some() {
            if let Some(event) = echo_core::intent::confirm_duplicate_skip(&mut self.state) {
                self.dispatch(event);
            }
            cx.notify();
            return;
        }
        let event = echo_core::intent::toggle_shuffle(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn cycle_repeat(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::cycle_repeat(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let event = echo_core::intent::toggle_mute(&mut self.state);
        self.dispatch(event);
        cx.notify();
    }

    pub(crate) fn seek_relative(&mut self, seconds: i64, cx: &mut Context<Self>) {
        if let Some(event) = echo_core::intent::seek_by(&mut self.state, seconds) {
            self.dispatch(event);
        }
        cx.notify();
    }

    fn seek_to_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let target = (self.state.playback.duration_ms as f32 * fraction.clamp(0.0, 1.0)) as u32;
        if let Some(event) = echo_core::intent::seek_to(&mut self.state, target) {
            self.dispatch(event);
        }
        cx.notify();
    }

    fn set_volume_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let volume = (fraction.clamp(0.0, 1.0) * 100.0).round() as u8;
        let event = echo_core::intent::set_volume(&mut self.state, volume);
        self.dispatch(event);
        cx.notify();
    }

    fn scrub_bounds(&self, target: Scrub) -> Bounds<Pixels> {
        match target {
            Scrub::Seek => self.seek_bounds.get(),
            Scrub::Volume => self.volume_bounds.get(),
        }
    }

    pub(crate) fn begin_scrub(&mut self, target: Scrub, x: Pixels, cx: &mut Context<Self>) {
        self.scrubbing = Some(target);
        self.update_scrub(x, cx);
    }

    pub(crate) fn update_scrub(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(target) = self.scrubbing else { return };
        let bounds = self.scrub_bounds(target);
        if bounds.size.width <= px(0.0) {
            return;
        }
        let fraction = ((x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
        match target {
            Scrub::Seek => {
                if self.state.playback.duration_ms > 0
                    && self.state.playback.playing_track_id.is_some()
                {
                    let target_ms = (self.state.playback.duration_ms as f32 * fraction) as u32;
                    self.state.playback.set_optimistic_progress(target_ms);
                }
            }
            Scrub::Volume => {
                self.state.playback.volume = (fraction * 100.0).round() as u32;
            }
        }
        self.notify_playback_position(cx);
    }

    pub(crate) fn finish_scrub(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(target) = self.scrubbing.take() else {
            return;
        };
        let bounds = self.scrub_bounds(target);
        if bounds.size.width <= px(0.0) {
            cx.notify();
            return;
        }
        let fraction = ((x - bounds.origin.x) / bounds.size.width).clamp(0.0, 1.0);
        match target {
            Scrub::Seek => self.seek_to_fraction(fraction, cx),
            Scrub::Volume => self.set_volume_fraction(fraction, cx),
        }
    }

    pub(crate) fn begin_shelf_scrub(&mut self, scroll: gpui::ScrollHandle, x: Pixels) {
        self.shelf_scrub = Some(scroll);
        self.update_shelf_scrub(x);
    }

    /// Scrolls the dragged shelf so the thumb follows the pointer's fraction of the bar (the
    /// bar spans the shelf row's width).
    pub(crate) fn update_shelf_scrub(&mut self, x: Pixels) {
        let Some(scroll) = self.shelf_scrub.as_ref() else {
            return;
        };
        let bounds = scroll.bounds();
        let fraction = ((x - bounds.left()) / bounds.size.width.max(px(1.0))).clamp(0.0, 1.0);
        scroll.set_offset(gpui::point(-scroll.max_offset().x * fraction, px(0.0)));
    }
}
