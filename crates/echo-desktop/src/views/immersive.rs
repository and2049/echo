use super::*;

/// The backdrop's picture over the whole window, under everything else: the keyframe before
/// `phase` with the one after it crossfaded on top. Painted straight from the texture as a fill
/// (its guard border just outside the window) rather than through `img`, which takes the
/// texture's aspect ratio as the box's and would paint a square.
pub fn backdrop_layer(
    backdrop: std::sync::Arc<Backdrop>,
    phase: f32,
    corners: Option<gpui::Tiling>,
) -> impl IntoElement {
    let radii = client_corner_radii(corners, ClientCorners::All);
    let (first, second, blend) = backdrop.frame(phase);
    let layer = |image: std::sync::Arc<gpui::RenderImage>| {
        let backdrop = backdrop.clone();
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let image_bounds = backdrop.image_bounds(bounds);
                window
                    .paint_image(bounds, image_bounds, radii, image, 0, false)
                    .ok();
            },
        )
        .absolute()
        .inset_0()
    };
    div()
        .absolute()
        .inset_0()
        .child(layer(first))
        .when(blend > 0.0, |el| {
            el.child(
                div()
                    .absolute()
                    .inset_0()
                    .opacity(blend)
                    .child(layer(second)),
            )
        })
}

/// The immersive toggle. Callers pick its colors: the search bar paints it muted in the theme,
/// the immersive view accented in the cover's colors. It is the one control the immersive view
/// keeps, so it lives here rather than inline in the search bar.
pub(super) fn immersive_button(
    color: Hsla,
    hover: Hsla,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    crate::icon_button(
        "immersive",
        "icons/full-screen.svg",
        color,
        hover,
        cx,
        |this, cx| this.toggle_immersive(cx),
    )
}

/// Edge of the immersive cover, in pixels, for a body of `width` x `height`: the largest square
/// that, with the caption under it, fits the height inside the margins, bounded by the
/// half-width; then scaled down to leave air around it, and capped for very tall windows.
pub(crate) fn immersive_cover_edge(width: f32, height: f32) -> f32 {
    let by_width = width * 0.5 - 2.0 * IMMERSIVE_MARGIN;
    let by_height = height - 2.0 * IMMERSIVE_MARGIN - IMMERSIVE_CAPTION_HEIGHT;
    (by_width.min(by_height) * IMMERSIVE_COVER_SCALE).clamp(0.0, IMMERSIVE_COVER_MAX)
}

/// Rows in the immersive lyric window for a panel `panel_height` tall: as many whole rows as
/// fit, made odd so one row is the exact center, and at least one.
pub(crate) fn lyric_window_rows(panel_height: f32, row_height: f32) -> usize {
    let rows = (panel_height / row_height).max(1.0) as usize;
    if rows.is_multiple_of(2) {
        rows - 1
    } else {
        rows
    }
}

/// Opacity of a lyric row `distance` rows from the center when the row just past the window's
/// edge is `reach` rows away: full at the center, easing out so the neighbors stay readable and
/// reaching transparent one row beyond the edge — the Apple Music fade. `distance` is
/// fractional mid-glide.
pub(crate) fn lyric_opacity(distance: f32, reach: f32) -> f32 {
    let t = (distance / reach).min(1.0);
    1.0 - t * t
}

const IMMERSIVE_MARGIN: f32 = 48.0;

/// Both halves stack a cover-high box, a gap and a caption-high block, centered; so the
/// right's lyrics box centers on the cover and its controls block tops out with the title.
/// The caption block is the title's 32px text_2xl line, gap_1, the artist's 28px text_lg line;
/// the controls block is the seek row, gap_1, the transport row, the same 64.
const IMMERSIVE_CAPTION_HEIGHT: f32 = IMMERSIVE_GAP + IMMERSIVE_CAPTION_BLOCK;

const IMMERSIVE_CAPTION_BLOCK: f32 = 32.0 + 4.0 + 28.0;

const IMMERSIVE_GAP: f32 = 24.0;

const IMMERSIVE_SEEK_ROW: f32 = 24.0;

const IMMERSIVE_TRANSPORT_ROW: f32 = IMMERSIVE_CAPTION_BLOCK - 4.0 - IMMERSIVE_SEEK_ROW;

const IMMERSIVE_COVER_SCALE: f32 = 0.75;

const IMMERSIVE_COVER_MAX: f32 = 480.0;

/// The lyric window fills up to this share of the body height, and never more than the cover.
const IMMERSIVE_LYRICS_SHARE: f32 = 0.5;

const IMMERSIVE_LYRIC_ROW: f32 = 40.0;

/// The immersive view: the cover with the track's title and artist under it, that whole block
/// centered in the left half; on the right the synced lyrics, centered on the cover, with the
/// seek bar and transport under them level with the caption; and top right, in the search
/// bar's slots, the toggle, the queue (where the theme picker sits otherwise) and settings.
/// Everything else — sidebar, navigation, search, playback bar — is gone until it is toggled
/// off. Lyric lines are centered so each shares its axis with the seek bar and play button. Every color comes from `backdrop`, derived from the cover
/// (see [`crate::backdrop`]); the backdrop's picture and window fill are the root's, so they
/// reach under the titlebar too.
pub fn immersive_view(
    app: &mut EchoApp,
    backdrop: &Backdrop,
    window: &mut Window,
    cx: &mut Context<EchoApp>,
) -> impl IntoElement {
    let colors = backdrop.colors;
    let (fg, muted) = (colors.text, colors.text_muted);
    let controls = ControlColors {
        fg,
        muted,
        accent: colors.accent,
        hover: colors.wash,
        track: colors.wash,
    };
    let playback = &app.state.playback;
    let has_track = playback.playing_track_id.is_some();
    let title: SharedString = if playback.playing_track_title.is_empty() {
        tr(&app.state, "desktop.nothing_playing")
    } else {
        playback.playing_track_title.clone().into()
    };
    let artist: SharedString = playback.playing_track_artist.clone().into();
    let viewport = window.viewport_size();
    let body_height = f32::from(viewport.height) - TITLEBAR_HEIGHT;
    let edge = immersive_cover_edge(f32::from(viewport.width), body_height);
    let rows = lyric_window_rows(
        edge.min(body_height * IMMERSIVE_LYRICS_SHARE),
        IMMERSIVE_LYRIC_ROW,
    );
    let cover = playback
        .playing_track_image
        .as_ref()
        .or(playback.previous_track_image.as_ref())
        .cloned()
        .and_then(|artwork| app.images.get(&artwork));
    let cover = match cover {
        Some(image) => cover_image(image, edge, 8.0).into_any_element(),
        None => div()
            .flex_none()
            .w(px(edge))
            .h(px(edge))
            .rounded_lg()
            .bg(colors.wash)
            .flex()
            .items_center()
            .justify_center()
            .child(
                svg()
                    .path("icons/music-note.svg")
                    .w(px(edge * 0.25))
                    .h(px(edge * 0.25))
                    .text_color(muted),
            )
            .into_any_element(),
    };
    let lyrics = lyric_status(app, muted).unwrap_or_else(|| {
        lyric_window(
            app,
            LyricColors::immersive(&colors),
            rows,
            IMMERSIVE_LYRIC_ROW,
        )
    });

    div()
        .id("immersive")
        .relative()
        .flex_grow(1.0)
        .flex()
        .flex_row()
        .overflow_hidden()
        .child(
            div()
                .w(relative(0.5))
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(IMMERSIVE_GAP))
                .child(cover)
                .child(
                    div()
                        .w(px(edge))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            crate::playing_track_link(
                                "immersive-title",
                                title,
                                fg,
                                has_track,
                                ActionMenuAction::GoToAlbum,
                                cx,
                            )
                            .text_2xl(),
                        )
                        .child(
                            crate::playing_track_link(
                                "immersive-artist",
                                artist,
                                muted,
                                has_track,
                                ActionMenuAction::GoToArtist,
                                cx,
                            )
                            .text_lg(),
                        ),
                ),
        )
        .child(
            div()
                .w(relative(0.5))
                .h_full()
                .pr(px(IMMERSIVE_MARGIN))
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(IMMERSIVE_GAP))
                .child(
                    div()
                        .h(px(edge))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .child(lyrics),
                )
                .child(
                    div()
                        .h(px(IMMERSIVE_CAPTION_BLOCK))
                        .px_4()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(app.seek_bar(controls, cx).h(px(IMMERSIVE_SEEK_ROW)))
                        .child(
                            app.transport_controls(controls, cx)
                                .h(px(IMMERSIVE_TRANSPORT_ROW))
                                .justify_center(),
                        ),
                ),
        )
        // The exact pixels the search bar gives its three buttons (pr_4 from the edge), so the
        // toggle and settings never move out from under the pointer. The queue takes the theme
        // picker's slot; opening it leaves the view.
        .child(
            div()
                .absolute()
                .top_2()
                .right(px(16.0))
                .flex()
                .flex_row()
                .child(immersive_button(colors.accent, colors.wash, cx))
                .child(crate::icon_button(
                    "queue",
                    "icons/playlist.svg",
                    muted,
                    colors.wash,
                    cx,
                    |this, cx| this.toggle_queue_view(cx),
                ))
                .child(crate::icon_button(
                    "settings",
                    "icons/settings.svg",
                    muted,
                    colors.wash,
                    cx,
                    |this, cx| this.toggle_settings(cx),
                )),
        )
}

#[cfg(test)]
mod immersive_tests {
    use super::*;
    use echo_core::models::LyricLine;

    fn line(start_ms: u32) -> LyricLine {
        LyricLine {
            start_ms,
            text: String::new(),
        }
    }

    #[test]
    fn current_lyric_is_last_started_line() {
        let lines = [line(0), line(1_000), line(2_000)];
        assert_eq!(current_lyric_index(&lines, 0), 0);
        assert_eq!(current_lyric_index(&lines, 1_500), 1);
        assert_eq!(current_lyric_index(&lines, 9_000), 2);
    }

    #[test]
    fn current_lyric_before_first_line_is_first() {
        let lines = [line(500), line(1_000)];
        assert_eq!(current_lyric_index(&lines, 100), 0);
        assert_eq!(current_lyric_index(&[], 100), 0);
    }

    #[test]
    fn cover_edge_fits_the_narrower_axis() {
        // Wide body: the half-width minus margins bounds the square (504), scaled to 378.
        assert_eq!(immersive_cover_edge(1200.0, 900.0), 378.0);
        // Short body: the height minus margins and the caption bounds it (316), scaled to 237.
        assert_eq!(immersive_cover_edge(1600.0, 500.0), 237.0);
    }

    #[test]
    fn lyric_opacity_eases_to_transparent_past_the_edge() {
        assert_eq!(lyric_opacity(0.0, 5.0), 1.0);
        assert!(lyric_opacity(1.0, 5.0) > 0.9);
        assert!(lyric_opacity(4.0, 5.0) < 0.4);
        assert!(lyric_opacity(4.0, 5.0) > 0.3);
        assert_eq!(lyric_opacity(5.0, 5.0), 0.0);
        assert_eq!(lyric_opacity(9.0, 5.0), 0.0);
    }

    #[test]
    fn lyric_window_holds_an_odd_number_of_whole_rows() {
        assert_eq!(lyric_window_rows(360.0, 40.0), 9);
        assert_eq!(lyric_window_rows(434.0, 40.0), 9);
        assert_eq!(lyric_window_rows(479.0, 40.0), 11);
        assert_eq!(lyric_window_rows(0.0, 40.0), 1);
        assert_eq!(lyric_window_rows(80.0, 40.0), 1);
    }

    #[test]
    fn cover_edge_is_capped_and_never_negative() {
        assert_eq!(immersive_cover_edge(4000.0, 3000.0), IMMERSIVE_COVER_MAX);
        assert_eq!(immersive_cover_edge(50.0, 50.0), 0.0);
    }
}
