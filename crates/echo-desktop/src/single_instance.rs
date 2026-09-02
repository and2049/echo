//! One echo per config directory. The first launch claims a local endpoint and answers every
//! later launch by showing its window; a later launch finds the endpoint taken, knocks on it
//! and exits. The endpoint is a Unix domain socket inside the config directory, or on Windows
//! a named pipe named after that directory's hash (pipes live in one flat namespace), so an
//! `ECHO_CONFIG_DIR` test instance never reaches the live one. The knock is a bare connection:
//! the server never reads, it only counts arrivals.

use std::path::Path;

use tokio::sync::mpsc::UnboundedSender;

use crate::tray::TrayEvent;

pub enum Instance {
    /// This process runs the app. The listener is `None` when the endpoint could not be set
    /// up, which switches the guard off rather than the app.
    Primary(Option<Listener>),
    /// Another echo holds the endpoint and has been asked to show its window.
    Secondary,
}

pub struct Listener(platform::Listener);

/// Claims the endpoint for `config_dir`. Must run inside the tokio runtime.
pub fn claim(config_dir: &Path) -> Instance {
    if platform::knock(config_dir) {
        return Instance::Secondary;
    }
    Instance::Primary(platform::listen(config_dir).map(Listener))
}

impl Listener {
    /// Turns every later launch into a `Show`, for the process lifetime.
    pub fn serve(self, tx: UnboundedSender<TrayEvent>) {
        tokio::spawn(platform::serve(self.0, tx));
    }
}

#[cfg(windows)]
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(unix)]
mod platform {
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};

    use tokio::net::UnixListener;
    use tokio::sync::mpsc::UnboundedSender;

    use crate::tray::TrayEvent;

    pub type Listener = UnixListener;

    fn socket_path(config_dir: &Path) -> PathBuf {
        config_dir.join("echo-desktop.sock")
    }

    pub fn knock(config_dir: &Path) -> bool {
        UnixStream::connect(socket_path(config_dir)).is_ok()
    }

    pub fn listen(config_dir: &Path) -> Option<Listener> {
        let path = socket_path(config_dir);
        std::fs::create_dir_all(config_dir).ok();
        std::fs::remove_file(&path).ok();
        UnixListener::bind(&path).ok()
    }

    pub async fn serve(listener: Listener, tx: UnboundedSender<TrayEvent>) {
        while listener.accept().await.is_ok() {
            if tx.send(TrayEvent::Show).is_err() {
                break;
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::path::Path;
    use std::time::Duration;

    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use tokio::sync::mpsc::UnboundedSender;

    use crate::tray::TrayEvent;

    pub struct Listener {
        name: String,
        server: NamedPipeServer,
    }

    pub fn pipe_name(config_dir: &Path) -> String {
        let hash = super::fnv1a(&config_dir.to_string_lossy());
        format!(r"\\.\pipe\echo-desktop-{hash:016x}")
    }

    pub fn knock(config_dir: &Path) -> bool {
        let name = pipe_name(config_dir);
        for _ in 0..10 {
            match std::fs::OpenOptions::new().read(true).write(true).open(&name) {
                Ok(_) => return true,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
        false
    }

    pub fn listen(config_dir: &Path) -> Option<Listener> {
        let name = pipe_name(config_dir);
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(&name)
            .ok()?;
        Some(Listener { name, server })
    }

    pub async fn serve(mut listener: Listener, tx: UnboundedSender<TrayEvent>) {
        while listener.server.connect().await.is_ok() {
            let Ok(next) = ServerOptions::new().create(&listener.name) else {
                break;
            };
            drop(std::mem::replace(&mut listener.server, next));
            if tx.send(TrayEvent::Show).is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Instance, claim};
    use crate::tray::TrayEvent;

    #[tokio::test]
    async fn a_second_claim_hands_show_to_the_first() {
        let dir = std::env::temp_dir().join(format!("echo-instance-{}", std::process::id()));
        let Instance::Primary(Some(listener)) = claim(&dir) else {
            panic!("the first claim should hold the endpoint");
        };
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        listener.serve(tx);
        assert!(matches!(claim(&dir), Instance::Secondary));
        assert_eq!(rx.recv().await, Some(TrayEvent::Show));
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(windows)]
    #[test]
    fn pipe_name_is_stable_and_differs_per_directory() {
        let name = |dir: &str| super::platform::pipe_name(std::path::Path::new(dir));
        assert_eq!(name(r"C:\a"), name(r"C:\a"));
        assert_ne!(name(r"C:\a"), name(r"C:\b"));
        assert!(name(r"C:\a").starts_with(r"\\.\pipe\echo-desktop-"));
    }
}
