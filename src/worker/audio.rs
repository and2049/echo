use futures_util::StreamExt;
use librespot_connect::{ConnectConfig, Spirc};
use librespot_core::authentication::Credentials;
use librespot_core::cache::Cache;
use librespot_core::config::SessionConfig;
use librespot_core::session::Session;

use crate::events::WorkerEvent;
use librespot_playback::audio_backend;
use librespot_playback::config::PlayerConfig;
use librespot_playback::player::Player;
use tokio::sync::mpsc;

pub async fn spawn_librespot_daemon(
    _access_token: String,
    device_name: String,
    tx: mpsc::Sender<WorkerEvent>,
) {
    tokio::spawn(async move {
        loop {
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                // Find or create cache directory
                let mut cache_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                cache_dir.push("echo");
                std::fs::create_dir_all(&cache_dir)?;
                let cache = Cache::new(Some(cache_dir.clone()), None, None, None)?;

                let credentials = if let Some(creds) = cache.credentials() {
                    creds
                } else {
                    let _ = std::fs::write(
                        "echo-librespot-status.log",
                        "FALLBACK: OPENING BROWSER FOR HARDCODED OAUTH",
                    );

                    let client_builder = librespot_oauth::OAuthClientBuilder::new(
                        "d420a117a32841c2b3474932e49fb54b",
                        "http://127.0.0.1:8989/login",
                        vec![
                            "streaming",
                            "user-read-playback-state",
                            "user-modify-playback-state",
                            "app-remote-control",
                        ],
                    )
                    .open_in_browser();

                    let oauth_client = client_builder
                        .build()
                        .expect("Failed to build OAuth client");
                    let t = oauth_client
                        .get_access_token()
                        .expect("Failed to get access token");

                    // Clear the terminal because librespot-oauth hardcodes a `println!` that corrupts the TUI layout.
                    let _ = crossterm::execute!(
                        std::io::stdout(),
                        crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
                    );
                    let _ = tx.send(WorkerEvent::ForceRedraw).await;

                    let creds = Credentials::with_access_token(t.access_token);
                    cache.save_credentials(&creds);
                    let _ = std::fs::remove_file("echo-librespot-status.log");
                    creds
                };

                let session_config = SessionConfig::default();
                let session = Session::new(session_config, Some(cache.clone()));

                let backend_fn = audio_backend::find(None).unwrap();
                let player_config = PlayerConfig::default();

                let mixer_fn = librespot_playback::mixer::find(None).unwrap();
                let mixer = mixer_fn(librespot_playback::mixer::MixerConfig::default()).unwrap();

                let player = Player::new(
                    player_config,
                    session.clone(),
                    mixer.get_soft_volume(),
                    move || backend_fn(None, Default::default()),
                );

                let mut connect_config = ConnectConfig::default();
                connect_config.name = device_name.clone();

                let (_spirc, spirc_task) =
                    Spirc::new(connect_config, session.clone(), credentials, player, mixer).await?;

                let _ = std::fs::write(
                    "echo-debug-fallback.log",
                    "Spirc Daemon initialized successfully, awaiting task...",
                );
                spirc_task.await;
                let _ = std::fs::write("echo-debug-fallback.log", "Spirc Daemon task exited!");

                Ok(())
            }
            .await;

            if let Err(e) = result {
                let err_msg = format!("{:?}", e);
                let _ = std::fs::write("echo-librespot-fatal.log", format!("Librespot Daemon crashed: {}", err_msg));
                
                // If the error was caused by invalid/expired credentials, delete the cache and retry immediately.
                if err_msg.contains("BadCredentials") || err_msg.contains("PermissionDenied") {
                    let mut cache_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                    cache_dir.push("echo");
                    let _ = std::fs::remove_file(cache_dir.join("credentials.json"));
                    continue; // Loop back and trigger the browser re-auth flow
                }
                
                break; // Unrecoverable error, break the loop
            } else {
                let _ = std::fs::write("echo-librespot-fatal.log", "Librespot Daemon exited normally.");
                break;
            }
        }
    });
}
