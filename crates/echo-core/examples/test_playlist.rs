use anyhow::Result;
use rspotify::{AuthCodeSpotify, Credentials, OAuth, prelude::*};
use std::collections::HashSet;

#[tokio::main]
async fn main() -> Result<()> {
    // load config manually
    let content = std::fs::read_to_string("echo-config.toml").unwrap_or_default();
    let config: crate_mock::AppConfig = toml::from_str(&content).unwrap();

    let creds = config.spotify_credentials.as_ref().unwrap();
    let credentials = Credentials::new(&creds.client_id, &creds.client_secret);

    let scopes: HashSet<String> = [
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
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    let oauth = OAuth {
        redirect_uri: "http://127.0.0.1:8888/callback".to_string(),
        scopes,
        ..Default::default()
    };

    let spotify = AuthCodeSpotify::with_config(credentials, oauth, Default::default());

    if let Some(tokens) = &config.auth_tokens {
        let token = rspotify::model::Token {
            access_token: tokens.access_token.clone().unwrap(),
            refresh_token: tokens.refresh_token.clone(),
            expires_in: chrono::Duration::seconds(0),
            expires_at: Some(chrono::Utc::now()),
            scopes: HashSet::new(),
        };
        *spotify.get_token().lock().await.unwrap() = Some(token);
    }

    use futures_util::StreamExt;
    let stream = spotify.current_user_saved_albums(None);
    let mut stream = Box::pin(stream);
    let mut albums = vec![];
    while let Some(item) = stream.next().await {
        albums.push(item);
        if albums.len() >= 5 {
            break;
        }
    }
    println!("Albums: {:?}", albums);

    Ok(())
}

mod crate_mock {
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Deserialize, Serialize, Default, Clone)]
    pub struct AppConfig {
        pub auth_tokens: Option<AuthTokens>,
        pub spotify_credentials: Option<SpotifyCredentials>,
    }
    #[derive(Debug, Deserialize, Serialize, Default, Clone)]
    pub struct AuthTokens {
        pub access_token: Option<String>,
        pub refresh_token: Option<String>,
    }
    #[derive(Debug, Deserialize, Serialize, Default, Clone)]
    pub struct SpotifyCredentials {
        pub client_id: String,
        pub client_secret: String,
    }
}
