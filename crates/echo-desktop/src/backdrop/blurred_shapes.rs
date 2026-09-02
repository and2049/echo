//! The blurred-shapes backdrop: a few discs in the cover's colors over a deep (or pale) tint
//! of its primary, blurred until they read as soft lights, and fading most of the way back to
//! the base toward the bottom. The "shapes, then blur" recipe behind Apple Music's
//! now-playing wash. Disc luminance is clamped per tone so the text stays readable wherever
//! the lyrics land on it.
//!
//! The picture is a function of `phase`, one trip around a loop in `0..1`: every disc circles
//! the middle of the canvas on its own tilted ellipse once per loop, each at a different size,
//! direction and starting point, and fades most of the way out and back once, so phase 1 is
//! phase 0 again and the loop never seams. The fade follows a squared sine, since perceived
//! brightness grows slower than the light itself: the disc lingers dim and blooms quickly.
//! Discs are painted largest first so the small ones stay visible on top. Run with
//! `ECHO_BACKDROP_BLUR=0` to skip the blur and watch the raw discs when tuning the motion.

use std::f32::consts::TAU;

use super::palette::{CoverPalette, Rgb};
use super::raster::Raster;
use super::Tone;

const SIZE: usize = 96;
const BLUR_RADIUS: usize = 11;
/// Per disc, prominent color first: its radius, the semi-axes of its orbit around the canvas
/// middle (a negative first axis runs the orbit the other way), the orbit's tilt, where on it
/// the loop starts and where in its fade, all as fractions of the canvas or in turns. Starts
/// are a quarter loop apart, and each fade is set so the disc blooms at the top of its orbit,
/// clear of the bottom fade, so one light blooms every quarter loop.
const DISCS: [Disc; 4] = [
    Disc { radius: 0.26, axes: (0.34, 0.22), tilt: 0.08, start: 0.89, fade: 0.50 },
    Disc { radius: 0.28, axes: (-0.30, 0.26), tilt: -0.06, start: 0.43, fade: 0.00 },
    Disc { radius: 0.20, axes: (0.36, 0.20), tilt: 0.15, start: 0.06, fade: 0.75 },
    Disc { radius: 0.22, axes: (-0.28, 0.28), tilt: 0.00, start: 0.75, fade: 0.25 },
];
const MIDDLE: (f32, f32) = (0.5, 0.5);
/// The fade never goes below this, so a disc keeps a trace of its color on the way out.
const PRESENCE_FLOOR: f32 = 0.2;
const DARK_SHAPE_LUMINANCE: (f32, f32) = (0.18, 0.42);
const LIGHT_SHAPE_LUMINANCE: (f32, f32) = (0.55, 0.80);
/// The fade toward the base runs between these fractions of the height and stops this far
/// short of it, so the bottom keeps a trace of the lights instead of going flat.
const FADE: (f32, f32) = (0.45, 0.92);
const FADE_DEPTH: f32 = 0.8;

struct Disc {
    radius: f32,
    axes: (f32, f32),
    tilt: f32,
    start: f32,
    fade: f32,
}

impl Disc {
    /// Where the disc's center is at `phase`, as fractions of the canvas.
    fn position(&self, phase: f32) -> (f32, f32) {
        let along = TAU * (phase + self.start);
        let (x, y) = (self.axes.0 * along.cos(), self.axes.1 * along.sin());
        let (sin, cos) = (TAU * self.tilt).sin_cos();
        (MIDDLE.0 + x * cos - y * sin, MIDDLE.1 + x * sin + y * cos)
    }

    /// How present the disc is at `phase`: from the floor to full, squared for perception.
    fn presence(&self, phase: f32) -> f32 {
        let wave = (1.0 + (TAU * (phase + self.fade)).sin()) / 2.0;
        PRESENCE_FLOOR + (1.0 - PRESENCE_FLOOR) * wave * wave
    }
}

pub fn paint(palette: &CoverPalette, base: Rgb, tone: Tone, phase: f32) -> Raster {
    let (lo, hi) = match tone {
        Tone::Dark => DARK_SHAPE_LUMINANCE,
        Tone::Light => LIGHT_SHAPE_LUMINANCE,
    };
    let mut raster = Raster::new(SIZE, SIZE, base);
    for ix in paint_order() {
        let disc = &DISCS[ix];
        let (cx, cy) = disc.position(phase);
        let color = palette.color(ix).with_luminance_in(lo, hi);
        raster.disc(cx, cy, disc.radius, color, disc.presence(phase));
    }
    if blur_enabled() {
        raster.blur(BLUR_RADIUS);
    }
    raster.map(|_, y, color| {
        let t = ((y - FADE.0) / (FADE.1 - FADE.0)).clamp(0.0, 1.0);
        color.mix(base, FADE_DEPTH * t * t * (3.0 - 2.0 * t))
    });
    raster
}

fn blur_enabled() -> bool {
    std::env::var("ECHO_BACKDROP_BLUR").map_or(true, |v| v != "0")
}

/// Disc indices largest first, so every disc paints over the bigger ones.
fn paint_order() -> [usize; 4] {
    let mut order = [0, 1, 2, 3];
    order.sort_by(|a, b| DISCS[*b].radius.total_cmp(&DISCS[*a].radius));
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blue() -> CoverPalette {
        CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230)])
    }

    fn at(raster: &Raster, (x, y): (f32, f32)) -> Rgb {
        raster.get((x * SIZE as f32) as usize, (y * SIZE as f32) as usize)
    }

    #[test]
    fn a_disc_lights_its_center_and_the_bottom_settles_toward_base() {
        let base = Rgb::from_u8(3, 5, 20);
        let phase = 0.75;
        let raster = paint(&blue(), base, Tone::Dark, phase);
        let center = at(&raster, DISCS[0].position(phase)).luminance();
        assert!(center > base.luminance() + 0.1);
        let bottom = raster.get(SIZE / 2, SIZE - 1).luminance();
        assert!(bottom < center && bottom >= base.luminance() && bottom < base.luminance() + 0.1);
    }

    #[test]
    fn a_white_cover_still_paints_readable_shapes() {
        let palette = CoverPalette::from_colors(vec![Rgb::WHITE]);
        let raster = paint(&palette, Rgb::BLACK, Tone::Dark, 0.75);
        assert!(at(&raster, DISCS[0].position(0.75)).luminance() <= DARK_SHAPE_LUMINANCE.1 + 0.01);
    }

    #[test]
    fn the_light_tone_keeps_the_lights_bright() {
        let base = Rgb::from_u8(240, 242, 250);
        let raster = paint(&blue(), base, Tone::Light, 0.75);
        let light = at(&raster, DISCS[0].position(0.75));
        assert!(light.luminance() >= LIGHT_SHAPE_LUMINANCE.0 - 0.05 && light.luminance() < base.luminance());
        assert!(light.saturation() > 0.3);
    }

    #[test]
    fn orbits_circle_the_middle() {
        for disc in &DISCS {
            let (mut min_x, mut max_x, mut min_y, mut max_y) = (1.0f32, 0.0f32, 1.0f32, 0.0f32);
            for step in 0..64 {
                let (x, y) = disc.position(step as f32 / 64.0);
                (min_x, max_x, min_y, max_y) = (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y));
            }
            assert!(min_x < MIDDLE.0 - 0.15 && max_x > MIDDLE.0 + 0.15);
            assert!(min_y < MIDDLE.1 - 0.15 && max_y > MIDDLE.1 + 0.15);
            let (x0, y0) = disc.position(0.0);
            let (x1, y1) = disc.position(1.0);
            assert!((x0 - x1).abs() < 1e-5 && (y0 - y1).abs() < 1e-5);
        }
    }

    #[test]
    fn every_disc_blooms_at_the_top_of_its_orbit() {
        for disc in &DISCS {
            let top = (0..64).map(|s| s as f32 / 64.0).min_by(|a, b| disc.position(*a).1.total_cmp(&disc.position(*b).1)).unwrap();
            assert!(disc.presence(top) > 0.9, "{top}");
        }
    }

    #[test]
    fn presence_runs_from_the_floor_to_full_on_a_square() {
        let disc = Disc { radius: 0.2, axes: (0.3, 0.2), tilt: 0.0, start: 0.0, fade: 0.0 };
        assert!((disc.presence(0.75) - PRESENCE_FLOOR).abs() < 1e-6);
        assert!((disc.presence(0.25) - 1.0).abs() < 1e-6);
        assert!((disc.presence(0.0) - (PRESENCE_FLOOR + (1.0 - PRESENCE_FLOOR) * 0.25)).abs() < 1e-6);
    }

    #[test]
    fn smaller_discs_paint_last() {
        let order = paint_order();
        assert!(order.windows(2).all(|w| DISCS[w[0]].radius >= DISCS[w[1]].radius));
        assert_eq!(order[0], 1);
    }

    #[test]
    fn the_loop_closes() {
        let base = Rgb::from_u8(3, 5, 20);
        let (start, end) = (paint(&blue(), base, Tone::Dark, 0.0), paint(&blue(), base, Tone::Dark, 1.0));
        for (x, y) in [(0, 0), (SIZE / 3, SIZE / 4), (SIZE - 1, SIZE / 2)] {
            let (a, b) = (start.get(x, y), end.get(x, y));
            assert!((a.luminance() - b.luminance()).abs() < 1e-4, "{x},{y}: {a:?} vs {b:?}");
        }
    }

    #[test]
    fn the_lights_move() {
        let base = Rgb::from_u8(3, 5, 20);
        let (start, half) = (paint(&blue(), base, Tone::Dark, 0.0), paint(&blue(), base, Tone::Dark, 0.5));
        let moved = (0..SIZE).any(|x| (start.get(x, SIZE / 4).luminance() - half.get(x, SIZE / 4).luminance()).abs() > 0.01);
        assert!(moved);
    }
}
