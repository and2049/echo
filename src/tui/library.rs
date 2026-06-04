use crate::app::{ActiveView, AppMode, AppState};
use crate::tui::render::{
    DURATION_COLUMN_WIDTH, format_duration_text, format_time, padded_library_list,
    repair_wide_grapheme_trailing_styles, row_text_width, stabilize_terminal_emoji_width,
    truncate_to_width_with_ellipsis,
};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, HighlightSpacing, ListItem, ListState, Row, StatefulWidget, Table,
        TableState,
    },
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
    let p_title = if state.active_library_tab == crate::app::LibraryTab::Playlists {
        format!(
            "[{}]",
            crate::i18n::t("ui.playlists", &state.library_config.language)
        )
    } else {
        format!(
            " {} ",
            crate::i18n::t("ui.playlists", &state.library_config.language)
        )
    };
    let a_title = if state.active_library_tab == crate::app::LibraryTab::Albums {
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
    let b_title = if state.active_library_tab == crate::app::LibraryTab::Browse {
        format!(
            "[{}]",
            crate::i18n::t("ui.browse", &state.library_config.language)
        )
    } else {
        format!(
            " {} ",
            crate::i18n::t("ui.browse", &state.library_config.language)
        )
    };
    let title_text = format!("{}{}{}", p_title, a_title, b_title);

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
                state
                    .active_theme
                    .selected_style()
                    .bg(state.active_theme.primary)
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
                    state
                        .active_theme
                        .selected_style()
                        .bg(state.active_theme.primary)
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
    } else if state.active_library_tab == crate::app::LibraryTab::Browse {
        let items: Vec<ListItem> =
            vec!["📈 Top Tracks", "🕒 Recently Played", "👤 Followed Artists"]
                .into_iter()
                .enumerate()
                .map(|(i, name)| {
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
                    } else if is_focused && i == state.selected_playlist_index {
                        state.active_theme.selected_style()
                    } else {
                        state.active_theme.base_style()
                    };
                    let text = stabilize_terminal_emoji_width(name);
                    ListItem::new(truncate_to_width_with_ellipsis(&text, library_text_width))
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
    let is_album_context = state
        .active_tracklist_context
        .as_ref()
        .map(|context| context.is_album())
        .unwrap_or(false);

    let visual_range = if state.active_view == ActiveView::TrackList {
        state.get_visual_selection_range()
    } else {
        None
    };

    let is_liked_songs = state
        .active_tracklist_context
        .as_ref()
        .map_or(false, |context| context.id == "LIKED_SONGS");

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
                state
                    .active_theme
                    .selected_style()
                    .bg(state.active_theme.primary)
            } else if i == state.selected_track_index {
                state.active_theme.selected_style()
            } else if is_match {
                state
                    .active_theme
                    .base_style()
                    .fg(state.active_theme.secondary)
            } else {
                state.active_theme.base_style()
            };

            let prefix = if Some(t.id.clone()) == state.playback.playing_track_id {
                "▶ "
            } else {
                ""
            };

            let number_cell = if state.library_config.track_index_base < 0 {
                Cell::from("")
            } else {
                Cell::from(format!(
                    "{:>3}",
                    (i as isize) + state.library_config.track_index_base
                ))
            };

            let liked_str = if is_liked_songs {
                ""
            } else if state.liked_tracks.contains(&t.id) {
                "♥"
            } else {
                " "
            };

            let is_selected = is_in_visual || i == state.selected_track_index;
            let liked_cell = if is_selected {
                Cell::from(liked_str)
            } else {
                Cell::from(liked_str).style(Style::default().fg(state.active_theme.secondary))
            };

            let title_cell = Cell::from(format!(
                "{}{}",
                prefix,
                stabilize_terminal_emoji_width(&t.name)
            ));

            let duration_cell = Cell::from(format_duration_text(format_time(t.duration_ms / 1000)));

            let row = if is_album_context {
                Row::new(vec![number_cell, liked_cell, title_cell, duration_cell])
            } else {
                let artist_cell = Cell::from(stabilize_terminal_emoji_width(&t.artist));
                Row::new(vec![
                    number_cell,
                    liked_cell,
                    title_cell,
                    artist_cell,
                    duration_cell,
                ])
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
        .title(crate::i18n::t("ui.tracks", &state.library_config.language))
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(track_border_style);
    let track_inner_area = track_block.inner(tracks_area);

    let header_style = track_border_style.add_modifier(Modifier::BOLD);

    let liked_width = if is_liked_songs { 0 } else { 2 };

    let table = if is_album_context {
        let number_header = if state.library_config.track_index_base < 0 {
            ""
        } else {
            "  #"
        };
        let header = Row::new(vec![number_header, "", "Track", "Duration "])
            .style(header_style)
            .height(1);
        let number_width = if state.library_config.track_index_base < 0 {
            0
        } else {
            4
        };
        let mut t = Table::new(
            track_rows,
            [
                Constraint::Length(number_width),
                Constraint::Length(liked_width),
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
        let number_header = if state.library_config.track_index_base < 0 {
            ""
        } else {
            "  #"
        };
        let header = Row::new(vec![number_header, "", "Track", "Artist", "Duration "])
            .style(header_style)
            .height(1);
        let number_width = if state.library_config.track_index_base < 0 {
            0
        } else {
            4
        };
        let mut t = Table::new(
            track_rows,
            [
                Constraint::Length(number_width),
                Constraint::Length(liked_width),
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

    let mut header_info: Option<(String, String)> = None;
    if !state.tracks.is_empty() {
        header_info = state
            .active_tracklist_context
            .as_ref()
            .map(|context| (context.title.clone(), context.subtitle.clone()));
    }

    let (header_area, table_area) = if header_info.is_some() {
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

    if let Some(h_area) = header_area
        && let Some((title, author)) = header_info
    {
        let has_image =
            state.active_library_header_image.is_some() || state.header_image_cache.is_some();
        let image_width = if has_image { 14 } else { 2 };

        let chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([
                ratatui::layout::Constraint::Length(image_width),
                ratatui::layout::Constraint::Min(0),
            ])
            .split(h_area);

        let img_area = Rect {
            x: chunks[0].x + if has_image { 2 } else { 0 },
            y: chunks[0].y + 1, // 1 top margin
            width: if has_image { 10 } else { 0 },
            height: if has_image { 5 } else { 0 },
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

        let title_para = ratatui::widgets::Paragraph::new(title).style(
            Style::default()
                .fg(state.active_theme.primary)
                .add_modifier(Modifier::BOLD),
        );
        let author_para = ratatui::widgets::Paragraph::new(author)
            .style(Style::default().fg(state.active_theme.secondary));
        let count_para = ratatui::widgets::Paragraph::new(format!(
            "{} {}",
            state.tracks.len(),
            crate::i18n::t("ui.tracks", &state.library_config.language)
        ))
        .style(Style::default().fg(Color::DarkGray));

        frame.render_widget(title_para, text_chunks[1]);
        frame.render_widget(author_para, text_chunks[2]);
        frame.render_widget(count_para, text_chunks[3]);
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

            let gradient_lines: Vec<Line> = ECHO_LOGO
                .iter()
                .map(|&line| {
                    let mut spans = Vec::new();
                    for (i, c) in line.chars().enumerate() {
                        let t = i as f32 / logo_width as f32;
                        let base_color =
                            lerp_color(state.active_theme.secondary, state.active_theme.primary, t);

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
                })
                .collect();
            let gradient_area = Rect {
                x: track_inner_area.x + x_offset,
                y: track_inner_area.y + y_offset,
                width: logo_width,
                height: logo_height,
            };
            frame.render_widget(
                ratatui::widgets::Paragraph::new(gradient_lines),
                gradient_area,
            );
        }
    }
}

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
    use crate::app::ArtistPageTab;
    use crate::tui::render::format_time;

    // Build tab bar title
    let artist_name = state
        .artist_page_data
        .as_ref()
        .map(|d| d.artist_name.clone())
        .unwrap_or_default();

    let is_active = state.active_view == crate::app::ActiveView::ArtistPage;
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
        // ── Top Tracks tab ──────────────────────────────────────────────────
        if data.top_tracks.is_empty() {
            let p = ratatui::widgets::Paragraph::new("  No top tracks found.")
                .style(state.active_theme.muted_style());
            frame.render_widget(p, inner_area);
            return;
        }

        // Column widths: # (3) | title (flex) | duration (9)
        let duration_w = DURATION_COLUMN_WIDTH;
        let num_w: u16 = 3;
        let available = inner_area.width.saturating_sub(num_w + duration_w + 2);
        // Split title/artist 60/40
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
        // ── Albums tab ──────────────────────────────────────────────────────
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
                // Format: " Album Name  (2024)"
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
