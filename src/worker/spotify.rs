use rspotify::{prelude::*, AuthCodeSpotify, Credentials, OAuth};
use std::collections::HashSet;
use crate::config::AppConfig;
use crate::models::{Playlist, Track};
use anyhow::Result;

pub struct SpotifyWorker {
    pub client: AuthCodeSpotify,
}

impl SpotifyWorker {
    pub async fn new(config: &AppConfig) -> Result<Self> {
        let creds = config.spotify_credentials.as_ref().unwrap();
        let credentials = Credentials::new(&creds.client_id, &creds.client_secret);
        
        let scopes: HashSet<String> = [
            "user-read-private",
            "playlist-read-private",
            "playlist-read-collaborative",
            "user-library-read",
            "user-modify-playback-state",
            "user-read-playback-state",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let oauth = OAuth {
            redirect_uri: "http://127.0.0.1:8888/callback".to_string(),
            scopes: scopes.clone(),
            ..Default::default()
        };

        let mut spotify = AuthCodeSpotify::new(credentials, oauth);
        
        if let Some(tokens) = &config.auth_tokens {
            if let Some(access) = &tokens.access_token {
                if let Some(refresh) = &tokens.refresh_token {
                    use rspotify::model::Token;
                    use chrono::Utc;
                    
                    let token = Token {
                        access_token: access.clone(),
                        refresh_token: Some(refresh.clone()),
                        expires_in: chrono::Duration::seconds(0),
                        expires_at: Some(Utc::now()),
                        scopes: scopes.clone(),
                    };
                    
                    *spotify.get_token().lock().await.unwrap() = Some(token);
                    return Ok(Self { client: spotify });
                }
            }
        }
        
        let auth_url = spotify.get_authorize_url(false)?;
        
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:8888").await?;

        // Open the browser to the REAL Spotify auth URL
        if let Err(_e) = webbrowser::open(&auth_url) {
            // fallback if webbrowser fails
        }

        if let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = [0; 1024];
            if let Ok(bytes_read) = socket.read(&mut buffer).await {
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);
                if let Some(code_start) = request.find("code=") {
                    let code_rest = &request[code_start + 5..];
                    let code = code_rest.split_whitespace().next().unwrap_or("").split('&').next().unwrap_or("");
                    let code_str = code.to_string();
                    
                    let body = "<!DOCTYPE html><html><head><title>Success</title></head><body style=\"background-color: #121212; color: #ffffff; font-family: sans-serif; text-align: center; margin-top: 20%;\"><h1>Success, return to echo app</h1><p>You can safely close this tab.</p><script>setTimeout(() => window.close(), 3000);</script></body></html>";
                    let response = format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\r\n{}", body.len(), body);
                    
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.flush().await;
                    drop(socket); // explicitly close the socket so the browser finishes loading
                    
                    spotify.request_token(&code_str).await?;
                }
            }
        }
        
        // Write the fetched tokens to our BYOK config.toml
        let token_mutex = spotify.get_token();
        let token_guard = token_mutex.lock().await;
        if let Some(t) = token_guard.unwrap().as_ref() {
            let mut app_config = AppConfig::load();
            app_config.auth_tokens = Some(crate::config::AuthTokens {
                access_token: Some(t.access_token.clone()),
                refresh_token: t.refresh_token.clone(),
            });
            let _ = app_config.save();
        }
        
        Ok(Self { client: spotify })
    }

    pub async fn fetch_playlists(&self) -> Result<Vec<Playlist>> {
        let page = self.client.current_user_playlists_manual(None, None).await?;
        let mut out = Vec::new();
        for p in page.items {
            out.push(Playlist {
                id: p.id.id().to_string(),
                name: p.name,
            });
        }
        Ok(out)
    }

    pub async fn fetch_tracks(&self, playlist_id: &str) -> Result<Vec<Track>> {
        let id = rspotify::model::PlaylistId::from_id(playlist_id)?;
        let page = self.client.playlist_items_manual(id, None, None, None, None).await?;
        let mut out = Vec::new();
        for item in page.items {
            if let Some(rspotify::model::PlayableItem::Track(track)) = item.item {
                out.push(Track {
                    id: track.id.map(|i| i.to_string()).unwrap_or_default(),
                    name: track.name,
                    artist: track.artists.into_iter().map(|a| a.name).collect::<Vec<_>>().join(", "),
                });
            }
        }
        Ok(out)
    }
}
