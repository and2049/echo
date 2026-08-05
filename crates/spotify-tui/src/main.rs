mod handlers;
mod tui;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use std::panic;
use std::time::Duration;
use tokio::sync::mpsc;

use echo_core::{apply_worker_event, i18n, image_tasks, thumbnails};

use echo_core::events::AppEvent;
use tui::Tui;
use tui::theme::ToRatatui;

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        use std::io::IsTerminal;
        if !std::io::stdout().is_terminal() {
            launch_in_terminal();
        }
    }

    print!("\x1b]0;spotify\x07");
    i18n::init();

    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        original_hook(panic_info);
    }));

    let echo_core::bootstrap::Bootstrap {
        mut state,
        config: _config,
        app_tx,
        mut app_rx,
        worker_tx: worker_tx_clone,
    } = echo_core::bootstrap::init();

    let mut tui = Tui::new()?;
    tui.enter()?;

    let mut is_first_frame = true;

    while state.ui.is_running {
        let mut needs_draw = is_first_frame;
        is_first_frame = false;

        if let Some(expiry) = state.ui.status_message_expiry
            && std::time::Instant::now() >= expiry
        {
            state.ui.status_message = None;
            state.ui.status_message_expiry = None;
            state.ui.recent_queue_count = 0;
            needs_draw = true;
        }

        if state.ui.needs_terminal_clear {
            needs_draw = true;
        }

        if event::poll(Duration::from_millis(16))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            needs_draw = true;
            let outgoing_event = handlers::handle_event(&mut state, &key);

            if !state.ui.is_running {
                let _ = app_tx.send(AppEvent::Quit);
            } else if let Some(ev) = outgoing_event {
                if let AppEvent::LoadContextTracks(ref context) = ev {
                    if let Some(url) = context.image_url.as_ref() {
                        state.data.tracklist_image_url = Some(url.clone());
                        image_tasks::spawn_header_for_url(
                            url,
                            worker_tx_clone.clone(),
                            state.ui.library_config.cover_img_pixels,
                        );
                    }
                    let _ = app_tx.send(ev);
                } else if let AppEvent::ReloadHeaderImage = ev {
                    if let Some(url) = &state.data.tracklist_image_url {
                        image_tasks::spawn_header_for_url(
                            url,
                            worker_tx_clone.clone(),
                            state.ui.library_config.cover_img_pixels,
                        );
                    }
                } else {
                    let _ = app_tx.send(ev);
                }
            }
        }

        while let Ok(worker_event) = app_rx.try_recv() {
            needs_draw = true;
            apply_worker_event::apply_worker_event(
                worker_event,
                &mut state,
                &app_tx,
                &worker_tx_clone,
            );
        }

        if needs_draw {
            let force_clear = state.ui.needs_terminal_clear;
            tui.apply_background(state.ui.active_theme.background.rat(), force_clear)?;
            state.ui.needs_terminal_clear = false;
            tui.terminal.draw(|f| {
                tui::render::render_app(f, &mut state);
            })?;
        }

        thumbnails::drain_pending(&mut state, &worker_tx_clone);
    }

    tui.exit()?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn launch_in_terminal() -> ! {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => std::process::exit(1),
    };
    let exe_str = exe.display().to_string();
    let args: Vec<String> = std::env::args().skip(1).collect();

    let terminals: &[&[&str]] = &[
        &["x-terminal-emulator", "-e"],
        &["gnome-terminal", "--"],
        &["konsole", "-e"],
        &["xfce4-terminal", "-e"],
        &["mate-terminal", "-e"],
        &["ghostty", "-e"],
        &["alacritty", "-e"],
        &["kitty", "--"],
        &["wezterm", "start", "--"],
        &["terminator", "-e"],
        &["xterm", "-e"],
    ];

    if let Ok(term) = std::env::var("TERMINAL") {
        let mut cmd = std::process::Command::new(&term);
        cmd.arg("-e").arg(&exe_str);
        for a in &args {
            cmd.arg(a);
        }
        if let Ok(mut child) = cmd.spawn() {
            let _ = child.wait();
            std::process::exit(0);
        }
    }

    for entry in terminals {
        let mut cmd = std::process::Command::new(entry[0]);
        cmd.args(&entry[1..]).arg(&exe_str);
        for a in &args {
            cmd.arg(a);
        }
        if let Ok(mut child) = cmd.spawn() {
            let _ = child.wait();
            std::process::exit(0);
        }
    }

    let _ = std::process::Command::new("zenity")
        .args([
            "--error",
            "--title=Echo",
            "--text=Echo is a terminal application but no terminal emulator was found.\nPlease run it from a terminal.",
        ])
        .spawn();
    std::process::exit(1);
}

