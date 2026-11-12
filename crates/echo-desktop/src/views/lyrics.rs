use super::*;

/// Index of the lyric line playing at `progress_ms`: the last line that has started, or the
/// first before any has.
pub(crate) fn current_lyric_index(
    lines: &[echo_core::models::LyricLine],
    progress_ms: u32,
) -> usize {
    lines
        .iter()
        .take_while(|line| line.start_ms <= progress_ms)
        .count()
        .saturating_sub(1)
}

/// The colors a lyric line takes by its place in the song: the current line accented, lines
/// already sung in the text color, upcoming ones muted.
pub(super) struct LyricColors {
    fg: Hsla,
    muted: Hsla,
    accent: Hsla,
}

impl LyricColors {
    fn resolve(app: &EchoApp) -> Self {
        let theme = &app.state.ui.active_theme;
        Self {
            fg: theme.text.gpui(WINDOW_FG()),
            muted: theme.text_muted.gpui(WINDOW_FG()),
            accent: theme.primary.gpui(WINDOW_FG()),
        }
    }

    pub(super) fn immersive(colors: &ImmersiveColors) -> Self {
        Self {
            fg: colors.text,
            muted: colors.text_muted,
            accent: colors.accent,
        }
    }

    fn line(&self, ix: usize, current: usize) -> Hsla {
        match ix.cmp(&current) {
            std::cmp::Ordering::Equal => self.accent,
            std::cmp::Ordering::Greater => self.muted,
            std::cmp::Ordering::Less => self.fg,
        }
    }
}

/// One lyric line on a single row of `height` pixels, ellipsized if it is too long. A block
/// with the line height set to the row, not a flex row: a flex item is measured at its
/// max-content width, which defeats the ellipsis.
fn lyric_row(text: SharedString, color: Hsla, height: f32, large: bool) -> Div {
    div()
        .w_full()
        .h(px(height))
        .px_4()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .map(|el| {
            if large {
                el.text_xl().text_center()
            } else {
                el.text_sm()
            }
        })
        .line_height(px(height))
        .text_color(color)
        .child(text)
}

/// What stands in for the lyrics while they load or when the track has none; `None` once there
/// are lines to show.
pub(super) fn lyric_status(app: &EchoApp, muted: Hsla) -> Option<AnyElement> {
    let playback = &app.state.playback;
    let key = if playback.is_fetching_lyrics {
        "desktop.loading_lyrics"
    } else if playback.current_lyrics.is_none() {
        "desktop.no_lyrics"
    } else {
        return None;
    };
    Some(
        div()
            .py_8()
            .flex()
            .justify_center()
            .text_color(muted)
            .child(tr(&app.state, key))
            .into_any_element(),
    )
}

/// The panel's lyric list: every line, scrolled so the current one (by playback position) sits
/// at the center whenever the list is long enough to allow it.
pub(super) fn lyric_list(app: &mut EchoApp, cx: &mut Context<EchoApp>) -> AnyElement {
    let lines = app
        .state
        .playback
        .current_lyrics
        .as_ref()
        .map(|lyrics| lyrics.lines.as_slice())
        .unwrap_or_default();
    let current = current_lyric_index(lines, app.state.playback.display_progress_ms());
    app.lyrics_scroll
        .scroll_to_item(current, gpui::ScrollStrategy::Center);

    uniform_list(
        "lyric-lines",
        lines.len(),
        cx.processor(
            |this: &mut EchoApp, range: std::ops::Range<usize>, _window, _cx| {
                let colors = LyricColors::resolve(this);
                let lines = this
                    .state
                    .playback
                    .current_lyrics
                    .as_ref()
                    .map(|lyrics| lyrics.lines.as_slice())
                    .unwrap_or_default();
                let current = current_lyric_index(lines, this.state.playback.display_progress_ms());

                range
                    .map(|ix| {
                        let text = lines
                            .get(ix)
                            .map(|line| line.text.clone())
                            .unwrap_or_default();
                        lyric_row(
                            text.into(),
                            colors.line(ix, current),
                            PANEL_LYRIC_ROW,
                            false,
                        )
                        .id(ix)
                    })
                    .collect()
            },
        ),
    )
    .track_scroll(&app.lyrics_scroll)
    .flex_grow(1.0)
    .into_any_element()
}

/// The immersive lyrics: a fixed window of `rows` (odd) whole lines with the current line
/// pinned to the center row, so nothing is ever clipped and the first and last lines of a song
/// sit exactly where the middle ones do. When the line advances the column glides up one row
/// over [`LYRIC_GLIDE`]; the extra row above the window carries the line on its way out. Rows
/// fade toward the edges (see [`lyric_opacity`]), the outermost still faintly legible.
pub(super) fn lyric_window(
    app: &EchoApp,
    colors: LyricColors,
    rows: usize,
    row_height: f32,
) -> AnyElement {
    let lines = app
        .state
        .playback
        .current_lyrics
        .as_ref()
        .map(|lyrics| lyrics.lines.as_slice())
        .unwrap_or_default();
    let current = current_lyric_index(lines, app.state.playback.display_progress_ms());
    let half = (rows / 2) as isize;
    let reach = (half + 1) as f32;
    let column: Vec<(isize, SharedString, Hsla)> = (-half - 1..=half)
        .map(|offset| {
            let ix = current.checked_add_signed(offset);
            let text = ix
                .and_then(|ix| lines.get(ix))
                .map(|line| SharedString::from(line.text.clone()))
                .unwrap_or_default();
            (offset, text, colors.line(ix.unwrap_or(current), current))
        })
        .collect();

    div()
        .relative()
        .w_full()
        .h(px(rows as f32 * row_height))
        .overflow_hidden()
        .child(div().absolute().w_full().with_animation(
            ("lyric-glide", current),
            Animation::new(LYRIC_GLIDE).with_easing(ease_out_quint()),
            move |el, t| {
                el.top(px(-row_height * t))
                    .children(column.iter().map(|(offset, text, color)| {
                        let distance = (*offset as f32 + 1.0 - t).abs();
                        lyric_row(text.clone(), *color, row_height, true)
                            .opacity(lyric_opacity(distance, reach))
                    }))
            },
        ))
        .into_any_element()
}

const PANEL_LYRIC_ROW: f32 = 26.0;

const LYRIC_GLIDE: std::time::Duration = std::time::Duration::from_millis(240);
