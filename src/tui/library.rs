use crate::app::{ActiveView, AppMode, AppState};
use ratatui::{
    buffer::Buffer,
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, HighlightSpacing, ListItem, ListState, Row, StatefulWidget, Table,
        TableState,
    },
};
use crate::tui::render::{
    format_duration_text, format_time, padded_library_list, repair_wide_grapheme_trailing_styles,
    row_text_width, stabilize_terminal_emoji_width, truncate_to_width_with_ellipsis,
    DURATION_COLUMN_WIDTH,
};

const ECHO_LOGO: [&str; 6] = [
    "███████╗ ██████╗██╗  ██╗ ██████╗               ██████╗ ███████╗",
    "██╔════╝██╔════╝██║  ██║██╔═══██╗              ██╔══██╗██╔════╝",
    "█████╗  ██║     ███████║██║   ██║    █████╗    ██████╔╝███████╗",
    "██╔══╝  ██║     ██╔══██║██║   ██║    ╚════╝    ██╔══██╗╚════██║",
    "███████╗╚██████╗██║  ██║╚██████╔╝              ██║  ██║███████║",
    "╚══════╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝               ╚═╝  ╚═╝╚══════╝",
];

fn color_to_rgb(color: Color) -> (f32, f32, f32) {
    match color {
        Color::Reset | Color::Black => (0., 0., 0.),
        Color::Red | Color::LightRed => (255., 0., 0.),
        Color::Green | Color::LightGreen => (0., 255., 0.),
        Color::Yellow | Color::LightYellow => (255., 255., 0.),
        Color::Blue | Color::LightBlue => (0., 0., 255.),
        Color::Magenta | Color::LightMagenta => (255., 0., 255.),
        Color::Cyan | Color::LightCyan => (0., 255., 255.),
        Color::Gray | Color::DarkGray => (128., 128., 128.),
        Color::White => (255., 255., 255.),
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        Color::Indexed(_) => (255., 255., 255.),
    }
}

fn lerp_color(c1: Color, c2: Color, t: f32) -> Color {
    let (r1, g1, b1) = color_to_rgb(c1);
    let (r2, g2, b2) = color_to_rgb(c2);
    
    let r = r1 + (r2 - r1) * t;
    let g = g1 + (g2 - g1) * t;
    let b = b1 + (b2 - b1) * t;
    
    Color::Rgb(r as u8, g as u8, b as u8)
}


pub fn render_library_list(frame: &mut Frame, state: &mut AppState, library_area: Rect) {
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
    let library_text_width = row_text_width(library_list_area);
    frame.render_widget(library_block, library_area);

    let visual_range = if is_focused && state.mode == AppMode::Visual {
        state.get_visual_selection_range()
    } else {
        None
    };

    let library_items: Vec<ListItem> = state
        .library_view
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let is_in_visual = if let Some((start, end)) = visual_range {
                i >= start && i <= end
            } else {
                false
            };

            let style = if is_in_visual {
                state.active_theme.selected_style().bg(state.active_theme.primary)
            } else if i == state.selected_playlist_index {
                state.active_theme.selected_style()
            } else {
                state.active_theme.base_style()
            };

            match node {
                crate::models::LibraryNode::Folder(f) => {
                    let prefix = if f.is_open { "▼" } else { "▶" };
                    let text = truncate_to_width_with_ellipsis(
                        &format!("{} {}", prefix, stabilize_terminal_emoji_width(&f.name)),
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
                        stabilize_terminal_emoji_width(&playlist.name)
                    );
                    let text = truncate_to_width_with_ellipsis(&text, library_text_width);

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
                let is_in_visual = if let Some((start, end)) = visual_range {
                    i >= start && i <= end
                } else {
                    false
                };
                let style = if is_in_visual {
                    state.active_theme.selected_style().bg(state.active_theme.primary)
                } else if is_focused && i == state.selected_playlist_index {
                    state.active_theme.selected_style()
                } else {
                    state.active_theme.base_style()
                };
                ListItem::new(truncate_to_width_with_ellipsis(
                    &stabilize_terminal_emoji_width(&album.name),
                    library_text_width,
                ))
                .style(style)
            })
            .collect();

        let list = padded_library_list(items).highlight_style(
            state
                .active_theme
                .selected_style()
                .add_modifier(Modifier::BOLD),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_playlist_index));
        frame.render_stateful_widget(list, library_list_area, &mut list_state);
        repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    } else {
        let playlist_list = padded_library_list(library_items);
        let mut playlist_state = ListState::default();
        playlist_state.select(Some(state.selected_playlist_index));
        frame.render_stateful_widget(playlist_list, library_list_area, &mut playlist_state);
        repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    }
}

pub fn render_track_list(frame: &mut Frame, state: &mut AppState, tracks_area: Rect) {
    let is_albums_tab = state.active_library_tab == crate::app::LibraryTab::Albums;

    let visual_range = if state.active_view == ActiveView::TrackList {
        state.get_visual_selection_range()
    } else {
        None
    };

    let track_rows: Vec<Row> = state
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_match = state.mode == AppMode::Search && state.search_matches.contains(&i);
            
            let is_in_visual = if let Some((start, end)) = visual_range {
                i >= start && i <= end
            } else {
                false
            };
            
            let style = if is_in_visual {
                state.active_theme.selected_style().bg(state.active_theme.primary)
            } else if i == state.selected_track_index {
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
                    stabilize_terminal_emoji_width(&t.name)
                ))
            } else {
                Cell::from(format!(
                    "{:>3} {}{}",
                    (i as isize) + state.library_config.track_index_base,
                    prefix,
                    stabilize_terminal_emoji_width(&t.name)
                ))
            };
            let duration_cell = Cell::from(format_duration_text(format_time(t.duration_ms / 1000)));

            let row = if is_albums_tab {
                Row::new(vec![track_cell, duration_cell])
            } else {
                let artist_cell = Cell::from(stabilize_terminal_emoji_width(&t.artist));
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
        let mut t = Table::new(
            track_rows,
            [
                Constraint::Min(20),
                Constraint::Length(DURATION_COLUMN_WIDTH),
            ],
        )
        .column_spacing(1)
        .row_highlight_style(state.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always);

        if !state.tracks.is_empty() {
            t = t.header(header);
        }
        t
    } else {
        let header_str = if state.library_config.track_index_base < 0 { "Track" } else { "  # Track" };
        let header = Row::new(vec![header_str, "Artist", "Duration "])
            .style(header_style)
            .height(1);
        let mut t = Table::new(
            track_rows,
            [
                Constraint::Percentage(50),
                Constraint::Percentage(50),
                Constraint::Length(DURATION_COLUMN_WIDTH),
            ],
        )
        .column_spacing(1)
        .row_highlight_style(state.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always);

        if !state.tracks.is_empty() {
            t = t.header(header);
        }
        t
    };

    frame.render_widget(track_block, tracks_area);

    let mut header_info: Option<(String, String, String, String)> = None;
    if state.active_view == ActiveView::TrackList && !state.tracks.is_empty() {
        header_info = state.tracklist_context_metadata.clone();
    }

    let (header_area, table_area) = if let Some(_) = header_info {
        let chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Length(7), // Header height
                ratatui::layout::Constraint::Min(0),
            ])
            .split(track_inner_area);
        (Some(chunks[0]), chunks[1])
    } else {
        (None, track_inner_area)
    };

    if let Some(h_area) = header_area {
        if let Some((_, title, author, _)) = header_info {
            let chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Horizontal)
                .constraints([
                    ratatui::layout::Constraint::Length(14), // 10 for image + 4 margin
                    ratatui::layout::Constraint::Min(0),
                ])
                .split(h_area);
            
            let img_area = Rect {
                x: chunks[0].x + 2, // 2 left margin
                y: chunks[0].y + 1, // 1 top margin
                width: 10,
                height: 5,
            };

            if state.header_image_dirty {
                if let Some(ref mut protocol) = state.active_library_header_image {
                    let cache_area = Rect::new(0, 0, img_area.width, img_area.height);
                    let mut cached = Buffer::empty(cache_area);
                    let image = ratatui_image::StatefulImage::default();
                    StatefulWidget::render(image, cache_area, &mut cached, protocol);
                    state.header_image_cache = Some(cached);
                }
                state.header_image_dirty = false;
            }

            if let Some(ref cached) = state.header_image_cache {
                let buf = frame.buffer_mut();
                for y in 0..cached.area.height.min(img_area.height) {
                    for x in 0..cached.area.width.min(img_area.width) {
                        let src = &cached[(x, y)];
                        let dst = &mut buf[(img_area.x + x, img_area.y + y)];
                        dst.set_style(src.style());
                        dst.set_symbol(src.symbol());
                        dst.set_skip(src.skip);
                    }
                }
            }

            let text_chunks = ratatui::layout::Layout::default()
                .direction(ratatui::layout::Direction::Vertical)
                .constraints([
                    ratatui::layout::Constraint::Min(0),
                    ratatui::layout::Constraint::Length(1),
                    ratatui::layout::Constraint::Length(1),
                    ratatui::layout::Constraint::Length(1),
                    ratatui::layout::Constraint::Min(0),
                ])
                .split(chunks[1]);

            let title_para = ratatui::widgets::Paragraph::new(title)
                .style(Style::default().fg(state.active_theme.primary).add_modifier(Modifier::BOLD));
            let author_para = ratatui::widgets::Paragraph::new(author)
                .style(Style::default().fg(state.active_theme.secondary));
            let count_para = ratatui::widgets::Paragraph::new(format!("{} tracks", state.tracks.len()))
                .style(Style::default().fg(Color::DarkGray));

            frame.render_widget(title_para, text_chunks[1]);
            frame.render_widget(author_para, text_chunks[2]);
            frame.render_widget(count_para, text_chunks[3]);
        }
    }

    let mut ts = TableState::default();
    let sel = if state.tracks.is_empty() {
        0
    } else {
        state.selected_track_index.min(state.tracks.len() - 1)
    };
    ts.select(Some(sel));
    frame.render_stateful_widget(table, table_area, &mut ts);

    if state.tracks.is_empty() {
        let logo_height = ECHO_LOGO.len() as u16;
        let logo_width = 63; // Width of the longest line in ECHO_LOGO
        
        if track_inner_area.width > logo_width && track_inner_area.height > logo_height {
            let x_offset = (track_inner_area.width - logo_width) / 2;
            let y_offset = (track_inner_area.height - logo_height) / 2;
            
            let gradient_lines: Vec<Line> = ECHO_LOGO.iter().map(|&line| {
                let mut spans = Vec::new();
                for (i, c) in line.chars().enumerate() {
                    let t = i as f32 / logo_width as f32;
                    let base_color = lerp_color(state.active_theme.secondary, state.active_theme.primary, t);
                    
                    let style = if c == '█' {
                        Style::default().fg(base_color)
                    } else if c != ' ' {
                        let (r, g, b) = color_to_rgb(base_color);
                        let (bg_r, bg_g, bg_b) = color_to_rgb(state.active_theme.background);
                        let alpha = 0.4;
                        let shadow_color = Color::Rgb(
                            (r * alpha + bg_r * (1.0 - alpha)) as u8,
                            (g * alpha + bg_g * (1.0 - alpha)) as u8,
                            (b * alpha + bg_b * (1.0 - alpha)) as u8,
                        );
                        Style::default().fg(shadow_color)
                    } else {
                        Style::default()
                    };
                    spans.push(Span::styled(c.to_string(), style));
                }
                Line::from(spans)
            }).collect();
            let gradient_area = Rect {
                x: track_inner_area.x + x_offset,
                y: track_inner_area.y + y_offset,
                width: logo_width,
                height: logo_height,
            };
            frame.render_widget(ratatui::widgets::Paragraph::new(gradient_lines), gradient_area);
        }
    }
}
