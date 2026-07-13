use crate::app::{ActiveView, AppMode, AppState};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, HighlightSpacing, List, ListItem, Paragraph},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

const ROW_TEXT_LEFT_GUTTER: u16 = 1;
const ROW_TEXT_RIGHT_GUTTER: u16 = 1;
pub const DURATION_COLUMN_WIDTH: u16 = 9;

pub fn render_app(frame: &mut Frame, state: &mut AppState) {
    fill_background(frame, state);

    if state.ui.mode == AppMode::Setup {
        render_setup(frame, state);
        return;
    }

    if state.ui.mode == AppMode::Authenticating {
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

    let main_content_area = chunks[0];
    let playback_bar_area = chunks[1];
    let command_bar_area = chunks[2];

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Length(1),
            Constraint::Percentage(70),
        ])
        .split(main_content_area);
    let library_area = main_chunks[0];
    let tracks_area = main_chunks[2];

    crate::tui::library::render_library_list(frame, state, library_area);

    if state.ui.active_view == ActiveView::SearchResults {
        crate::tui::search::render_search_results(frame, state, tracks_area);
    } else if state.ui.active_view == ActiveView::Queue {
        crate::tui::queue::render_queue(frame, state, tracks_area);
    } else if state.ui.active_view == ActiveView::ArtistList {
        crate::tui::library::render_artist_list(frame, state, tracks_area);
    } else if state.ui.active_view == ActiveView::ArtistPage {
        crate::tui::library::render_artist_page(frame, state, tracks_area);
    } else {
        crate::tui::library::render_track_list(frame, state, tracks_area);
    }

    crate::tui::playback::render_playback_bar(frame, state, playback_bar_area);

    // Render Command Bar
    let (cmd_text, cmd_style) = command_bar_content(state);
    let cmd_bar = Paragraph::new(cmd_text).style(cmd_style);
    frame.render_widget(cmd_bar, command_bar_area);

    if state.ui.mode == AppMode::Command && !state.ui.command_suggestions.is_empty() {
        let max_len = state.ui
            .command_suggestions
            .iter()
            .map(|s| s.len())
            .max()
            .unwrap_or(10) as u16;
        let width = max_len + 4;
        let height = state.ui.command_suggestions.len() as u16 + 2;
        let x = 2;
        let y = command_bar_area.y.saturating_sub(height);
        let popup_area = Rect {
            x,
            y,
            width,
            height,
        };

        let items: Vec<ratatui::widgets::ListItem> = state.ui
            .command_suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let style = if Some(i) == state.ui.command_suggestion_index {
                    ratatui::style::Style::default()
                        .bg(state.ui.active_theme.highlight_bg)
                        .fg(state.ui.active_theme.highlight_fg)
                } else {
                    state.ui.active_theme.base_style()
                };
                ratatui::widgets::ListItem::new(s.clone()).style(style)
            })
            .collect();

        let list = ratatui::widgets::List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .style(state.ui.active_theme.base_style()),
        );

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(list, popup_area);
    }

    if let Some(playlist_ids) = &state.ui.playlist_delete_prompt {
        let count = playlist_ids.len();
        let popup_area = centered_rect(60, 20, frame.area());
        let popup = Paragraph::new(vec![
            Line::from(Span::styled(
                if count == 1 {
                    "Are you sure you want to delete this playlist?".to_string()
                } else {
                    format!("Are you sure you want to delete these {} playlists?", count)
                },
                state.ui.active_theme.error_style(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'y' to confirm or any other key to cancel.",
                state.ui.active_theme.base_style().add_modifier(Modifier::BOLD),
            )),
        ])
        .style(state.ui.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Delete Playlist ")
                .style(state.ui.active_theme.base_style())
                .border_style(state.ui.active_theme.error_style()),
        );
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    if let Some(album_ids) = &state.ui.album_mass_delete_prompt {
        let count = album_ids.len();
        let popup_area = centered_rect(60, 20, frame.area());
        let popup = Paragraph::new(vec![
            Line::from(Span::styled(
                if count == 1 {
                    "Are you sure you want to remove this album from your library?".to_string()
                } else {
                    format!(
                        "Are you sure you want to remove these {} albums from your library?",
                        count
                    )
                },
                state.ui.active_theme.error_style(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'y' to confirm or any other key to cancel.",
                state.ui.active_theme.base_style().add_modifier(Modifier::BOLD),
            )),
        ])
        .style(state.ui.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Remove Album ")
                .style(state.ui.active_theme.base_style())
                .border_style(state.ui.active_theme.error_style()),
        );
        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    if let Some(folder_name) = &state.ui.folder_delete_prompt {
        let popup_area = centered_rect(60, 40, frame.area());
        let popup = Paragraph::new(vec![
            Line::from(Span::styled(
                format!(
                    "Are you sure you want to delete the folder '{}'?",
                    folder_name
                ),
                state.ui.active_theme.error_style(),
            )),
            Line::from(""),
            Line::from("Any playlists inside will be safely returned to the main library."),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'y' to confirm or any other key to cancel.",
                state.ui.active_theme.base_style().add_modifier(Modifier::BOLD),
            )),
        ])
        .style(state.ui.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Delete Folder ")
                .style(state.ui.active_theme.base_style())
                .border_style(state.ui.active_theme.error_style()),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    if let Some((_, track_ids)) = &state.ui.track_delete_prompt {
        let popup_area = centered_rect(60, 40, frame.area());
        let popup = Paragraph::new(vec![
            Line::from(Span::styled(
                if track_ids.len() == 1 {
                    "Are you sure you want to remove this track from the playlist?".to_string()
                } else {
                    format!(
                        "Are you sure you want to remove these {} tracks from the playlist?",
                        track_ids.len()
                    )
                },
                state.ui.active_theme.error_style(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Press 'y' to confirm or any other key to cancel.",
                state.ui.active_theme.base_style().add_modifier(Modifier::BOLD),
            )),
        ])
        .style(state.ui.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Remove From Playlist ")
                .style(state.ui.active_theme.base_style())
                .border_style(state.ui.active_theme.error_style()),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    if state.ui.liked_track_remove_prompt.is_some() {
        let popup_area = centered_rect(60, 40, frame.area());
        let popup = Paragraph::new(vec![Line::from(Span::styled(
            crate::i18n::t("prompts.remove_from_liked", &state.ui.library_config.language),
            state.ui.active_theme.error_style(),
        ))])
        .style(state.ui.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Remove Liked Song ")
                .style(state.ui.active_theme.base_style())
                .border_style(state.ui.active_theme.error_style()),
        )
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    if state.ui.playlist_add_modal_open {
        let popup_area = centered_rect(50, 60, frame.area());

        let user_playlists = crate::handlers::normal::playlist_modal_choices(state);

        let items: Vec<ratatui::widgets::ListItem> = user_playlists
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let style = if i == state.ui.selected_playlist_modal_index {
                    state.ui.active_theme.selected_style()
                } else {
                    state.ui.active_theme.base_style()
                };
                let label = if p.owner_id == "local" {
                    format!("{} · Local", p.name)
                } else {
                    p.name.clone()
                };
                ratatui::widgets::ListItem::new(label).style(style)
            })
            .collect();

        let list = ratatui::widgets::List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Add to Playlist ")
                    .style(state.ui.active_theme.base_style())
                    .border_style(state.ui.active_theme.primary_style()),
            )
            .highlight_style(state.ui.active_theme.selected_style());

        let mut list_state = ratatui::widgets::ListState::default();
        list_state.select(Some(state.ui.selected_playlist_modal_index));

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_stateful_widget(list, popup_area, &mut list_state);
    }

    if state.ui.device_modal_open {
        let popup_area = centered_rect(50, 60, frame.area());

        let items: Vec<ratatui::widgets::ListItem> = state.data
            .devices
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let mut style = if i == state.ui.selected_device_index {
                    state.ui.active_theme.selected_style()
                } else {
                    state.ui.active_theme.base_style()
                };

                let active_marker = if d.is_active { " [Active]" } else { "" };
                let text = format!("{} ({}%){}", d.name, d.volume_percent, active_marker);

                if d.is_active && i != state.ui.selected_device_index {
                    style = style.fg(state.ui.active_theme.primary);
                }

                ratatui::widgets::ListItem::new(text).style(style)
            })
            .collect();

        let list = ratatui::widgets::List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Switch Device ")
                    .style(state.ui.active_theme.base_style())
                    .border_style(state.ui.active_theme.primary_style()),
            )
            .highlight_style(state.ui.active_theme.selected_style());

        let mut list_state = ratatui::widgets::ListState::default();
        list_state.select(Some(state.ui.selected_device_index));

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_stateful_widget(list, popup_area, &mut list_state);
    }

    // Check if we are waiting for discovery
    if std::path::Path::new("echo-librespot-status.log").exists() {
        let popup_area = centered_rect(72, 32, frame.area());
        let popup = Paragraph::new(vec![
            Line::from("Spotify Connect Onboarding Required")
                .style(state.ui.active_theme.secondary_style()),
            Line::from(""),
            Line::from("1. Open Spotify on your phone or desktop."),
            Line::from("2. Open the Devices picker."),
            Line::from("3. Select 'echo-rs' from available devices."),
            Line::from(""),
            Line::from("Echo will continue once the device connects."),
        ])
        .style(state.ui.active_theme.base_style())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Setup ")
                .style(state.ui.active_theme.base_style())
                .border_style(state.ui.active_theme.secondary_style()),
        )
        .alignment(ratatui::layout::Alignment::Left)
        .wrap(ratatui::widgets::Wrap { trim: true });

        frame.render_widget(ratatui::widgets::Clear, popup_area);
        frame.render_widget(popup, popup_area);
    }

    render_action_menu(frame, state);
    render_lyrics_modal(frame, state);
}

fn command_bar_content(state: &AppState) -> (String, ratatui::style::Style) {
    match state.ui.mode {
        AppMode::Command => (
            format!(":{}", state.ui.command_buffer),
            state.ui.active_theme.base_style(),
        ),
        AppMode::Search => (
            format!("/{}", state.ui.search_query),
            state.ui.active_theme.base_style(),
        ),
        _ => match state.ui.audio_output_error.as_ref() {
            Some(message) => (message.clone(), state.ui.active_theme.error_style()),
            None => (
                state.ui.status_message.clone().unwrap_or_default(),
                state.ui.active_theme.muted_style(),
            ),
        },
    }
}

pub fn render_action_menu(frame: &mut Frame, state: &AppState) {
    if !state.ui.action_menu_open {
        return;
    }

    let ctx = match &state.ui.action_menu_context {
        Some(c) => c,
        None => return,
    };

    let is_liked = state.data.liked_tracks.contains(&ctx.track_id);
    let lang = &state.ui.library_config.language;
    let lbl_album = crate::i18n::t("actions.go_to_album", lang);
    let lbl_artist = crate::i18n::t("actions.go_to_artist", lang);
    let lbl_playlist = crate::i18n::t("actions.add_to_playlist", lang);
    let lbl_queue = crate::i18n::t("actions.add_to_queue", lang);
    let lbl_unlike = crate::i18n::t("actions.unlike_track", lang);
    let lbl_like = crate::i18n::t("actions.like_track", lang);

    let album_is_saved = ctx
        .album_id
        .as_ref()
        .is_some_and(|id| state.data.saved_albums.iter().any(|album| &album.id == id));
    let actions = ctx.actions();
    let labels: Vec<String> = actions
        .iter()
        .map(|action| match action {
            crate::models::ActionMenuAction::GoToAlbum => lbl_album.clone(),
            crate::models::ActionMenuAction::GoToArtist => lbl_artist.clone(),
            crate::models::ActionMenuAction::AddToPlaylist => lbl_playlist.clone(),
            crate::models::ActionMenuAction::AddToQueue => lbl_queue.clone(),
            crate::models::ActionMenuAction::ToggleLike => {
                if is_liked { lbl_unlike.clone() } else { lbl_like.clone() }
            }
            crate::models::ActionMenuAction::ToggleSavedAlbum => {
                if album_is_saved { "Remove album from library" } else { "Save album to library" }.to_string()
            }
            crate::models::ActionMenuAction::CopyLink => "Copy Spotify link".to_string(),
            crate::models::ActionMenuAction::CopyPath => "Copy file path".to_string(),
            crate::models::ActionMenuAction::OpenFolder => "Show in file manager".to_string(),
        })
        .collect();

    // Measure popup size — title + 2 border lines + 1 blank + N actions
    let title_format = crate::i18n::t("actions.title", lang);
    let track_label = title_format.replace("{}", &ctx.track_name);
    let label_width = track_label.len() as u16;
    let popup_w = label_width.max(32).min(50);
    let popup_h = (labels.len() as u16) + 4;

    let area = frame.area();
    let x = (area.width.saturating_sub(popup_w)) / 2;
    let y = (area.height.saturating_sub(popup_h)) / 2;
    let popup_area = Rect {
        x,
        y,
        width: popup_w,
        height: popup_h,
    };

    frame.render_widget(ratatui::widgets::Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(track_label)
        .style(state.ui.active_theme.base_style())
        .border_style(state.ui.active_theme.primary_style());

    let items: Vec<ListItem> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let style = if i == state.ui.selected_action_index {
                state.ui.active_theme.selected_style()
            } else {
                state.ui.active_theme.base_style()
            };
            let text = format!("  {}  ", label);
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_spacing(HighlightSpacing::Never);

    frame.render_widget(list, popup_area);
}

pub fn render_lyrics_modal(frame: &mut Frame, state: &mut AppState) {
    if !state.ui.lyrics_modal_open {
        return;
    }

    let popup_area = centered_rect(60, 80, frame.area());

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Lyrics ")
        .style(state.ui.active_theme.base_style())
        .border_style(state.ui.active_theme.primary_style());

    frame.render_widget(ratatui::widgets::Clear, popup_area);

    if state.playback.is_fetching_lyrics {
        let p = Paragraph::new("Loading lyrics...")
            .alignment(Alignment::Center)
            .block(block)
            .style(state.ui.active_theme.base_style());
        frame.render_widget(p, popup_area);
        return;
    }

    if let Some(lyrics) = &state.playback.current_lyrics {
        let pb = &state.playback;
        let mut current_idx = 0;

        for (i, line) in lyrics.lines.iter().enumerate() {
            if line.start_ms <= pb.progress_ms {
                current_idx = i;
            } else {
                break;
            }
        }

        let mut items = Vec::new();
        for (i, line) in lyrics.lines.iter().enumerate() {
            let style = if i == current_idx {
                state.ui
                    .active_theme
                    .primary_style()
                    .add_modifier(Modifier::BOLD)
            } else if i > current_idx {
                state.ui.active_theme.muted_style()
            } else {
                state.ui.active_theme.base_style()
            };
            items.push(ratatui::widgets::ListItem::new(line.text.clone()).style(style));
        }

        let list = ratatui::widgets::List::new(items)
            .block(block)
            .highlight_style(
                state.ui
                    .active_theme
                    .primary_style()
                    .add_modifier(Modifier::BOLD),
            );

        let mut list_state = ratatui::widgets::ListState::default();

        // Center the active item
        let height = popup_area.height.saturating_sub(2) as usize;
        let offset = height / 2;
        let start = current_idx.saturating_sub(offset);

        *list_state.offset_mut() = start;
        list_state.select(Some(current_idx));

        frame.render_stateful_widget(list, popup_area, &mut list_state);
    } else {
        let p = Paragraph::new("No lyrics found.")
            .alignment(Alignment::Center)
            .block(block)
            .style(state.ui.active_theme.base_style());
        frame.render_widget(p, popup_area);
    }
}

pub fn fill_background(frame: &mut Frame, state: &AppState) {
    let area = frame.area();
    let style = state.ui.active_theme.base_style();
    let buffer = frame.buffer_mut();

    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &mut buffer[(x, y)];
            if cell.skip {
                continue;
            }
            cell.set_style(style);
            cell.set_symbol(" ");
        }
    }
}

pub fn repair_wide_grapheme_trailing_styles(buffer: &mut Buffer, area: Rect) {
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

pub fn padded_library_list(items: Vec<ListItem>) -> List {
    List::new(items)
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always)
}

pub fn row_text_width(area: Rect) -> u16 {
    area.width
        .saturating_sub(ROW_TEXT_LEFT_GUTTER + ROW_TEXT_RIGHT_GUTTER)
}

pub fn format_duration_text(time: String) -> String {
    format!("{:>8} ", time)
}

pub fn format_time(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

pub fn truncate_to_width_with_ellipsis(text: &str, max_width: u16) -> String {
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

pub fn stabilize_terminal_emoji_width(text: &str) -> String {
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

pub fn needs_emoji_variation_selector(ch: char) -> bool {
    matches!(ch, '\u{1f578}')
}

// Helper function to create a centered rect
pub fn centered_rect(
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

pub fn render_authenticating(frame: &mut Frame, state: &AppState) {
    let block = Block::default()
        .title(" Authenticating ")
        .borders(Borders::ALL)
        .style(state.ui.active_theme.base_style())
        .border_style(state.ui.active_theme.primary_style());
    let text = vec![
        Line::from("Waiting for Spotify sign-in or reauthorization..."),
        Line::from(
            "Please check your browser. The Web API redirect URI is http://127.0.0.1:8888/callback.",
        ),
    ];
    let paragraph = Paragraph::new(text)
        .style(state.ui.active_theme.base_style())
        .block(block)
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, frame.area());
}

pub fn render_setup(frame: &mut Frame, state: &AppState) {
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
        .style(state.ui.active_theme.base_style())
        .border_style(state.ui.active_theme.primary_style());

    let id_style = if !state.ui.setup_focus_secret {
        state.ui.active_theme.secondary_style()
    } else {
        state.ui.active_theme.base_style()
    };

    let secret_style = if state.ui.setup_focus_secret {
        state.ui.active_theme.secondary_style()
    } else {
        state.ui.active_theme.base_style()
    };

    let text = vec![
        Line::from("Spotify Developer credentials not found in config.toml"),
        Line::from("Please paste your Client ID and Client Secret."),
        Line::from("Press [TAB] to switch fields, [ENTER] to save and authenticate."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Client ID: ", id_style),
            Span::raw(&state.ui.setup_client_id),
        ]),
        Line::from(vec![
            Span::styled("Client Secret: ", secret_style),
            Span::raw(&state.ui.setup_client_secret),
        ]),
    ];

    let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Left);
    let paragraph = paragraph.style(state.ui.active_theme.base_style());
    frame.render_widget(paragraph, setup_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        buffer::Buffer,
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
    fn persistent_audio_error_has_priority_over_timed_status() {
        let mut state = AppState::new();
        state.ui.status_message = Some("ordinary status".to_string());
        state.ui.audio_output_error = Some("audio disconnected".to_string());

        let (text, style) = command_bar_content(&state);

        assert_eq!(text, "audio disconnected");
        assert_eq!(style, state.ui.active_theme.error_style());
    }

    #[test]
    fn command_input_temporarily_covers_persistent_audio_error() {
        let mut state = AppState::new();
        state.ui.audio_output_error = Some("audio disconnected".to_string());
        state.ui.mode = AppMode::Command;
        state.ui.command_buffer = "redraw".to_string();

        assert_eq!(command_bar_content(&state).0, ":redraw");
        assert_eq!(state.ui.audio_output_error.as_deref(), Some("audio disconnected"));

        state.ui.mode = AppMode::Normal;
        assert_eq!(command_bar_content(&state).0, "audio disconnected");
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
