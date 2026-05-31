use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, List, ListItem, ListState},
    Frame,
};
use crate::app::{AppMode, AppState, ActiveView};
use crate::tui::layout::AppLayout;

pub fn render_app(frame: &mut Frame, state: &AppState) {
    let layout = AppLayout::compute(frame.area());

    // Render Header
    let mode_str = match state.mode {
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
    let (items, title, selected_idx) = match state.active_view {
        ActiveView::Library => {
            (
                &state.playlists,
                " Library (Playlists) ",
                state.selected_playlist_index,
            )
        }
        ActiveView::TrackList => {
            (
                &state.tracks,
                " Tracks ",
                state.selected_track_index,
            )
        }
    };

    let list_items: Vec<ListItem> = items.iter().enumerate().map(|(i, item)| {
        let style = if i == selected_idx {
            Style::default().bg(Color::White).fg(Color::Black)
        } else {
            Style::default()
        };
        ListItem::new(item.clone()).style(style)
    }).collect();

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(title));
    
    // We can use ListState to track selection and scrolling, but for Phase 1 we just highlight the item manually
    frame.render_widget(list, layout.main_content);

    // Render Command Bar
    let cmd_text = match state.mode {
        AppMode::Command => ":",
        _ => "",
    };
    let cmd_bar = Paragraph::new(cmd_text).style(Style::default());
    frame.render_widget(cmd_bar, layout.command_bar);
}
