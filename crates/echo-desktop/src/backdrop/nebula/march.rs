//! The nebula's field: a ray from the eye through the canvas, stepped a dozen times, each
//! point folded by octaves of travelling sines before it is weighed. The recipe follows the
//! shader `for (z, d; i++ < 20;) { p = z * dir; d = 4; 6 times: d += d, p = p.yzx +
//! sin(p * d - T) / d; z += .1 - len(p) / 9; O += z * z * (2 - sin(p * 5)) / len(p) }` with
//! the near steps skipped (see [`NEAR`]) and the color term reduced to a hue so the palette
//! can supply the colors.

const STEPS: usize = 20;
const OCTAVES: usize = 6;
/// Half the first octave's frequency; every octave doubles the one before.
const SEED_FREQUENCY: f32 = 4.0;
/// The hue is read from sines of the point at this frequency.
const HUE_FREQUENCY: f32 = 5.0;
/// The filaments: a fine sine of the point, folded into the distance the light divides by.
const FILAMENT_FREQUENCY: f32 = 33.0;
const FILAMENT_STRENGTH: f32 = 1.0 / 99.0;
/// Steps closer to the eye than this shed no light: there every ray still sits at the fold's
/// fixed point, so in the shader they flash the whole canvas whenever the sines' offset
/// passes zero.
const NEAR: f32 = 0.25;

/// What one ray gathered: how much light, and where along the palette (in `0..1`).
pub struct Sample {
    pub glow: f32,
    pub hue: f32,
}

#[derive(Clone, Copy)]
struct V3 {
    x: f32,
    y: f32,
    z: f32,
}

impl V3 {
    fn len(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    fn scale(self, k: f32) -> V3 {
        V3 {
            x: self.x * k,
            y: self.y * k,
            z: self.z * k,
        }
    }

    /// One octave of the fold: rotate the axes and push each by a sine of itself.
    fn fold(self, frequency: f32, time: f32) -> V3 {
        let push = |v: f32| (v * frequency - time).sin() / frequency;
        V3 {
            x: self.y + push(self.x),
            y: self.z + push(self.y),
            z: self.x + push(self.z),
        }
    }
}

/// The ray through canvas point (`u`, `v`), both about `-0.8..0.8`, at `time` in radians.
pub fn march(u: f32, v: f32, time: f32) -> Sample {
    let dir = V3 { x: u, y: v, z: 1.0 };
    let dir = dir.scale(1.0 / dir.len());
    let (mut z, mut glow, mut hue) = (0.0f32, 0.0f32, 0.0f32);
    for _ in 0..STEPS {
        let mut p = dir.scale(z);
        let mut frequency = SEED_FREQUENCY;
        for _ in 0..OCTAVES {
            frequency += frequency;
            p = p.fold(frequency, time);
        }
        let len = p.len();
        z += 0.1 - len / 9.0;
        if z < NEAR {
            continue;
        }
        let filament = |v: f32| (v * FILAMENT_FREQUENCY).sin() * FILAMENT_STRENGTH;
        let filaments = V3 {
            x: filament(p.x),
            y: filament(p.y),
            z: filament(p.z),
        }
        .len();
        let weight = z * z / (0.001 + (len * len + filaments * filaments).sqrt());
        glow += weight;
        hue += weight * (p.x * HUE_FREQUENCY).sin();
    }
    Sample {
        glow,
        hue: (hue / glow + 1.0) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    #[test]
    fn the_field_is_periodic_in_time_and_finite() {
        for (u, v) in [(0.0, 0.0), (0.6, -0.3), (-0.7, 0.4)] {
            let (a, b) = (march(u, v, 0.0), march(u, v, TAU));
            assert!(
                (a.glow - b.glow).abs() < 1e-3 * a.glow && (a.hue - b.hue).abs() < 1e-3,
                "{u},{v}: {} {} {} {}",
                a.glow,
                b.glow,
                a.hue,
                b.hue
            );
            assert!(a.glow.is_finite() && a.glow > 0.0 && (0.0..=1.0).contains(&a.hue));
        }
    }

    #[test]
    fn neighbouring_rays_gather_similar_light() {
        let (a, b) = (march(0.2, 0.1, 1.0), march(0.21, 0.1, 1.0));
        assert!((a.glow - b.glow).abs() < 0.2 * a.glow.max(b.glow));
    }
}
