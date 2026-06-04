use std::time::Duration;

const MAX_INLINE_RETRY_AFTER: Duration = Duration::from_secs(5);
pub const DEFAULT_RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub struct SpotifyRateLimitError {
    pub retry_after: Option<Duration>,
    pub body: String,
}

impl SpotifyRateLimitError {
    pub fn cooldown(&self) -> Duration {
        self.retry_after.unwrap_or(DEFAULT_RATE_LIMIT_COOLDOWN)
    }

    pub fn retry_after_secs(&self) -> u64 {
        self.cooldown().as_secs()
    }
}

impl std::fmt::Display for SpotifyRateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Spotify Web API request failed (429 Too Many Requests, retry_after={:?}): {}",
            self.retry_after, self.body
        )
    }
}

impl std::error::Error for SpotifyRateLimitError {}

pub fn rate_limit_error(err: &anyhow::Error) -> Option<&SpotifyRateLimitError> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<SpotifyRateLimitError>())
}

pub(crate) fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

pub(crate) fn fallback_backoff(attempt: usize) -> Duration {
    Duration::from_millis(match attempt {
        0 => 750,
        1 => 1_500,
        _ => 3_000,
    })
}

pub(crate) fn should_retry_inline(delay: Duration) -> bool {
    delay <= MAX_INLINE_RETRY_AFTER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_retry_after_seconds() {
        assert_eq!(parse_retry_after("12"), Some(Duration::from_secs(12)));
    }

    #[test]
    fn rejects_missing_retry_after() {
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("later"), None);
    }

    #[test]
    fn retries_short_rate_limits_inline() {
        assert!(should_retry_inline(Duration::from_secs(5)));
    }

    #[test]
    fn surfaces_long_rate_limits_without_sleeping() {
        assert!(!should_retry_inline(Duration::from_secs(6)));
        assert!(!should_retry_inline(Duration::from_secs(43_300)));
    }

    #[test]
    fn missing_retry_after_uses_default_cooldown() {
        let err = SpotifyRateLimitError {
            retry_after: None,
            body: String::new(),
        };
        assert_eq!(err.cooldown(), DEFAULT_RATE_LIMIT_COOLDOWN);
    }

    #[test]
    fn retry_after_uses_spotify_cooldown() {
        let err = SpotifyRateLimitError {
            retry_after: Some(Duration::from_secs(43_300)),
            body: String::new(),
        };
        assert_eq!(err.cooldown(), Duration::from_secs(43_300));
    }
}
