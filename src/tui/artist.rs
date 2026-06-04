use crate::{
    app::{ActiveView, AppState, ArtistPageTab},
    tui::render::{
        DURATION_COLUMN_WIDTH, format_duration_text, format_time, padded_library_list,
        stabilize_terminal_emoji_width, truncate_to_width_with_ellipsis,
    },
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Modifier,
    widgets::{
        Block, Borders, Cell, HighlightSpacing, ListItem, ListState, Row, StatefulWidget, Table,
        TableState,
    },
};

pub fn render_artist_list(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("  Followed Artists  ")
        .style(state.active_theme.base_style())
        .border_style(if state.active_view == ActiveView::ArtistList {
            state.active_theme.secondary_style()
        } else {
            state.active_theme.primary_style()
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let items: Vec<ListItem> = state
        .followed_artists
        .iter()
        .enumerate()
        .map(|(i, artist)| {
            let style = if i == state.selected_artist_index
                && state.active_view == ActiveView::ArtistList
            {
                state.active_theme.selected_style()
            } else {
                state.active_theme.base_style()
            };

            let text = format!(" {} ", stabilize_terminal_emoji_width(&artist.name));
            ListItem::new(truncate_to_width_with_ellipsis(&text, inner_area.width)).style(style)
        })
        .collect();

    let list = padded_library_list(items).highlight_style(
        state
            .active_theme
            .selected_style()
            .add_modifier(Modifier::BOLD),
    );

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_artist_index));
    frame.render_stateful_widget(list, inner_area, &mut list_state);
}

pub fn render_artist_page(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let artist_name = state
        .artist_page_data
        .as_ref()
        .map(|d| d.artist_name.clone())
        .unwrap_or_default();

    let is_active = state.active_view == ActiveView::ArtistPage;
    let on_tracks = state.artist_page_tab == ArtistPageTab::TopTracks;

    let tab_tracks = if on_tracks {
        " [Top Tracks] "
    } else {
        "  Top Tracks  "
    };
    let tab_albums = if !on_tracks {
        " [Albums] "
    } else {
        "  Albums  "
    };
    let title = format!("  {}  {}|{}", artist_name, tab_tracks, tab_albums);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(state.active_theme.base_style())
        .border_style(if is_active {
            state.active_theme.secondary_style()
        } else {
            state.active_theme.primary_style()
        });

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if state.artist_page_loading {
        let p =
            ratatui::widgets::Paragraph::new("  Loading…").style(state.active_theme.muted_style());
        frame.render_widget(p, inner_area);
        return;
    }

    let Some(data) = state.artist_page_data.as_ref() else {
        let p = ratatui::widgets::Paragraph::new("  Artist page unavailable.")
            .style(state.active_theme.muted_style());
        frame.render_widget(p, inner_area);
        return;
    };

    if on_tracks {
        if data.top_tracks.is_empty() {
            let p = ratatui::widgets::Paragraph::new("  No top tracks found.")
                .style(state.active_theme.muted_style());
            frame.render_widget(p, inner_area);
            return;
        }

        let duration_w = DURATION_COLUMN_WIDTH;
        let num_w: u16 = 3;
        let available = inner_area.width.saturating_sub(num_w + duration_w + 2);
        let title_w = available * 6 / 10;
        let artist_w = available.saturating_sub(title_w);

        let selected_idx = state.artist_page_track_index;
        let rows: Vec<Row> = data
            .top_tracks
            .iter()
            .enumerate()
            .map(|(i, track)| {
                let is_playing = state
                    .playback
                    .playing_track_id
                    .as_deref()
                    .map(|pid| pid == track.id.as_str())
                    .unwrap_or(false);

                let num_str = if is_playing {
                    "▶".to_string()
                } else {
                    format!("{}", i + 1)
                };

                let style = if i == selected_idx && is_active {
                    state.active_theme.selected_style()
                } else if is_playing {
                    state.active_theme.primary_style()
                } else {
                    state.active_theme.base_style()
                };

                let duration_str = format_duration_text(format_time(track.duration_ms / 1000));
                let title_cell =
                    truncate_to_width_with_ellipsis(&format!(" {}", track.name), title_w);
                let artist_cell = truncate_to_width_with_ellipsis(&track.artist, artist_w);

                Row::new(vec![
                    Cell::from(num_str).style(state.active_theme.muted_style()),
                    Cell::from(title_cell),
                    Cell::from(artist_cell).style(state.active_theme.muted_style()),
                    Cell::from(duration_str).style(state.active_theme.muted_style()),
                ])
                .style(style)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(num_w),
                Constraint::Length(title_w),
                Constraint::Length(artist_w),
                Constraint::Length(duration_w),
            ],
        )
        .highlight_style(
            state
                .active_theme
                .selected_style()
                .add_modifier(Modifier::BOLD),
        )
        .highlight_spacing(HighlightSpacing::Always);

        let mut table_state = TableState::default();
        table_state.select(Some(selected_idx));
        StatefulWidget::render(table, inner_area, frame.buffer_mut(), &mut table_state);
    } else {
        if data.albums.is_empty() {
            let p = ratatui::widgets::Paragraph::new("  No albums found.")
                .style(state.active_theme.muted_style());
            frame.render_widget(p, inner_area);
            return;
        }

        let selected_idx = state.artist_page_album_index;
        let items: Vec<ListItem> = data
            .albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let style = if i == selected_idx && is_active {
                    state.active_theme.selected_style()
                } else {
                    state.active_theme.base_style()
                };
                let year_part = if album.release_year.is_empty() {
                    String::new()
                } else {
                    format!("  ({})", album.release_year)
                };
                let text = format!(" {}{}", album.name, year_part);
                let display =
                    truncate_to_width_with_ellipsis(&text, inner_area.width.saturating_sub(1));
                ListItem::new(display).style(style)
            })
            .collect();

        let list = padded_library_list(items).highlight_style(
            state
                .active_theme
                .selected_style()
                .add_modifier(Modifier::BOLD),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(selected_idx));
        frame.render_stateful_widget(list, inner_area, &mut list_state);
    }
}
