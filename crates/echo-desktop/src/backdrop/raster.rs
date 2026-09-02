//! A tiny CPU canvas for backdrop modes: discs and a blur over a few thousand pixels, uploaded
//! once and stretched across the window by the GPU. Painting this small is what makes the blur
//! cheap: a 96-pixel canvas blurred by 12 pixels reads as a 1400-pixel window blurred by 175,
//! and the texture's bilinear upscale hides the coarse grid. Coordinates handed to the painting
//! calls are fractions of the canvas, so a mode's layout is independent of the size it picks.
//! Run with `ECHO_BACKDROP_BLUR=0` to skip every blur and see a mode's raw shapes when tuning
//! its motion.

use std::sync::Arc;

use gpui::RenderImage;

use super::palette::Rgb;

pub struct Raster {
    width: usize,
    height: usize,
    pixels: Vec<Rgb>,
}

impl Raster {
    pub fn new(width: usize, height: usize, fill: Rgb) -> Self {
        Self {
            width,
            height,
            pixels: vec![fill; width * height],
        }
    }

    #[cfg(test)]
    pub fn get(&self, x: usize, y: usize) -> Rgb {
        self.pixels[y * self.width + x]
    }

    #[cfg(test)]
    pub fn pixels(&self) -> &[Rgb] {
        &self.pixels
    }

    /// A filled disc centered at (`cx`, `cy`) — fractions of the width and height — with radius
    /// `r` as a fraction of the width, its edge softened over one pixel, blended in at `opacity`.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, color: Rgb, opacity: f32) {
        let (cx, cy, r) = (cx * self.width as f32, cy * self.height as f32, r * self.width as f32);
        for y in 0..self.height {
            for x in 0..self.width {
                let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let coverage = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let ix = y * self.width + x;
                    self.pixels[ix] = self.pixels[ix].mix(color, coverage * opacity);
                }
            }
        }
    }

    /// Three passes of a box blur `radius` pixels each side, which is a close Gaussian. Edges
    /// clamp, so the canvas never darkens toward its border.
    pub fn blur(&mut self, radius: usize) {
        if radius == 0 || std::env::var("ECHO_BACKDROP_BLUR").is_ok_and(|v| v == "0") {
            return;
        }
        for _ in 0..3 {
            self.box_pass(radius, true);
            self.box_pass(radius, false);
        }
    }

    fn box_pass(&mut self, radius: usize, horizontal: bool) {
        let (lines, len) = if horizontal { (self.height, self.width) } else { (self.width, self.height) };
        let index = |line: usize, i: usize| if horizontal { line * self.width + i } else { i * self.width + line };
        let window = (2 * radius + 1) as f32;
        let mut out = vec![Rgb::BLACK; len];
        for line in 0..lines {
            let at = |i: isize| self.pixels[index(line, i.clamp(0, len as isize - 1) as usize)];
            let (mut r, mut g, mut b) = (0.0, 0.0, 0.0);
            for i in -(radius as isize)..=radius as isize {
                let c = at(i);
                r += c.r;
                g += c.g;
                b += c.b;
            }
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = Rgb::new(r / window, g / window, b / window);
                let (leaving, entering) = (at(i as isize - radius as isize), at(i as isize + radius as isize + 1));
                r += entering.r - leaving.r;
                g += entering.g - leaving.g;
                b += entering.b - leaving.b;
            }
            for (i, color) in out.iter().enumerate() {
                self.pixels[index(line, i)] = *color;
            }
        }
    }

    /// Rewrites every pixel from its fractional position and current color.
    pub fn map(&mut self, f: impl Fn(f32, f32, Rgb) -> Rgb) {
        let (w, h) = (self.width as f32, self.height as f32);
        for y in 0..self.height {
            for x in 0..self.width {
                let ix = y * self.width + x;
                self.pixels[ix] = f((x as f32 + 0.5) / w, (y as f32 + 0.5) / h, self.pixels[ix]);
            }
        }
    }

    /// Fades the picture toward `base` down the canvas: untouched above `from` (a fraction of
    /// the height), `depth` of the way to `base` from `to` down, on a smooth ramp between. The
    /// lyrics sit low, so every mode settles its bottom this way to keep them readable.
    pub fn settle(&mut self, base: Rgb, from: f32, to: f32, depth: f32) {
        self.map(|_, y, color| {
            let t = ((y - from) / (to - from)).clamp(0.0, 1.0);
            color.mix(base, depth * t * t * (3.0 - 2.0 * t))
        });
    }

    /// The canvas as a texture gpui can draw, in its BGRA byte order, with `guard` texels of
    /// replicated edge on every side. The texture lives in gpui's sprite atlas beside other
    /// tiles, and bilinear sampling at its edge bleeds into them; stretched across a window
    /// that is a dark trim several pixels wide. Painting only the inner region keeps the edge
    /// samples inside the guard.
    pub fn into_render_image(self, guard: usize) -> Arc<RenderImage> {
        let (width, height) = (self.width + 2 * guard, self.height + 2 * guard);
        let bytes: Vec<u8> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| {
                let sample = |v: usize, len: usize| v.saturating_sub(guard).min(len - 1);
                let c = self.pixels[sample(y, self.height) * self.width + sample(x, self.width)];
                [c.b, c.g, c.r, 1.0].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            })
            .collect();
        let buffer = image::RgbaImage::from_raw(width as u32, height as u32, bytes)
            .expect("pixel buffer matches its dimensions");
        Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean(raster: &Raster) -> f32 {
        let n = (raster.width * raster.height) as f32;
        raster.pixels.iter().map(|c| c.luminance()).sum::<f32>() / n
    }

    #[test]
    fn disc_paints_inside_and_leaves_outside() {
        let mut raster = Raster::new(20, 20, Rgb::BLACK);
        raster.disc(0.5, 0.5, 0.25, Rgb::WHITE, 1.0);
        assert_eq!(raster.get(10, 10), Rgb::WHITE);
        assert_eq!(raster.get(0, 0), Rgb::BLACK);
        assert_eq!(raster.get(19, 10), Rgb::BLACK);
    }

    #[test]
    fn disc_opacity_scales_the_blend() {
        let mut raster = Raster::new(20, 20, Rgb::BLACK);
        raster.disc(0.5, 0.5, 0.25, Rgb::WHITE, 0.5);
        assert_eq!(raster.get(10, 10), Rgb::new(0.5, 0.5, 0.5));
        raster.disc(0.5, 0.5, 0.25, Rgb::BLACK, 0.0);
        assert_eq!(raster.get(10, 10), Rgb::new(0.5, 0.5, 0.5));
    }

    #[test]
    fn blur_keeps_the_mean_and_flattens() {
        let mut raster = Raster::new(24, 24, Rgb::BLACK);
        raster.disc(0.5, 0.5, 0.2, Rgb::WHITE, 1.0);
        let before = mean(&raster);
        let peak_before = raster.get(12, 12).luminance();
        raster.blur(3);
        assert!((mean(&raster) - before).abs() < 0.02);
        assert!(raster.get(12, 12).luminance() < peak_before);
        assert!(raster.get(0, 0).luminance() < 0.05);
        assert!(raster.get(6, 12).luminance() > 0.05);
    }

    #[test]
    fn map_sees_fractional_positions() {
        let mut raster = Raster::new(4, 2, Rgb::BLACK);
        raster.map(|x, y, _| Rgb::new(x, y, 0.0));
        assert_eq!(raster.get(0, 0), Rgb::new(0.125, 0.25, 0.0));
        assert_eq!(raster.get(3, 1), Rgb::new(0.875, 0.75, 0.0));
    }

    #[test]
    fn settle_fades_only_the_bottom_and_stops_short_of_base() {
        let mut raster = Raster::new(1, 10, Rgb::WHITE);
        raster.settle(Rgb::BLACK, 0.5, 0.9, 0.8);
        assert_eq!(raster.get(0, 0), Rgb::WHITE);
        assert_eq!(raster.get(0, 4), Rgb::WHITE);
        assert!(raster.get(0, 6).luminance() < 1.0 && raster.get(0, 6).luminance() > 0.2);
        assert!((raster.get(0, 9).luminance() - 0.2).abs() < 1e-5);
    }

    #[test]
    fn render_image_is_bgra_of_the_canvas() {
        let raster = Raster::new(1, 1, Rgb::new(1.0, 0.5, 0.0));
        let image = raster.into_render_image(0);
        assert_eq!(image.size(0), gpui::size(gpui::DevicePixels(1), gpui::DevicePixels(1)));
        assert_eq!(image.as_bytes(0), Some(&[0u8, 128, 255, 255][..]));
    }

    #[test]
    fn guard_replicates_the_edges() {
        let mut raster = Raster::new(2, 1, Rgb::BLACK);
        raster.map(|x, _, _| if x < 0.5 { Rgb::new(1.0, 0.0, 0.0) } else { Rgb::new(0.0, 0.0, 1.0) });
        let image = raster.into_render_image(1);
        assert_eq!(image.size(0), gpui::size(gpui::DevicePixels(4), gpui::DevicePixels(3)));
        let bytes = image.as_bytes(0).unwrap();
        let texel = |x: usize, y: usize| &bytes[(y * 4 + x) * 4..(y * 4 + x) * 4 + 3];
        assert_eq!(texel(0, 0), [0, 0, 255]);
        assert_eq!(texel(1, 1), [0, 0, 255]);
        assert_eq!(texel(2, 1), [255, 0, 0]);
        assert_eq!(texel(3, 2), [255, 0, 0]);
    }
}
