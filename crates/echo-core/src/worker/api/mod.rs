pub mod cache;
pub mod client;
pub mod first_party;
pub mod library;
pub mod lyrics;
pub mod parse;
pub mod playback;
pub mod policy;
pub mod rate_limit;
pub mod search;

use crate::config::{AppConfig, AuthTokens};
use rspotify::{
    AuthCodeSpotify, ClientError, Credentials, OAuth,
    http::HttpError,
    model::Token,
    prelude::{BaseClient, OAuthClient},
};
use std::collections::HashSet;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CALLBACK_ADDRESS: &str = "127.0.0.1:8888";
const CALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Debug, Error)]
pub enum SpotifyAuthError {
    #[error("Spotify authorization has expired")]
    ReauthorizationRequired,
    #[error("{0}")]
    TemporaryFailure(String),
}

#[derive(Clone)]
pub struct SpotifyWorker {
    pub client: AuthCodeSpotify,
    pub device_id: Option<String>,
}

impl SpotifyWorker {
    pub async fn new(config: &AppConfig) -> Result<Self, SpotifyAuthError> {
        let spotify = Self::client(config)?;

        if Self::restore_session(&spotify, config).await? {
            return Ok(Self {
                client: spotify,
                device_id: None,
            });
        }

        Self::authorize_interactively(&spotify).await?;
        Self::persist_tokens(&spotify).await?;
        Ok(Self {
            client: spotify,
            device_id: None,
        })
    }

    fn client(config: &AppConfig) -> Result<AuthCodeSpotify, SpotifyAuthError> {
        let creds = config.spotify_credentials.as_ref().ok_or_else(|| {
            SpotifyAuthError::TemporaryFailure(
                "Spotify developer credentials are not configured".to_string(),
            )
        })?;
        let credentials = Credentials::new(&creds.client_id, &creds.client_secret);
        let scopes = spotify_scopes();
        let oauth = OAuth {
            redirect_uri: format!("http://{CALLBACK_ADDRESS}/callback"),
            scopes,
            ..Default::default()
        };

        Ok(AuthCodeSpotify::with_config(
            credentials,
            oauth,
            rspotify::Config::default(),
        ))
    }

    async fn restore_session(
        spotify: &AuthCodeSpotify,
        config: &AppConfig,
    ) -> Result<bool, SpotifyAuthError> {
        let Some(tokens) = config.auth_tokens.as_ref() else {
            return Ok(false);
        };
        let (Some(access), Some(refresh)) = (&tokens.access_token, &tokens.refresh_token) else {
            return Ok(false);
        };

        *spotify.get_token().lock().await.unwrap() = Some(Token {
            access_token: access.clone(),
            refresh_token: Some(refresh.clone()),
            expires_in: chrono::Duration::zero(),
            expires_at: Some(chrono::Utc::now()),
            scopes: spotify_scopes(),
        });

        match spotify.refresh_token().await {
            Ok(()) => {
                Self::persist_tokens(spotify).await?;
                Ok(true)
            }
            Err(error) => match classify_refresh_error(error).await {
                SpotifyAuthError::ReauthorizationRequired => {
                    AppConfig::clear_auth_tokens().map_err(|error| {
                        SpotifyAuthError::TemporaryFailure(format!(
                            "Failed to discard expired Spotify credentials: {error}"
                        ))
                    })?;
                    Ok(false)
                }
                error => Err(error),
            },
        }
    }

    async fn authorize_interactively(spotify: &AuthCodeSpotify) -> Result<(), SpotifyAuthError> {
        let auth_url = spotify
            .get_authorize_url(true)
            .map_err(temporary_auth_error)?;
        let listener = tokio::net::TcpListener::bind(CALLBACK_ADDRESS)
            .await
            .map_err(|error| {
                SpotifyAuthError::TemporaryFailure(format!(
                    "Failed to listen for Spotify's authorization callback: {error}"
                ))
            })?;

        webbrowser::open(&auth_url).map_err(|error| {
            SpotifyAuthError::TemporaryFailure(format!(
                "Failed to open Spotify authorization in the browser: {error}"
            ))
        })?;

        let (mut socket, _) = tokio::time::timeout(CALLBACK_TIMEOUT, listener.accept())
            .await
            .map_err(|_| {
                SpotifyAuthError::TemporaryFailure(
                    "Spotify authorization timed out; run :spotifylogin to retry".to_string(),
                )
            })?
            .map_err(|error| {
                SpotifyAuthError::TemporaryFailure(format!(
                    "Failed to accept Spotify's authorization callback: {error}"
                ))
            })?;

        let mut buffer = [0; 4096];
        let bytes_read = socket.read(&mut buffer).await.map_err(|error| {
            SpotifyAuthError::TemporaryFailure(format!(
                "Failed to read Spotify's authorization callback: {error}"
            ))
        })?;
        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        let code = callback_parameter(&request, "code").ok_or_else(|| {
            let detail = callback_parameter(&request, "error")
                .map(|error| format!(": {error}"))
                .unwrap_or_default();
            SpotifyAuthError::TemporaryFailure(format!(
                "Spotify authorization was cancelled or rejected{detail}"
            ))
        })?;

        let body = "<!DOCTYPE html><html><head><title>Success</title></head><body style=\"background-color: #121212; color: #ffffff; font-family: sans-serif; text-align: center; margin-top: 20%;\"><h1>Success, return to echo app</h1><p>You can safely close this tab.</p><script>setTimeout(() => window.close(), 3000);</script></body></html>";
        let response = format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.flush().await;

        spotify
            .request_token(&code)
            .await
            .map_err(temporary_auth_error)
    }

    pub async fn validate_session(&self) -> Result<(), SpotifyAuthError> {
        let expired = self
            .client
            .get_token()
            .lock()
            .await
            .unwrap()
            .as_ref()
            .is_none_or(Token::is_expired);
        if !expired {
            return Ok(());
        }

        match self.client.refresh_token().await {
            Ok(()) => Self::persist_tokens(&self.client).await,
            Err(error) => {
                let error = classify_refresh_error(error).await;
                if matches!(error, SpotifyAuthError::ReauthorizationRequired) {
                    AppConfig::clear_auth_tokens().map_err(|save_error| {
                        SpotifyAuthError::TemporaryFailure(format!(
                            "Failed to discard expired Spotify credentials: {save_error}"
                        ))
                    })?;
                }
                Err(error)
            }
        }
    }

    async fn persist_tokens(spotify: &AuthCodeSpotify) -> Result<(), SpotifyAuthError> {
        let token = spotify.get_token().lock().await.unwrap().clone();
        let Some(token) = token else {
            return Err(SpotifyAuthError::TemporaryFailure(
                "Spotify did not return an authorization token".to_string(),
            ));
        };
        let mut config = AppConfig::load();
        config.auth_tokens = Some(AuthTokens {
            access_token: Some(token.access_token),
            refresh_token: token.refresh_token,
        });
        config.save().map_err(|error| {
            SpotifyAuthError::TemporaryFailure(format!(
                "Failed to save Spotify authorization: {error}"
            ))
        })
    }

    /// Create a SpotifyWorker from an already-authenticated client (for spawned tasks).
    pub fn from_client(client: AuthCodeSpotify) -> Self {
        Self {
            client,
            device_id: None,
        }
    }
}

fn spotify_scopes() -> HashSet<String> {
    [
        "user-read-private",
        "playlist-read-private",
        "playlist-read-collaborative",
        "user-library-read",
        "user-library-modify",
        "user-modify-playback-state",
        "user-read-playback-state",
        "streaming",
        "app-remote-control",
        "playlist-modify-public",
        "playlist-modify-private",
        "user-top-read",
        "user-read-recently-played",
        "user-follow-read",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn callback_parameter(request: &str, name: &str) -> Option<String> {
    let target = request.split_whitespace().nth(1)?;
    let query = target.split_once('?')?.1;
    query.split('&').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key == name).then(|| {
            urlencoding::decode(value)
                .ok()
                .map(|value| value.into_owned())
        })?
    })
}

fn oauth_error_code(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(str::to_string)
}

async fn classify_refresh_error(error: ClientError) -> SpotifyAuthError {
    if let ClientError::Http(http_error) = error {
        if let HttpError::StatusCode(response) = *http_error {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status.as_u16() == 400 && oauth_error_code(&body).as_deref() == Some("invalid_grant")
            {
                return SpotifyAuthError::ReauthorizationRequired;
            }
            return SpotifyAuthError::TemporaryFailure(format!(
                "Spotify token refresh failed ({status}): {body}"
            ));
        }
        return SpotifyAuthError::TemporaryFailure(format!(
            "Spotify token refresh failed: {http_error}"
        ));
    }
    SpotifyAuthError::TemporaryFailure(format!("Spotify token refresh failed: {error}"))
}

fn temporary_auth_error(error: ClientError) -> SpotifyAuthError {
    SpotifyAuthError::TemporaryFailure(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_oauth_error_code() {
        assert_eq!(
            oauth_error_code(r#"{"error":"invalid_grant","error_description":"expired"}"#),
            Some("invalid_grant".to_string())
        );
        assert_eq!(
            oauth_error_code(r#"{"error":"invalid_client"}"#),
            Some("invalid_client".to_string())
        );
        assert_eq!(oauth_error_code("not json"), None);
    }

    #[test]
    fn parses_encoded_callback_parameters() {
        let request = "GET /callback?code=one%2Ftwo&state=test HTTP/1.1\r\n";
        assert_eq!(
            callback_parameter(request, "code").as_deref(),
            Some("one/two")
        );
        assert_eq!(callback_parameter(request, "error"), None);
    }
}
