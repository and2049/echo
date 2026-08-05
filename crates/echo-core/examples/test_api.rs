use anyhow::Result;
use reqwest::Client;

#[tokio::main]
async fn main() -> Result<()> {
    let client = Client::new();
    let resp = client
        .get("https://open.spotify.com/get_access_token?reason=transport&productType=web_player")
        .send()
        .await?;

    let txt = resp.text().await?;
    println!("Web Token Response: {}", txt);

    Ok(())
}
