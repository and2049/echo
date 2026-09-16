use crate::tui::render::{
    format_duration_text, format_time, stabilize_terminal_emoji_width,
    truncate_to_width_with_ellipsis,
};
use crate::tui::theme::{ThemeStyles, ToRatatui};
use echo_core::app::{AppState, QueueRow, QueueTab};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    widgets::{Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table, TableState},
};

pub fn render_queue(frame: &mut Frame, state: &AppState, area: Rect) {
    let header_style = state.ui.active_theme.muted_style();
    let lang = &state.ui.library_config.language;
    let recent = state.ui.queue_tab == QueueTab::Recent;
    let queue_label = echo_core::i18n::t("ui.queue", lang);
    let recent_label = echo_core::i18n::t("ui.recent", lang);
    let title = if recent {
        format!(" {queue_label} | [{recent_label}] ")
    } else {
        format!(
            " [{queue_label}] | {recent_label} ({} upcoming) ",
            state.data.queue.len()
        )
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(state.ui.active_theme.primary_style());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if recent {
        render_recent(frame, state, inner);
        return;
    }

    if state.data.queue.is_empty() {
        let msg = Paragraph::new("Queue is empty. Press q on any track to add it.")
            .style(state.ui.active_theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let w_track = inner.width.saturating_sub(11) * 60 / 100;
    let w_artist = inner.width.saturating_sub(11).saturating_sub(w_track);

    let header = Row::new(vec![
        "".to_string(), // liked col
        echo_core::i18n::t("ui.tracks", &state.ui.library_config.language),
        echo_core::i18n::t("ui.artist", &state.ui.library_config.language),
        echo_core::i18n::t("ui.duration", &state.ui.library_config.language),
    ])
    .style(header_style)
    .height(1);

    let visual_range = if state.ui.active_view == echo_core::app::ActiveView::Queue {
        state.get_visual_selection_range()
    } else {
        None
    };

    let sel = state.ui.selected_queue_index;
    let queue_rows = state.queue_rows();
    let selected_row = state.queue_row_of(sel);
    let rows: Vec<Row> = queue_rows
        .into_iter()
        .map(|row| match row {
            QueueRow::Header(text) => {
                Row::new(vec![Cell::from(""), Cell::from(text)]).style(header_style)
            }
            QueueRow::Track(i, track) => {
                let is_in_visual = if let Some((start, end)) = visual_range {
                    i >= start && i <= end
                } else {
                    false
                };

                let style = if is_in_visual {
                    state
                        .ui
                        .active_theme
                        .selected_style()
                        .bg(state.ui.active_theme.primary.rat())
                } else if i == sel {
                    state.ui.active_theme.selected_style()
                } else {
                    state.ui.active_theme.base_style()
                };
                let name = truncate_to_width_with_ellipsis(
                    &stabilize_terminal_emoji_width(&track.name),
                    w_track,
                );
                let artist = truncate_to_width_with_ellipsis(
                    &stabilize_terminal_emoji_width(&track.artist),
                    w_artist,
                );
                let dur = format_duration_text(format_time(track.duration_ms / 1000));
                let liked_str = if state.data.liked_tracks.contains(&track.id) {
                    "♥"
                } else {
                    " "
                };
                let liked_cell =
                    Cell::from(liked_str).style(state.ui.active_theme.secondary_style());

                Row::new(vec![
                    liked_cell,
                    Cell::from(name),
                    Cell::from(artist).style(style.fg(state.ui.active_theme.text_muted.rat())),
                    Cell::from(dur).style(style.fg(state.ui.active_theme.text_muted.rat())),
                ])
                .style(style)
            }
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(w_track),
            Constraint::Min(0),
            Constraint::Length(9), // DURATION_COLUMN_WIDTH
        ],
    )
    .column_spacing(1)
    .header(header)
    .row_highlight_style(state.ui.active_theme.selected_style())
    .highlight_symbol(" ")
    .highlight_spacing(HighlightSpacing::Always);

    let mut ts = TableState::default();
    ts.select(selected_row);
    frame.render_stateful_widget(table, inner, &mut ts);
}

fn render_recent(frame: &mut Frame, state: &AppState, inner: Rect) {
    let lang = &state.ui.library_config.language;
    if state.data.recent_plays.is_empty() {
        let msg = Paragraph::new(echo_core::i18n::t("desktop.recent_empty", lang))
            .style(state.ui.active_theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let w_ago = 16u16;
    let w_track = inner.width.saturating_sub(w_ago + 4) * 60 / 100;
    let w_artist = inner
        .width
        .saturating_sub(w_ago + 4)
        .saturating_sub(w_track);
    let header = Row::new(vec![
        "".to_string(),
        echo_core::i18n::t("ui.tracks", lang),
        echo_core::i18n::t("ui.artist", lang),
        echo_core::i18n::t("ui.recent", lang),
    ])
    .style(state.ui.active_theme.muted_style())
    .height(1);

    let visual_range = state.get_visual_selection_range();
    let sel = state.ui.selected_queue_index;
    let tr = |key: &str| echo_core::i18n::t(key, lang);
    let rows: Vec<Row> = state
        .data
        .recent_plays
        .iter()
        .enumerate()
        .map(|(i, record)| {
            let track = &record.track;
            let in_visual = visual_range.is_some_and(|(start, end)| i >= start && i <= end);
            let style = if in_visual {
                state
                    .ui
                    .active_theme
                    .selected_style()
                    .bg(state.ui.active_theme.primary.rat())
            } else if i == sel {
                state.ui.active_theme.selected_style()
            } else {
                state.ui.active_theme.base_style()
            };
            let liked_str = if state.data.liked_tracks.contains(&track.id) {
                "♥"
            } else {
                " "
            };
            Row::new(vec![
                Cell::from(liked_str).style(state.ui.active_theme.secondary_style()),
                Cell::from(truncate_to_width_with_ellipsis(
                    &stabilize_terminal_emoji_width(&track.name),
                    w_track,
                )),
                Cell::from(truncate_to_width_with_ellipsis(
                    &stabilize_terminal_emoji_width(&track.artist),
                    w_artist,
                ))
                .style(style.fg(state.ui.active_theme.text_muted.rat())),
                Cell::from(echo_core::context_details::format_added_at_with(
                    std::time::SystemTime::now().into(),
                    &record.played_at,
                    &tr,
                ))
                .style(style.fg(state.ui.active_theme.text_muted.rat())),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(w_track),
            Constraint::Min(0),
            Constraint::Length(w_ago),
        ],
    )
    .column_spacing(1)
    .header(header)
    .row_highlight_style(state.ui.active_theme.selected_style())
    .highlight_symbol(" ")
    .highlight_spacing(HighlightSpacing::Always);

    let mut ts = TableState::default();
    ts.select(Some(sel));
    frame.render_stateful_widget(table, inner, &mut ts);
}
