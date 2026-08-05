//! Volume mapping shared by both playback paths.
//!
//! Spotify volume is applied client-side by librespot's `SoftMixer`, local playback by rodio's
//! sink gain. The two use different units, so both go through here to keep the same taper.

/// Matches librespot's `VolumeCtrl::DEFAULT_DB_RANGE`.
pub const VOLUME_DB_RANGE: f64 = 60.0;

/// Maps a 0-100 UI volume onto librespot's `u16` mixer range. 100 must land exactly on
/// `u16::MAX`, which `VolumeCtrl` special-cases to unity gain.
pub fn volume_to_mixer(vol: u32) -> u16 {
    ((vol.min(100) * 65535) / 100) as u16
}

/// Mirrors librespot's `CubicMapping` so the local rodio sink follows the same curve as the
/// Spotify mixer. `rodio::Sink::set_volume` is a plain linear multiplier, so the curve has to be
/// applied here rather than by the sink.
pub fn cubic_gain(vol: u32) -> f32 {
    // librespot special-cases both ends; without this, 0 would map to 0.1^3 rather than silence.
    if vol == 0 {
        return 0.0;
    }
    if vol >= 100 {
        return 1.0;
    }
    // 10f32.powf(-VOLUME_DB_RANGE / 60.0), i.e. the cubic voltage-to-dB ratio.
    const MIN_NORM: f32 = 0.1;
    let normalized = vol as f32 / 100.0;
    (normalized * (1.0 - MIN_NORM) + MIN_NORM).powi(3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_0_maps_to_mixer_0() {
        assert_eq!(volume_to_mixer(0), 0);
    }

    #[test]
    fn volume_100_maps_to_mixer_max() {
        assert_eq!(volume_to_mixer(100), 65535);
    }

    #[test]
    fn volume_50_maps_to_half_range() {
        assert_eq!(volume_to_mixer(50), 32767);
    }

    #[test]
    fn volume_boundary_u8_max_maps_without_overflow() {
        let vol: u8 = 100;
        let mixer_vol = ((vol as u32 * 65535) / 100) as u16;
        assert_eq!(mixer_vol, 65535);
        assert_eq!(volume_to_mixer(vol.into()), 65535);
    }

    #[test]
    fn volume_above_100_is_clamped() {
        assert_eq!(volume_to_mixer(255), 65535);
    }

    #[test]
    fn cubic_gain_0_is_silent() {
        assert_eq!(cubic_gain(0), 0.0);
    }

    // The property this whole fix rests on: 100% is unity gain, so playback is bit-exact.
    #[test]
    fn cubic_gain_100_is_unity() {
        assert_eq!(cubic_gain(100), 1.0);
    }

    #[test]
    fn cubic_gain_above_100_is_clamped_to_unity() {
        assert_eq!(cubic_gain(255), 1.0);
    }

    #[test]
    fn cubic_gain_50_matches_librespot_curve() {
        // (0.5 * 0.9 + 0.1)^3 = 0.55^3 ≈ 0.166, about -15.6 dB.
        assert!((cubic_gain(50) - 0.166_375).abs() < 1e-6);
    }

    // `cubic_gain` reimplements librespot's curve because rodio's sink only takes a linear
    // factor. Check the two agree across the range so the sources cannot drift apart if
    // librespot changes its mapping. They are not bit-identical: the Spotify side quantises
    // through a u16, which costs up to ~1e-5 of gain (far below 0.001 dB).
    #[test]
    fn cubic_gain_matches_librespot_mixer_curve() {
        use librespot_playback::config::VolumeCtrl;
        use librespot_playback::mixer::mappings::MappedCtrl;

        let ctrl = VolumeCtrl::Cubic(VOLUME_DB_RANGE);
        for vol in 0..=100u32 {
            let librespot = ctrl.to_mapped(volume_to_mixer(vol)) as f32;
            let ours = cubic_gain(vol);
            assert!(
                (librespot - ours).abs() < 1e-4,
                "volume {vol}: librespot {librespot} vs ours {ours}"
            );
        }
    }

    #[test]
    fn cubic_gain_is_monotonic() {
        for vol in 1..=100u32 {
            assert!(
                cubic_gain(vol) > cubic_gain(vol - 1),
                "gain must increase at {vol}"
            );
        }
    }
}
