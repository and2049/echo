pub(crate) fn artist_retry_after_secs(err: &anyhow::Error) -> Option<u64> {
    super::api::first_party::rate_limit_error(err).map(|err| err.retry_after_secs())
}

pub(crate) fn api_request_error_message(err: &anyhow::Error) -> String {
    if let Some(rate_limit) = super::api::first_party::rate_limit_error(err) {
        return format!(
            "rate limited. Try again in {}.",
            super::api::client::format_retry_after(rate_limit.cooldown())
        );
    }
    if super::api::rate_limit::is_probable_rate_limit(err) {
        return "rate limited. Try again in 1m.".to_string();
    }

    err.to_string()
}

/// User-facing message for a failed play request.
///
/// The dominant failure is echo's own librespot Connect device not being registered, which
/// Spotify reports as a bare HTTP 404 on `/v1/me/player/play`. Surfaced verbatim that reads
/// like a bad track id, so it gets named for what it is and points at the daemon log.
pub(crate) fn playback_error_message(err: &anyhow::Error) -> String {
    let rendered = format!("{err:?}");
    if rendered.contains(super::api::playback::NO_DEVICE_ERROR)
        || rendered.contains("status: 404")
        || rendered.contains("status code 404")
    {
        return "no playback device. echo's Spotify Connect device isn't running — see echo-debug-fallback.log.".to_string();
    }
    api_request_error_message(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn typed_rate_limit_drives_artist_retry_after() {
        let err: anyhow::Error = crate::worker::api::first_party::SpotifyRateLimitError {
            retry_after: Some(Duration::from_secs(4)),
            body: String::new(),
        }
        .into();

        assert_eq!(artist_retry_after_secs(&err), Some(4));
    }

    #[test]
    fn typed_rate_limit_formats_browse_status() {
        let err: anyhow::Error = crate::worker::api::first_party::SpotifyRateLimitError {
            retry_after: Some(Duration::from_secs(43)),
            body: String::new(),
        }
        .into();

        assert_eq!(
            api_request_error_message(&err),
            "rate limited. Try again in 43s."
        );
    }

    #[test]
    fn missing_device_is_named_rather_than_shown_as_a_404() {
        let err = anyhow::anyhow!(crate::worker::api::playback::NO_DEVICE_ERROR);
        assert!(playback_error_message(&err).starts_with("no playback device"));

        // Spotify's own 404 for the same condition maps to the same message.
        let http = anyhow::anyhow!(
            "Http(StatusCode(Response {{ url: \"https://api.spotify.com/v1/me/player/play\", status: 404 }}))"
        );
        assert!(playback_error_message(&http).starts_with("no playback device"));
    }

    #[test]
    fn unrelated_errors_keep_their_own_message() {
        // A 404 appearing in a track id or URL must not be read as a missing device.
        let err = anyhow::anyhow!("bad track id 404abc404def");
        assert_eq!(playback_error_message(&err), "bad track id 404abc404def");
    }
}
