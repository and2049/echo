use chrono::{DateTime, Utc};

use crate::models::{Track, TrackListContext, TrackListContextKind};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ContextDetails {
    pub public: Option<bool>,
    pub collaborative: bool,
    pub album_type: Option<String>,
    pub owner: String,
    pub collaborators: Vec<String>,
    pub track_count: Option<u32>,
    pub duration_ms: u64,
    pub release_year: Option<String>,
    pub description: Option<String>,
}

impl ContextDetails {
    pub fn label_key(&self, context: &TrackListContext) -> &'static str {
        if context.is_album() {
            return match self.album_type.as_deref() {
                Some("single") => "desktop.context.single",
                Some("compilation") => "desktop.context.compilation",
                _ => "desktop.context.album",
            };
        }
        if context.id == "LIKED_SONGS" || context.kind != TrackListContextKind::Playlist {
            return "desktop.context.playlist";
        }
        match (self.collaborative, self.public) {
            (true, _) => "desktop.context.collaborative",
            (_, Some(true)) => "desktop.context.public",
            (_, Some(false)) => "desktop.context.private",
            _ => "desktop.context.playlist",
        }
    }
}

pub fn total_duration(tracks: &[Track]) -> u64 {
    tracks
        .iter()
        .map(|track| u64::from(track.duration_ms))
        .sum()
}

pub fn header_summary(
    context: &TrackListContext,
    details: &ContextDetails,
    tracks: &[Track],
    tr: &impl Fn(&str) -> String,
) -> String {
    let owner = if details.owner.is_empty() {
        &context.subtitle
    } else {
        &details.owner
    };
    let mut segments = Vec::new();
    if !owner.is_empty() {
        segments.push(owner.clone());
    }
    if context.is_album() {
        if let Some(year) = &details.release_year {
            segments.push(year.clone());
        }
    } else if !details.collaborators.is_empty() {
        segments.push(tr("desktop.context.with").replace(
            "{names}",
            &join_names(&details.collaborators, &tr("desktop.context.and")),
        ));
    }
    let count = details
        .track_count
        .map(|n| n as usize)
        .unwrap_or(tracks.len());
    segments.push(
        tr("desktop.context.songs")
            .replace("{n}", &count.to_string())
            .replace("{total}", &format_duration_with(total_duration(tracks), tr)),
    );
    segments.join(" • ")
}

pub fn join_names(names: &[String], conjunction: &str) -> String {
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{}{conjunction}{last}", rest.join(", ")),
    }
}

pub fn format_duration(duration_ms: u64) -> String {
    format_duration_with(duration_ms, &|key| crate::i18n::t(key, "en"))
}

pub fn format_duration_with(duration_ms: u64, tr: &impl Fn(&str) -> String) -> String {
    let seconds = duration_ms / 1000;
    let (key, first, second) = if seconds >= 3600 {
        (
            "desktop.context.hours_minutes",
            seconds / 3600,
            seconds / 60 % 60,
        )
    } else {
        (
            "desktop.context.minutes_seconds",
            seconds / 60,
            seconds % 60,
        )
    };
    tr(key)
        .replace("{a}", &first.to_string())
        .replace("{b}", &second.to_string())
}

pub fn format_added_at(now: DateTime<Utc>, added_at: &str) -> String {
    format_added_at_with(now, added_at, &|key| crate::i18n::t(key, "en"))
}

pub fn format_added_at_with(
    now: DateTime<Utc>,
    added_at: &str,
    tr: &impl Fn(&str) -> String,
) -> String {
    let Ok(date) = DateTime::parse_from_rfc3339(added_at) else {
        return tr("desktop.context.unknown_date");
    };
    let seconds = (now - date.with_timezone(&Utc)).num_seconds().max(0);
    let (unit, count) = match seconds {
        0..60 => ("seconds", seconds),
        60..3600 => ("minutes", seconds / 60),
        3600..86400 => ("hours", seconds / 3600),
        86400..604800 => ("days", seconds / 86400),
        604800..2419200 => ("weeks", seconds / 604800),
        _ => return date.format(&tr("desktop.context.date_format")).to_string(),
    };
    let singular = if count == 1 { "_one" } else { "" };
    tr(&format!("desktop.context.{unit}{singular}_ago")).replace("{n}", &count.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_summary_uses_reported_count_and_kind_specific_credits() {
        crate::i18n::init();
        let tr = |key: &str| crate::i18n::t(key, "en");
        let mut context = TrackListContext::playlist(
            "p".into(),
            "P".into(),
            "Owner".into(),
            "owner".into(),
            None,
        );
        let mut details = ContextDetails {
            track_count: Some(125),
            collaborators: vec!["A".into(), "B".into()],
            release_year: Some("2026".into()),
            ..Default::default()
        };
        assert_eq!(
            header_summary(&context, &details, &[], &tr),
            "Owner • with A and B • 125 songs, 0 min 0 sec"
        );
        context.kind = TrackListContextKind::Album;
        details.owner = "Artists".into();
        assert_eq!(
            header_summary(&context, &details, &[], &tr),
            "Artists • 2026 • 125 songs, 0 min 0 sec"
        );
        details.track_count = None;
        assert!(header_summary(&context, &details, &[], &tr).ends_with("0 songs, 0 min 0 sec"));
    }

    #[test]
    fn duration_formats_boundaries_and_large_totals() {
        crate::i18n::init();
        for (ms, expected) in [
            (0, "0 min 0 sec"),
            (2712000, "45 min 12 sec"),
            (3600000, "1 hr 0 min"),
            (7980000, "2 hr 13 min"),
        ] {
            assert_eq!(format_duration(ms), expected);
        }
        assert_eq!(total_duration(&[]), 0);
        let track: Track = serde_json::from_value(serde_json::json!({"id":"a", "name":"a", "artist":"a", "duration_ms":u32::MAX,"image_url":null,"album_id":null})).unwrap();
        assert_eq!(
            total_duration(&[track.clone(), track]),
            u64::from(u32::MAX) * 2
        );
    }

    #[test]
    fn added_dates_cover_all_buckets_and_invalid_input() {
        crate::i18n::init();
        let now = "2026-02-01T00:00:00Z".parse::<DateTime<Utc>>().unwrap();
        for (seconds, expected) in [
            (0, "0 seconds ago"),
            (1, "1 second ago"),
            (31, "31 seconds ago"),
            (300, "5 minutes ago"),
            (10800, "3 hours ago"),
            (172800, "2 days ago"),
            (1209600, "2 weeks ago"),
            (2419200, "Jan 4, 2026"),
        ] {
            assert_eq!(
                format_added_at(
                    now,
                    &(now - chrono::Duration::seconds(seconds)).to_rfc3339()
                ),
                expected
            );
        }
        assert_eq!(format_added_at(now, "invalid"), "—");
        assert_eq!(
            format_added_at(now, "2027-01-01T00:00:00Z"),
            "0 seconds ago"
        );
    }

    #[test]
    fn context_labels_use_kind_and_visibility() {
        let mut context =
            TrackListContext::playlist("p".into(), "p".into(), "o".into(), "o".into(), None);
        let mut details = ContextDetails::default();
        assert_eq!(details.label_key(&context), "desktop.context.playlist");
        details.public = Some(false);
        assert_eq!(details.label_key(&context), "desktop.context.private");
        details.public = Some(true);
        assert_eq!(details.label_key(&context), "desktop.context.public");
        details.collaborative = true;
        assert_eq!(details.label_key(&context), "desktop.context.collaborative");
        context.id = "LIKED_SONGS".into();
        assert_eq!(details.label_key(&context), "desktop.context.playlist");
        context.kind = TrackListContextKind::Album;
        for kind in ["album", "single", "compilation"] {
            details.album_type = Some(kind.into());
            assert_eq!(
                details.label_key(&context),
                format!("desktop.context.{kind}")
            );
        }
    }

    #[test]
    fn names_have_a_final_conjunction() {
        assert_eq!(join_names(&[], " and "), "");
        assert_eq!(join_names(&["A".into()], " and "), "A");
        assert_eq!(join_names(&["A".into(), "B".into()], " and "), "A and B");
        assert_eq!(
            join_names(&["A".into(), "B".into(), "C".into()], " and "),
            "A, B and C"
        );
    }
}
