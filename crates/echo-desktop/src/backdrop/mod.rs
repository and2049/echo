//! The immersive view's backdrop: every color it paints, derived from the playing cover.
//!
//! Three layers, each ignorant of the next:
//!
//! 1. [`CoverPalette`] — what the cover is made of: its few prominent colors, from pixels
//!    (`palette.rs`). Pure math, no gpui.
//! 2. [`ImmersiveColors`] — the roles the view paints with (background, text, accent, wash),
//!    derived from the palette by fixed formulas so text is always readable on the base. A
//!    light cover flips the whole set to a light [`Tone`]: pale base, dark text.
//! 3. The modes — the picture behind everything, one module each with the same entry point,
//!    a [`Painter`]: the picture at a loop phase for a palette, over the view's base color, for
//!    the tone. [`painter`] is the one place that maps a [`BackdropMode`] (the name the config
//!    saves and `:backdrop` takes) to its module. A new mode is a variant there, a module here
//!    with a `paint`, and one arm in [`painter`]; a mode that needs helpers is a folder whose
//!    `mod.rs` holds that `paint`. Modes clamp their colors to [`Tone::shape_luminance`] and
//!    settle their bottom toward the base so the lyrics stay readable.
//!
//! Motion costs nothing per frame: a [`Backdrop`] is [`keyframes`] pictures painted once around
//! the loop and uploaded once, and each frame paints the two nearest with the second at a
//! crossfade opacity. Modes paint soft, slow fields, so the crossfade reads as motion.
//! [`BackdropCache`] keeps the last result and the clock. A change of cover, theme fallback or
//! mode gets an immediate one-frame stub (keyframe 0, painted on the spot) and a [`Build`] for
//! the caller to run off the UI thread; [`BackdropCache::install`] swaps the finished frames
//! in if nothing changed meanwhile. The textures a rebuild replaces are handed back through
//! [`BackdropCache::release`], because gpui's atlas never evicts on its own.

mod aurora;
mod lights;
mod mesh;
mod nebula;
mod palette;
mod raster;
mod vinyl;

use std::sync::Arc;
use std::time::{Duration, Instant};

use echo_core::artwork::SharedArtwork;
use echo_core::theme::ResolvedTheme;
use gpui::{Bounds, Hsla, Pixels, RenderImage, Rgba, point, size};

pub use echo_core::config::BackdropMode;
pub use palette::{CoverPalette, Rgb};

use crate::theme::{ToGpui, WINDOW_FG};

/// Every mode's entry point: the picture at `phase`, one trip around the loop in `0..1`, for
/// the cover's palette over the view's base color in its tone. Phase 1 must repeat phase 0.
type Painter = fn(&CoverPalette, Rgb, Tone, f32) -> raster::Raster;

fn painter(mode: BackdropMode) -> Painter {
    match mode {
        BackdropMode::Lights => lights::paint,
        BackdropMode::Mesh => mesh::paint,
        BackdropMode::Aurora => aurora::paint,
        BackdropMode::Vinyl => vinyl::paint,
        BackdropMode::Nebula => nebula::paint,
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
const DARK_SHAPE_LUMINANCE: (f32, f32) = (0.18, 0.42);
const LIGHT_SHAPE_LUMINANCE: (f32, f32) = (0.55, 0.80);

impl Tone {
    fn of(palette: &CoverPalette) -> Self {
        if palette.average.luminance() >= LIGHT_COVER {
            Self::Light
        } else {
            Self::Dark
        }
    }

    /// The luminance range a mode clamps its colors into: on the dark base they may glow but
    /// never approach the light text, on the pale base never approach the dark text.
    pub fn shape_luminance(self) -> (f32, f32) {
        match self {
            Self::Dark => DARK_SHAPE_LUMINANCE,
            Self::Light => LIGHT_SHAPE_LUMINANCE,
        }
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
/// Pictures painted around one loop of the motion. Soft modes crossfade cleanly at 64; the
/// nebula's sharp streaks step between pictures that far apart, so it gets twice as many.
const KEYFRAMES: usize = 128;

fn keyframes(mode: BackdropMode) -> usize {
    match mode {
        BackdropMode::Nebula => 2 * KEYFRAMES,
        _ => KEYFRAMES,
    }
}
/// One trip around the loop.
pub const LOOP: Duration = Duration::from_secs(30);
/// How often the view repaints while the backdrop moves: 20 frames a second is plenty for a
/// drift this slow and soft, and every frame re-renders the whole window.
pub const FRAME: Duration = Duration::from_millis(50);

pub struct Backdrop {
    pub colors: ImmersiveColors,
    /// The pictures plus their guard borders, in loop order: paint them at
    /// [`Backdrop::image_bounds`]. One frame while the full set is still building.
    frames: Vec<Arc<RenderImage>>,
}

impl Backdrop {
    /// The two keyframes around `phase` (in `0..1`) and how far the picture is from the first
    /// toward the second: paint the first, then the second at that opacity, unless it is 0.
    pub fn frame(&self, phase: f32) -> (Arc<RenderImage>, Arc<RenderImage>, f32) {
        let n = self.frames.len();
        let at = phase.rem_euclid(1.0) * n as f32;
        let ix = at as usize % n;
        let next = (ix + 1) % n;
        let blend = if next == ix { 0.0 } else { at - at.floor() };
        (self.frames[ix].clone(), self.frames[next].clone(), blend)
    }

    /// Where to paint a whole texture so that its picture, minus the guard, exactly fills
    /// `bounds`: each guard texel lands one stretched texel outside.
    pub fn image_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        let texels = self.frames[0].size(0);
        let guard = BLEED_GUARD as f32;
        let picture_w = u32::from(texels.width) as f32 - 2.0 * guard;
        let picture_h = u32::from(texels.height) as f32 - 2.0 * guard;
        let (sx, sy) = (
            bounds.size.width / picture_w,
            bounds.size.height / picture_h,
        );
        Bounds {
            origin: point(bounds.origin.x - sx * guard, bounds.origin.y - sy * guard),
            size: size(
                sx * (picture_w + 2.0 * guard),
                sy * (picture_h + 2.0 * guard),
            ),
        }
    }
}

/// The keyframes of one backdrop, ready to paint anywhere but the UI thread; [`Build::run`]
/// spreads them over the machine's cores. Pure data, so it can cross to a background task.
pub struct Build {
    generation: u64,
    mode: BackdropMode,
    palette: CoverPalette,
    colors: ImmersiveColors,
}

/// What a [`Build`] produced, for [`BackdropCache::install`].
pub struct Built {
    generation: u64,
    colors: ImmersiveColors,
    frames: Vec<Arc<RenderImage>>,
}

impl Build {
    fn keyframe(&self, k: usize) -> Arc<RenderImage> {
        let phase = k as f32 / keyframes(self.mode) as f32;
        painter(self.mode)(&self.palette, self.colors.base, self.colors.tone, phase)
            .into_render_image(BLEED_GUARD)
    }

    pub fn run(self) -> Built {
        let count = keyframes(self.mode);
        let threads = std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(count);
        let per_thread = count.div_ceil(threads);
        let mut frames: Vec<Option<Arc<RenderImage>>> = vec![None; count];
        std::thread::scope(|scope| {
            for (chunk_ix, chunk) in frames.chunks_mut(per_thread).enumerate() {
                let build = &self;
                scope.spawn(move || {
                    for (i, slot) in chunk.iter_mut().enumerate() {
                        *slot = Some(build.keyframe(chunk_ix * per_thread + i));
                    }
                });
            }
        });
        Built {
            generation: self.generation,
            colors: self.colors,
            frames: frames.into_iter().flatten().collect(),
        }
    }
}

/// The last backdrop, what it was built from, the loop clock, the build waiting for a thread,
/// and the textures replaced backdrops left in the atlas.
pub struct BackdropCache {
    last: Option<(BackdropMode, Source, Arc<Backdrop>)>,
    generation: u64,
    pending: Option<Build>,
    started: Instant,
    retired: Vec<Arc<RenderImage>>,
}

impl Default for BackdropCache {
    fn default() -> Self {
        Self {
            last: None,
            generation: 0,
            pending: None,
            started: Instant::now(),
            retired: Vec::new(),
        }
    }
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
            Source::Cover(art) => {
                CoverPalette::from_pixels(art.width, art.height, &art.pixels).with_variety()
            }
            Source::Theme(palette) => palette.clone().with_variety(),
        }
    }
}

impl BackdropCache {
    /// The backdrop for `cover`, or for `fallback` when there is none (local files often have
    /// no art; see [`theme_palette`]), rebuilt only when either or the mode changes. A rebuild
    /// returns a still stub at once and leaves the full build in [`BackdropCache::take_build`].
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
                let palette = source.palette();
                let colors = ImmersiveColors::derive(&palette);
                self.generation += 1;
                let build = Build {
                    generation: self.generation,
                    mode,
                    palette,
                    colors,
                };
                let stub = Arc::new(Backdrop {
                    colors,
                    frames: vec![build.keyframe(0)],
                });
                self.pending = Some(build);
                self.replace(mode, source, stub.clone());
                stub
            }
        }
    }

    /// The build the last [`BackdropCache::get`] left behind, if any: run it off the UI
    /// thread and hand the result to [`BackdropCache::install`].
    pub fn take_build(&mut self) -> Option<Build> {
        self.pending.take()
    }

    /// Swaps a finished build's frames in, unless the cover or mode has moved on since.
    pub fn install(&mut self, built: Built) {
        if built.generation != self.generation {
            return;
        }
        if let Some((mode, source, stub)) = self.last.take() {
            self.retired.extend(stub.frames.iter().cloned());
            let full = Arc::new(Backdrop {
                colors: built.colors,
                frames: built.frames,
            });
            self.last = Some((mode, source, full));
        }
    }

    fn replace(&mut self, mode: BackdropMode, source: Source, backdrop: Arc<Backdrop>) {
        if let Some((_, _, old)) = self.last.replace((mode, source, backdrop)) {
            self.retired.extend(old.frames.iter().cloned());
        }
    }

    /// Where the loop is right now, in `0..1`; the clock runs from the cache's creation so
    /// toggling the view never restarts the motion.
    pub fn phase(&self) -> f32 {
        (self.started.elapsed().as_secs_f32() / LOOP.as_secs_f32()).fract()
    }

    /// Hands every texture a rebuild has replaced to `drop`, which should remove it from the
    /// window's atlas.
    pub fn release(&mut self, drop: impl FnMut(Arc<RenderImage>)) {
        self.retired.drain(..).for_each(drop);
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

    fn full(mode: BackdropMode, palette: &CoverPalette) -> Backdrop {
        let colors = ImmersiveColors::derive(palette);
        let built = Build {
            generation: 0,
            mode,
            palette: palette.clone(),
            colors,
        }
        .run();
        Backdrop {
            colors,
            frames: built.frames,
        }
    }

    fn white_backdrop() -> Backdrop {
        full(
            BackdropMode::Lights,
            &CoverPalette::from_colors(vec![Rgb::WHITE]),
        )
    }

    #[test]
    fn roles_keep_text_readable_on_the_base() {
        for primary in [
            Rgb::WHITE,
            Rgb::BLACK,
            Rgb::from_u8(30, 60, 230),
            Rgb::from_u8(250, 240, 200),
        ] {
            let colors = ImmersiveColors::derive(&CoverPalette::from_colors(vec![primary]));
            let (bg, text, accent) = (lum(colors.background), lum(colors.text), lum(colors.accent));
            match colors.tone {
                Tone::Dark => assert!(
                    bg <= 0.111 && text >= 0.879 && accent >= 0.549,
                    "{primary:?}"
                ),
                Tone::Light => assert!(
                    bg >= 0.859 && text <= 0.121 && accent <= 0.451,
                    "{primary:?}"
                ),
            }
            let (muted, wash) = (
                (lum(colors.text_muted) - bg).abs(),
                (lum(colors.wash) - bg).abs(),
            );
            assert!(muted < (text - bg).abs() && muted > wash, "{primary:?}");
        }
    }

    #[test]
    fn light_covers_get_the_light_tone() {
        assert_eq!(
            Tone::of(&CoverPalette::from_colors(vec![Rgb::from_u8(
                250, 240, 200
            )])),
            Tone::Light
        );
        assert_eq!(
            Tone::of(&CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230)])),
            Tone::Dark
        );
    }

    #[test]
    fn every_mode_loops_moves_and_keeps_its_tone() {
        let palettes = [
            CoverPalette::from_colors(vec![Rgb::from_u8(30, 60, 230), Rgb::from_u8(220, 40, 90)]),
            CoverPalette::from_colors(vec![
                Rgb::from_u8(250, 240, 200),
                Rgb::from_u8(120, 200, 230),
            ]),
        ];
        for mode in BackdropMode::ALL {
            for palette in &palettes {
                let palette = palette.clone().with_variety();
                let colors = ImmersiveColors::derive(&palette);
                let paint = |phase| painter(mode)(&palette, colors.base, colors.tone, phase);
                let (start, end, later) = (paint(0.0), paint(1.0), paint(0.3));
                let seam = start
                    .pixels()
                    .iter()
                    .zip(end.pixels())
                    .map(|(a, b)| (a.luminance() - b.luminance()).abs());
                assert!(seam.fold(0.0f32, f32::max) < 1e-3, "{mode:?} seams");
                let motion = start
                    .pixels()
                    .iter()
                    .zip(later.pixels())
                    .map(|(a, b)| (a.luminance() - b.luminance()).abs());
                assert!(motion.fold(0.0f32, f32::max) > 0.02, "{mode:?} is still");
                let (lo, hi) = colors.tone.shape_luminance();
                let base = colors.base.luminance();
                for pixel in start.pixels() {
                    let lum = pixel.luminance();
                    match colors.tone {
                        Tone::Dark => {
                            assert!(lum <= hi + 0.01 && lum >= base - 0.01, "{mode:?} {lum}")
                        }
                        Tone::Light => {
                            assert!(lum >= lo - 0.01 && lum <= base + 0.01, "{mode:?} {lum}")
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn image_bounds_put_the_guard_just_outside() {
        let backdrop = white_backdrop();
        let texels = u32::from(backdrop.frames[0].size(0).width) as f32;
        let picture = texels - 2.0 * BLEED_GUARD as f32;
        let bounds = Bounds {
            origin: point(gpui::px(10.0), gpui::px(20.0)),
            size: size(gpui::px(picture * 10.0), gpui::px(picture * 5.0)),
        };
        let image = backdrop.image_bounds(bounds);
        assert_eq!(
            image.origin,
            point(gpui::px(10.0 - 20.0), gpui::px(20.0 - 10.0))
        );
        assert_eq!(
            image.size,
            size(gpui::px(texels * 10.0), gpui::px(texels * 5.0))
        );
    }

    #[test]
    fn frames_crossfade_neighbours_and_wrap() {
        let backdrop = white_backdrop();
        assert_eq!(backdrop.frames.len(), KEYFRAMES);
        let (a, b, t) = backdrop.frame(0.0);
        assert!(
            Arc::ptr_eq(&a, &backdrop.frames[0])
                && Arc::ptr_eq(&b, &backdrop.frames[1])
                && t == 0.0
        );
        let (a, b, t) = backdrop.frame(1.5 / KEYFRAMES as f32);
        assert!(Arc::ptr_eq(&a, &backdrop.frames[1]) && Arc::ptr_eq(&b, &backdrop.frames[2]));
        assert!((t - 0.5).abs() < 1e-4);
        let (a, b, t) = backdrop.frame((KEYFRAMES as f32 - 0.5) / KEYFRAMES as f32);
        assert!(
            Arc::ptr_eq(&a, &backdrop.frames[KEYFRAMES - 1])
                && Arc::ptr_eq(&b, &backdrop.frames[0])
        );
        assert!((t - 0.5).abs() < 1e-4);
        let (a, _, _) = backdrop.frame(1.0);
        assert!(Arc::ptr_eq(&a, &backdrop.frames[0]));
    }

    #[test]
    fn a_stub_never_blends() {
        let stub = Backdrop {
            colors: white_backdrop().colors,
            frames: vec![white_backdrop().frames[0].clone()],
        };
        let (a, b, t) = stub.frame(0.7);
        assert!(Arc::ptr_eq(&a, &b) && t == 0.0);
    }

    #[test]
    fn cache_stubs_then_installs_and_ignores_stale_builds() {
        let theme = CoverPalette::from_colors(vec![Rgb::from_u8(0, 200, 200)]);
        let art = |v: u8| {
            Arc::new(echo_core::artwork::Artwork {
                width: 1,
                height: 1,
                pixels: vec![v, 0, 0, 255],
            })
        };
        let (a, b) = (art(200), art(20));
        let mut cache = BackdropCache::default();
        let first = cache.get(BackdropMode::Lights, Some(&a), &theme);
        assert_eq!(first.frames.len(), 1);
        let build = cache.take_build().expect("a miss leaves a build");
        assert!(cache.take_build().is_none());
        assert!(Arc::ptr_eq(
            &first,
            &cache.get(BackdropMode::Lights, Some(&a), &theme)
        ));
        let mut released = 0;
        cache.release(|_| released += 1);
        assert_eq!(released, 0);
        cache.install(build.run());
        let full = cache.get(BackdropMode::Lights, Some(&a), &theme);
        assert!(!Arc::ptr_eq(&first, &full) && full.frames.len() == KEYFRAMES);
        cache.release(|_| released += 1);
        assert_eq!(released, 1);
        let second = cache.get(BackdropMode::Lights, Some(&b), &theme);
        let stale = cache.take_build().unwrap();
        let third = cache.get(BackdropMode::Mesh, Some(&b), &theme);
        let current = cache.take_build().unwrap();
        cache.install(stale.run());
        assert!(Arc::ptr_eq(
            &third,
            &cache.get(BackdropMode::Mesh, Some(&b), &theme)
        ));
        cache.install(current.run());
        let installed = cache.get(BackdropMode::Mesh, Some(&b), &theme);
        assert!(
            !Arc::ptr_eq(&third, &installed)
                && installed.frames.len() == keyframes(BackdropMode::Mesh)
        );
        released = 0;
        cache.release(|_| released += 1);
        assert_eq!(released, KEYFRAMES + 2);
        assert!(!Arc::ptr_eq(&second, &installed));
        let fallback = cache.get(BackdropMode::Mesh, None, &theme);
        assert!(Arc::ptr_eq(
            &fallback,
            &cache.get(BackdropMode::Mesh, None, &theme)
        ));
    }

    #[test]
    fn the_nebula_gets_more_keyframes_than_the_soft_modes() {
        assert!(keyframes(BackdropMode::Nebula) >= keyframes(BackdropMode::Lights) * 3 / 2);
        assert!(
            BackdropMode::ALL
                .iter()
                .all(|mode| keyframes(*mode) >= KEYFRAMES)
        );
    }

    #[test]
    fn the_clock_wraps_within_the_loop() {
        let phase = BackdropCache::default().phase();
        assert!((0.0..1.0).contains(&phase));
    }
}
