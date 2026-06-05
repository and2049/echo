use anyhow::{Context, Result};
use librespot_core::{
    authentication::Credentials, cache::Cache, config::SessionConfig, session::Session,
};
use librespot_oauth::OAuthClientBuilder;
use reqwest::header::RETRY_AFTER;
use std::{io::Write, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{Mutex, mpsc};

use crate::events::WorkerEvent;

pub use super::rate_limit::{SpotifyRateLimitError, rate_limit_error};
use super::rate_limit::{fallback_backoff, parse_retry_after, should_retry_inline};

pub const SPOTIFY_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
const REDIRECT_URI: &str = "http://127.0.0.1:8989/login";
const TOKEN_TIMEOUT: Duration = Duration::from_secs(5);

const OAUTH_SCOPES: &[&str] = &[
    "user-read-playback-state",
    "user-modify-playback-state",
    "user-read-currently-playing",
    "app-remote-control",
    "streaming",
    "playlist-read-private",
    "playlist-read-collaborative",
    "playlist-modify-private",
    "playlist-modify-public",
    "user-follow-modify",
    "user-follow-read",
    "user-read-playback-position",
    "user-top-read",
    "user-read-recently-played",
    "user-library-modify",
    "user-library-read",
    "user-read-private",
    "user-personalized",
];

#[derive(Clone)]
pub struct SpotifySessionManager {
    cache: Cache,
    session: Arc<Mutex<Option<Session>>>,
    tx: mpsc::Sender<WorkerEvent>,
}

impl SpotifySessionManager {
    pub fn new(tx: mpsc::Sender<WorkerEvent>) -> Result<Self> {
        let cache = Cache::new(Some(Self::cache_dir()?), None, None, None)
            .context("Failed to create first-party Spotify session cache")?;

        Ok(Self {
            cache,
            session: Arc::new(Mutex::new(None)),
            tx,
        })
    }

    pub async fn bearer_token(&self) -> Result<String> {
        let session = self.session().await?;
        match self.auth_token(&session).await {
            Ok(token) => Ok(token),
            Err(err) => {
                self.invalidate_session().await;
                anyhow::bail!(err);
            }
        }
    }

    pub async fn invalidate_session(&self) {
        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref()
            && !session.is_invalid()
        {
            session.shutdown();
        }
        *guard = None;
    }

    async fn session(&self) -> Result<Session> {
        {
            let guard = self.session.lock().await;
            if let Some(session) = guard.as_ref()
                && !session.is_invalid()
            {
                return Ok(session.clone());
            }
        }

        let mut guard = self.session.lock().await;
        if let Some(session) = guard.as_ref()
            && !session.is_invalid()
        {
            return Ok(session.clone());
        }

        let session = Session::new(SessionConfig::default(), Some(self.cache.clone()));

        if let Some(credentials) = self.cached_reusable_credentials() {
            match connect_session(&session, credentials).await {
                Ok(()) => {
                    *guard = Some(session.clone());
                    return Ok(session);
                }
                Err(err) => {
                    append_api_log(&format!(
                        "cached first-party session connect failed: {err:?}"
                    ));
                }
            }
        }

        let oauth_credentials = self.oauth_credentials()?;
        connect_session(&session, oauth_credentials).await.map_err(|err| {
            let message = format!(
                "fresh OAuth first-party session connect failed with client_id={SPOTIFY_CLIENT_ID}, scopes={}: {err:?}\n",
                OAUTH_SCOPES.join(","),
            );
            append_api_log(&message);
            err
        })?;

        *guard = Some(session.clone());
        Ok(session)
    }

    fn cached_reusable_credentials(&self) -> Option<Credentials> {
        if let Some(credentials) = self.cache.credentials()
            && credentials.username.is_some()
        {
            return Some(credentials);
        }

        None
    }

    fn oauth_credentials(&self) -> Result<Credentials> {
        let oauth_client =
            OAuthClientBuilder::new(SPOTIFY_CLIENT_ID, REDIRECT_URI, OAUTH_SCOPES.to_vec())
                .open_in_browser()
                .build()
                .context("Failed to build first-party Spotify OAuth client")?;

        let token = oauth_client
            .get_access_token()
            .context("Failed to get first-party Spotify OAuth token")?;

        clear_terminal_after_oauth(&self.tx);
        Ok(Credentials::with_access_token(token.access_token))
    }

    async fn auth_token(&self, session: &Session) -> Result<String> {
        let token = match tokio::time::timeout(TOKEN_TIMEOUT, session.login5().auth_token()).await {
            Ok(Ok(token)) => token,
            Ok(Err(err)) => anyhow::bail!("Failed to get first-party Spotify token: {err:?}"),
            Err(_) => anyhow::bail!("Timed out while getting first-party Spotify token"),
        };

        Ok(token.access_token)
    }

    fn cache_dir() -> Result<PathBuf> {
        let mut cache_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        cache_dir.push("echo");
        cache_dir.push("spotify_session");
        std::fs::create_dir_all(&cache_dir)
            .context("Failed to create first-party Spotify session cache directory")?;
        Ok(cache_dir)
    }
}

async fn connect_session(session: &Session, credentials: Credentials) -> Result<()> {
    session
        .connect(credentials, true)
        .await
        .context("Failed to connect first-party Spotify session")
}

#[derive(Clone)]
pub struct SpotifyWebApi {
    session: SpotifySessionManager,
    client: reqwest::Client,
}

impl SpotifyWebApi {
    pub fn new(session: SpotifySessionManager) -> Self {
        Self {
            session,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_json(&self, url: &str) -> Result<serde_json::Value> {
        let mut refreshed_session = false;

        for attempt in 0..=2 {
            match self.get_json_once(url).await {
                Ok(value) => return Ok(value),
                Err(SpotifyWebError::Unauthorized(message)) if !refreshed_session => {
                    append_api_log(&format!("401 retry url={url} err={message}"));
                    refreshed_session = true;
                    self.session.invalidate_session().await;
                }
                Err(SpotifyWebError::RateLimited { retry_after, body }) if attempt < 2 => {
                    let delay = retry_after.unwrap_or_else(|| fallback_backoff(attempt));
                    if retry_after.is_some() && !should_retry_inline(delay) {
                        return Err(SpotifyWebError::RateLimited { retry_after, body }.into());
                    }
                    append_api_log(&format!(
                        "429 retry attempt={} delay_ms={} url={} body={}",
                        attempt + 1,
                        delay.as_millis(),
                        url,
                        body
                    ));
                    tokio::time::sleep(delay).await;
                }
                Err(err) => return Err(err.into()),
            }
        }

        anyhow::bail!("Spotify Web API request failed after retries: {url}")
    }

    async fn get_json_once(
        &self,
        url: &str,
    ) -> std::result::Result<serde_json::Value, SpotifyWebError> {
        let access_token = self.session.bearer_token().await?;
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .send()
            .await?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let body = response.text().await?;
        append_api_log(&format!(
            "http route=first_party status={} retry_after={retry_after:?} url={url}",
            status.as_u16()
        ));

        if status.as_u16() == 401 {
            return Err(SpotifyWebError::Unauthorized(body));
        }

        if status.as_u16() == 429 {
            return Err(SpotifyWebError::RateLimited { retry_after, body });
        }

        if !status.is_success() {
            return Err(SpotifyWebError::Other(anyhow::anyhow!(
                "Spotify Web API request failed ({status}): {body}"
            )));
        }

        Ok(serde_json::from_str(&body)?)
    }
}

#[derive(Debug)]
enum SpotifyWebError {
    Unauthorized(String),
    RateLimited {
        retry_after: Option<Duration>,
        body: String,
    },
    Other(anyhow::Error),
}

impl From<anyhow::Error> for SpotifyWebError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

impl From<reqwest::Error> for SpotifyWebError {
    fn from(value: reqwest::Error) -> Self {
        Self::Other(value.into())
    }
}

impl From<serde_json::Error> for SpotifyWebError {
    fn from(value: serde_json::Error) -> Self {
        Self::Other(value.into())
    }
}

impl From<SpotifyWebError> for anyhow::Error {
    fn from(value: SpotifyWebError) -> Self {
        match value {
            SpotifyWebError::Unauthorized(body) => {
                anyhow::anyhow!("Spotify Web API request failed (401 Unauthorized): {body}")
            }
            SpotifyWebError::RateLimited { retry_after, body } => {
                SpotifyRateLimitError { retry_after, body }.into()
            }
            SpotifyWebError::Other(err) => err,
        }
    }
}

fn clear_terminal_after_oauth(tx: &mpsc::Sender<WorkerEvent>) {
    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
    );
    let _ = tx.try_send(WorkerEvent::ForceRedraw);
}

fn append_api_log(message: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("echo-debug-artist.log")
    {
        let _ = writeln!(file, "{} {message}", chrono::Utc::now().to_rfc3339());
    }
}
