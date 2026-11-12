use super::*;

/// Drag payload for a sidebar playlist or folder row. `id` is the row's key: a playlist id, or
/// a folder key.
pub(crate) struct DraggedPlaylist {
    pub id: String,
    pub name: SharedString,
}

/// Drag payload for track rows: the whole selection when the dragged row is part of it, else
/// that one row. `source_context` names the tracklist the rows came from, so a drop back into
/// the same list reorders instead of copying. `name`, `artist` and `cover` feed the preview
/// chip: the dragged row's cover with the track's title and artist, or the song count.
pub(crate) struct DraggedTracks {
    pub tracks: Vec<echo_core::models::Track>,
    pub from_row: usize,
    pub source_context: Option<String>,
    pub name: SharedString,
    pub artist: SharedString,
    pub cover: Option<std::sync::Arc<gpui::RenderImage>>,
}

/// The payload for dragging one `track` from outside any track list (the playback bar, an
/// artist's Popular rows).
pub(crate) fn single_drag_payload(
    track: &echo_core::models::Track,
    cover: Option<std::sync::Arc<gpui::RenderImage>>,
) -> DraggedTracks {
    DraggedTracks {
        tracks: vec![track.clone()],
        from_row: 0,
        source_context: None,
        name: SharedString::from(track.name.clone()),
        artist: SharedString::from(track.artist.clone()),
        cover,
    }
}

/// The payload for dragging row `ix` of `view`: every selected track in display order when the
/// row is one of them, otherwise just `track`.
pub(super) fn drag_payload(
    this: &mut EchoApp,
    view: ActiveView,
    ix: usize,
    track: &echo_core::models::Track,
) -> DraggedTracks {
    let cover = thumb_image(this, track.image_url.as_deref(), DRAG_COVER_EDGE);
    let mut payload = single_drag_payload(track, cover);
    let state = &this.state;
    let rows = if state.ui.active_view == view {
        echo_core::intent::selected_rows(state)
    } else {
        Vec::new()
    };
    if rows.len() > 1 && rows.contains(&ix) {
        payload.tracks = echo_core::intent::selected_tracks(state);
        payload.name = tr(state, "desktop.drag_songs")
            .replace("{}", &payload.tracks.len().to_string())
            .into();
        payload.artist = SharedString::default();
    }
    payload.from_row = ix;
    payload.source_context = (view == ActiveView::TrackList)
        .then(|| {
            state
                .data
                .active_tracklist_context
                .as_ref()
                .map(|c| c.id.clone())
        })
        .flatten();
    payload
}

pub(super) const DRAG_COVER_EDGE: f32 = 32.0;

const DRAG_CHIP_MAX_WIDTH: f32 = 320.0;

/// The colours of the chip that follows the cursor during a drag.
#[derive(Clone, Copy)]
pub(crate) struct DragColors {
    fg: gpui::Hsla,
    muted: gpui::Hsla,
    bg: gpui::Hsla,
    border: gpui::Hsla,
}

impl DragColors {
    pub(crate) fn resolve(theme: &echo_core::theme::ResolvedTheme) -> Self {
        Self {
            fg: theme.text.gpui(WINDOW_FG()),
            muted: theme.text_muted.gpui(WINDOW_FG()),
            bg: theme.surface.gpui(crate::theme::PANEL_BG()),
            border: DesktopPalette::resolve(theme).menu_border,
        }
    }
}

/// The chip that follows the cursor while something is dragged: a name, or for tracks the
/// cover with title and artist like a shortened track row.
pub(crate) struct DragPreview {
    name: SharedString,
    artist: SharedString,
    cover: Option<std::sync::Arc<gpui::RenderImage>>,
    colors: DragColors,
}

impl DragPreview {
    pub(super) fn label(name: SharedString, colors: DragColors) -> Self {
        Self {
            name,
            artist: SharedString::default(),
            cover: None,
            colors,
        }
    }
}

/// The preview entity for a track drag, for `on_drag`.
pub(crate) fn track_drag_preview(
    drag: &DraggedTracks,
    colors: DragColors,
    cx: &mut App,
) -> Entity<DragPreview> {
    let preview = DragPreview {
        name: drag.name.clone(),
        artist: drag.artist.clone(),
        cover: drag.cover.clone(),
        colors,
    };
    cx.new(|_| preview)
}

impl gpui::Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors;
        let line = |text: SharedString| {
            div()
                .whitespace_nowrap()
                .text_ellipsis()
                .overflow_hidden()
                .child(text)
        };
        div()
            .max_w(px(DRAG_CHIP_MAX_WIDTH))
            .px_2()
            .py_1()
            .rounded_md()
            .bg(colors.bg)
            .border_1()
            .border_color(colors.border)
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .when_some(self.cover.clone(), |el, image| {
                el.child(cover_image(image, DRAG_COVER_EDGE, 4.0))
            })
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(line(self.name.clone()).text_sm().text_color(colors.fg))
                    .when(!self.artist.is_empty(), |el| {
                        el.child(line(self.artist.clone()).text_xs().text_color(colors.muted))
                    }),
            )
    }
}
