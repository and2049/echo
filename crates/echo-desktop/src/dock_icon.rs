//! Recolours the macOS Dock icon to match the active theme.
//!
//! The bundled `.icns` is drawn in the default theme's colors (see `build.rs`). While the app
//! runs, the Dock and app switcher instead show the same icon rendered from the active
//! theme, so a theme added to `themes/` (or a user's own) gets a matching icon with no extra
//! assets. macOS only shows this image while the app is running; Finder, Launchpad and the
//! Dock after quitting keep the bundled icon.
//!
//! Other platforms are a no-op: Windows reads its taskbar icon from the executable's
//! resources and Linux from the installed desktop entry.

use echo_core::theme::ResolvedTheme;
use gpui::Context;

use crate::EchoApp;

/// Tracks which colors the Dock icon was last drawn with, so a render pass only redraws it
/// when the theme actually changed.
#[derive(Default)]
pub(crate) struct DockIcon {
    #[cfg(target_os = "macos")]
    shown: Option<echo_icon::IconColors>,
    /// The in-flight render. Replacing it drops (cancels) the previous one, so switching
    /// themes quickly never lets a stale render land after a newer one.
    #[cfg(target_os = "macos")]
    _render: Option<gpui::Task<()>>,
}

impl DockIcon {
    /// Redraws the Dock icon if `theme` differs from what it shows. Cheap when nothing
    /// changed, so it is called on every render of the app.
    #[cfg(target_os = "macos")]
    pub(crate) fn sync(&mut self, theme: &ResolvedTheme, cx: &mut Context<EchoApp>) {
        let colors = icon_colors(theme);
        if self.shown == Some(colors) {
            return;
        }
        self.shown = Some(colors);
        // ~512 px is the largest the Dock draws (256 pt at 2x). Rendering takes tens of
        // milliseconds, so it runs off the main thread; AppKit is only touched back on it.
        self._render = Some(cx.spawn(async move |_, cx| {
            let png: Result<Vec<u8>, String> = cx
                .background_executor()
                .spawn(async move { echo_icon::render_png(colors, 512) })
                .await;
            match png {
                Ok(png) => set_application_icon(&png),
                Err(e) => eprintln!("echo: could not render the Dock icon: {e}"),
            }
        }));
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn sync(&mut self, _theme: &ResolvedTheme, _cx: &mut Context<EchoApp>) {}
}

#[cfg(target_os = "macos")]
fn icon_colors(theme: &ResolvedTheme) -> echo_icon::IconColors {
    use crate::theme::{ToGpui, WINDOW_BG, WINDOW_FG};

    let rgb = |hsla: gpui::Hsla| {
        let c = gpui::Rgba::from(hsla);
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        echo_icon::Rgb(byte(c.r), byte(c.g), byte(c.b))
    };
    // The same fallbacks the window uses for a theme slot left at `Reset`.
    echo_icon::IconColors {
        primary: rgb(theme.primary.gpui(WINDOW_FG())),
        secondary: rgb(theme.secondary.gpui(WINDOW_FG())),
        background: rgb(theme.background.gpui(WINDOW_BG())),
    }
}

#[cfg(target_os = "macos")]
fn set_application_icon(png: &[u8]) {
    use objc2::AllocAnyThread as _;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // Spawned tasks resume on the main thread, so this only fails if that ever changes.
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("echo: Dock icon update ran off the main thread; skipped");
        return;
    };
    let data = NSData::with_bytes(png);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("echo: AppKit could not decode the rendered Dock icon");
        return;
    };
    // SAFETY: called on the main thread (checked above) with a valid, retained NSImage.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}
