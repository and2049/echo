//! The cover palette: the handful of colors a backdrop mode paints with, pulled from the
//! cover's pixels. Pure pixel math with no gpui types, so it is unit-testable on its own.
//!
//! Extraction is a coarse histogram (16 levels per channel), scored by population times
//! vividness so a saturated stripe beats a large gray field, then greedily picked for
//! distinctness so four shades of the same blue do not fill every slot.

/// A color in 0..=1 sRGB components; the arithmetic here is deliberately in sRGB because gpui
/// blends there too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0.0, g: 0.0, b: 0.0 };
    pub const WHITE: Rgb = Rgb { r: 1.0, g: 1.0, b: 1.0 };

    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn from_u8(r: u8, g: u8, b: u8) -> Self {
        Self::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    }

    /// Rec. 709 luma, a good enough stand-in for perceived brightness.
    pub fn luminance(self) -> f32 {
        0.2126 * self.r + 0.7152 * self.g + 0.0722 * self.b
    }

    /// HSV saturation: 0 for grays, 1 for pure hues.
    pub fn saturation(self) -> f32 {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        if max <= f32::EPSILON { 0.0 } else { (max - min) / max }
    }

    /// `t` of the way from `self` to `other`.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb::new(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
        )
    }

    /// `self` moved toward white (if too dark) or black (if too bright) until its luminance
    /// lands inside `lo..=hi`. Mixing keeps the hue, so this is the one tool for making a
    /// cover color readable or paintable.
    pub fn with_luminance_in(self, lo: f32, hi: f32) -> Rgb {
        let lum = self.luminance();
        if lum < lo {
            let room = 1.0 - lum;
            if room <= f32::EPSILON { self } else { self.mix(Rgb::WHITE, (lo - lum) / room) }
        } else if lum > hi {
            if lum <= f32::EPSILON { self } else { self.mix(Rgb::BLACK, (lum - hi) / lum) }
        } else {
            self
        }
    }

    fn distance(self, other: Rgb) -> f32 {
        let (dr, dg, db) = (self.r - other.r, self.g - other.g, self.b - other.b);
        (dr * dr + dg * dg + db * db).sqrt()
    }
}

/// The colors a backdrop paints with, most prominent first, never empty. `average` is the
/// whole cover's mean, the tint that survives when nothing vivid does.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverPalette {
    pub colors: Vec<Rgb>,
    pub average: Rgb,
}

/// How many colors extraction keeps.
const PALETTE_SIZE: usize = 4;
/// About this many pixels are sampled regardless of cover size.
const SAMPLE_BUDGET: usize = 16_384;
/// Picked colors must be at least this far apart (sRGB euclidean, 0..=√3).
const MIN_DISTANCE: f32 = 0.22;
/// Histogram levels per channel (4 bits).
const LEVELS: usize = 16;

#[derive(Clone, Copy, Default)]
struct Bin {
    count: u32,
    r: f32,
    g: f32,
    b: f32,
}

impl CoverPalette {
    /// The palette of `width` x `height` RGBA `pixels` (row-major, 4 bytes each). Transparent
    /// pixels are ignored; a fully transparent or empty image yields a single black.
    pub fn from_pixels(width: u32, height: u32, pixels: &[u8]) -> Self {
        let total = (width as usize * height as usize).min(pixels.len() / 4);
        let stride = (total / SAMPLE_BUDGET).max(1);
        let mut bins = vec![Bin::default(); LEVELS * LEVELS * LEVELS];
        let mut sum = Bin::default();

        for i in (0..total).step_by(stride) {
            let px = &pixels[i * 4..i * 4 + 4];
            if px[3] < 128 {
                continue;
            }
            let color = Rgb::from_u8(px[0], px[1], px[2]);
            let key = (px[0] as usize >> 4) * LEVELS * LEVELS
                + (px[1] as usize >> 4) * LEVELS
                + (px[2] as usize >> 4);
            for bin in [&mut bins[key], &mut sum] {
                bin.count += 1;
                bin.r += color.r;
                bin.g += color.g;
                bin.b += color.b;
            }
        }

        if sum.count == 0 {
            return Self { colors: vec![Rgb::BLACK], average: Rgb::BLACK };
        }
        let mean = |bin: &Bin| {
            let n = bin.count as f32;
            Rgb::new(bin.r / n, bin.g / n, bin.b / n)
        };
        let average = mean(&sum);

        let mut candidates: Vec<(f32, Rgb)> = bins
            .iter()
            .filter(|bin| bin.count > 0)
            .map(|bin| {
                let color = mean(bin);
                (bin.count as f32 * vividness(color), color)
            })
            .collect();
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));

        let mut colors: Vec<Rgb> = Vec::with_capacity(PALETTE_SIZE);
        for (_, color) in candidates {
            if colors.len() == PALETTE_SIZE {
                break;
            }
            if colors.iter().all(|picked| picked.distance(color) >= MIN_DISTANCE) {
                colors.push(color);
            }
        }
        if colors.is_empty() {
            colors.push(average);
        }
        Self { colors, average }
    }

    /// A palette from colors already chosen by hand, for the no-cover fallback.
    pub fn from_colors(colors: Vec<Rgb>) -> Self {
        let n = colors.len().max(1) as f32;
        let average = colors
            .iter()
            .fold(Rgb::BLACK, |acc, c| Rgb::new(acc.r + c.r / n, acc.g + c.g / n, acc.b + c.b / n));
        let colors = if colors.is_empty() { vec![Rgb::BLACK] } else { colors };
        Self { colors, average }
    }

    /// The most prominent color.
    pub fn primary(&self) -> Rgb {
        self.colors[0]
    }

    /// The `ix`th color, cycling with progressively lighter tints past the end so modes that
    /// want more shapes than the cover has colors still get distinct ones.
    pub fn color(&self, ix: usize) -> Rgb {
        let n = self.colors.len();
        let lap = (ix / n) as f32;
        self.colors[ix % n].mix(Rgb::WHITE, (0.25 * lap).min(0.75))
    }
}

/// Population weight: saturated mid-tones count most, near-black and near-white least, since
/// the cover's border and its background are seldom the color anyone remembers it by.
fn vividness(color: Rgb) -> f32 {
    let lum = color.luminance();
    let extremes = if !(0.08..=0.92).contains(&lum) { 0.25 } else { 1.0 };
    (0.15 + color.saturation()) * extremes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let paint = &paint;
        (0..height)
            .flat_map(|y| (0..width).flat_map(move |x| paint(x, y)))
            .collect()
    }

    fn close(a: Rgb, b: Rgb) -> bool {
        a.distance(b) < 0.08
    }

    #[test]
    fn solid_cover_yields_its_color() {
        let pixels = image(8, 8, |_, _| [200, 40, 40, 255]);
        let palette = CoverPalette::from_pixels(8, 8, &pixels);
        assert_eq!(palette.colors.len(), 1);
        assert!(close(palette.primary(), Rgb::from_u8(200, 40, 40)));
        assert!(close(palette.average, Rgb::from_u8(200, 40, 40)));
    }

    #[test]
    fn vivid_stripe_beats_a_larger_gray_field() {
        let pixels = image(40, 10, |x, _| if x < 30 { [128, 128, 128, 255] } else { [30, 60, 230, 255] });
        let palette = CoverPalette::from_pixels(40, 10, &pixels);
        assert!(close(palette.primary(), Rgb::from_u8(30, 60, 230)));
        assert!(palette.colors.iter().any(|c| close(*c, Rgb::from_u8(128, 128, 128))));
    }

    #[test]
    fn similar_shades_collapse_into_one_slot() {
        let pixels = image(40, 10, |x, _| if x % 2 == 0 { [30, 60, 230, 255] } else { [36, 66, 236, 255] });
        let palette = CoverPalette::from_pixels(40, 10, &pixels);
        assert_eq!(palette.colors.len(), 1);
    }

    #[test]
    fn transparent_pixels_are_ignored() {
        let pixels = image(8, 8, |x, _| if x < 4 { [255, 0, 0, 0] } else { [0, 200, 0, 255] });
        let palette = CoverPalette::from_pixels(8, 8, &pixels);
        assert!(close(palette.primary(), Rgb::from_u8(0, 200, 0)));
        let empty = CoverPalette::from_pixels(8, 8, &image(8, 8, |_, _| [255, 0, 0, 0]));
        assert_eq!(empty.colors, vec![Rgb::BLACK]);
        assert_eq!(CoverPalette::from_pixels(0, 0, &[]).colors, vec![Rgb::BLACK]);
    }

    #[test]
    fn luminance_clamp_keeps_hue_and_lands_in_range() {
        let dark = Rgb::from_u8(20, 0, 60).with_luminance_in(0.5, 0.6);
        assert!((0.499..=0.601).contains(&dark.luminance()));
        assert!(dark.b > dark.r && dark.r > dark.g);
        let bright = Rgb::from_u8(250, 250, 200).with_luminance_in(0.1, 0.2);
        assert!((0.099..=0.201).contains(&bright.luminance()));
        let fine = Rgb::from_u8(100, 100, 100);
        assert_eq!(fine.with_luminance_in(0.1, 0.9), fine);
    }

    #[test]
    fn color_cycles_with_lighter_tints() {
        let palette = CoverPalette::from_colors(vec![Rgb::from_u8(0, 0, 200)]);
        assert_eq!(palette.color(0), palette.primary());
        assert!(palette.color(1).luminance() > palette.color(0).luminance());
        assert!(palette.color(2).luminance() > palette.color(1).luminance());
        assert_eq!(CoverPalette::from_colors(vec![]).colors, vec![Rgb::BLACK]);
    }
}
