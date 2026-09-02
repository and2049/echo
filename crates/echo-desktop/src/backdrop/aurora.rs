//! The aurora backdrop: curtains of the cover's colors hung across the upper canvas, each a
//! band whose spine ripples with two travelling waves and whose light shimmers along its
//! length. The curtains drift left or right a whole turn per loop, so they fold past one
//! another and the loop never seams; a light blur softens the bands into light.

use std::f32::consts::TAU;

use super::palette::{CoverPalette, Rgb};
use super::raster::Raster;
use super::Tone;

const SIZE: (usize, usize) = (192, 120);
/// Every curtain leans this much: height gained from the left edge to the right.
const LEAN: f32 = 0.12;
const BLUR_RADIUS: usize = 4;
const FADE: (f32, f32) = (0.55, 0.95);
const FADE_DEPTH: f32 = 0.75;
/// Per curtain, prominent color first: the height of its spine, how far the spine ripples,
/// the ripple's cycles across the width, its travel in turns per loop (negative runs the other
/// way), half the curtain's thickness, the shimmer's cycles across the width and where the
/// ripple starts, in turns. Heights and thicknesses are fractions of the canvas.
const CURTAINS: [Curtain; 4] = [
    Curtain { height: 0.30, ripple: 0.11, waves: 1.0, travel: 1, thickness: 0.15, shimmer: 2.0, start: 0.00 },
    Curtain { height: 0.46, ripple: 0.09, waves: 1.5, travel: -1, thickness: 0.12, shimmer: 3.0, start: 0.35 },
    Curtain { height: 0.16, ripple: 0.07, waves: 2.0, travel: 1, thickness: 0.09, shimmer: 3.5, start: 0.60 },
    Curtain { height: 0.60, ripple: 0.10, waves: 1.0, travel: -2, thickness: 0.13, shimmer: 2.5, start: 0.80 },
];

struct Curtain {
    height: f32,
    ripple: f32,
    waves: f32,
    travel: i32,
    thickness: f32,
    shimmer: f32,
    start: f32,
}

impl Curtain {
    /// The height of the curtain's spine above `x` at `phase`.
    fn spine(&self, x: f32, phase: f32) -> f32 {
        let travel = self.travel as f32 * phase;
        let wave = (TAU * (x * self.waves + travel + self.start)).sin();
        let ripple = (TAU * (2.0 * x * self.waves - travel + 0.3)).sin();
        self.height + self.ripple * (wave + 0.5 * ripple) + LEAN * (x - 0.5)
    }

    /// How much of the curtain's color lands on (`x`, `y`) at `phase`.
    fn glow(&self, x: f32, y: f32, phase: f32) -> f32 {
        let d = (y - self.spine(x, phase)) / self.thickness;
        let band = (-d * d).exp();
        let shimmer = 0.7 + 0.3 * (TAU * (x * self.shimmer + 2.0 * self.travel as f32 * phase)).sin();
        band * shimmer
    }
}

pub fn paint(palette: &CoverPalette, base: Rgb, tone: Tone, phase: f32) -> Raster {
    let (lo, hi) = tone.shape_luminance();
    let colors: Vec<Rgb> = (0..CURTAINS.len()).map(|ix| palette.color(ix).with_luminance_in(lo, hi)).collect();
    let mut raster = Raster::new(SIZE.0, SIZE.1, base);
    raster.map(|x, y, color| {
        CURTAINS
            .iter()
            .zip(&colors)
            .fold(color, |color, (curtain, tint)| color.mix(*tint, curtain.glow(x, y, phase)))
    });
    raster.blur(BLUR_RADIUS);
    raster.settle(base, FADE.0, FADE.1, FADE_DEPTH);
    raster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spines_ripple_within_bounds_and_close_the_loop() {
        for curtain in &CURTAINS {
            for step in 0..16 {
                let x = step as f32 / 16.0;
                assert!((curtain.spine(x, 0.0) - curtain.spine(x, 1.0)).abs() < 1e-4);
                let y = curtain.spine(x, 0.37);
                assert!((y - curtain.height).abs() <= 1.5 * curtain.ripple + LEAN / 2.0 + 1e-5);
            }
        }
    }

    #[test]
    fn the_glow_peaks_on_the_spine_and_dies_off_it() {
        let curtain = &CURTAINS[0];
        let spine = curtain.spine(0.5, 0.2);
        let on = curtain.glow(0.5, spine, 0.2);
        assert!(on >= 0.4 && curtain.glow(0.5, spine + 3.0 * curtain.thickness, 0.2) < 0.01 * on);
    }

    #[test]
    fn curtains_light_the_upper_canvas_in_palette_colors() {
        let palette = CoverPalette::from_colors(vec![Rgb::from_u8(30, 200, 120)]);
        let base = Rgb::from_u8(3, 12, 8);
        let raster = paint(&palette, base, Tone::Dark, 0.0);
        let spine = CURTAINS[0].spine(0.5, 0.0);
        let lit = raster.get(SIZE.0 / 2, (spine * SIZE.1 as f32) as usize);
        assert!(lit.luminance() > base.luminance() + 0.08 && lit.g > lit.r);
        assert!(raster.get(SIZE.0 / 2, SIZE.1 - 1).luminance() < base.luminance() + 0.05);
    }
}
