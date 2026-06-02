use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Rect},
    widgets::{Block, Borders, Cell, HighlightSpacing, Paragraph, Row, Table, TableState},
};
use crate::tui::render::{
    format_duration_text, format_time, stabilize_terminal_emoji_width, truncate_to_width_with_ellipsis,
};

pub fn render_queue(frame: &mut Frame, state: &AppState, area: Rect) {
    let header_style = state.active_theme.muted_style();
    let title = format!(" {} ({} upcoming) ", crate::i18n::t("ui.queue", &state.library_config.language), state.queue.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(state.active_theme.primary_style());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.queue.is_empty() {
        let msg = Paragraph::new("Queue is empty. Press q on any track to add it.")
            .style(state.active_theme.muted_style())
            .alignment(Alignment::Center);
        frame.render_widget(msg, inner);
        return;
    }

    let w_track = inner.width.saturating_sub(11) * 60 / 100;
    let w_artist = inner.width.saturating_sub(11).saturating_sub(w_track);

    let header = Row::new(vec![
        crate::i18n::t("ui.tracks", &state.library_config.language),
        crate::i18n::t("ui.artist", &state.library_config.language),
        crate::i18n::t("ui.duration", &state.library_config.language)
    ])
        .style(header_style)
        .height(1);

    let visual_range = if state.active_view == crate::app::ActiveView::Queue {
        state.get_visual_selection_range()
    } else {
        None
    };

    let sel = state.selected_queue_index;
    let rows: Vec<Row> = state
        .queue
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let is_in_visual = if let Some((start, end)) = visual_range {
                i >= start && i <= end
            } else {
                false
            };
            
            let style = if is_in_visual {
                state.active_theme.selected_style().bg(state.active_theme.primary)
            } else if i == sel {
                state.active_theme.selected_style()
            } else {
                state.active_theme.base_style()
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
            Row::new(vec![
                Cell::from(name),
                Cell::from(artist).style(style.fg(state.active_theme.text_muted)),
                Cell::from(dur).style(style.fg(state.active_theme.text_muted)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(w_track),
            Constraint::Min(0),
            Constraint::Length(9), // DURATION_COLUMN_WIDTH
        ],
    )
    .column_spacing(1)
    .header(header)
    .row_highlight_style(state.active_theme.selected_style())
    .highlight_symbol(" ")
    .highlight_spacing(HighlightSpacing::Always);

    let mut ts = TableState::default();
    ts.select(Some(sel));
    frame.render_stateful_widget(table, inner, &mut ts);
}
