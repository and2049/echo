use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};

/// Keep console-based child processes (`clip`, `powershell`) from flashing a
/// console window when spawned from the GUI-subsystem desktop app.
#[cfg(target_os = "windows")]
fn hide_console(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn hide_console(_command: &mut Command) {}

pub fn copy_to_clipboard(text: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &[&str])] = &[("clip", &[])];
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, &[&str])] = &[("pbcopy", &[])];
    #[cfg(all(unix, not(target_os = "macos")))]
    let candidates: &[(&str, &[&str])] = &[
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
    ];

    for (program, args) in candidates {
        let mut command = Command::new(program);
        command
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        hide_console(&mut command);
        let Ok(mut child) = command.spawn() else {
            continue;
        };
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(text.as_bytes())?;
        }
        if child.wait()?.success() {
            return Ok(());
        }
    }
    bail!("no supported clipboard command was found")
}

pub fn read_clipboard() -> Result<String> {
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &[&str])] = &[(
        "powershell",
        &["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard -Raw"],
    )];
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, &[&str])] = &[("pbpaste", &[])];
    #[cfg(all(unix, not(target_os = "macos")))]
    let candidates: &[(&str, &[&str])] = &[
        ("wl-paste", &["--no-newline"]),
        ("xclip", &["-o", "-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--output"]),
    ];

    for (program, args) in candidates {
        let mut command = Command::new(program);
        command.args(*args);
        hide_console(&mut command);
        let Ok(output) = command.output() else {
            continue;
        };
        if output.status.success() {
            return String::from_utf8(output.stdout)
                .context("clipboard contained invalid UTF-8")
                .map(|text| text.trim().to_string());
        }
    }
    bail!("no supported clipboard command was found")
}

pub fn reveal_file(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(format!("/select,{}", path.display()));
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(path));
        command
    };

    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to open the file manager")?;
    Ok(())
}
