//! Argv handling for the `spotify` binary.
//!
//! Deliberately hand-rolled: the crate has no CLI dependency and this is three subcommands.
//! Anything unrecognised falls through to the TUI, because desktop launchers and terminal
//! emulators pass through arguments we do not control.

use std::io::{IsTerminal, Write};

use echo_core::update::{self, UpdateError};

const HELP: &str = "\
spotify — a terminal Spotify client

Usage:
  spotify                    Start the player
  spotify upgrade            Upgrade to the latest release
  spotify upgrade <version>  Upgrade (or downgrade) to a specific version
  spotify upgrade --check    Report whether a newer release exists, change nothing

Options:
  -V, --version              Print the installed version
  -h, --help                 Print this message
";

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Start the player — the default for a bare invocation or unknown arguments.
    Tui,
    Version,
    Help,
    Upgrade {
        target: Option<String>,
        check_only: bool,
    },
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Command {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Command::Tui;
    };
    match first.as_str() {
        "upgrade" => {
            let mut target = None;
            let mut check_only = false;
            for arg in args {
                match arg.as_str() {
                    "--check" | "-n" => check_only = true,
                    // The first bare word is the version; `v1.2.3` and `1.2.3` both work.
                    _ if !arg.starts_with('-') && target.is_none() => target = Some(arg),
                    _ => {}
                }
            }
            Command::Upgrade { target, check_only }
        }
        "--version" | "-V" => Command::Version,
        "--help" | "-h" => Command::Help,
        _ => Command::Tui,
    }
}

/// Run a non-TUI subcommand and return the process exit code.
pub async fn run(command: Command) -> i32 {
    match command {
        Command::Tui => 0,
        Command::Help => {
            print!("{HELP}");
            0
        }
        Command::Version => {
            if update::is_dev_build() {
                println!("spotify {} (dev)", update::current_version());
            } else {
                println!("spotify {}", update::current_version());
            }
            0
        }
        Command::Upgrade { target, check_only } => match upgrade(target, check_only).await {
            Ok(code) => code,
            Err(error) => {
                eprintln!("error: {error}");
                1
            }
        },
    }
}

async fn upgrade(target: Option<String>, check_only: bool) -> Result<i32, UpdateError> {
    // Not an error: a working-tree build simply has nothing to upgrade to, and scripts that
    // call this in a dev checkout should not fail because of it.
    if update::is_dev_build() {
        println!(
            "  {} is a development build — install a release to use upgrade",
            update::current_version()
        );
        return Ok(0);
    }

    let current = update::current_version();
    println!("  current  {current}");

    let release = match &target {
        Some(version) => update::release_for(version).await?,
        None => update::latest_release().await?,
    };
    let version = release.version().to_string();
    println!("  latest   {version}");

    if check_only {
        if update::is_newer(&version, current) {
            println!("  an upgrade is available — run `spotify upgrade` to install it");
        } else {
            println!("  already up to date");
        }
        return Ok(0);
    }

    // An explicit version is honoured even when it is older, so a bad release can be rolled back.
    if target.is_none() && !update::is_newer(&version, current) {
        println!("  already up to date");
        return Ok(0);
    }

    // Resolved before downloading so an install we cannot write to fails in a second rather
    // than after 25 MB.
    let plan = update::plan()?;
    let asset = plan.asset_name()?;

    let mut progress = Progress::new(&asset);
    let staged =
        update::download(plan.clone(), &release, |percent| progress.update(percent)).await?;
    progress.finish();

    for path in &plan.targets {
        println!("  replacing {}", path.display());
    }
    if let Some(themes) = &plan.themes {
        println!("  replacing {}", themes.display());
    }

    let installed = update::apply(staged)?;
    println!("  upgraded to {installed} — restart spotify to use it");
    Ok(0)
}

/// A single rewritten line on a terminal; one line per decile when piped, so logs stay sane.
struct Progress {
    asset: String,
    interactive: bool,
    last_decile: Option<u8>,
}

impl Progress {
    fn new(asset: &str) -> Self {
        Self {
            asset: asset.to_string(),
            interactive: std::io::stdout().is_terminal(),
            last_decile: None,
        }
    }

    fn update(&mut self, percent: u8) {
        if self.interactive {
            print!("\r  downloading {}  {percent:>3}%", self.asset);
            let _ = std::io::stdout().flush();
            return;
        }
        let decile = percent / 10;
        if self.last_decile != Some(decile) {
            self.last_decile = Some(decile);
            println!("  downloading {}  {percent:>3}%", self.asset);
        }
    }

    fn finish(&mut self) {
        if self.interactive {
            println!();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Command {
        parse(args.iter().map(|arg| (*arg).to_string()))
    }

    #[test]
    fn no_arguments_start_the_player() {
        assert_eq!(parse_args(&[]), Command::Tui);
    }

    #[test]
    fn unknown_arguments_fall_through_to_the_player() {
        // .desktop launchers and terminal emulators pass through things we do not control,
        // so an unrecognised argument must never turn into an error.
        assert_eq!(parse_args(&["--enable-features=x"]), Command::Tui);
        assert_eq!(parse_args(&["spotify:track:123"]), Command::Tui);
    }

    #[test]
    fn upgrade_takes_an_optional_version_and_a_check_flag() {
        assert_eq!(
            parse_args(&["upgrade"]),
            Command::Upgrade {
                target: None,
                check_only: false
            }
        );
        assert_eq!(
            parse_args(&["upgrade", "0.4.6"]),
            Command::Upgrade {
                target: Some("0.4.6".into()),
                check_only: false
            }
        );
        assert_eq!(
            parse_args(&["upgrade", "--check"]),
            Command::Upgrade {
                target: None,
                check_only: true
            }
        );
        assert_eq!(
            parse_args(&["upgrade", "v0.4.6", "--check"]),
            Command::Upgrade {
                target: Some("v0.4.6".into()),
                check_only: true
            }
        );
    }

    #[test]
    fn version_and_help_have_short_forms() {
        assert_eq!(parse_args(&["--version"]), Command::Version);
        assert_eq!(parse_args(&["-V"]), Command::Version);
        assert_eq!(parse_args(&["--help"]), Command::Help);
        assert_eq!(parse_args(&["-h"]), Command::Help);
    }
}
