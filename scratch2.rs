use std::error::Error;
use echo::config::AppConfig;
use echo::worker::spotify::SpotifyWorker;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = AppConfig::load();
    let client = SpotifyWorker::new(&config).await?;
    match client.fetch_playlists().await {
        Ok(playlists) => println!("Loaded {} playlists", playlists.len()),
        Err(e) => println!("Failed to fetch playlists: {:?}", e),
    }
    Ok(())
}
