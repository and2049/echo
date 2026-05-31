use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, List, ListItem},
    Frame,
};
use crate::app::{AppMode, AppState, ActiveView};
use crate::tui::layout::AppLayout;

pub fn render_app(frame: &mut Frame, state: &AppState) {
    if state.mode == AppMode::Setup {
        render_setup(frame, state);
        return;
    }
    
    if state.mode == AppMode::Authenticating {
        render_authenticating(frame);
        return;
    }

    let layout = AppLayout::compute(frame.area());

    // Render Header
    let mode_str = match state.mode {
        AppMode::Setup => "SETUP",
        AppMode::Authenticating => "AUTH",
        AppMode::Normal => "NORMAL",
        AppMode::Visual => "VISUAL",
        AppMode::Command => "COMMAND",
    };
    let header_text = Paragraph::new(Line::from(vec![
        Span::styled(format!(" ECHO [{}] ", mode_str), Style::default().bg(Color::Blue).fg(Color::White)),
    ]))
    .alignment(Alignment::Left);
    frame.render_widget(header_text, layout.header);

    // Render Main Content
    let (items_str, title, selected_idx) = match state.active_view {
        ActiveView::Library => {
            let items: Vec<String> = state.playlists.iter().map(|p| p.name.clone()).collect();
            (items, " Library (Playlists) ", state.selected_playlist_index)
        }
        ActiveView::TrackList => {
            let items: Vec<String> = state.tracks.iter().map(|t| format!("{} - {}", t.name, t.artist)).collect();
            (items, " Tracks ", state.selected_track_index)
        }
    };

    let list_items: Vec<ListItem> = items_str.into_iter().enumerate().map(|(i, item)| {
        let style = if i == selected_idx {
            Style::default().bg(Color::White).fg(Color::Black)
        } else {
            Style::default()
        };
        ListItem::new(item).style(style)
    }).collect();

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(title));
    
    frame.render_widget(list, layout.main_content);

    // Render Command Bar
    let cmd_text = match state.mode {
        AppMode::Command => ":",
        _ => "",
    };
    let cmd_bar = Paragraph::new(cmd_text).style(Style::default());
    frame.render_widget(cmd_bar, layout.command_bar);
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
        Line::from("Spotify Developer credentials not found in ~/.config/echo/config.toml"),
        Line::from("Please paste your Client ID and Client Secret."),
        Line::from("Press [TAB] to switch fields, [ENTER] to save and authenticate."),
        Line::from(""),
        Line::from(vec![Span::styled("Client ID: ", id_style), Span::raw(&state.setup_client_id)]),
        Line::from(vec![Span::styled("Client Secret: ", secret_style), Span::raw(&state.setup_client_secret)]),
    ];
    
    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Left);
    frame.render_widget(paragraph, setup_area);
}
