use echo_core::app::{ActiveView, AppState};
use crate::tui::render::{
    padded_library_list, stabilize_terminal_emoji_width, truncate_to_width_with_ellipsis,
};
use crate::tui::theme::{ThemeStyles, ToRatatui};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    widgets::{
        Block, Borders, Cell, HighlightSpacing, ListItem, ListState, Row, Table, TableState,
    },
};

pub fn render_artist_list(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Followed Artists  ")
        .style(state.ui.active_theme.base_style())
        .border_style(if state.ui.active_view == ActiveView::ArtistList {
            state.ui.active_theme.secondary_style()
        } else {
            state.ui.active_theme.primary_style()
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = state.data
        .followed_artists
        .iter()
        .enumerate()
        .map(|(i, artist)| {
            let style = if i == state.ui.selected_artist_index
                && state.ui.active_view == ActiveView::ArtistList
            {
                state.ui.active_theme.selected_style()
            } else {
                state.ui.active_theme.base_style()
            };

            let text = format!(" {} ", stabilize_terminal_emoji_width(&artist.name));
            ListItem::new(truncate_to_width_with_ellipsis(&text, inner_area.width)).style(style)
        })
        .collect();

    let list = padded_library_list(items).highlight_style(
        state.ui
            .active_theme
            .selected_style()
            .add_modifier(Modifier::BOLD),
    );

    let mut list_state = ListState::default();
    list_state.select(Some(state.ui.selected_artist_index));
    frame.render_stateful_widget(list, inner_area, &mut list_state);
}

pub fn render_artist_page(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let artist_name = state.data
        .artist_page_data
        .as_ref()
        .map(|d| d.artist_name.clone())
        .unwrap_or_default();

    let is_active = state.ui.active_view == ActiveView::ArtistPage;
    let title = format!("  {}  ", artist_name);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(state.ui.active_theme.base_style())
        .border_style(if is_active {
            state.ui.active_theme.secondary_style()
        } else {
            state.ui.active_theme.primary_style()
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let Some((artist_image_url, albums)) = state.data
        .artist_page_data
        .as_ref()
        .map(|data| (data.image_url.clone(), data.albums.clone()))
    else {
        let message = if state.data.artist_page_loading {
            "  Loading artist..."
        } else {
            "  Artist page unavailable."
        };
        let p = ratatui::widgets::Paragraph::new(message).style(state.ui.active_theme.muted_style());
        frame.render_widget(p, inner_area);
        return;
    };

    let has_image =
        artist_image_url.is_some() && state.ui.active_library_header_image.is_some();
    let (header_area, albums_area) = if has_image {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(0)])
            .split(inner_area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, inner_area)
    };

    if let Some(header_area) = header_area {
        render_artist_header(frame, state, header_area, &artist_name);
    }

    if albums.is_empty() {
        let message = if state.data.artist_albums_loading {
            "  Loading albums..."
        } else {
            "  No albums loaded."
        };
        let p = ratatui::widgets::Paragraph::new(message).style(state.ui.active_theme.muted_style());
        frame.render_widget(p, albums_area);
        return;
    }

    let selected_idx = state.ui.artist_page_album_index;
    let show_track_count = albums.iter().any(|album| album.track_count.is_some());
    let header_style = if is_active {
        state.ui
            .active_theme
            .secondary_style()
            .add_modifier(Modifier::BOLD)
    } else {
        state.ui
            .active_theme
            .primary_style()
            .add_modifier(Modifier::BOLD)
    };

    let rows: Vec<Row> = albums
        .iter()
        .enumerate()
        .map(|(i, album)| {
            let style = if i == selected_idx && is_active {
                state.ui.active_theme.selected_style()
            } else {
                state.ui.active_theme.base_style()
            };
            let album_name_width = if show_track_count {
                albums_area.width.saturating_mul(70) / 100
            } else {
                albums_area.width.saturating_mul(82) / 100
            };
            let name_cell = Cell::from(truncate_to_width_with_ellipsis(
                &stabilize_terminal_emoji_width(&album.name),
                album_name_width.saturating_sub(1),
            ));
            let year_cell = Cell::from(album.release_year.clone());
            let row = if show_track_count {
                let track_count = album
                    .track_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "-".to_string());
                Row::new(vec![name_cell, Cell::from(track_count), year_cell])
            } else {
                Row::new(vec![name_cell, year_cell])
            };
            row.style(style)
        })
        .collect();

    let table = if show_track_count {
        Table::new(
            rows,
            [
                Constraint::Percentage(70),
                Constraint::Length(8),
                Constraint::Length(6),
            ],
        )
        .header(
            Row::new(vec!["Album", "Tracks", "Year"])
                .style(header_style)
                .height(1),
        )
    } else {
        Table::new(rows, [Constraint::Percentage(82), Constraint::Length(6)]).header(
            Row::new(vec!["Album", "Year"])
                .style(header_style)
                .height(1),
        )
    }
    .column_spacing(1)
    .row_highlight_style(state.ui.active_theme.selected_style())
    .highlight_symbol(" ")
    .highlight_spacing(HighlightSpacing::Always);

    let mut table_state = TableState::default();
    table_state.select(Some(selected_idx));
    frame.render_stateful_widget(table, albums_area, &mut table_state);
}

fn render_artist_header(frame: &mut Frame, state: &mut AppState, area: Rect, artist_name: &str) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(14), Constraint::Min(0)])
        .split(area);

    let img_area = Rect {
        x: chunks[0].x + 2,
        y: chunks[0].y + 1,
        width: 10,
        height: 5,
    };

    if let Some(artwork) = state.ui.active_library_header_image.clone() {
        crate::tui::image::draw(frame.buffer_mut(), img_area, &artwork);
    }

    let text_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(chunks[1]);

    let title_para = ratatui::widgets::Paragraph::new(artist_name.to_string()).style(
        Style::default()
            .fg(state.ui.active_theme.primary.rat())
            .add_modifier(Modifier::BOLD),
    );
    frame.render_widget(title_para, text_chunks[1]);
}
