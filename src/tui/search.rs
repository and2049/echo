use crate::app::{ActiveView, AppState, SearchTab};
use crate::tui::render::{
    DURATION_COLUMN_WIDTH, format_duration_text, truncate_to_width_with_ellipsis,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Modifier,
    widgets::{Block, Borders, Cell, HighlightSpacing, Row, Table, TableState},
};

pub fn render_search_results(frame: &mut Frame, state: &AppState, area: Rect) {
    let is_focused = state.active_view == ActiveView::SearchResults;
    let border_style = if is_focused {
        state.active_theme.secondary_style()
    } else {
        state.active_theme.primary_style()
    };

    let t_title = if state.active_search_tab == SearchTab::Tracks {
        format!(
            "[{}]",
            crate::i18n::t("ui.tracks", &state.library_config.language)
        )
    } else {
        format!(
            " {} ",
            crate::i18n::t("ui.tracks", &state.library_config.language)
        )
    };
    let a_title = if state.active_search_tab == SearchTab::Albums {
        format!(
            "[{}]",
            crate::i18n::t("ui.albums", &state.library_config.language)
        )
    } else {
        format!(
            " {} ",
            crate::i18n::t("ui.albums", &state.library_config.language)
        )
    };
    let ar_title = if state.active_search_tab == SearchTab::Artists {
        "[Artists]".to_string()
    } else {
        " Artists ".to_string()
    };
    let tab_title = format!("{} {} {}", t_title, a_title, ar_title);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(border_style)
        .title(format!(
            " {}: {} — {} ",
            crate::i18n::t("ui.search", &state.library_config.language),
            state.search_context_query,
            tab_title
        ));

    let inner = search_block.inner(area);
    frame.render_widget(search_block, area);

    let sel = state.selected_search_index;
    let header_style = border_style.add_modifier(Modifier::BOLD);

    match state.active_search_tab {
        SearchTab::Tracks => {
            let header = Row::new(vec![
                "".to_string(), // liked col
                crate::i18n::t("ui.tracks", &state.library_config.language),
                crate::i18n::t("ui.artist", &state.library_config.language),
                crate::i18n::t("ui.album", &state.library_config.language),
                crate::i18n::t("ui.duration", &state.library_config.language),
            ])
            .style(header_style)
            .height(1);
            let visual_range = if state.active_view == ActiveView::SearchResults {
                state.get_visual_selection_range()
            } else {
                None
            };

            let rows: Vec<Row> = state
                .search_results
                .tracks
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let is_in_visual = if let Some((start, end)) = visual_range {
                        i >= start && i <= end
                    } else {
                        false
                    };

                    let style = if is_in_visual {
                        state
                            .active_theme
                            .selected_style()
                            .bg(state.active_theme.primary)
                    } else if i == sel {
                        state.active_theme.selected_style()
                    } else {
                        state.active_theme.base_style()
                    };
                    let dur = format!(
                        "{}:{:02}",
                        t.duration_ms / 1000 / 60,
                        t.duration_ms / 1000 % 60
                    );
                    let w_track = (inner.width * 35 / 100).saturating_sub(1);
                    let w_artist = (inner.width * 25 / 100).saturating_sub(1);
                    let w_album = (inner.width * 30 / 100).saturating_sub(1);

                    let liked_str = if state.liked_tracks.contains(&t.id) {
                        "♥"
                    } else {
                        " "
                    };
                    let liked_cell =
                        Cell::from(liked_str).style(state.active_theme.secondary_style());

                    Row::new(vec![
                        liked_cell,
                        Cell::from(truncate_to_width_with_ellipsis(&t.name, w_track)),
                        Cell::from(truncate_to_width_with_ellipsis(&t.artist, w_artist)),
                        Cell::from(truncate_to_width_with_ellipsis(&t.album, w_album)),
                        Cell::from(format_duration_text(dur)),
                    ])
                    .style(style)
                })
                .collect();
            let table = Table::new(
                rows,
                [
                    Constraint::Length(2),
                    Constraint::Percentage(35),
                    Constraint::Percentage(25),
                    Constraint::Percentage(30),
                    Constraint::Length(DURATION_COLUMN_WIDTH),
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
        SearchTab::Albums => {
            let header = Row::new(vec!["Album", "Artist"])
                .style(header_style)
                .height(1);
            let rows: Vec<Row> = state
                .search_results
                .albums
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    let style = if i == sel {
                        state.active_theme.selected_style()
                    } else {
                        state.active_theme.base_style()
                    };
                    let w_album = (inner.width * 50 / 100).saturating_sub(1);
                    let w_artist = (inner.width * 50 / 100).saturating_sub(1);
                    Row::new(vec![
                        Cell::from(truncate_to_width_with_ellipsis(&a.name, w_album)),
                        Cell::from(truncate_to_width_with_ellipsis(&a.artist, w_artist)),
                    ])
                    .style(style)
                })
                .collect();
            let table = Table::new(
                rows,
                [Constraint::Percentage(50), Constraint::Percentage(50)],
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
        SearchTab::Artists => {
            let header = Row::new(vec!["Artist"]).style(header_style).height(1);
            let rows: Vec<Row> = state
                .search_results
                .artists
                .iter()
                .enumerate()
                .map(|(i, artist)| {
                    let style = if i == sel {
                        state.active_theme.selected_style()
                    } else {
                        state.active_theme.base_style()
                    };
                    Row::new(vec![Cell::from(truncate_to_width_with_ellipsis(
                        &artist.name,
                        inner.width.saturating_sub(1),
                    ))])
                    .style(style)
                })
                .collect();
            let table = Table::new(rows, [Constraint::Percentage(100)])
                .column_spacing(1)
                .header(header)
                .row_highlight_style(state.active_theme.selected_style())
                .highlight_symbol(" ")
                .highlight_spacing(HighlightSpacing::Always);
            let mut ts = TableState::default();
            ts.select(Some(sel));
            frame.render_stateful_widget(table, inner, &mut ts);
        }
    }
}
