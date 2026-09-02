//! The nebula backdrop: a ray march through a field folded by a stack of sines, after a
//! shader-golf classic, brought from the fragment shader to the keyframe canvas. Each pixel
//! casts a ray from the middle of the canvas and, step by step, the point on it is warped by
//! octaves of travelling sines (`march.rs`), which decide how much glow the step adds and
//! where along the palette its hue leans. The glow, tone-mapped by tanh, lifts the pixel from
//! the base toward the palette color its hue picks, and a light blur takes the edge off where
//! the hue turns fast. The sines run in whole turns per
//! loop, so the field at phase 1 is the field at phase 0.

mod march;

use std::f32::consts::TAU;

use super::palette::{CoverPalette, Rgb};
use super::raster::Raster;
use super::Tone;

const SIZE: (usize, usize) = (80, 50);
const ASPECT: f32 = 1.6;
/// Raw glow below the floor is dark; the floor plus the exposure maps to about three
/// quarters of full. The field gathers between about 1 and 6, its median drifting from 1.3 to
/// 3 around the loop, so the nebula breathes as well as drifts.
const GLOW_FLOOR: f32 = 1.0;
const EXPOSURE: f32 = 2.5;
const BLUR_RADIUS: usize = 2;
const FADE: (f32, f32) = (0.55, 0.95);
const FADE_DEPTH: f32 = 0.7;

pub fn paint(palette: &CoverPalette, base: Rgb, tone: Tone, phase: f32) -> Raster {
    let (lo, hi) = tone.shape_luminance();
    let n = palette.colors.len();
    let colors: Vec<Rgb> = (0..n).map(|ix| palette.color(ix).with_luminance_in(lo, hi)).collect();
    let mut raster = Raster::new(SIZE.0, SIZE.1, base);
    raster.map(|x, y, _| {
        let sample = march::march((x - 0.5) * ASPECT, y - 0.5, TAU * phase);
        let along = sample.hue.clamp(0.0, 1.0) * (n - 1) as f32;
        let ix = (along as usize).min(n - 1);
        let f = along - ix as f32;
        let color = colors[ix].mix(colors[(ix + 1).min(n - 1)], f * f * (3.0 - 2.0 * f));
        base.mix(color, ((sample.glow - GLOW_FLOOR) / EXPOSURE).max(0.0).tanh())
    });
    raster.blur(BLUR_RADIUS);
    raster.settle(base, FADE.0, FADE.1, FADE_DEPTH);
    raster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_glows_in_palette_colors_and_varies_across_the_canvas() {
        let palette = CoverPalette::from_colors(vec![Rgb::from_u8(200, 40, 160)]);
        let base = Rgb::from_u8(20, 4, 16);
        let raster = paint(&palette, base, Tone::Dark, 0.0);
        let top: Vec<Rgb> = (0..SIZE.0).map(|x| raster.get(x, SIZE.1 / 4)).collect();
        let (min, max) = top.iter().fold((1.0f32, 0.0f32), |(lo, hi), c| (lo.min(c.luminance()), hi.max(c.luminance())));
        assert!(max > base.luminance() + 0.08 && max - min > 0.05);
        assert!(top.iter().all(|c| c.r >= c.g && c.b >= c.g));
    }
}
