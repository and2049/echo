use crate::app::{ActiveView, AppMode, AppState};
use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render_app(frame: &mut Frame, state: &AppState) {
    if state.mode == AppMode::Setup {
        render_setup(frame, state);
        return;
    }

    if state.mode == AppMode::Authenticating {
        render_authenticating(frame);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(0),    // Main content
            Constraint::Length(7), // Track Info
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Command bar
        ])
        .split(frame.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // Render Header
    let mode_str = match state.mode {
        AppMode::Setup => "SETUP",
        AppMode::Authenticating => "AUTH",
        AppMode::Normal => "NORMAL",
        AppMode::Visual => "VISUAL",
        AppMode::Command => "COMMAND",
    };
    let shuffle_str = if state.playback.is_shuffled {
        " | SHUFFLE: ON "
    } else {
        ""
    };
    let header_text = Paragraph::new(Line::from(vec![Span::styled(
        format!(" ECHO [{}] {} ", mode_str, shuffle_str),
        Style::default().bg(Color::Blue).fg(Color::White),
    )]))
    .alignment(Alignment::Left);
    frame.render_widget(header_text, chunks[0]);

    // Render Main Content
    let library_items: Vec<ListItem> = state
        .library_view
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let style = if i == state.selected_playlist_index {
                Style::default().bg(Color::White).fg(Color::Black)
            } else {
                Style::default()
            };
            
            match node {
                crate::models::LibraryNode::Folder(f) => {
                    let prefix = if f.is_open { "▼" } else { "▶" };
                    let text = format!("{} {}", prefix, f.name);
                    ListItem::new(text).style(style.fg(Color::Cyan).add_modifier(ratatui::style::Modifier::BOLD))
                }
                crate::models::LibraryNode::Playlist { playlist, indent } => {
                    let mut prefix = String::new();
                    for _ in 0..*indent {
                        prefix.push_str("  ");
                    }
                    if state.library_config.pinned.contains(&playlist.id) {
                        prefix.push_str("📌 ");
                    }
                    
                    let text = format!("{}{}", prefix, playlist.name);
                    
                    // Mark as ghosted if it is in the cut register
                    let list_style = if state.operation_register.contains(&playlist.id) {
                        style.fg(Color::DarkGray)
                    } else {
                        style
                    };
                    
                    ListItem::new(text).style(list_style)
                }
            }
        })
        .collect();
    let is_focused = state.active_view == ActiveView::Library;
    let title_color = if is_focused {
        Color::Cyan
    } else {
        Color::White
    };

    let title_text = match state.active_library_tab {
        crate::app::LibraryTab::Playlists => "[ Playlists ] Albums ",
        crate::app::LibraryTab::Albums => " Playlists [ Albums ]",
    };

    let library_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_color))
        .title(title_text);

    if state.active_library_tab == crate::app::LibraryTab::Albums {
        let items: Vec<ListItem> = state
            .saved_albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let style = if is_focused && i == state.selected_playlist_index {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("{} - {}", album.name, album.artist)).style(style)
            })
            .collect();
            
        let list = List::new(items)
            .block(library_block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
            
        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_playlist_index));
        frame.render_stateful_widget(list, main_chunks[0], &mut list_state);
    } else {
        let playlist_list = List::new(library_items).block(library_block);
        let mut playlist_state = ListState::default();
        playlist_state.select(Some(state.selected_playlist_index));
        frame.render_stateful_widget(playlist_list, main_chunks[0], &mut playlist_state);
    }

    let track_items: Vec<ListItem> = state
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let style = if i == state.selected_track_index {
                Style::default().bg(Color::White).fg(Color::Black)
            } else {
                Style::default()
            };
            let prefix = if Some(t.id.clone()) == state.playback.playing_track_id {
                "▶ "
            } else {
                ""
            };
            ListItem::new(format!("{}{} - {}", prefix, t.name, t.artist)).style(style)
        })
        .collect();
    let track_style = if state.active_view == ActiveView::TrackList {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };
    let track_block = Block::default()
        .title(" Tracks ")
        .borders(Borders::ALL)
        .border_style(track_style);
    let track_list_widget = List::new(track_items).block(track_block);
    let mut track_state = ratatui::widgets::ListState::default();
    track_state.select(Some(state.selected_track_index));
    frame.render_stateful_widget(track_list_widget, main_chunks[1], &mut track_state);

    // Render Progress Bar
    let pb = &state.playback;
    let ratio = if pb.duration_ms > 0 {
        (pb.progress_ms as f64 / pb.duration_ms as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let progress_sec = pb.progress_ms / 1000;
    let duration_sec = pb.duration_ms / 1000;

    let format_time = |s: u32| format!("{}:{:02}", s / 60, s % 60);

    let progress_str = format_time(progress_sec);
    let duration_str = format_time(duration_sec);

    let pb_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(6), // current time
            Constraint::Min(0),    // gauge
            Constraint::Length(6), // duration
        ])
        .split(chunks[3]);

    let current_time_p = Paragraph::new(progress_str)
        .alignment(Alignment::Right)
        .style(Style::default().fg(Color::White));
    let total_time_p = Paragraph::new(duration_str)
        .alignment(Alignment::Left)
        .style(Style::default().fg(Color::White));

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::White).bg(Color::DarkGray))
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
        .split(chunks[2]);

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
        "Not Playing".to_string()
    } else {
        state.playback.playing_track_title.clone()
    };

    let track_artist = state.playback.playing_track_artist.clone();

    let text_lines = vec![
        Line::from(Span::styled(
            track_title,
            Style::default()
                .fg(Color::White)
                .add_modifier(ratatui::style::Modifier::BOLD),
        )),
        Line::from(Span::styled(
            track_artist,
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let track_text_p = Paragraph::new(text_lines)
        .alignment(Alignment::Left)
        // Add top padding to vertically align with the center of the image
        .block(Block::default().padding(ratatui::widgets::Padding::new(0, 0, 2, 0)));
    frame.render_widget(track_text_p, track_info_chunks[3]);

    // Render Command Bar
    let cmd_text = match state.mode {
        AppMode::Command => format!(":{}", state.command_buffer),
        _ => String::new(),
    };
    let cmd_bar = Paragraph::new(cmd_text).style(Style::default());
    frame.render_widget(cmd_bar, chunks[4]);

    if let Some(folder_name) = &state.folder_delete_prompt {
        let popup_area = centered_rect(60, 40, frame.area());
        let popup = Paragraph::new(vec![
            Line::from(Span::styled(
                format!("Are you sure you want to delete the folder '{}'?", folder_name),
                Style::default().fg(Color::Red),
            )),
            Line::from(""),
            Line::from("Any playlists inside will be safely returned to the main library."),
            Line::from(""),
            Line::from(Span::styled("Press 'y' to confirm or any other key to cancel.", Style::default().add_modifier(ratatui::style::Modifier::BOLD))),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Delete Folder ")
                .style(Style::default().bg(Color::Black)),
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
                .style(Style::default().fg(Color::Yellow)),
            Line::from(""),
            Line::from("1. Open the official Spotify app on your phone or desktop."),
            Line::from("2. Tap the 'Devices' icon."),
            Line::from("3. Select 'Echo TUI' from the list of available devices."),
            Line::from(""),
            Line::from("Once connected, Echo will automatically transition to normal operation!"),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Setup ")
                .style(Style::default().bg(Color::Black)),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }
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

fn render_authenticating(frame: &mut Frame) {
    let block = Block::default()
        .title(" Authenticating ")
        .borders(Borders::ALL);
    let text = vec![
        Line::from("Waiting for Spotify authentication..."),
        Line::from(
            "Please check your browser. A local server is listening on port 8888 for the redirect.",
        ),
    ];
    let paragraph = Paragraph::new(text)
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
        .borders(Borders::ALL);

    let id_style = if !state.setup_focus_secret {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let secret_style = if state.setup_focus_secret {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
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
    frame.render_widget(paragraph, setup_area);
}
