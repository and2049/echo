use crate::models::{LyricLine, Lyrics};
use anyhow::Result;

pub async fn fetch_lyrics(title: &str, artist: &str, duration_ms: u32) -> Result<Option<Lyrics>> {
    let client = reqwest::Client::new();
    let duration_sec = duration_ms / 1000;
    
    let url = format!(
        "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
        urlencoding::encode(title),
        urlencoding::encode(artist),
        duration_sec
    );

    let resp = client.get(&url)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let json: serde_json::Value = resp.json().await?;

    if let Some(synced_lyrics) = json.get("syncedLyrics").and_then(|v| v.as_str()) {
        let lines = parse_lrc(synced_lyrics);
        if !lines.is_empty() {
            return Ok(Some(Lyrics { lines }));
        }
    }

    Ok(None)
}

fn parse_lrc(lrc: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    for line in lrc.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse [mm:ss.xx]
        if line.starts_with('[') {
            if let Some(close_bracket) = line.find(']') {
                let time_str = &line[1..close_bracket];
                let text = line[close_bracket + 1..].trim();
                
                if let Some(ms) = parse_time(time_str) {
                    lines.push(LyricLine {
                        start_ms: ms,
                        text: text.to_string(),
                    });
                }
            }
        }
    }
    
    // Sort by timestamp
    lines.sort_by_key(|l| l.start_ms);
    lines
}

fn parse_time(time_str: &str) -> Option<u32> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let minutes: u32 = parts[0].parse().ok()?;
    
    let sec_parts: Vec<&str> = parts[1].split('.').collect();
    let seconds: u32 = sec_parts[0].parse().ok()?;
    
    let ms: u32 = if sec_parts.len() > 1 {
        let frac_str = sec_parts[1];
        if frac_str.len() == 2 {
            frac_str.parse::<u32>().ok()? * 10
        } else if frac_str.len() == 3 {
            frac_str.parse().ok()?
        } else {
            0
        }
    } else {
        0
    };

    Some(minutes * 60 * 1000 + seconds * 1000 + ms)
}
