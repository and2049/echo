//! The vinyl backdrop: the palette laid around the middle of the canvas as a conic gradient
//! that turns once per loop, like a record on its platter, over faint grooves that crawl
//! outward and a plain label at the hub. Sectors blend on a smooth ramp.

use std::f32::consts::TAU;

use super::palette::{CoverPalette, Rgb};
use super::raster::Raster;
use super::Tone;

const SIZE: (usize, usize) = (256, 160);
const ASPECT: f32 = 1.6;
/// The palette goes around the hub this many times, so its lighter tints past the end join in.
const LAPS: usize = 2;
/// Groove rings from the hub to the far corner, and how far each dips toward the base.
const GROOVES: f32 = 9.0;
const GROOVE_DEPTH: f32 = 0.3;
/// The label at the hub, as a fraction of the height, and the width of its soft rim.
const HUB: f32 = 0.10;
const HUB_RIM: f32 = 0.06;
const FADE: (f32, f32) = (0.6, 0.95);
const FADE_DEPTH: f32 = 0.6;

pub fn paint(palette: &CoverPalette, base: Rgb, tone: Tone, phase: f32) -> Raster {
    let (lo, hi) = tone.shape_luminance();
    let sectors = LAPS * palette.colors.len();
    let colors: Vec<Rgb> = (0..sectors).map(|ix| palette.color(ix).with_luminance_in(lo, hi)).collect();
    let mut raster = Raster::new(SIZE.0, SIZE.1, base);
    raster.map(|x, y, _| {
        let (dx, dy) = ((x - 0.5) * ASPECT, y - 0.5);
        let along = (dy.atan2(dx) / TAU + phase).rem_euclid(1.0) * sectors as f32;
        let ix = along as usize % sectors;
        let f = along.fract();
        let color = colors[ix].mix(colors[(ix + 1) % sectors], f * f * (3.0 - 2.0 * f));
        let r = (dx * dx + dy * dy).sqrt();
        let groove = 0.5 + 0.5 * (TAU * (r * GROOVES - phase)).cos();
        let hub = 1.0 - ((r - HUB) / HUB_RIM).clamp(0.0, 1.0);
        color.mix(base, GROOVE_DEPTH * groove).mix(base, hub)
    });
    raster.settle(base, FADE.0, FADE.1, FADE_DEPTH);
    raster
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_tone() -> CoverPalette {
        CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230), Rgb::from_u8(230, 60, 30)])
    }

    #[test]
    fn the_hub_is_plain_base() {
        let base = Rgb::from_u8(5, 5, 20);
        let raster = paint(&two_tone(), base, Tone::Dark, 0.0);
        let hub = raster.get(SIZE.0 / 2, SIZE.1 / 2);
        assert!((hub.luminance() - base.luminance()).abs() < 0.02);
    }

    #[test]
    fn sectors_turn_with_the_phase() {
        let base = Rgb::from_u8(5, 5, 20);
        let sample = |phase: f32| {
            let raster = paint(&two_tone(), base, Tone::Dark, phase);
            raster.get(SIZE.0 * 3 / 4, SIZE.1 / 4)
        };
        let (start, quarter) = (sample(0.0), sample(0.25));
        assert!((start.r - quarter.r).abs() > 0.05 || (start.b - quarter.b).abs() > 0.05);
    }
}
