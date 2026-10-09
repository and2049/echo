//! Width policy shared by the queue and lyrics dock.
pub(crate) const RIGHT_PANEL_DEFAULT: f32 = 320.0;
pub(crate) const RIGHT_PANEL_MIN: f32 = 180.0;
const RIGHT_PANEL_MAX: f32 = 480.0;
const MAIN_AREA_PREFERRED_MIN: f32 = 320.0;
const QUEUE_DURATION_MIN_WIDTH: f32 = 280.0;

pub(crate) fn preferred_width(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(RIGHT_PANEL_MIN, RIGHT_PANEL_MAX)
    } else {
        RIGHT_PANEL_DEFAULT
    }
}

/// Reserve room for the center where possible; at very small window sizes keep the
/// dock's controls usable. Window constraints never overwrite the saved preference.
pub(crate) fn fitted_width(preferred: f32, available: f32) -> f32 {
    preferred_width(preferred).min((available - MAIN_AREA_PREFERRED_MIN).max(RIGHT_PANEL_MIN))
}

pub(crate) fn show_queue_duration(width: f32) -> bool {
    width >= QUEUE_DURATION_MIN_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_constraints_are_temporary_and_keep_controls_usable() {
        let preferred = preferred_width(420.0);
        assert_eq!(fitted_width(preferred, 900.0), 420.0);
        assert_eq!(fitted_width(preferred, 600.0), 280.0);
        assert_eq!(fitted_width(preferred, 300.0), 180.0);
        assert_eq!(fitted_width(preferred, 900.0), 420.0);
        assert_eq!(preferred_width(-10.0), 180.0);
        assert_eq!(preferred_width(900.0), 480.0);
        assert_eq!(preferred_width(f32::NAN), 320.0);
    }

    #[test]
    fn duration_tracks_effective_width_including_window_constraints() {
        assert!(!show_queue_duration(fitted_width(320.0, 599.0)));
        assert!(show_queue_duration(fitted_width(320.0, 600.0)));
        assert!(!show_queue_duration(279.5));
        assert!(show_queue_duration(280.0));
    }
}
