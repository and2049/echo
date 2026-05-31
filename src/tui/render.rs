use crate::app::{ActiveView, AppMode, AppState};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Gauge, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Table, TableState,
    },
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ROW_TEXT_LEFT_GUTTER: u16 = 1;
const ROW_TEXT_RIGHT_GUTTER: u16 = 1;
const DURATION_COLUMN_WIDTH: u16 = 9;

pub fn render_app(frame: &mut Frame, state: &AppState) {
    fill_background(frame, state);

    if state.mode == AppMode::Setup {
        render_setup(frame, state);
        return;
    }

    if state.mode == AppMode::Authenticating {
        render_authenticating(frame, state);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),     // Main content
            Constraint::Length(10), // Playback Bar (7 info + 1 pb + 2 border)
            Constraint::Length(1),  // Command bar
        ])
        .split(frame.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Length(1),
            Constraint::Percentage(70),
        ])
        .split(chunks[0]);
    let library_area = main_chunks[0];
    let tracks_area = main_chunks[2];

    // Render Main Content
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
                let style = if is_focused && i == state.selected_playlist_index {
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

    let format_time = |s: u32| format!("{}:{:02}", s / 60, s % 60);

    let is_albums_tab = state.active_library_tab == crate::app::LibraryTab::Albums;

    let track_rows: Vec<Row> = state
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let style = if i == state.selected_track_index {
                state.active_theme.selected_style()
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
        Table::new(
            track_rows,
            [
                Constraint::Min(20),
                Constraint::Length(DURATION_COLUMN_WIDTH),
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
                Constraint::Percentage(45),
                Constraint::Min(20),
                Constraint::Length(DURATION_COLUMN_WIDTH),
            ],
        )
        .column_spacing(1)
        .header(header)
        .block(track_block)
        .row_highlight_style(state.active_theme.selected_style())
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always)
    };

    let mut track_state = TableState::default();
    track_state.select(Some(state.selected_track_index));
    frame.render_stateful_widget(table, tracks_area, &mut track_state);
    repair_wide_grapheme_trailing_styles(frame.buffer_mut(), track_inner_area);

    // Render Playback Bar Border
    let shuffle_str = if state.playback.is_shuffled {
        "On"
    } else {
        "Off"
    };
    let border_title = format!(
        " Playing (Shuffle: {:<7} | Repeat: {:<7} | Volume: {:>3}%) ",
        shuffle_str, state.playback.repeat_mode, state.playback.volume
    );

    let playback_block = Block::default()
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(state.active_theme.primary_style())
        .title(border_title);

    let playback_inner = playback_block.inner(chunks[1]);
    frame.render_widget(playback_block, chunks[1]);

    let playback_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // Track Info
            Constraint::Length(1), // Progress bar
        ])
        .split(playback_inner);

    // Render Progress Bar
    let pb = &state.playback;
    let ratio = if pb.duration_ms > 0 {
        (pb.progress_ms as f64 / pb.duration_ms as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let progress_sec = pb.progress_ms / 1000;
    let duration_sec = pb.duration_ms / 1000;

    let progress_str = format_time(progress_sec);
    let duration_str = format_time(duration_sec);

    let pb_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(6), // current time
            Constraint::Min(0),    // gauge
            Constraint::Length(6), // duration
        ])
        .split(playback_chunks[1]);

    let current_time_p = Paragraph::new(progress_str)
        .alignment(Alignment::Right)
        .style(state.active_theme.base_style());
    let total_time_p = Paragraph::new(duration_str)
        .alignment(Alignment::Left)
        .style(state.active_theme.base_style());

    let gauge = Gauge::default()
        .gauge_style(state.active_theme.gauge_style())
        .ratio(ratio)
        .label(""); // hide inner text

    frame.render_widget(current_time_p, pb_chunks[0]);

    // Add a tiny margin around the gauge to separate it from the text
    let mut gauge_area = pb_chunks[1];
    if gauge_area.width > 2 {
        gauge_area.x += 1;
        gauge_area.width -= 2;
    }
    frame.render_widget(gauge, gauge_area);
    frame.render_widget(total_time_p, pb_chunks[2]);

    // Render Track Info
    let track_info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),  // Left padding
            Constraint::Length(10), // Image width
            Constraint::Length(2),  // Middle gap to text
            Constraint::Min(0),     // Text width
        ])
        .split(playback_chunks[0]);

    if let Some(ref protocol) = state.playback.playing_track_image {
        let image = ratatui_image::Image::new(protocol);
        let mut image_area = track_info_chunks[1];
        // Center vertically in the 7-row tall block (1 row top padding, 1 row bottom padding)
        if image_area.height >= 7 {
            image_area.y += 1;
            image_area.height = 5;
        }
        frame.render_widget(image, image_area);
    }

    // Create Title & Artist Text
    let track_title = if state.playback.playing_track_title.is_empty() {
        String::new()
    } else {
        stabilize_terminal_emoji_width(&state.playback.playing_track_title)
    };

    let track_artist = stabilize_terminal_emoji_width(&state.playback.playing_track_artist);

    let text_lines = vec![
        Line::from(Span::styled(
            track_title,
            state.active_theme.base_style().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(track_artist, state.active_theme.muted_style())),
    ];
    let track_text_p = Paragraph::new(text_lines)
        .alignment(Alignment::Left)
        .style(state.active_theme.base_style())
        // Add top padding to vertically align with the center of the image
        .block(Block::default().padding(ratatui::widgets::Padding::new(0, 0, 2, 0)));
    frame.render_widget(track_text_p, track_info_chunks[3]);

    // Render Command Bar
    let (cmd_text, cmd_style) = match state.mode {
        AppMode::Command => (
            format!(":{}", state.command_buffer),
            state.active_theme.base_style(),
        ),
        _ => (
            state.status_message.clone().unwrap_or_default(),
            state.active_theme.muted_style(),
        ),
    };
    let cmd_bar = Paragraph::new(cmd_text).style(cmd_style);
    frame.render_widget(cmd_bar, chunks[2]);

    if let Some(folder_name) = &state.folder_delete_prompt {
        let popup_area = centered_rect(60, 40, frame.area());
        let popup = Paragraph::new(vec![
            Line::from(Span::styled(
                format!(
                    "Are you sure you want to delete the folder '{}'?",
                    folder_name
                ),
                state.active_theme.error_style(),
            )),
            Line::from(""),
            Line::from("Any playlists inside will be safely returned to the main library."),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'y' to confirm or any other key to cancel.",
                state.active_theme.base_style().add_modifier(Modifier::BOLD),
            )),
        ])
        .style(state.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Delete Folder ")
                .style(state.active_theme.base_style())
                .border_style(state.active_theme.error_style()),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    // Check if we are waiting for discovery
    if std::path::Path::new("echo-librespot-status.log").exists() {
        let popup_area = centered_rect(60, 30, frame.area());
        let popup = Paragraph::new(vec![
            Line::from("Spotify Connect Onboarding Required")
                .style(state.active_theme.secondary_style()),
            Line::from(""),
            Line::from("1. Open the official Spotify app on your phone or desktop."),
            Line::from("2. Tap the 'Devices' icon."),
            Line::from("3. Select 'Echo TUI' from the list of available devices."),
            Line::from(""),
            Line::from("Once connected, Echo will automatically transition to normal operation!"),
        ])
        .style(state.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Setup ")
                .style(state.active_theme.base_style())
                .border_style(state.active_theme.secondary_style()),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }
}

fn fill_background(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let style = state.active_theme.base_style();
    let buffer = frame.buffer_mut();
    buffer.set_style(area, style);

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buffer[(x, y)].set_symbol(" ");
        }
    }
}

fn repair_wide_grapheme_trailing_styles(buffer: &mut Buffer, area: Rect) {
    let area = buffer.area().intersection(area);

    for y in area.top()..area.bottom() {
        let mut x = area.left();
        while x < area.right() {
            let width = buffer[(x, y)].symbol().width() as u16;
            if width > 1 {
                let style = buffer[(x, y)].style();
                for hidden_x in (x + 1)..x.saturating_add(width).min(area.right()) {
                    buffer[(hidden_x, y)].set_style(style).set_skip(true);
                }
            }
            x = x.saturating_add(width.max(1));
        }
    }
}

fn padded_library_list(items: Vec<ListItem>) -> List {
    List::new(items)
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always)
}

fn row_text_width(area: Rect) -> u16 {
    area.width
        .saturating_sub(ROW_TEXT_LEFT_GUTTER + ROW_TEXT_RIGHT_GUTTER)
}

fn format_duration_text(time: String) -> String {
    format!("{:>8} ", time)
}

fn truncate_to_width_with_ellipsis(text: &str, max_width: u16) -> String {
    let max_width = max_width as usize;

    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis = "...";
    let ellipsis_width = ellipsis.width();
    if max_width == 0 {
        return String::new();
    }
    if max_width <= ellipsis_width {
        return ".".repeat(max_width);
    }

    let content_width = max_width - ellipsis_width;
    let mut truncated = String::new();
    let mut width = 0;

    for grapheme in UnicodeSegmentation::graphemes(text, true) {
        let grapheme_width = grapheme.width();
        if width + grapheme_width > content_width {
            break;
        }

        truncated.push_str(grapheme);
        width += grapheme_width;
    }

    truncated.push_str(ellipsis);
    truncated
}

fn stabilize_terminal_emoji_width(text: &str) -> String {
    let mut stabilized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        stabilized.push(ch);

        if needs_emoji_variation_selector(ch)
            && !matches!(chars.peek(), Some('\u{fe0e}' | '\u{fe0f}'))
        {
            stabilized.push('\u{fe0f}');
        }
    }

    stabilized
}

fn needs_emoji_variation_selector(ch: char) -> bool {
    matches!(ch, '\u{1f578}')
}

// Helper function to create a centered rect
fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let popup_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
            ratatui::layout::Constraint::Percentage(percent_y),
            ratatui::layout::Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
            ratatui::layout::Constraint::Percentage(percent_x),
            ratatui::layout::Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_authenticating(frame: &mut Frame, state: &AppState) {
    let block = Block::default()
        .title(" Authenticating ")
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(state.active_theme.primary_style());
    let text = vec![
        Line::from("Waiting for Spotify authentication..."),
        Line::from(
            "Please check your browser. A local server is listening on port 8888 for the redirect.",
        ),
    ];
    let paragraph = Paragraph::new(text)
        .style(state.active_theme.base_style())
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, frame.area());
}

fn render_setup(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage(30),
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Percentage(30),
        ])
        .split(area);

    let inner_layout = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(10),
            ratatui::layout::Constraint::Percentage(80),
            ratatui::layout::Constraint::Percentage(10),
        ])
        .split(layout[1]);

    let setup_area = inner_layout[1];

    let block = Block::default()
        .title(" BYOK Setup (Bring Your Own Key) ")
        .borders(Borders::ALL)
        .style(state.active_theme.base_style())
        .border_style(state.active_theme.primary_style());

    let id_style = if !state.setup_focus_secret {
        state.active_theme.secondary_style()
    } else {
        state.active_theme.base_style()
    };

    let secret_style = if state.setup_focus_secret {
        state.active_theme.secondary_style()
    } else {
        state.active_theme.base_style()
    };

    let text = vec![
        Line::from("Spotify Developer credentials not found in config.toml"),
        Line::from("Please paste your Client ID and Client Secret."),
        Line::from("Press [TAB] to switch fields, [ENTER] to save and authenticate."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Client ID: ", id_style),
            Span::raw(&state.setup_client_id),
        ]),
        Line::from(vec![
            Span::styled("Client Secret: ", secret_style),
            Span::raw(&state.setup_client_secret),
        ]),
    ];

    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Left);
    let paragraph = paragraph.style(state.active_theme.base_style());
    frame.render_widget(paragraph, setup_area);
}

#[cfg(test)]
mod tests {
    use super::{
        DURATION_COLUMN_WIDTH, format_duration_text, repair_wide_grapheme_trailing_styles,
        row_text_width, stabilize_terminal_emoji_width, truncate_to_width_with_ellipsis,
    };
    use ratatui::{
        buffer::Buffer,
        layout::Rect,
        style::{Color, Style},
    };
    use unicode_width::UnicodeWidthStr;

    fn render_row_text(text: &str, style: Style) -> Buffer {
        let area = Rect::new(0, 0, 24, 1);
        let mut buffer = Buffer::empty(area);
        buffer.set_style(area, style);
        buffer.set_string(0, 0, text, style);
        buffer
    }

    #[test]
    fn repairs_liked_songs_hidden_cell_style() {
        let style = Style::default().fg(Color::White).bg(Color::Rgb(12, 34, 56));
        let mut buffer = render_row_text("♥️ Liked Songs", style);

        assert_eq!(buffer[(1, 0)].bg, Color::Reset);

        repair_wide_grapheme_trailing_styles(&mut buffer, Rect::new(0, 0, 24, 1));

        assert_eq!(buffer[(1, 0)].symbol(), " ");
        assert_eq!(buffer[(1, 0)].fg, Color::White);
        assert_eq!(buffer[(1, 0)].bg, Color::Rgb(12, 34, 56));
        assert!(buffer[(1, 0)].skip);
    }

    #[test]
    fn repairs_pinned_playlist_hidden_cell_style() {
        let style = Style::default().fg(Color::Cyan).bg(Color::Rgb(8, 9, 10));
        let mut buffer = render_row_text("📌 Playlist", style);

        assert_eq!(buffer[(1, 0)].bg, Color::Reset);

        repair_wide_grapheme_trailing_styles(&mut buffer, Rect::new(0, 0, 24, 1));

        assert_eq!(buffer[(1, 0)].symbol(), " ");
        assert_eq!(buffer[(1, 0)].fg, Color::Cyan);
        assert_eq!(buffer[(1, 0)].bg, Color::Rgb(8, 9, 10));
        assert!(buffer[(1, 0)].skip);
    }

    #[test]
    fn leaves_ascii_playlist_names_unchanged() {
        let style = Style::default().fg(Color::White).bg(Color::Rgb(1, 2, 3));
        let mut buffer = render_row_text("Plain Playlist", style);
        let original = buffer.clone();

        repair_wide_grapheme_trailing_styles(&mut buffer, Rect::new(0, 0, 24, 1));

        assert_eq!(buffer, original);
    }

    #[test]
    fn preserves_selected_style_on_hidden_cells() {
        let style = Style::default().fg(Color::Black).bg(Color::White);
        let mut buffer = render_row_text("📌 Selected Playlist", style);

        assert_eq!(buffer[(1, 0)].bg, Color::Reset);

        repair_wide_grapheme_trailing_styles(&mut buffer, Rect::new(0, 0, 24, 1));

        assert_eq!(buffer[(1, 0)].fg, Color::Black);
        assert_eq!(buffer[(1, 0)].bg, Color::White);
        assert!(buffer[(1, 0)].skip);
    }

    #[test]
    fn repair_does_not_cross_area_boundary() {
        let selected = Style::default().fg(Color::Black).bg(Color::White);
        let border = Style::default().fg(Color::Magenta).bg(Color::Rgb(1, 2, 3));
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));
        buffer[(3, 0)].set_symbol("📌").set_style(selected);
        buffer[(4, 0)].set_symbol("│").set_style(border);

        repair_wide_grapheme_trailing_styles(&mut buffer, Rect::new(1, 0, 3, 1));

        assert_eq!(buffer[(4, 0)].symbol(), "│");
        assert_eq!(buffer[(4, 0)].fg, Color::Magenta);
        assert_eq!(buffer[(4, 0)].bg, Color::Rgb(1, 2, 3));
    }

    #[test]
    fn repair_skips_hidden_cells_in_diff() {
        let old_style = Style::default().fg(Color::White).bg(Color::Black);
        let new_style = Style::default().fg(Color::Black).bg(Color::White);
        let mut previous = render_row_text("♥️ Liked Songs", old_style);
        repair_wide_grapheme_trailing_styles(&mut previous, Rect::new(0, 0, 24, 1));

        let mut next = render_row_text("♥️ Liked Songs", new_style);
        repair_wide_grapheme_trailing_styles(&mut next, Rect::new(0, 0, 24, 1));

        let diff = previous.diff(&next);

        assert!(
            diff.iter()
                .any(|(x, y, cell)| { *x == 0 && *y == 0 && cell.symbol() == "♥️" })
        );
        assert!(!diff.iter().any(|(x, y, _)| *x == 1 && *y == 0));
    }

    #[test]
    fn stabilizes_cobweb_to_emoji_width() {
        let stabilized = stabilize_terminal_emoji_width("🕸");

        assert_eq!(stabilized, "🕸️");
        assert_eq!(stabilized.width(), 2);
    }

    #[test]
    fn does_not_duplicate_existing_cobweb_variation_selector() {
        let stabilized = stabilize_terminal_emoji_width("🕸️");

        assert_eq!(stabilized, "🕸️");
        assert_eq!(stabilized.width(), 2);
    }

    #[test]
    fn row_text_width_reserves_left_and_right_gutters() {
        assert_eq!(row_text_width(Rect::new(0, 0, 12, 1)), 10);
        assert_eq!(row_text_width(Rect::new(0, 0, 1, 1)), 0);
    }

    #[test]
    fn truncates_long_ascii_text_with_right_gap_budget() {
        let truncated = truncate_to_width_with_ellipsis("abcdef", 5);

        assert_eq!(truncated, "ab...");
        assert_eq!(truncated.width(), 5);
    }

    #[test]
    fn keeps_text_that_fits_width_budget() {
        let text = truncate_to_width_with_ellipsis("abc", 5);

        assert_eq!(text, "abc");
    }

    #[test]
    fn truncates_wide_text_without_exceeding_width() {
        let truncated = truncate_to_width_with_ellipsis("我的歌单 36", 8);

        assert!(truncated.ends_with("..."));
        assert!(truncated.width() <= 8);
    }

    #[test]
    fn duration_text_keeps_one_trailing_cell() {
        let duration = format_duration_text("2:46".to_string());

        assert_eq!(duration, "    2:46 ");
        assert_eq!(duration.width(), DURATION_COLUMN_WIDTH as usize);
    }
}
