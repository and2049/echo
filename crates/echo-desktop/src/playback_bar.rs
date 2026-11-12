use super::*;

/// The playback bar's two rows share this three-slot geometry — fixed side columns around a
/// growing center — so the seek bar lines up with the transport and volume controls below it.
pub(crate) const PLAYBACK_SIDE_WIDTH: f32 = 240.0;

/// Fixed so that showing or hiding the lyric line and the visualizer can't move the seek bar.
pub(crate) const PLAYBACK_ROW_HEIGHT: f32 = 40.0;

/// The playback bar's heart for the playing track: the list rows' liked cell at bar size.
pub(crate) fn playing_like_button(
    track_id: String,
    liked: bool,
    color: Hsla,
    dim: Hsla,
    cx: &mut Context<EchoApp>,
) -> gpui::Stateful<Div> {
    div()
        .id("playing-like")
        .group("playing-like")
        .flex_none()
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_click(cx.listener(move |this, _event, _window, cx| {
            if let Some(event) =
                echo_core::intent::toggle_like_track(&mut this.state, track_id.clone())
            {
                this.dispatch(event);
            }
            cx.notify();
        }))
        .child(
            svg()
                .path("icons/heart.svg")
                .size(px(14.0))
                .text_color(if liked { color } else { dim })
                .when(!liked, |el| {
                    el.group_hover("playing-like", move |style| style.text_color(color))
                }),
        )
}

/// The playing track's title or artist as a link: clicking jumps to the album or artist page
/// through the same action-menu path the context menu uses. Callers add the text size.
pub(crate) fn playing_track_link(
    id: &'static str,
    text: SharedString,
    color: Hsla,
    has_track: bool,
    action: echo_core::models::ActionMenuAction,
    cx: &mut Context<EchoApp>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .text_color(color)
        .whitespace_nowrap()
        .text_ellipsis()
        .overflow_hidden()
        .when(has_track, |el| {
            el.cursor_pointer()
                .hover(|style| style.underline())
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    if let Some(ctx) = this.playing_track_context() {
                        this.go_to(ctx, action, cx);
                    }
                }))
        })
        .child(text)
}

pub(crate) fn icon_button(
    id: &'static str,
    icon: &'static str,
    color: Hsla,
    hover_bg: Hsla,
    cx: &mut Context<EchoApp>,
    on_click: impl Fn(&mut EchoApp, &mut Context<EchoApp>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex_none()
        .w(px(32.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .hover(move |style| style.bg(hover_bg))
        .on_click(cx.listener(move |this, _event, _window, cx| on_click(this, cx)))
        .child(svg().path(icon).w(px(16.0)).h(px(16.0)).text_color(color))
}

/// The colors the transport and seek bar draw in: the theme's in the playback bar, the
/// cover's in the immersive view.
#[derive(Clone, Copy)]
pub(crate) struct ControlColors {
    pub fg: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub hover: Hsla,
    pub track: Hsla,
}

pub(crate) fn format_time(ms: u32) -> String {
    let seconds = ms / 1000;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

impl EchoApp {
    pub(crate) fn render_playback_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.state.ui.active_theme;
        let palette = DesktopPalette::resolve(theme);
        let fg = theme.text.gpui(WINDOW_FG());
        let muted = theme.text_muted.gpui(WINDOW_FG());
        let accent = theme.primary.gpui(WINDOW_FG());

        let playback = &self.state.playback;
        let title: SharedString = if playback.playing_track_title.is_empty() {
            views::tr(&self.state, "desktop.nothing_playing")
        } else {
            playback.playing_track_title.clone().into()
        };
        let artist: SharedString = playback.playing_track_artist.clone().into();
        let has_track = playback.playing_track_id.is_some();

        let controls = ControlColors {
            fg,
            muted,
            accent,
            hover: palette.wash,
            track: palette.border,
        };
        let volume_fraction = (playback.volume as f32 / 100.0).clamp(0.0, 1.0);
        let volume_label: SharedString = format!("{}%", playback.volume).into();
        let mute_icon = if playback.volume == 0 {
            "icons/volume-off.svg"
        } else {
            "icons/volume-high.svg"
        };
        let queue_color = if self.right_panel == Some(RightPanel::Queue) {
            accent
        } else {
            muted
        };
        let lyrics_color = if self.right_panel == Some(RightPanel::Lyrics) {
            accent
        } else {
            muted
        };
        let secondary = theme.secondary.gpui(WINDOW_FG());
        let liked_button = playback
            .playing_track_id
            .clone()
            .filter(|_| {
                playback.playing_track_source == Some(echo_core::models::TrackSource::Spotify)
            })
            .map(|track_id| {
                let liked = self.state.data.liked_tracks.contains(&track_id);
                playing_like_button(track_id, liked, secondary, palette.like_dim, cx)
            });
        // Playback on another Connect device (a phone, a speaker) gets a pill naming it;
        // echo's own device and local files are the norm and stay unlabelled.
        let playing_on: Option<SharedString> = (has_track
            && playback.device_name != echo_core::worker::audio::DEVICE_NAME
            && playback.device_name != "Local"
            && !playback.device_name.is_empty())
        .then(|| {
            views::tr(&self.state, "desktop.playing_on")
                .replace("{}", &playback.device_name)
                .into()
        });
        let visualizer_bands = (self.state.ui.library_config.enable_visualizer
            && self.state.playback.is_playing)
            .then(|| self.state.playback.audio_visualization.clone())
            .flatten();
        let vis_bins = self.state.ui.vis_bins.clamp(5, 32);

        // Keep showing the previous cover while the current one refetches after a pixelate
        // change.
        let cover = self
            .state
            .playback
            .playing_track_image
            .as_ref()
            .or(self.state.playback.previous_track_image.as_ref())
            .cloned()
            .and_then(|artwork| self.images.get(&artwork));
        let drag_colors = views::DragColors::resolve(&self.state.ui.active_theme);
        let drag_payload = echo_core::history::playing_track(&self.state)
            .map(|track| views::single_drag_payload(&track, cover.clone()));

        // The condensed-lyrics line shows the current lyric accented with the next one below,
        // riding the same playback tick that advances the seek bar.
        let inline_lyrics = self
            .state
            .ui
            .condensed_lyrics_enabled
            .then(|| {
                if let Some(lyrics) = self.state.playback.current_lyrics.as_ref() {
                    if lyrics.lines.is_empty() {
                        return (
                            views::tr(&self.state, "desktop.no_lyrics_found").to_string(),
                            String::new(),
                        );
                    }
                    let progress = self.state.playback.display_progress_ms();
                    let current = views::current_lyric_index(&lyrics.lines, progress);
                    (
                        lyrics.lines[current].text.clone(),
                        lyrics
                            .lines
                            .get(current + 1)
                            .map(|line| line.text.clone())
                            .unwrap_or_default(),
                    )
                } else if !self.state.playback.playing_track_title.is_empty() {
                    (
                        views::tr(&self.state, "desktop.no_lyrics_found").to_string(),
                        String::new(),
                    )
                } else {
                    (String::new(), String::new())
                }
            })
            .filter(|(current, _)| !current.is_empty());

        // Inline lyrics keep the top-row center slot; "Playing from X" fills it otherwise.
        let playing_from = if inline_lyrics.is_none() && has_track {
            views::playing_context_label(&self.state)
        } else {
            None
        };

        let volume_bounds = self.volume_bounds.clone();

        div()
            .flex()
            .flex_col()
            .justify_center()
            .gap_1()
            .h(px(regions::PLAYBACK_BAR_HEIGHT))
            .px_4()
            .border_t_1()
            .border_color(palette.border)
            .child(
                // Top row: song card, condensed lyric line, visualizer. Its contents come and go;
                // its height doesn't, so nothing here can move the row below.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(px(PLAYBACK_ROW_HEIGHT))
                    .child(
                        div()
                            .id("playing-card")
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .when_some(drag_payload, |el, payload| {
                                el.on_drag(payload, move |drag, _offset, _window, cx| {
                                    views::track_drag_preview(drag, drag_colors, cx)
                                })
                            })
                            .child(match cover {
                                Some(image) => {
                                    views::cover_image(image, 36.0, 6.0).into_any_element()
                                }
                                None => div()
                                    .flex_none()
                                    .w(px(36.0))
                                    .h(px(36.0))
                                    .rounded_md()
                                    .bg(palette.wash)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        svg()
                                            .path("icons/music-note.svg")
                                            .w(px(16.0))
                                            .h(px(16.0))
                                            .text_color(muted),
                                    )
                                    .into_any_element(),
                            })
                            .child(
                                div()
                                    .flex_col()
                                    .flex_grow(1.0)
                                    .overflow_hidden()
                                    .child(
                                        playing_track_link(
                                            "playing-title",
                                            title,
                                            fg,
                                            has_track,
                                            echo_core::models::ActionMenuAction::GoToAlbum,
                                            cx,
                                        )
                                        .text_sm(),
                                    )
                                    .child(
                                        playing_track_link(
                                            "playing-artist",
                                            artist,
                                            muted,
                                            has_track,
                                            echo_core::models::ActionMenuAction::GoToArtist,
                                            cx,
                                        )
                                        .text_xs(),
                                    ),
                            )
                            .children(liked_button),
                    )
                    .child(
                        div()
                            .flex_grow(1.0)
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            .when_some(inline_lyrics, |el, (current, next)| {
                                el.child(
                                    div()
                                        .text_sm()
                                        .text_color(accent)
                                        .whitespace_nowrap()
                                        .max_w_full()
                                        .overflow_hidden()
                                        .child(SharedString::from(current)),
                                )
                                .when(!next.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_xs()
                                            .text_color(muted)
                                            .whitespace_nowrap()
                                            .max_w_full()
                                            .overflow_hidden()
                                            .child(SharedString::from(next)),
                                    )
                                })
                            })
                            .when_some(playing_from, |el, label| {
                                el.child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .whitespace_nowrap()
                                        .max_w_full()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(SharedString::from(label)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_3()
                            .when_some(playing_on, |el, label| {
                                el.child(
                                    div()
                                        .id("playing-on")
                                        .flex_none()
                                        .max_w(px(160.0))
                                        .px_2()
                                        .py_1()
                                        .rounded_full()
                                        .border_1()
                                        .border_color(accent)
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(palette.wash))
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.open_devices(cx)
                                        }))
                                        .child(
                                            svg()
                                                .path("icons/computer.svg")
                                                .size(px(12.0))
                                                .text_color(accent),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(accent)
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .overflow_hidden()
                                                .child(label),
                                        ),
                                )
                            })
                            .when_some(visualizer_bands, |el, bands| {
                                // The engine always fills 32 bands, 0–100; they are averaged down
                                // to the configured bin count (`:visbins`) and painted as
                                // bottom-anchored bars. Repaints ride the fast tick.
                                el.child(
                                    div().flex_none().w(px(120.0)).h(px(32.0)).child(
                                        canvas(
                                            |_, _, _| (),
                                            move |bounds, _, window, _| {
                                                let bands = bands.lock();
                                                let chunk = bands.len() as f32 / vis_bins as f32;
                                                let band_width =
                                                    bounds.size.width / vis_bins as f32;
                                                for index in 0..vis_bins {
                                                    let start = (index as f32 * chunk) as usize;
                                                    let end = if index == vis_bins - 1 {
                                                        bands.len()
                                                    } else {
                                                        ((index + 1) as f32 * chunk) as usize
                                                    };
                                                    let slice = &bands[start..end.max(start + 1)];
                                                    let value = slice.iter().sum::<f32>()
                                                        / slice.len() as f32;
                                                    let ratio = (value / 100.0).clamp(0.0, 1.0);
                                                    let bar_height = bounds.size.height * ratio;
                                                    let origin = gpui::point(
                                                        bounds.origin.x + band_width * index as f32,
                                                        bounds.origin.y + bounds.size.height
                                                            - bar_height,
                                                    );
                                                    let bar = Bounds {
                                                        origin,
                                                        size: size(band_width * 0.8, bar_height),
                                                    };
                                                    window.paint_quad(gpui::fill(bar, accent));
                                                }
                                            },
                                        )
                                        .size_full(),
                                    ),
                                )
                            }),
                    ),
            )
            .child(
                // Bottom row: transport, seek and volume on one line, so the seek bar sits on the
                // buttons' baseline no matter what the row above is doing.
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .h(px(PLAYBACK_ROW_HEIGHT))
                    .child(
                        self.transport_controls(controls, cx)
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .child(icon_button(
                                "lyrics",
                                "icons/mic.svg",
                                lyrics_color,
                                palette.wash,
                                cx,
                                |this, cx| this.toggle_lyrics(cx),
                            )),
                    )
                    .child(self.seek_bar(controls, cx))
                    .child(
                        div()
                            .flex_none()
                            .w(px(PLAYBACK_SIDE_WIDTH))
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_end()
                            .gap_1()
                            .child(icon_button(
                                "queue",
                                "icons/playlist.svg",
                                queue_color,
                                palette.wash,
                                cx,
                                |this, cx| this.toggle_right_panel(RightPanel::Queue, cx),
                            ))
                            .child(icon_button(
                                "devices",
                                "icons/computer.svg",
                                muted,
                                palette.wash,
                                cx,
                                |this, cx| this.open_devices(cx),
                            ))
                            .child(icon_button(
                                "mute",
                                mute_icon,
                                muted,
                                palette.wash,
                                cx,
                                |this, cx| this.toggle_mute(cx),
                            ))
                            .child(
                                div()
                                    .id("volume-bar")
                                    .flex_none()
                                    .w(px(90.0))
                                    .py_2()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        gpui::MouseButton::Left,
                                        cx.listener(
                                            |this, event: &gpui::MouseDownEvent, _window, cx| {
                                                this.begin_scrub(
                                                    Scrub::Volume,
                                                    event.position.x,
                                                    cx,
                                                );
                                            },
                                        ),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h(px(6.0))
                                            .rounded_full()
                                            .bg(palette.border)
                                            .child(
                                                canvas(
                                                    move |bounds, _window, _cx| {
                                                        volume_bounds.set(bounds)
                                                    },
                                                    |_, _, _, _| {},
                                                )
                                                .absolute()
                                                .size_full(),
                                            )
                                            .child(
                                                div()
                                                    .h_full()
                                                    .rounded_full()
                                                    .bg(fg)
                                                    .w(gpui::relative(volume_fraction)),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(34.0))
                                    .text_xs()
                                    .text_color(fg)
                                    .child(volume_label),
                            ),
                    ),
            )
    }

    /// Shuffle, previous, play/pause, next and repeat in a row, two each side of play so it
    /// centers; the caller sizes the row.
    pub(crate) fn transport_controls(&self, colors: ControlColors, cx: &mut Context<Self>) -> Div {
        let playback = &self.state.playback;
        let play_icon = if playback.is_playing {
            "icons/pause.svg"
        } else {
            "icons/play.svg"
        };
        let shuffle_color = if playback.is_shuffled {
            colors.accent
        } else {
            colors.muted
        };
        let (repeat_icon, repeat_color) = match playback.repeat_mode.as_str() {
            "Track" => ("icons/repeat-one.svg", colors.accent),
            "Context" => ("icons/repeat.svg", colors.accent),
            _ => ("icons/repeat.svg", colors.muted),
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(icon_button(
                "shuffle",
                "icons/shuffle.svg",
                shuffle_color,
                colors.hover,
                cx,
                |this, cx| this.toggle_shuffle(cx),
            ))
            .child(icon_button(
                "previous",
                "icons/previous.svg",
                colors.fg,
                colors.hover,
                cx,
                |this, cx| this.play_previous(cx),
            ))
            .child(
                div()
                    .id("play-pause")
                    .flex_none()
                    .w(px(36.0))
                    .h(px(36.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .hover(move |style| style.bg(colors.hover))
                    .on_click(cx.listener(|this, _event, _window, cx| this.toggle_playback(cx)))
                    .child(
                        svg()
                            .path(play_icon)
                            .w(px(20.0))
                            .h(px(20.0))
                            .text_color(colors.fg),
                    ),
            )
            .child(icon_button(
                "next",
                "icons/next.svg",
                colors.fg,
                colors.hover,
                cx,
                |this, cx| this.play_next(cx),
            ))
            .child(icon_button(
                "repeat",
                repeat_icon,
                repeat_color,
                colors.hover,
                cx,
                |this, cx| this.cycle_repeat(cx),
            ))
    }

    /// Elapsed time, the seek bar and the duration on one line, growing to the caller's width.
    /// The canvas overlay records the track's bounds each paint, and a click anywhere in the
    /// (taller) hit area seeks to that fraction.
    pub(crate) fn seek_bar(&self, colors: ControlColors, cx: &mut Context<Self>) -> Div {
        let playback = &self.state.playback;
        let progress_ms = playback.display_progress_ms();
        let fraction = if playback.duration_ms > 0 {
            (progress_ms as f32 / playback.duration_ms as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let seek_bounds = self.seek_bounds.clone();
        let label = |ms: u32| {
            div()
                .flex_none()
                .text_color(colors.muted)
                .text_xs()
                .child(SharedString::from(format_time(ms)))
        };
        div()
            .flex_grow(1.0)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(label(progress_ms))
            .child(
                div()
                    .id("seek-bar")
                    .flex_grow(1.0)
                    .py_2()
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, _window, cx| {
                            this.begin_scrub(Scrub::Seek, event.position.x, cx);
                        }),
                    )
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(6.0))
                            .rounded_full()
                            .bg(colors.track)
                            .child(
                                canvas(
                                    move |bounds, _window, _cx| seek_bounds.set(bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .child(
                                div()
                                    .h_full()
                                    .rounded_full()
                                    .bg(colors.accent)
                                    .w(gpui::relative(fraction)),
                            ),
                    ),
            )
            .child(label(playback.duration_ms))
    }
}
