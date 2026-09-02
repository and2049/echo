use crate::tui::render::{
    DURATION_COLUMN_WIDTH, format_duration_text, format_time, padded_library_list,
    repair_wide_grapheme_trailing_styles, row_text_width, stabilize_terminal_emoji_width,
    truncate_to_width_with_ellipsis,
};
use crate::tui::theme::{ThemeStyles, ToRatatui};
use echo_core::app::{ActiveView, AppMode, AppState, displayed_track_number};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, HighlightSpacing, ListItem, ListState, Row, Table, TableState,
    },
};

pub use crate::tui::artist::{render_artist_list, render_artist_page, render_whats_new};

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
    let is_focused = state.ui.active_view == ActiveView::Library;
    let p_title = if state.ui.active_library_tab == echo_core::app::LibraryTab::Playlists {
        format!(
            "[{}]",
            echo_core::i18n::t("ui.playlists", &state.ui.library_config.language)
        )
    } else {
        format!(
            " {} ",
            echo_core::i18n::t("ui.playlists", &state.ui.library_config.language)
        )
    };
    let a_title = if state.ui.active_library_tab == echo_core::app::LibraryTab::Albums {
        format!(
            "[{}]",
            echo_core::i18n::t("ui.albums", &state.ui.library_config.language)
        )
    } else {
        format!(
            " {} ",
            echo_core::i18n::t("ui.albums", &state.ui.library_config.language)
        )
    };
    let b_title = if state.ui.active_library_tab == echo_core::app::LibraryTab::Browse {
        format!(
            "[{}]",
            echo_core::i18n::t("ui.browse", &state.ui.library_config.language)
        )
    } else {
        format!(
            " {} ",
            echo_core::i18n::t("ui.browse", &state.ui.library_config.language)
        )
    };
    let title_text = format!("{} {} {}", p_title, a_title, b_title);

    let library_border_style = if is_focused {
        state.ui.active_theme.secondary_style()
    } else {
        state.ui.active_theme.primary_style()
    };

    let library_block = Block::default()
        .borders(Borders::ALL)
        .style(state.ui.active_theme.base_style())
        .border_style(library_border_style)
        .title(title_text);
    let library_list_area = library_block.inner(library_area);
    let library_text_width = row_text_width(library_list_area);
    frame.render_widget(library_block, library_area);

    let visual_range = if is_focused && state.ui.mode == AppMode::Visual {
        state.get_visual_selection_range()
    } else {
        None
    };

    if state.ui.library_config.library_thumbnails
        && state.ui.active_library_tab != echo_core::app::LibraryTab::Browse
    {
        render_library_thumbnails(frame, state, library_list_area, is_focused, visual_range);
        return;
    }

    let library_items: Vec<ListItem> = state
        .data
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
                    .ui
                    .active_theme
                    .selected_style()
                    .bg(state.ui.active_theme.primary.rat())
            } else if i == state.ui.selected_playlist_index {
                state.ui.active_theme.selected_style()
            } else {
                state.ui.active_theme.base_style()
            };

            match node {
                echo_core::models::LibraryNode::Folder(f) => {
                    let prefix = if f.is_open { "▼" } else { "▶" };
                    let text = truncate_to_width_with_ellipsis(
                        &format!("{} {}", prefix, stabilize_terminal_emoji_width(&f.name)),
                        library_text_width,
                    );
                    let folder_style = if i == state.ui.selected_playlist_index {
                        style
                    } else {
                        state.ui.active_theme.primary_style()
                    };
                    ListItem::new(text).style(folder_style.add_modifier(Modifier::BOLD))
                }
                echo_core::models::LibraryNode::Playlist { playlist, indent } => {
                    let mut prefix = String::new();
                    for _ in 0..*indent {
                        prefix.push_str("  ");
                    }
                    if state.ui.library_config.pinned.contains(&playlist.id) {
                        prefix.push_str("📌 ");
                    }

                    let text = format!(
                        "{}{}",
                        prefix,
                        stabilize_terminal_emoji_width(&playlist.name)
                    );
                    let text = truncate_to_width_with_ellipsis(&text, library_text_width);

                    // Mark as ghosted if it is in the cut register
                    let list_style = if state.ui.operation_register.contains(&playlist.id) {
                        style.fg(state.ui.active_theme.text_muted.rat())
                    } else {
                        style
                    };

                    ListItem::new(text).style(list_style)
                }
            }
        })
        .collect();

    if state.ui.active_library_tab == echo_core::app::LibraryTab::Albums {
        let items: Vec<ListItem> = state
            .data
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
                        .ui
                        .active_theme
                        .selected_style()
                        .bg(state.ui.active_theme.primary.rat())
                } else if is_focused && i == state.ui.selected_playlist_index {
                    state.ui.active_theme.selected_style()
                } else {
                    state.ui.active_theme.base_style()
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
                .ui
                .active_theme
                .selected_style()
                .add_modifier(Modifier::BOLD),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.ui.selected_playlist_index));
        frame.render_stateful_widget(list, library_list_area, &mut list_state);
        repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    } else if state.ui.active_library_tab == echo_core::app::LibraryTab::Browse {
        let items: Vec<ListItem> = vec![
            "📈 Top Tracks",
            "🕒 Recently Played",
            "👤 Followed Artists",
            "⭐ Top Artists",
            "🆕 What's New",
        ]
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
                    .ui
                    .active_theme
                    .selected_style()
                    .bg(state.ui.active_theme.primary.rat())
            } else if is_focused && i == state.ui.selected_playlist_index {
                state.ui.active_theme.selected_style()
            } else {
                state.ui.active_theme.base_style()
            };
            let text = stabilize_terminal_emoji_width(name);
            ListItem::new(truncate_to_width_with_ellipsis(&text, library_text_width)).style(style)
        })
        .collect();

        let list = padded_library_list(items).highlight_style(
            state
                .ui
                .active_theme
                .selected_style()
                .add_modifier(Modifier::BOLD),
        );

        let mut list_state = ListState::default();
        list_state.select(Some(state.ui.selected_playlist_index));
        frame.render_stateful_widget(list, library_list_area, &mut list_state);
        repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    } else {
        let playlist_list = padded_library_list(library_items);
        let mut playlist_state = ListState::default();
        playlist_state.select(Some(state.ui.selected_playlist_index));
        frame.render_stateful_widget(playlist_list, library_list_area, &mut playlist_state);
        repair_wide_grapheme_trailing_styles(frame.buffer_mut(), library_list_area);
    }
}

struct ThumbRow {
    title: String,
    subtitle: String,
    thumb_url: Option<String>,
    indent: u16,
    ghosted: bool,
    is_folder: bool,
}

/// Folders have no cover art and stay compact single-liners; everything else
/// gets a cover-sized multi-line row.
fn thumb_row_height(row: &ThumbRow) -> u16 {
    if row.is_folder {
        1
    } else {
        echo_core::thumbnails::ROW_H
    }
}

/// Scroll math mirroring a per-frame recreated `ListState`, generalized to
/// variable row heights: the viewport only ever scrolls down far enough that
/// the selected row is fully visible at the bottom.
pub fn thumb_first_visible(selected: usize, heights: &[u16], viewport: u16) -> usize {
    if heights.is_empty() {
        return 0;
    }
    let selected = selected.min(heights.len() - 1);
    let mut used = 0u16;
    let mut first = selected;
    for i in (0..=selected).rev() {
        if used.saturating_add(heights[i]) > viewport {
            // Selected row taller than the viewport: render it clipped.
            return if i == selected { selected } else { first };
        }
        used = used.saturating_add(heights[i]);
        first = i;
    }
    first
}

fn render_library_thumbnails(
    frame: &mut Frame,
    state: &mut AppState,
    area: Rect,
    is_focused: bool,
    visual_range: Option<(usize, usize)>,
) {
    use echo_core::thumbnails::{THUMB_H, THUMB_W, ThumbState};

    let rows: Vec<ThumbRow> = if state.ui.active_library_tab == echo_core::app::LibraryTab::Albums {
        state
            .data
            .saved_albums
            .iter()
            .map(|album| ThumbRow {
                title: stabilize_terminal_emoji_width(&album.name),
                subtitle: album.artists.clone(),
                thumb_url: album.thumb_url.clone().or_else(|| album.image_url.clone()),
                indent: 0,
                ghosted: false,
                is_folder: false,
            })
            .collect()
    } else {
        state
            .data
            .library_view
            .iter()
            .map(|node| match node {
                echo_core::models::LibraryNode::Folder(f) => ThumbRow {
                    title: format!(
                        "{} {}",
                        if f.is_open { "▼" } else { "▶" },
                        stabilize_terminal_emoji_width(&f.name)
                    ),
                    subtitle: String::new(),
                    thumb_url: None,
                    indent: 0,
                    ghosted: false,
                    is_folder: true,
                },
                echo_core::models::LibraryNode::Playlist { playlist, indent } => {
                    let mut prefix = String::new();
                    if state.ui.library_config.pinned.contains(&playlist.id) {
                        prefix.push_str("📌 ");
                    }
                    ThumbRow {
                        title: format!(
                            "{}{}",
                            prefix,
                            stabilize_terminal_emoji_width(&playlist.name)
                        ),
                        subtitle: playlist.owner.clone(),
                        thumb_url: playlist
                            .thumb_url
                            .clone()
                            .or_else(|| playlist.image_url.clone()),
                        indent: (*indent as u16).saturating_mul(2),
                        ghosted: state.ui.operation_register.contains(&playlist.id),
                        is_folder: false,
                    }
                }
            })
            .collect()
    };

    if rows.is_empty() {
        return;
    }

    let heights: Vec<u16> = rows.iter().map(thumb_row_height).collect();
    let selected = state.ui.selected_playlist_index.min(rows.len() - 1);
    let first = thumb_first_visible(selected, &heights, area.height);
    let mut last = first;
    let mut used = 0u16;
    while last < rows.len() && used < area.height {
        used = used.saturating_add(heights[last]);
        last += 1;
    }

    // Request pass: queue loads for any visible thumbnail not yet decoded.
    for row in &rows[first..last] {
        let Some(url) = row.thumb_url.as_deref() else {
            continue;
        };
        if !state.ui.thumbnails.entries.contains_key(url) {
            state.ui.thumbnails.request(url);
        }
    }

    let base_style = state.ui.active_theme.base_style();
    let selected_style = state.ui.active_theme.selected_style();
    let visual_style = state.ui.active_theme.selected_style().bg(state
        .ui
        .active_theme
        .primary
        .rat());
    let folder_style = state
        .ui
        .active_theme
        .primary_style()
        .add_modifier(Modifier::BOLD);
    let muted = state.ui.active_theme.text_muted.rat();
    let show_selection =
        state.ui.active_library_tab == echo_core::app::LibraryTab::Playlists || is_focused;

    let mut y = area.y;
    for i in first..last {
        let row = &rows[i];
        if y >= area.bottom() {
            break;
        }
        let row_bottom = (y + heights[i]).min(area.bottom());
        let in_visual = visual_range.is_some_and(|(start, end)| i >= start && i <= end);
        let mut style = if in_visual {
            visual_style
        } else if show_selection && i == selected {
            selected_style
        } else if row.is_folder {
            folder_style
        } else {
            base_style
        };
        if row.is_folder {
            style = style.add_modifier(Modifier::BOLD);
        }
        if row.ghosted {
            style = style.fg(muted);
        }

        let buf = frame.buffer_mut();
        for yy in y..row_bottom {
            for xx in area.left()..area.right() {
                let cell = &mut buf[(xx, yy)];
                cell.set_style(style);
                cell.set_symbol(" ");
            }
        }

        let img_x = area.x + 1 + row.indent;
        let mut text_x = area.x + 1 + row.indent;
        if !row.is_folder && img_x + THUMB_W < area.right() {
            let img_area = Rect {
                x: img_x,
                y,
                width: THUMB_W,
                height: THUMB_H.min(row_bottom.saturating_sub(y)),
            };
            let artwork =
                row.thumb_url
                    .as_deref()
                    .and_then(|url| match state.ui.thumbnails.get(url) {
                        Some(ThumbState::Ready { artwork }) => Some(artwork.clone()),
                        _ => None,
                    });
            if let Some(artwork) = artwork {
                crate::tui::image::draw(buf, img_area, &artwork);
            } else {
                draw_thumb_placeholder(buf, img_area, style.fg(muted), "♪");
            }
            text_x = img_x + THUMB_W + 1;
        }

        if text_x < area.right() {
            let text_w = (area.right() - text_x).saturating_sub(1);
            let title = truncate_to_width_with_ellipsis(&row.title, text_w);
            buf.set_stringn(text_x, y, &title, text_w as usize, style);
            if !row.subtitle.is_empty() && y + 1 < row_bottom {
                let subtitle = truncate_to_width_with_ellipsis(&row.subtitle, text_w);
                buf.set_stringn(text_x, y + 1, &subtitle, text_w as usize, style.fg(muted));
            }
            // Repair wide-grapheme trailers over the text columns only; the
            // image cells carry protocol payloads and must not be restyled.
            repair_wide_grapheme_trailing_styles(
                buf,
                Rect {
                    x: text_x,
                    y,
                    width: area.right() - text_x,
                    height: row_bottom.saturating_sub(y),
                },
            );
        }

        y = row_bottom;
    }
}

fn draw_thumb_placeholder(buf: &mut Buffer, area: Rect, style: Style, symbol: &str) {
    if area.width < 2 || area.height == 0 {
        return;
    }
    let right = area.x + area.width - 1;
    let bottom_row = area.y + area.height - 1;
    for y in area.y..=bottom_row {
        for x in area.x..=right {
            let cell = &mut buf[(x, y)];
            let glyph = match (y, x) {
                (yy, xx) if yy == area.y && xx == area.x => "┌",
                (yy, xx) if yy == area.y && xx == right => "┐",
                (yy, xx) if yy == bottom_row && xx == area.x => "└",
                (yy, xx) if yy == bottom_row && xx == right => "┘",
                (yy, _) if yy == area.y || yy == bottom_row => "─",
                (_, xx) if xx == area.x || xx == right => "│",
                _ => " ",
            };
            cell.set_style(style);
            cell.set_symbol(glyph);
        }
    }
    if !symbol.is_empty() && area.height >= 2 {
        let center_x = area.x + area.width / 2;
        let center_y = area.y + area.height / 2;
        buf[(center_x, center_y)].set_symbol(symbol);
    }
}

pub fn render_track_list(frame: &mut Frame, state: &mut AppState, tracks_area: Rect) {
    let is_album_context = state
        .data
        .active_tracklist_context
        .as_ref()
        .map(|context| context.is_album())
        .unwrap_or(false);

    let visual_range = if state.ui.active_view == ActiveView::TrackList {
        state.get_visual_selection_range()
    } else {
        None
    };

    let is_liked_songs = state
        .data
        .active_tracklist_context
        .as_ref()
        .is_some_and(|context| context.id == "LIKED_SONGS");

    let track_rows: Vec<Row> = state
        .data
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_match = state.ui.mode == AppMode::Search && state.ui.search_matches.contains(&i);

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
            } else if i == state.ui.selected_track_index {
                state.ui.active_theme.selected_style()
            } else if is_match {
                state
                    .ui
                    .active_theme
                    .base_style()
                    .fg(state.ui.active_theme.secondary.rat())
            } else {
                state.ui.active_theme.base_style()
            };

            let prefix = if Some(t.id.clone()) == state.playback.playing_track_id {
                "▶ "
            } else {
                ""
            };

            let number_cell = if state.ui.library_config.track_index_base < 0 {
                Cell::from("")
            } else {
                Cell::from(format!(
                    "{:>3}",
                    displayed_track_number(
                        i,
                        state.ui.selected_track_index,
                        state.ui.library_config.track_index_base,
                        state.ui.library_config.relative_line_numbers,
                    )
                ))
            };

            let liked_str = if is_liked_songs {
                ""
            } else if state.data.liked_tracks.contains(&t.id) {
                "♥"
            } else {
                " "
            };

            let is_selected = is_in_visual || i == state.ui.selected_track_index;
            let liked_cell = if is_selected {
                Cell::from(liked_str)
            } else {
                Cell::from(liked_str)
                    .style(Style::default().fg(state.ui.active_theme.secondary.rat()))
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

    let is_track_focused = state.ui.active_view == ActiveView::TrackList;
    let track_border_style = if is_track_focused {
        state.ui.active_theme.secondary_style()
    } else {
        state.ui.active_theme.primary_style()
    };

    let track_block = Block::default()
        .title(echo_core::i18n::t(
            "ui.tracks",
            &state.ui.library_config.language,
        ))
        .borders(Borders::ALL)
        .style(state.ui.active_theme.base_style())
        .border_style(track_border_style);
    let track_inner_area = track_block.inner(tracks_area);

    let header_style = track_border_style.add_modifier(Modifier::BOLD);

    let liked_width = if is_liked_songs { 0 } else { 2 };

    let table = if is_album_context {
        let number_header = if state.ui.library_config.track_index_base < 0 {
            ""
        } else {
            "  #"
        };
        let header = Row::new(vec![number_header, "", "Track", "Duration "])
            .style(header_style)
            .height(1);
        let number_width = if state.ui.library_config.track_index_base < 0 {
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
        .row_highlight_style(state.ui.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always);

        if !state.data.tracks.is_empty() {
            t = t.header(header);
        }
        t
    } else {
        let number_header = if state.ui.library_config.track_index_base < 0 {
            ""
        } else {
            "  #"
        };
        let header = Row::new(vec![number_header, "", "Track", "Artist", "Duration "])
            .style(header_style)
            .height(1);
        let number_width = if state.ui.library_config.track_index_base < 0 {
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
        .row_highlight_style(state.ui.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always);

        if !state.data.tracks.is_empty() {
            t = t.header(header);
        }
        t
    };

    frame.render_widget(track_block, tracks_area);

    let mut header_info: Option<(String, String)> = None;
    if !state.data.tracks.is_empty() {
        header_info = state
            .data
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
        let has_image = state.ui.active_library_header_image.is_some();
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

        if let Some(artwork) = state.ui.active_library_header_image.clone() {
            crate::tui::image::draw(frame.buffer_mut(), img_area, &artwork);
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
                .fg(state.ui.active_theme.primary.rat())
                .add_modifier(Modifier::BOLD),
        );
        let author_para = ratatui::widgets::Paragraph::new(author)
            .style(Style::default().fg(state.ui.active_theme.secondary.rat()));
        let count_para = ratatui::widgets::Paragraph::new(format!(
            "{} {}",
            state.data.tracks.len(),
            echo_core::i18n::t("ui.tracks", &state.ui.library_config.language)
        ))
        .style(Style::default().fg(Color::DarkGray));

        frame.render_widget(title_para, text_chunks[1]);
        frame.render_widget(author_para, text_chunks[2]);
        frame.render_widget(count_para, text_chunks[3]);
    }

    let mut ts = TableState::default();
    let sel = if state.data.tracks.is_empty() {
        0
    } else {
        state
            .ui
            .selected_track_index
            .min(state.data.tracks.len() - 1)
    };
    ts.select(Some(sel));
    frame.render_stateful_widget(table, table_area, &mut ts);

    if state.data.tracks.is_empty() {
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
                        let base_color = lerp_color(
                            state.ui.active_theme.secondary.rat(),
                            state.ui.active_theme.primary.rat(),
                            t,
                        );

                        let style = if c == '█' {
                            Style::default().fg(base_color)
                        } else if c != ' ' {
                            let (r, g, b) = color_to_rgb(base_color);
                            let (bg_r, bg_g, bg_b) =
                                color_to_rgb(state.ui.active_theme.background.rat());
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

#[cfg(test)]
mod thumb_tests {
    use super::thumb_first_visible;

    fn visible_height(first: usize, selected: usize, heights: &[u16]) -> u16 {
        heights[first..=selected].iter().sum()
    }

    #[test]
    fn selection_is_always_fully_visible() {
        let uniform = vec![3u16; 50];
        let mixed: Vec<u16> = (0..50).map(|i| if i % 4 == 0 { 1 } else { 3 }).collect();
        for heights in [&uniform, &mixed] {
            for viewport in [3u16, 7, 20] {
                for selected in [0usize, 1, 10, 49, 60] {
                    let first = thumb_first_visible(selected, heights, viewport);
                    let clamped = selected.min(heights.len() - 1);
                    assert!(first <= clamped);
                    assert!(visible_height(first, clamped, heights) <= viewport);
                }
            }
        }
    }

    #[test]
    fn empty_list_starts_at_zero() {
        assert_eq!(thumb_first_visible(0, &[], 5), 0);
    }

    #[test]
    fn short_list_starts_at_top() {
        assert_eq!(thumb_first_visible(2, &[3, 3, 3], 24), 0);
    }

    #[test]
    fn selection_is_last_visible_once_past_first_page() {
        // 15 rows of height 3, viewport 9 -> rows 8..=10 visible.
        assert_eq!(thumb_first_visible(10, &[3u16; 15], 9), 8);
    }

    #[test]
    fn single_line_folders_let_more_rows_fit() {
        // folder(1) + three playlists(3) = 10 cells: all fit in a 10-tall
        // viewport, whereas uniform 3-tall rows would not.
        let heights = [1u16, 3, 3, 3];
        assert_eq!(thumb_first_visible(3, &heights, 10), 0);
        assert_eq!(thumb_first_visible(3, &[3u16, 3, 3, 3], 10), 1);
    }

    #[test]
    fn oversized_selected_row_renders_clipped() {
        assert_eq!(thumb_first_visible(2, &[3u16, 3, 3], 2), 2);
    }
}
