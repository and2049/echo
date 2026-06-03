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
        crate::i18n::t("ui.on", &state.library_config.language)
    } else {
        crate::i18n::t("ui.off", &state.library_config.language)
    };
    
    let repeat_str = if state.playback.repeat_mode == "Off" {
        crate::i18n::t("ui.off", &state.library_config.language)
    } else {
        state.playback.repeat_mode.clone()
    };
    
    let mut border_title = crate::i18n::t("ui.playing", &state.library_config.language);
    border_title = border_title.replacen("{}", &state.playback.device_name, 1);
    border_title = border_title.replacen("{}", &format!("{:<7}", shuffle_str), 1);
    border_title = border_title.replacen("{}", &format!("{:<7}", repeat_str), 1);
    border_title = border_title.replacen("{}", &format!("{:>3}", state.playback.volume), 1);

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
            Constraint::Length(2), // left padding (aligns with track cover)
            Constraint::Length(2), // play/pause icon
            Constraint::Length(6), // current time
            Constraint::Min(0),    // gauge
            Constraint::Length(6), // duration
        ])
        .split(playback_chunks[1]);

    let play_icon = if state.playback.is_playing {
        "▌▌" // Use two half-blocks to avoid emoji rendering with a background
    } else {
        "▶ "
    };

    let play_icon_p = Paragraph::new(play_icon)
        .alignment(Alignment::Left)
        .style(state.active_theme.primary_style());

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

    frame.render_widget(play_icon_p, pb_chunks[1]);
    frame.render_widget(current_time_p, pb_chunks[2]);

    // Add a tiny margin around the gauge to separate it from the text
    let mut gauge_area = pb_chunks[3];
    if gauge_area.width > 2 {
        gauge_area.x += 1;
        gauge_area.width -= 2;
    }
    frame.render_widget(gauge, gauge_area);
    frame.render_widget(total_time_p, pb_chunks[4]);

    // Render Track Info
    let is_vis_enabled = state.playback.enable_visualizer.as_ref().map(|f| f.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
    
    let mut constraints = vec![
        Constraint::Length(2),  // 0: Left padding
        Constraint::Length(10), // 1: Image width
        Constraint::Length(2),  // 2: Middle gap to text
        Constraint::Length(35), // 3: Text width
        Constraint::Min(0),     // 4: Gap to visualizer
    ];
    
    let text_idx = 3;
    let mut vis_idx = None;

    if is_vis_enabled {
        let v_idx = constraints.len();
        constraints.push(Constraint::Length(32)); // Visualizer width
        constraints.push(Constraint::Length(2));  // Right padding
        vis_idx = Some(v_idx);
    }

    let track_info_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(playback_chunks[0]);

    // Independent Layout for perfectly centered lyrics
    let lyrics_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ])
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
    frame.render_widget(track_text_p, track_info_chunks[text_idx]);

    // Render Condensed Lyrics perfectly centered
    if state.condensed_lyrics_enabled {
        if let Some(lyrics) = &state.current_lyrics {
            let mut current_lyric_idx = 0;
            let current_progress = state.playback.progress_ms;
            for (i, line) in lyrics.lines.iter().enumerate() {
                if line.start_ms <= current_progress {
                    current_lyric_idx = i;
                } else {
                    break;
                }
            }
            
            let current_line = lyrics.lines.get(current_lyric_idx).map(|l| l.text.as_str()).unwrap_or("");
            let next_line = lyrics.lines.get(current_lyric_idx + 1).map(|l| l.text.as_str()).unwrap_or("");

            let lyrics_lines = vec![
                Line::from(Span::styled(current_line, state.active_theme.primary_style().add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(next_line, state.active_theme.muted_style())),
            ];
            
            let lyrics_p = Paragraph::new(lyrics_lines)
                .alignment(Alignment::Center)
                .style(state.active_theme.base_style())
                .block(Block::default().padding(ratatui::widgets::Padding::new(0, 0, 2, 0)));
            frame.render_widget(lyrics_p, lyrics_layout[1]);
        }
    }

    if is_vis_enabled
        && let Some(shared_bands) = &state.playback.audio_visualization
        && let Some(bands) = shared_bands.try_lock() {
            use ratatui::widgets::{BarChart, BarGroup, Bar};
            let c_primary = state.active_theme.primary;
            let c_secondary = state.active_theme.secondary;
            let c_mid_low = interpolate_color(c_secondary, c_primary, 0.33);
            let c_mid_high = interpolate_color(c_secondary, c_primary, 0.66);

            let num_bins = state.vis_bins.clamp(5, 32);
            let mut bars = Vec::with_capacity(num_bins);
            let chunk_size = 32.0 / num_bins as f32;

            for i in 0..num_bins {
                let start_idx = (i as f32 * chunk_size).floor() as usize;
                let mut end_idx = ((i + 1) as f32 * chunk_size).floor() as usize;
                if i == num_bins - 1 {
                    end_idx = 32;
                }
                
                let mut sum = 0.0;
                let mut count = 0;
                for j in start_idx..end_idx {
                    sum += bands[j];
                    count += 1;
                }
                let val = if count > 0 { sum / count as f32 } else { 0.0 };
                
                let ratio = (val / 100.0).clamp(0.0, 1.0);
                
                let color = if ratio < 0.25 {
                    c_secondary
                } else if ratio < 0.50 {
                    c_mid_low
                } else if ratio < 0.75 {
                    c_mid_high
                } else {
                    c_primary
                };
                
                let bar = Bar::default().value(val as u64).text_value("").style(ratatui::style::Style::default().fg(color));
                bars.push(bar);
            }
            
            let bw = (32 / num_bins).max(1) as u16;
            let barchart = BarChart::default()
                .data(BarGroup::default().bars(&bars))
                .bar_width(bw)
                .bar_gap(0)
                .max(100);
            if let Some(idx) = vis_idx {
                let mut vis_area = track_info_chunks[idx];
                if vis_area.height >= 7 {
                    vis_area.y += 2;
                    vis_area.height = 4;
                }
                frame.render_widget(barchart, vis_area);
            }
        }
}

fn color_to_rgb(c: ratatui::style::Color) -> (u8, u8, u8) {
    match c {
        ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
        ratatui::style::Color::Black => (0, 0, 0),
        ratatui::style::Color::Red => (255, 0, 0),
        ratatui::style::Color::Green => (0, 255, 0),
        ratatui::style::Color::Yellow => (255, 255, 0),
        ratatui::style::Color::Blue => (0, 0, 255),
        ratatui::style::Color::Magenta => (255, 0, 255),
        ratatui::style::Color::Cyan => (0, 255, 255),
        ratatui::style::Color::Gray => (128, 128, 128),
        ratatui::style::Color::DarkGray => (64, 64, 64),
        ratatui::style::Color::LightRed => (255, 128, 128),
        ratatui::style::Color::LightGreen => (128, 255, 128),
        ratatui::style::Color::LightYellow => (255, 255, 128),
        ratatui::style::Color::LightBlue => (128, 128, 255),
        ratatui::style::Color::LightMagenta => (255, 128, 255),
        ratatui::style::Color::LightCyan => (128, 255, 255),
        ratatui::style::Color::White => (255, 255, 255),
        _ => (255, 255, 255),
    }
}

fn interpolate_color(c1: ratatui::style::Color, c2: ratatui::style::Color, ratio: f32) -> ratatui::style::Color {
    let rgb1 = color_to_rgb(c1);
    let rgb2 = color_to_rgb(c2);
    
    let r = (rgb1.0 as f32 + (rgb2.0 as f32 - rgb1.0 as f32) * ratio) as u8;
    let g = (rgb1.1 as f32 + (rgb2.1 as f32 - rgb1.1 as f32) * ratio) as u8;
    let b = (rgb1.2 as f32 + (rgb2.2 as f32 - rgb1.2 as f32) * ratio) as u8;
    
    ratatui::style::Color::Rgb(r, g, b)
}
