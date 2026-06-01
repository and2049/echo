use crate::app::{ActiveView, AppMode, AppState};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::Modifier,
    text::Line,
    widgets::{Block, Borders, Cell, HighlightSpacing, ListItem, ListState, Row, Table, TableState},
};
use crate::tui::mod::{row_text_width, truncate_to_width_with_ellipsis, stabilize_terminal_emoji_width, padded_library_list, repair_wide_grapheme_trailing_styles, format_duration_text, format_time};

pub fn render_library_list(frame: &mut Frame, state: &AppState, library_area: Rect) {
    let is_focused = state.active_view == ActiveView::Library;
    let title_text = match state.active_library_tab {
        crate::app::LibraryTab::Playlists => "[ Playlists ] Albums ",
        crate::app::LibraryTab::Albums => " Playlists [ Albums ]",
    };

    let library_border_style = if is_focused {
        state.active_theme.secondary_style()
    } else {
        state.active_theme.primary_style()
    };

    let library_block = Block::default()
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(library_border_style)
        .title(title_text);
    let library_list_area = library_block.inner(library_area);
    let library_text_width = super::row_text_width(library_list_area);
    frame.render_widget(library_block, library_area);

    let library_items: Vec<ListItem> = state
        .library_view
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let style = if i == state.selected_playlist_index {
                state.active_theme.selected_style()
            } else {
                state.active_theme.base_style()
            };

            match node {
                crate::models::LibraryNode::Folder(f) => {
                    let prefix = if f.is_open { "▼" } else { "▶" };
                    let text = super::truncate_to_width_with_ellipsis(
                        &format!("{} {}", prefix, super::stabilize_terminal_emoji_width(&f.name)),
                        library_text_width,
                    );
                    let folder_style = if i == state.selected_playlist_index {
                        style
                    } else {
                        state.active_theme.primary_style()
                    };
                    ListItem::new(text).style(folder_style.add_modifier(Modifier::BOLD))
                }
                crate::models::LibraryNode::Playlist { playlist, indent } => {
                    let mut prefix = String::new();
                    for _ in 0..*indent {
                        prefix.push_str("  ");
                    }
                    if state.library_config.pinned.contains(&playlist.id) {
                        prefix.push_str("📌 ");
                    }

                    let text = format!(
                        "{}{}",
                        prefix,
                        super::stabilize_terminal_emoji_width(&playlist.name)
                    );
                    let text = super::truncate_to_width_with_ellipsis(&text, library_text_width);

                    // Mark as ghosted if it is in the cut register
                    let list_style = if state.operation_register.contains(&playlist.id) {
                        style.fg(state.active_theme.text_muted)
                    } else {
                        style
                    };

                    ListItem::new(text).style(list_style)
                }
            }
        })
        .collect();

    if state.active_library_tab == crate::app::LibraryTab::Albums {
        let items: Vec<ListItem> = state
            .saved_albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let style = if is_focused && i == state.selected_playlist_index {
                    state.active_theme.selected_style()
                } else {
                    state.active_theme.base_style()
                };
                ListItem::new(super::truncate_to_width_with_ellipsis(
                    &super::stabilize_terminal_emoji_width(&album.name),
                    library_text_width,
                ))
                .style(style)
            })
            .collect();

        let list = super::padded_library_list(items).highlight_style(
            state
                .active_theme
                .selected_style()
                .add_modifier(Modifier::BOLD),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_playlist_index));
        frame.render_stateful_widget(list, library_list_area, &mut list_state);
        super::repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    } else {
        let playlist_list = super::padded_library_list(library_items);
        let mut playlist_state = ListState::default();
        playlist_state.select(Some(state.selected_playlist_index));
        frame.render_stateful_widget(playlist_list, library_list_area, &mut playlist_state);
        super::repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    }
}

pub fn render_track_list(frame: &mut Frame, state: &AppState, tracks_area: Rect) {
    let is_albums_tab = state.active_library_tab == crate::app::LibraryTab::Albums;

    let track_rows: Vec<Row> = state
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_match = state.mode == AppMode::Search && state.search_matches.contains(&i);
            
            let style = if i == state.selected_track_index {
                state.active_theme.selected_style()
            } else if is_match {
                state.active_theme.base_style().fg(state.active_theme.secondary)
            } else {
                state.active_theme.base_style()
            };
            
            let prefix = if Some(t.id.clone()) == state.playback.playing_track_id {
                "▶ "
            } else {
                ""
            };

            let track_cell = if state.library_config.track_index_base < 0 {
                Cell::from(format!(
                    "{}{}",
                    prefix,
                    super::stabilize_terminal_emoji_width(&t.name)
                ))
            } else {
                Cell::from(format!(
                    "{:>3} {}{}",
                    (i as isize) + state.library_config.track_index_base,
                    prefix,
                    super::stabilize_terminal_emoji_width(&t.name)
                ))
            };
            let duration_cell = Cell::from(super::format_duration_text(super::format_time(t.duration_ms / 1000)));

            let row = if is_albums_tab {
                Row::new(vec![track_cell, duration_cell])
            } else {
                let artist_cell = Cell::from(super::stabilize_terminal_emoji_width(&t.artist));
                Row::new(vec![track_cell, artist_cell, duration_cell])
            };

            row.style(style)
        })
        .collect();

    let is_track_focused = state.active_view == ActiveView::TrackList;
    let track_border_style = if is_track_focused {
        state.active_theme.secondary_style()
    } else {
        state.active_theme.primary_style()
    };

    let track_block = Block::default()
        .title(" Tracks ")
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(track_border_style);
    let track_inner_area = track_block.inner(tracks_area);

    let header_style = track_border_style.add_modifier(Modifier::BOLD);

    let table = if is_albums_tab {
        let header_str = if state.library_config.track_index_base < 0 { "Track" } else { "  # Track" };
        let header = Row::new(vec![header_str, "Duration "])
            .style(header_style)
            .height(1);
        Table::new(
            track_rows,
            [
                Constraint::Min(20),
                Constraint::Length(super::DURATION_COLUMN_WIDTH),
            ],
        )
        .column_spacing(1)
        .header(header)
        .block(track_block)
        .row_highlight_style(state.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always)
    } else {
        let header_str = if state.library_config.track_index_base < 0 { "Track" } else { "  # Track" };
        let header = Row::new(vec![header_str, "Artist", "Duration "])
            .style(header_style)
            .height(1);
        Table::new(
            track_rows,
            [
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Length(super::DURATION_COLUMN_WIDTH),
            ],
        )
        .column_spacing(1)
        .header(header)
        .block(track_block)
        .row_highlight_style(state.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always)
    };

    let mut ts = TableState::default();
    let sel = if state.tracks.is_empty() {
        0
    } else {
        state.selected_track_index.min(state.tracks.len() - 1)
    };
    ts.select(Some(sel));
    frame.render_stateful_widget(table, track_inner_area, &mut ts);
}
