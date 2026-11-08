//! The echo app icon: a ribbon sound wave with two fainter, out-of-step echoes trailing it, on
//! a rounded tile.
//!
//! Everything is derived from three theme colors, so any theme (bundled or user-made) gets an
//! icon without a hand-drawn asset: the wave blends `primary` into `secondary` and the tile is
//! built from `background`. Light backgrounds flip to a pale tile with a slightly darker wave,
//! since a pale wave on a pale tile would vanish.
//!
//! Geometry follows the macOS icon grid: a 1024 canvas with an 824 rounded tile centred in it
//! and the drop shadow in the margin. macOS 26 shows icons that skip that margin on a grey
//! tile instead.

use std::fmt::Write as _;

/// An sRGB color, 0–255 per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#rrggbb` (case-insensitive, `#` optional).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self(channel(0)?, channel(2)?, channel(4)?))
    }

    fn channels(self) -> [f64; 3] {
        [self.0, self.1, self.2].map(|c| f64::from(c) / 255.0)
    }

    fn from_channels(c: [f64; 3]) -> Self {
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Self(byte(c[0]), byte(c[1]), byte(c[2]))
    }

    /// Linear interpolation toward `other` by `t` (0 = self, 1 = other).
    fn mix(self, other: Self, t: f64) -> Self {
        let (a, b) = (self.channels(), other.channels());
        Self::from_channels([0, 1, 2].map(|i| a[i] * (1.0 - t) + b[i] * t))
    }

    /// Relative luminance with gamma-encoded channels: only used to tell light tiles from dark.
    fn luminance(self) -> f64 {
        let [r, g, b] = self.channels();
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

const BLACK: Rgb = Rgb(0, 0, 0);
const WHITE: Rgb = Rgb(255, 255, 255);

/// The three theme colors the icon is drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IconColors {
    pub primary: Rgb,
    pub secondary: Rgb,
    pub background: Rgb,
}

/// The tile: an 824 square inset 100 from each edge with 185 corners, roughly Apple's
/// continuous-corner template.
const TILE: &str = "M285,100 H739 C841,100 924,183 924,285 V739 C924,841 841,924 739,924 \
H285 C183,924 100,841 100,739 V285 C100,183 183,100 285,100 Z";

/// One ribbon: a band around a sine centreline whose amplitude and thickness swell in the
/// middle and taper to points at both ends. The band is sheared against the slope so it reads
/// as a twisting ribbon rather than a constant-width stroke.
fn ribbon(amplitude: f64, phase: f64, thickness: f64) -> String {
    const X0: f64 = 150.0;
    const X1: f64 = 874.0;
    const CYCLES: f64 = 2.0;
    const STEPS: usize = 160;
    let tau = std::f64::consts::TAU;
    let pi = std::f64::consts::PI;

    let mut top = Vec::with_capacity(STEPS + 1);
    let mut bottom = Vec::with_capacity(STEPS + 1);
    for i in 0..=STEPS {
        let u = i as f64 / STEPS as f64;
        let x = X0 + (X1 - X0) * u;
        let swell = (pi * u).sin();
        let envelope = swell.powf(1.6) * (0.55 + 0.45 * (pi * u + 0.3).sin());
        let angle = tau * CYCLES * u + phase;
        let y = 512.0 - amplitude * envelope * angle.sin();
        let t = 4.0 + thickness * swell.powf(2.2);
        let shear = 0.25 * t * angle.cos();
        top.push((x, y - t / 2.0 - shear));
        bottom.push((x, y + t / 2.0 - shear));
    }

    let mut d = String::with_capacity((STEPS + 1) * 2 * 14);
    for (i, (x, y)) in top.iter().chain(bottom.iter().rev()).enumerate() {
        let _ = write!(d, "{}{x:.1},{y:.1} ", if i == 0 { "M" } else { "L" });
    }
    d.push('Z');
    d
}

/// The icon as a standalone 1024×1024 SVG document.
pub fn svg(colors: IconColors) -> String {
    let IconColors {
        primary,
        secondary,
        background,
    } = colors;
    let light = background.luminance() > 0.5;

    let (tile_top, tile_bottom, wave_start, wave_end, glow_opacity, blend) = if light {
        (
            background.mix(WHITE, 0.5),
            background.mix(primary, 0.05),
            primary.mix(BLACK, 0.15),
            secondary.mix(BLACK, 0.1),
            0.25,
            "multiply",
        )
    } else {
        (
            background.mix(primary, 0.35),
            background.mix(BLACK, 0.35),
            primary,
            secondary,
            0.35,
            "screen",
        )
    };
    let wave_mid = wave_start.mix(wave_end, 0.5);
    let (tile_top, tile_bottom, glow) = (tile_top.hex(), tile_bottom.hex(), secondary.hex());
    let (wave_start, wave_mid, wave_end) = (wave_start.hex(), wave_mid.hex(), wave_end.hex());

    // Back to front: the faintest echo first so the main wave sits on top.
    // (amplitude, phase, thickness, opacity)
    const RIBBONS: [(f64, f64, f64, f64); 3] = [
        (120.0, 1.7, 52.0, 0.3),
        (160.0, 0.85, 80.0, 0.55),
        (200.0, 0.0, 120.0, 0.92),
    ];
    let mut ribbons = String::new();
    for (amplitude, phase, thickness, opacity) in RIBBONS {
        let _ = write!(
            ribbons,
            r#"<path d="{}" fill="url(#wave)" opacity="{opacity}" style="mix-blend-mode:{blend}"/>"#,
            ribbon(amplitude, phase, thickness)
        );
    }
    let halo = ribbon(200.0, 0.0, 120.0);

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
<defs>
<linearGradient id="tile" x1="0" y1="0" x2=".6" y2="1"><stop offset="0" stop-color="{tile_top}"/><stop offset="1" stop-color="{tile_bottom}"/></linearGradient>
<radialGradient id="glow" cx=".62" cy=".55" r=".55"><stop offset="0" stop-color="{glow}" stop-opacity="{glow_opacity}"/><stop offset="1" stop-color="{glow}" stop-opacity="0"/></radialGradient>
<linearGradient id="wave" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{wave_start}"/><stop offset=".55" stop-color="{wave_mid}"/><stop offset="1" stop-color="{wave_end}"/></linearGradient>
<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".28"/><stop offset=".45" stop-color="#fff" stop-opacity="0"/></linearGradient>
<linearGradient id="rim" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#fff" stop-opacity=".7"/><stop offset=".5" stop-color="#fff" stop-opacity=".08"/><stop offset="1" stop-color="#fff" stop-opacity=".35"/></linearGradient>
<clipPath id="clip"><path d="{TILE}"/></clipPath>
<filter id="shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="12" stdDeviation="14" flood-opacity=".3"/></filter>
<filter id="blur"><feGaussianBlur stdDeviation="18"/></filter>
</defs>
<path d="{TILE}" fill="url(#tile)" filter="url(#shadow)"/>
<g clip-path="url(#clip)">
<rect width="1024" height="1024" fill="url(#glow)"/>
<path d="{halo}" fill="{wave_start}" opacity=".45" filter="url(#blur)"/>
{ribbons}
<path d="{TILE}" fill="url(#sheen)"/>
</g>
<path d="{TILE}" fill="none" stroke="url(#rim)" stroke-width="5"/>
</svg>"##
    )
}

/// Renders the icon to a `size`×`size` PNG.
pub fn render_png(colors: IconColors, size: u32) -> Result<Vec<u8>, String> {
    let tree = resvg::usvg::Tree::from_str(&svg(colors), &resvg::usvg::Options::default())
        .map_err(|e| format!("icon SVG did not parse: {e}"))?;
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size, size).ok_or("icon size must be non-zero")?;
    let scale = size as f32 / 1024.0;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|e| format!("icon PNG did not encode: {e}"))
}

/// Packs PNGs into an `.icns`. Each entry is `(OSType, png)`; every modern type code accepts
/// PNG data, which is what `iconutil` writes too.
pub fn icns(entries: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
    let total: usize = 8 + entries.iter().map(|(_, png)| 8 + png.len()).sum::<usize>();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"icns");
    out.extend_from_slice(&(total as u32).to_be_bytes());
    for (kind, png) in entries {
        out.extend_from_slice(*kind);
        out.extend_from_slice(&((8 + png.len()) as u32).to_be_bytes());
        out.extend_from_slice(png);
    }
    out
}

/// Packs square PNGs into an `.ico` (PNG-compressed entries, supported since Windows Vista).
/// Each entry is `(size, png)`; 256 is stored as 0 per the format.
pub fn ico(entries: &[(u32, &[u8])]) -> Vec<u8> {
    let header = 6 + 16 * entries.len();
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut offset = header;
    for (size, png) in entries {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in entries {
        out.extend_from_slice(png);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECHO: IconColors = IconColors {
        primary: Rgb(0x2c, 0xab, 0xf5),
        secondary: Rgb(0x57, 0xe4, 0xe9),
        background: Rgb(0x20, 0x21, 0x30),
    };

    #[test]
    fn parses_hex() {
        assert_eq!(Rgb::from_hex("#2cabf5"), Some(Rgb(0x2c, 0xab, 0xf5)));
        assert_eq!(Rgb::from_hex("A7C080"), Some(Rgb(0xa7, 0xc0, 0x80)));
        assert_eq!(Rgb::from_hex("#abc"), None);
        assert_eq!(Rgb::from_hex("#zzzzzz"), None);
    }

    #[test]
    fn renders_a_png_of_the_requested_size() {
        let png = render_png(ECHO, 64).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        // IHDR width and height, big-endian, right after the 8-byte signature + chunk header.
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 64);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 64);
    }

    #[test]
    fn light_backgrounds_get_a_darker_wave() {
        let light = IconColors {
            background: Rgb(0xec, 0xea, 0xd9),
            ..ECHO
        };
        assert!(svg(light).contains("mix-blend-mode:multiply"));
        assert!(svg(ECHO).contains("mix-blend-mode:screen"));
    }

    #[test]
    fn container_lengths_add_up() {
        let png = [1u8, 2, 3];
        let icns = icns(&[(b"ic07", &png), (b"ic08", &png)]);
        assert_eq!(
            u32::from_be_bytes(icns[4..8].try_into().unwrap()) as usize,
            icns.len()
        );
        let ico = ico(&[(16, &png), (256, &png)]);
        assert_eq!(ico.len(), 6 + 32 + 6);
        assert_eq!(ico[6 + 16], 0, "256 px is stored as 0");
    }
}
