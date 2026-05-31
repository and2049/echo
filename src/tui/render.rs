use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, List, ListItem, Gauge},
    Frame,
};
use crate::app::{AppMode, AppState, ActiveView};

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
            Constraint::Length(5), // Track Info
            Constraint::Length(1), // Progress bar
            Constraint::Length(1), // Command bar
        ])
        .split(frame.area());

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(70),
        ])
        .split(chunks[1]);

    // Render Header
    let mode_str = match state.mode {
        AppMode::Setup => "SETUP",
        AppMode::Authenticating => "AUTH",
        AppMode::Normal => "NORMAL",
        AppMode::Visual => "VISUAL",
        AppMode::Command => "COMMAND",
    };
    let shuffle_str = if state.playback.is_shuffled { " | SHUFFLE: ON " } else { "" };
    let header_text = Paragraph::new(Line::from(vec![
        Span::styled(format!(" ECHO [{}] {} ", mode_str, shuffle_str), Style::default().bg(Color::Blue).fg(Color::White)),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(header_text, chunks[0]);

    // Render Main Content
    let library_items: Vec<ListItem> = state.playlists.iter().enumerate().map(|(i, p)| {
        let style = if i == state.selected_playlist_index { Style::default().bg(Color::White).fg(Color::Black) } else { Style::default() };
        ListItem::new(p.name.clone()).style(style)
    }).collect();
    let library_style = if state.active_view == ActiveView::Library { Style::default().fg(Color::Green) } else { Style::default() };
    let playlist_block = Block::default().title(" Library (Playlists) ").borders(Borders::ALL).border_style(library_style);
    let playlist_list = List::new(library_items).block(playlist_block);
    let mut playlist_state = ratatui::widgets::ListState::default();
    playlist_state.select(Some(state.selected_playlist_index));
    frame.render_stateful_widget(playlist_list, main_chunks[0], &mut playlist_state);

    let track_items: Vec<ListItem> = state.tracks.iter().enumerate().map(|(i, t)| {
        let style = if i == state.selected_track_index { Style::default().bg(Color::White).fg(Color::Black) } else { Style::default() };
        let prefix = if Some(t.id.clone()) == state.playback.playing_track_id { "▶ " } else { "" };
        ListItem::new(format!("{}{} - {}", prefix, t.name, t.artist)).style(style)
    }).collect();
    let track_style = if state.active_view == ActiveView::TrackList { Style::default().fg(Color::Green) } else { Style::default() };
    let track_block = Block::default().title(" Tracks ").borders(Borders::ALL).border_style(track_style);
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
        
    let current_time_p = Paragraph::new(progress_str).alignment(Alignment::Right).style(Style::default().fg(Color::White));
    let total_time_p = Paragraph::new(duration_str).alignment(Alignment::Left).style(Style::default().fg(Color::White));
    
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
            Constraint::Length(14), // Image width
            Constraint::Min(0),     // Text width
        ])
        .split(chunks[2]);
        
    if let Some(ref protocol) = state.playback.playing_track_image {
        let image = ratatui_image::Image::new(protocol);
        frame.render_widget(image, track_info_chunks[0]);
    }
    
    // Create Title & Artist Text
    let track_title = if state.playback.playing_track_title.is_empty() {
        "Not Playing".to_string()
    } else {
        state.playback.playing_track_title.clone()
    };
    
    let track_artist = state.playback.playing_track_artist.clone();
    
    let text_lines = vec![
        Line::from(Span::styled(track_title, Style::default().fg(Color::White).add_modifier(ratatui::style::Modifier::BOLD))),
        Line::from(Span::styled(track_artist, Style::default().fg(Color::DarkGray))),
    ];
    let track_text_p = Paragraph::new(text_lines).alignment(Alignment::Left).block(Block::default().padding(ratatui::widgets::Padding::new(1, 0, 1, 0)));
    frame.render_widget(track_text_p, track_info_chunks[1]);

    // Render Command Bar
    let cmd_text = match state.mode {
        AppMode::Command => ":",
        _ => "",
    };
    let cmd_bar = Paragraph::new(cmd_text).style(Style::default());
    frame.render_widget(cmd_bar, chunks[3]);

    // Check if we are waiting for discovery
    if std::path::Path::new("echo-librespot-status.log").exists() {
        let popup_area = centered_rect(60, 30, frame.area());
        let popup = Paragraph::new(vec![
            Line::from("Spotify Connect Onboarding Required").style(Style::default().fg(Color::Yellow)),
            Line::from(""),
            Line::from("1. Open the official Spotify app on your phone or desktop."),
            Line::from("2. Tap the 'Devices' icon."),
            Line::from("3. Select 'Echo TUI' from the list of available devices."),
            Line::from(""),
            Line::from("Once connected, Echo will automatically transition to normal operation!"),
        ])
        .block(Block::default().borders(Borders::ALL).title(" Setup ").style(Style::default().bg(Color::Black)))
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }
}

// Helper function to create a centered rect
fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
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
        Line::from("Please check your browser. A local server is listening on port 8888 for the redirect."),
    ];
    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Center);
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
        Line::from(vec![Span::styled("Client ID: ", id_style), Span::raw(&state.setup_client_id)]),
        Line::from(vec![Span::styled("Client Secret: ", secret_style), Span::raw(&state.setup_client_secret)]),
    ];
    
    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Left);
    frame.render_widget(paragraph, setup_area);
}

