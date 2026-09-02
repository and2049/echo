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
        let max = self.value();
        let min = self.r.min(self.g).min(self.b);
        if max <= f32::EPSILON { 0.0 } else { (max - min) / max }
    }

    /// HSV value: the strongest channel.
    pub fn value(self) -> f32 {
        self.r.max(self.g).max(self.b)
    }

    /// HSV hue in degrees, 0 for grays.
    pub fn hue(self) -> f32 {
        let max = self.value();
        let d = max - self.r.min(self.g).min(self.b);
        if d <= f32::EPSILON {
            return 0.0;
        }
        let sector = if max == self.r {
            (self.g - self.b) / d
        } else if max == self.g {
            (self.b - self.r) / d + 2.0
        } else {
            (self.r - self.g) / d + 4.0
        };
        (sector * 60.0).rem_euclid(360.0)
    }

    pub fn from_hsv(hue: f32, saturation: f32, value: f32) -> Rgb {
        let h = hue.rem_euclid(360.0) / 60.0;
        let c = value * saturation;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let m = value - c;
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Rgb::new(r + m, g + m, b + m)
    }

    fn scaled(self, k: f32) -> Rgb {
        Rgb::new(self.r * k, self.g * k, self.b * k)
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

    /// `self` brightened (if too dark) or darkened (if too bright) until its luminance lands
    /// inside `lo..=hi`. A dark color is scaled up first, which keeps its saturation, and only
    /// mixed toward white once a channel is full; so this is the one tool for making a cover
    /// color readable or paintable without turning it gray.
    pub fn with_luminance_in(self, lo: f32, hi: f32) -> Rgb {
        let lum = self.luminance();
        if lum < lo {
            let max = self.value();
            let lifted = if max <= f32::EPSILON { self } else { self.scaled((lo / lum).min(1.0 / max)) };
            let lum = lifted.luminance();
            let room = 1.0 - lum;
            if lum >= lo || room <= f32::EPSILON { lifted } else { lifted.mix(Rgb::WHITE, (lo - lum) / room) }
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
/// whole cover's mean, the tint that survives when nothing vivid does; `fingerprint` hashes
/// the sampled pixels, the dice a colorless cover rolls for its tint.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverPalette {
    pub colors: Vec<Rgb>,
    pub average: Rgb,
    fingerprint: u32,
}

/// How many colors extraction keeps.
const PALETTE_SIZE: usize = 4;
/// About this many pixels are sampled regardless of cover size.
const SAMPLE_BUDGET: usize = 16_384;
/// Picked colors must be at least this far apart (sRGB euclidean, 0..=√3).
const MIN_DISTANCE: f32 = 0.22;
/// Histogram levels per channel (4 bits).
const LEVELS: usize = 16;
/// Saturation below which a color reads as gray.
const GRAY: f32 = 0.12;
/// A palette whose hues all sit within this many degrees of each other is one tint.
const ONE_HUE: f32 = 30.0;
/// The tints introduced for one-tint and colorless covers sit this far around the hue circle.
const TINT_STEP: f32 = 40.0;
const TINT_SATURATION: f32 = 0.55;
const TINT_VALUE: f32 = 0.8;

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
        let mut fingerprint = Fnv::default();

        for i in (0..total).step_by(stride) {
            let px = &pixels[i * 4..i * 4 + 4];
            fingerprint.feed(px);
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

        let fingerprint = fingerprint.0;
        if sum.count == 0 {
            return Self { colors: vec![Rgb::BLACK], average: Rgb::BLACK, fingerprint };
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
        Self { colors, average, fingerprint }
    }

    /// A palette from colors already chosen by hand, for the no-cover fallback.
    pub fn from_colors(colors: Vec<Rgb>) -> Self {
        let n = colors.len().max(1) as f32;
        let average = colors
            .iter()
            .fold(Rgb::BLACK, |acc, c| Rgb::new(acc.r + c.r / n, acc.g + c.g / n, acc.b + c.b / n));
        let mut fingerprint = Fnv::default();
        for c in &colors {
            fingerprint.feed(&[c.r, c.g, c.b].map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8));
        }
        let colors = if colors.is_empty() { vec![Rgb::BLACK] } else { colors };
        Self { colors, average, fingerprint: fingerprint.0 }
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

    /// A cover of one hue, or of none, paints a dull wash. This widens such palettes with
    /// tints: neighbours of the cover's own hue when it has one, and, in place of the grays
    /// when it has none, of a hue rolled from its fingerprint, so grayscale covers differ from
    /// each other yet each keeps its look. The tints are at least moderately saturated, so a
    /// pastel cover still gets some life.
    pub fn with_variety(mut self) -> Self {
        let colored: Vec<Rgb> = self.colors.iter().copied().filter(|c| c.saturation() >= GRAY).collect();
        match colored.first() {
            None => {
                let hue = (self.fingerprint % 360) as f32;
                self.colors = [0.0, TINT_STEP, -TINT_STEP]
                    .map(|offset| Rgb::from_hsv(hue + offset, TINT_SATURATION, TINT_VALUE))
                    .to_vec();
            }
            Some(lead) if colored.iter().all(|c| hue_distance(c.hue(), lead.hue()) <= ONE_HUE) => {
                let saturation = lead.saturation().max(TINT_SATURATION);
                let tint = |offset| Rgb::from_hsv(lead.hue() + offset, saturation, lead.value());
                self.colors.insert(1, tint(TINT_STEP));
                self.colors.insert(2, tint(-TINT_STEP));
            }
            _ => {}
        }
        self
    }
}

/// Degrees between two hues the short way round.
fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// FNV-1a, 32-bit: a few multiplies per pixel, enough to tell covers apart.
struct Fnv(u32);

impl Default for Fnv {
    fn default() -> Self {
        Self(0x811c9dc5)
    }
}

impl Fnv {
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u32::from(b)).wrapping_mul(0x01000193);
        }
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
    fn lifting_a_dark_color_scales_before_it_whitens() {
        let deep = Rgb::from_u8(25, 0, 75);
        let lifted = deep.with_luminance_in(0.1, 0.2);
        assert!((0.099..=0.201).contains(&lifted.luminance()));
        assert!(lifted.saturation() > 0.95);
        assert!((lifted.hue() - deep.hue()).abs() < 1.0);
        let pale = deep.with_luminance_in(0.6, 0.7);
        assert!((0.599..=0.701).contains(&pale.luminance()));
        assert!(pale.saturation() < lifted.saturation());
    }

    #[test]
    fn hsv_round_trips() {
        for (r, g, b) in [(200, 40, 40), (30, 60, 230), (10, 200, 90), (250, 250, 250)] {
            let color = Rgb::from_u8(r, g, b);
            let back = Rgb::from_hsv(color.hue(), color.saturation(), color.value());
            assert!(close(color, back), "{color:?} -> {back:?}");
        }
        assert!(Rgb::from_hsv(360.0, 1.0, 1.0).hue().abs() < 0.01);
        assert_eq!(hue_distance(350.0, 10.0), 20.0);
    }

    #[test]
    fn one_hue_palette_gains_neighbours() {
        let red = Rgb::from_u8(200, 40, 40);
        let palette = CoverPalette::from_colors(vec![red, Rgb::from_u8(120, 30, 30), Rgb::from_u8(40, 40, 40)])
            .with_variety();
        assert_eq!(palette.colors.len(), 5);
        assert_eq!(palette.primary(), red);
        assert!((hue_distance(palette.colors[1].hue(), red.hue()) - TINT_STEP).abs() < 0.5);
        assert!((hue_distance(palette.colors[2].hue(), red.hue()) - TINT_STEP).abs() < 0.5);
        let two = CoverPalette::from_colors(vec![red, Rgb::from_u8(30, 60, 230)]).with_variety();
        assert_eq!(two.colors.len(), 2);
        let pastel = CoverPalette::from_colors(vec![Rgb::from_u8(183, 178, 217)]).with_variety();
        assert_eq!(pastel.colors.len(), 3);
        assert!(pastel.colors[1..].iter().all(|c| c.saturation() >= TINT_SATURATION - 0.01));
    }

    #[test]
    fn colorless_palette_rolls_a_hue_from_its_pixels() {
        let gray = |shade: u8| {
            let pixels = image(8, 8, |x, _| if x < 4 { [shade, shade, shade, 255] } else { [240, 240, 240, 255] });
            CoverPalette::from_pixels(8, 8, &pixels)
        };
        let (a, b) = (gray(20).with_variety(), gray(60).with_variety());
        for palette in [&a, &b] {
            assert_eq!(palette.colors.len(), 3);
            assert!(palette.colors.iter().all(|c| c.saturation() >= GRAY));
        }
        assert!(hue_distance(a.primary().hue(), b.primary().hue()) > 1.0);
        assert_eq!(gray(20).with_variety().primary(), a.primary());
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
