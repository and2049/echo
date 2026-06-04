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
}
