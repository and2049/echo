//! The blurred-shapes backdrop: a few large discs in the cover's colors over a near-black tint
//! of its primary, blurred until they read as clouds of color, and fading back to the base
//! toward the bottom. The "shapes, then blur" recipe behind Apple Music's now-playing wash.
//! Disc luminance is clamped so light text stays readable wherever the lyrics land on it.

use super::palette::{CoverPalette, Rgb};
use super::raster::Raster;

const SIZE: usize = 96;
const BLUR_RADIUS: usize = 11;
/// Disc centers and radii as fractions of the canvas, prominent color first.
const SHAPES: [(f32, f32, f32); 4] = [
    (0.18, 0.26, 0.42),
    (0.74, 0.20, 0.44),
    (0.50, 0.58, 0.30),
    (0.98, 0.56, 0.30),
];
const SHAPE_LUMINANCE: (f32, f32) = (0.18, 0.42);
/// The fade to the base runs between these fractions of the height.
const FADE: (f32, f32) = (0.45, 0.92);

pub fn paint(palette: &CoverPalette, base: Rgb) -> Raster {
    let mut raster = Raster::new(SIZE, SIZE, base);
    for (ix, (cx, cy, r)) in SHAPES.iter().enumerate() {
        let color = palette.color(ix).with_luminance_in(SHAPE_LUMINANCE.0, SHAPE_LUMINANCE.1);
        raster.disc(*cx, *cy, *r, color);
    }
    raster.blur(BLUR_RADIUS);
    raster.map(|_, y, color| {
        let t = ((y - FADE.0) / (FADE.1 - FADE.0)).clamp(0.0, 1.0);
        color.mix(base, t * t * (3.0 - 2.0 * t))
    });
    raster
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_light_the_top_and_the_bottom_stays_base() {
        let palette = CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230)]);
        let base = Rgb::from_u8(3, 5, 20);
        let raster = paint(&palette, base);
        assert!(raster.get(SIZE / 4, SIZE / 4).luminance() > base.luminance() + 0.1);
        let bottom = raster.get(SIZE / 2, SIZE - 1);
        assert!((bottom.luminance() - base.luminance()).abs() < 0.01);
    }

    #[test]
    fn a_white_cover_still_paints_readable_shapes() {
        let palette = CoverPalette::from_colors(vec![Rgb::WHITE]);
        let raster = paint(&palette, Rgb::BLACK);
        assert!(raster.get(SIZE / 4, SIZE / 4).luminance() <= SHAPE_LUMINANCE.1 + 0.01);
    }
}

