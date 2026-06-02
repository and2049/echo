use crate::app::AppState;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};
use crate::tui::render::{format_time, stabilize_terminal_emoji_width};

pub fn render_playback_bar(frame: &mut Frame, state: &mut AppState, area: Rect) {
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

    let playback_inner = playback_block.inner(area);
    frame.render_widget(playback_block, area);

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
    let is_vis_enabled = state.playback.enable_visualizer.as_ref().map(|f| f.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
    let mut constraints = vec![
        Constraint::Length(2),  // Left padding
        Constraint::Length(10), // Image width
        Constraint::Length(2),  // Middle gap to text
        Constraint::Min(0),     // Text width
    ];
    if is_vis_enabled {
        constraints.push(Constraint::Length(4));  // Gap to visualizer
        constraints.push(Constraint::Length(32)); // Visualizer width
        constraints.push(Constraint::Length(2));  // Right padding
    }
    
    let track_info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(playback_chunks[0]);

    let protocol = state
        .playback
        .playing_track_image
        .as_mut()
        .or(state.playback.previous_track_image.as_mut());

    if let Some(protocol) = protocol {
        let image = ratatui_image::StatefulImage::default();
        let mut image_area = track_info_chunks[1];
        // Center vertically in the 7-row tall block (1 row top padding, 1 row bottom padding)
        if image_area.height >= 7 {
            image_area.y += 1;
            image_area.height = 5;
        }
        frame.render_stateful_widget(image, image_area, protocol);
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

    if is_vis_enabled
        && let Some(shared_bands) = &state.playback.audio_visualization
        && let Some(bands) = shared_bands.try_lock() {
            use ratatui::widgets::{BarChart, BarGroup, Bar};
            let mut bars = Vec::with_capacity(32);
            for i in 0..32 {
                let val = bands[i];
                let color = if val > 60.0 { state.active_theme.primary } else { state.active_theme.secondary };
                let bar = Bar::default().value(val as u64).style(ratatui::style::Style::default().fg(color));
                bars.push(bar);
            }
            
            let mut vis_area = track_info_chunks[5];
            if vis_area.height >= 7 {
                vis_area.y += 2;
                vis_area.height = 4;
            }
            
            let barchart = BarChart::default()
                .data(BarGroup::default().bars(&bars))
                .bar_width(1)
                .bar_gap(0)
                .max(100);
            frame.render_widget(barchart, vis_area);
        }
}
