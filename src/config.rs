use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use anyhow::Result;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SpotifyCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuthTokens {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AppConfig {
    pub spotify_credentials: Option<SpotifyCredentials>,
    pub auth_tokens: Option<AuthTokens>,
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("echo");
        path.push("config.toml");
        path
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(contents) = fs::read_to_string(&path) {
            toml::from_str(&contents).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string(self)?;
        fs::write(path, contents)?;
        Ok(())
    }
}
