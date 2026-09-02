//! The immersive view's backdrop: every color it paints, derived from the playing cover.
//!
//! Three layers, each ignorant of the next:
//!
//! 1. [`CoverPalette`] — what the cover is made of: its few prominent colors, from pixels
//!    (`palette.rs`). Pure math, no gpui.
//! 2. [`ImmersiveColors`] — the roles the view paints with (background, text, accent, wash),
//!    derived from the palette by fixed formulas so text is always readable on the base. A
//!    light cover flips the whole set to a light [`Tone`]: pale base, dark text.
//! 3. [`BackdropMode`] — the picture behind everything, painted from the palette onto a
//!    [`raster::Raster`] and uploaded once as a texture. One mode today; a new one is a variant
//!    here, a `paint` function in its own file, and nothing else — the view, the cache and the
//!    color roles do not change.
//!
//! [`BackdropCache`] keeps the last result: a backdrop is rebuilt only when the cover, the
//! theme fallback or the mode changes, never per frame.

pub mod blurred_shapes;
mod palette;
mod raster;

use std::sync::Arc;

use echo_core::artwork::SharedArtwork;
use echo_core::theme::ResolvedTheme;
use gpui::{Bounds, Hsla, Pixels, RenderImage, Rgba, point, size};

pub use palette::{CoverPalette, Rgb};

use crate::theme::{ToGpui, WINDOW_FG};

/// How the backdrop is painted. Session-only for now; the config gets a key when there is a
/// second mode to choose.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackdropMode {
    #[default]
    BlurredShapes,
}

impl BackdropMode {
    fn paint(self, palette: &CoverPalette, colors: &ImmersiveColors) -> raster::Raster {
        match self {
            Self::BlurredShapes => blurred_shapes::paint(palette, colors.base, colors.tone),
        }
    }
}

/// Whether the view reads as a dark or a light theme, decided by how light the cover is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Dark,
    Light,
}

/// Covers whose mean luminance reaches this get the light tone.
const LIGHT_COVER: f32 = 0.5;

impl Tone {
    fn of(palette: &CoverPalette) -> Self {
        if palette.average.luminance() >= LIGHT_COVER { Self::Light } else { Self::Dark }
    }
}

/// The immersive view's color roles, all tints of the cover: a deep (or, in the light tone,
/// pale) base, text at the other end, the primary pulled until it reads on the base, and a
/// wash for hovers and placeholders.
#[derive(Clone, Copy, Debug)]
pub struct ImmersiveColors {
    pub background: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub accent: Hsla,
    pub wash: Hsla,
    pub tone: Tone,
    base: Rgb,
}

impl ImmersiveColors {
    pub fn derive(palette: &CoverPalette) -> Self {
        let primary = palette.primary();
        let tone = Tone::of(palette);
        let (base, text, accent) = match tone {
            Tone::Dark => (
                primary.mix(Rgb::BLACK, 0.7).with_luminance_in(0.07, 0.11),
                Rgb::WHITE.mix(primary, 0.10).with_luminance_in(0.88, 1.0),
                primary.with_luminance_in(0.55, 0.80),
            ),
            Tone::Light => (
                primary.mix(Rgb::WHITE, 0.7).with_luminance_in(0.86, 0.92),
                Rgb::BLACK.mix(primary, 0.15).with_luminance_in(0.0, 0.12),
                primary.with_luminance_in(0.25, 0.45),
            ),
        };
        Self {
            background: hsla(base),
            text: hsla(text),
            text_muted: hsla(text.mix(base, 0.38)),
            accent: hsla(accent),
            wash: hsla(text.mix(base, 0.85)),
            tone,
            base,
        }
    }
}

/// Replicated edge texels around the picture; see [`raster::Raster::into_render_image`].
const BLEED_GUARD: usize = 2;

pub struct Backdrop {
    pub colors: ImmersiveColors,
    /// The picture plus its guard border: paint it at [`Backdrop::image_bounds`].
    pub image: Arc<RenderImage>,
}

impl Backdrop {
    fn build(mode: BackdropMode, palette: &CoverPalette) -> Self {
        let colors = ImmersiveColors::derive(palette);
        let image = mode.paint(palette, &colors).into_render_image(BLEED_GUARD);
        Self { colors, image }
    }

    /// Where to paint the whole texture so that its picture, minus the guard, exactly fills
    /// `bounds`: each guard texel lands one stretched texel outside.
    pub fn image_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let texels = self.image.size(0);
        let guard = BLEED_GUARD as f32;
        let picture_w = u32::from(texels.width) as f32 - 2.0 * guard;
        let picture_h = u32::from(texels.height) as f32 - 2.0 * guard;
        let (sx, sy) = (bounds.size.width / picture_w, bounds.size.height / picture_h);
        Bounds {
            origin: point(bounds.origin.x - sx * guard, bounds.origin.y - sy * guard),
            size: size(sx * (picture_w + 2.0 * guard), sy * (picture_h + 2.0 * guard)),
        }
    }
}

/// The last backdrop and what it was built from.
#[derive(Default)]
pub struct BackdropCache {
    last: Option<(BackdropMode, Source, Arc<Backdrop>)>,
}

enum Source {
    Cover(SharedArtwork),
    Theme(CoverPalette),
}

impl Source {
    fn same(&self, other: &Source) -> bool {
        match (self, other) {
            (Source::Cover(a), Source::Cover(b)) => Arc::ptr_eq(a, b),
            (Source::Theme(a), Source::Theme(b)) => a == b,
            _ => false,
        }
    }

    fn palette(&self) -> CoverPalette {
        match self {
            Source::Cover(art) => CoverPalette::from_pixels(art.width, art.height, &art.pixels).with_variety(),
            Source::Theme(palette) => palette.clone().with_variety(),
        }
    }
}

impl BackdropCache {
    /// The backdrop for `cover`, or for `fallback` when there is none (local files often have
    /// no art; see [`theme_palette`]), rebuilt only when either changes.
    pub fn get(
        &mut self,
        mode: BackdropMode,
        cover: Option<&SharedArtwork>,
        fallback: &CoverPalette,
    ) -> Arc<Backdrop> {
        let source = match cover {
            Some(art) => Source::Cover(art.clone()),
            None => Source::Theme(fallback.clone()),
        };
        match &self.last {
            Some((last_mode, last, backdrop)) if *last_mode == mode && last.same(&source) => {
                backdrop.clone()
            }
            _ => {
                let backdrop = Arc::new(Backdrop::build(mode, &source.palette()));
                self.last = Some((mode, source, backdrop.clone()));
                backdrop
            }
        }
    }
}

/// The theme's accents as a palette, so a track without art still gets a backdrop in the
/// theme's own hues.
pub fn theme_palette(theme: &ResolvedTheme) -> CoverPalette {
    CoverPalette::from_colors(
        [&theme.primary, &theme.secondary, &theme.highlight_bg]
            .into_iter()
            .map(|color| rgb(color.gpui(WINDOW_FG())))
            .collect(),
    )
}

fn rgb(color: Hsla) -> Rgb {
    let c = Rgba::from(color);
    Rgb::new(c.r, c.g, c.b)
}

fn hsla(color: Rgb) -> Hsla {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: 1.0,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(color: Hsla) -> f32 {
        rgb(color).luminance()
    }

    #[test]
    fn roles_keep_text_readable_on_the_base() {
        for primary in [Rgb::WHITE, Rgb::BLACK, Rgb::from_u8(30, 60, 230), Rgb::from_u8(250, 240, 200)] {
            let colors = ImmersiveColors::derive(&CoverPalette::from_colors(vec![primary]));
            let (bg, text, accent) = (lum(colors.background), lum(colors.text), lum(colors.accent));
            match colors.tone {
                Tone::Dark => assert!(bg <= 0.111 && text >= 0.879 && accent >= 0.549, "{primary:?}"),
                Tone::Light => assert!(bg >= 0.859 && text <= 0.121 && accent <= 0.451, "{primary:?}"),
            }
            let (muted, wash) = ((lum(colors.text_muted) - bg).abs(), (lum(colors.wash) - bg).abs());
            assert!(muted < (text - bg).abs() && muted > wash, "{primary:?}");
        }
    }

    #[test]
    fn light_covers_get_the_light_tone() {
        assert_eq!(Tone::of(&CoverPalette::from_colors(vec![Rgb::from_u8(250, 240, 200)])), Tone::Light);
        assert_eq!(Tone::of(&CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230)])), Tone::Dark);
    }

    #[test]
    fn image_bounds_put_the_guard_just_outside() {
        let backdrop = Backdrop::build(BackdropMode::BlurredShapes, &CoverPalette::from_colors(vec![Rgb::WHITE]));
        let texels = u32::from(backdrop.image.size(0).width) as f32;
        let picture = texels - 2.0 * BLEED_GUARD as f32;
        let bounds = Bounds { origin: point(gpui::px(10.0), gpui::px(20.0)), size: size(gpui::px(picture * 10.0), gpui::px(picture * 5.0)) };
        let image = backdrop.image_bounds(bounds);
        assert_eq!(image.origin, point(gpui::px(10.0 - 20.0), gpui::px(20.0 - 10.0)));
        assert_eq!(image.size, size(gpui::px(texels * 10.0), gpui::px(texels * 5.0)));
    }

    #[test]
    fn cache_rebuilds_only_when_the_source_changes() {
        let theme = CoverPalette::from_colors(vec![Rgb::from_u8(0, 200, 200)]);
        let art = |v: u8| Arc::new(echo_core::artwork::Artwork { width: 1, height: 1, pixels: vec![v, 0, 0, 255] });
        let (a, b) = (art(200), art(20));
        let mut cache = BackdropCache::default();
        let first = cache.get(BackdropMode::BlurredShapes, Some(&a), &theme);
        assert!(Arc::ptr_eq(&first, &cache.get(BackdropMode::BlurredShapes, Some(&a), &theme)));
        let second = cache.get(BackdropMode::BlurredShapes, Some(&b), &theme);
        assert!(!Arc::ptr_eq(&first, &second));
        let fallback = cache.get(BackdropMode::BlurredShapes, None, &theme);
        assert!(Arc::ptr_eq(&fallback, &cache.get(BackdropMode::BlurredShapes, None, &theme)));
        assert!(!Arc::ptr_eq(&fallback, &second));
    }
}
