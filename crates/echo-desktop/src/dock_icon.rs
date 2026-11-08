//! Recolours the macOS app icon to match the active theme.
//!
//! The bundled `.icns` is drawn in the default theme's colors (see `build.rs`). The running
//! app redraws the icon from the active theme and shows it in two places:
//!
//! - the live Dock and app-switcher icon (`NSApplication.applicationIconImage`), which only
//!   lasts while the app runs;
//! - a custom icon attached to `echo.app` itself (`NSWorkspace.setIcon`, the same mechanism as
//!   pasting an icon in Finder's Get Info), so the Dock, Finder and Launchpad keep the themed
//!   icon after quitting.
//!
//! Both are redrawn on launch from whatever theme is active then, so a theme changed in the
//! config file while the app was closed is picked up on the next start. A theme added to
//! `themes/` (or a user's own) gets a matching icon with no extra assets.
//!
//! Other platforms are a no-op: Windows reads its taskbar icon from the executable's
//! resources and Linux from the installed desktop entry.

use echo_core::theme::ResolvedTheme;
use gpui::Context;

use crate::EchoApp;

/// Tracks which colors the icon was last drawn with, so a render pass only redraws it when
/// the theme actually changed.
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
    /// Redraws the icon if `theme` differs from what it shows. Cheap when nothing changed, so
    /// it is called on every render of the app.
    #[cfg(target_os = "macos")]
    pub(crate) fn sync(&mut self, theme: &ResolvedTheme, cx: &mut Context<EchoApp>) {
        let colors = icon_colors(theme);
        if self.shown == Some(colors) {
            return;
        }
        self.shown = Some(colors);
        // 1024 px covers Finder's largest previews as well as the Dock. Rendering takes tens
        // of milliseconds, so it runs off the main thread; AppKit is only touched back on it.
        self._render = Some(cx.spawn(async move |_, cx| {
            let png: Result<Vec<u8>, String> = cx
                .background_executor()
                .spawn(async move { echo_icon::render_png(colors, 1024) })
                .await;
            match png {
                Ok(png) => apply(colors, &png),
                Err(e) => eprintln!("echo: could not render the app icon: {e}"),
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

/// Shows `png` as the live Dock icon and, when running from an app bundle, saves it onto the
/// bundle so it outlives this process.
#[cfg(target_os = "macos")]
fn apply(colors: echo_icon::IconColors, png: &[u8]) {
    use objc2::AllocAnyThread as _;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    // Spawned tasks resume on the main thread, so this only fails if that ever changes.
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("echo: app icon update ran off the main thread; skipped");
        return;
    };
    let data = NSData::with_bytes(png);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        eprintln!("echo: AppKit could not decode the rendered app icon");
        return;
    };
    // SAFETY: called on the main thread (checked above) with a valid, retained NSImage.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };

    if let Some(bundle) = app_bundle() {
        persist(&bundle, colors, &image);
    }
}

/// `…/echo.app` when this binary runs from inside a bundle; `None` for `cargo run` builds,
/// which have no bundle to put an icon on.
#[cfg(target_os = "macos")]
fn app_bundle() -> Option<std::path::PathBuf> {
    // …/echo.app/Contents/MacOS/echo-desktop -> …/echo.app
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    exe.parent()?
        .parent()?
        .parent()
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
        .map(Into::into)
}

/// Attaches `image` to the bundle as its custom icon, unless the bundle already carries this
/// theme's icon.
///
/// Writing the icon makes Finder and the Dock reload it, so it is skipped when nothing changed.
/// The colors last written are kept in a small file in the user's cache directory. The bundle's
/// `Icon\r` file is checked too, since reinstalling from the DMG replaces the bundle and drops
/// the custom icon while the cache file stays.
///
/// The bundle carries no code-signature seal (cargo-packager doesn't sign it; the binary only
/// has the linker's ad-hoc signature), so the `Icon\r` file this adds invalidates nothing. A
/// bundle the user can't write to (installed by another account) keeps the bundled icon.
#[cfg(target_os = "macos")]
fn persist(
    bundle: &std::path::Path,
    colors: echo_icon::IconColors,
    image: &objc2_app_kit::NSImage,
) {
    use objc2_app_kit::{NSWorkspace, NSWorkspaceIconCreationOptions};
    use objc2_foundation::NSString;

    let stamp = format!(
        "{:?} {:?} {:?}",
        colors.primary, colors.secondary, colors.background
    );
    let marker = std::env::var_os("HOME")
        .map(|home| std::path::PathBuf::from(home).join("Library/Caches/echo/bundle-icon"));
    let has_custom_icon = bundle.join("Icon\r").exists();
    if has_custom_icon
        && marker
            .as_deref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .is_some_and(|saved| saved == stamp)
    {
        return;
    }

    let path = NSString::from_str(&bundle.to_string_lossy());
    let saved = NSWorkspace::sharedWorkspace().setIcon_forFile_options(
        Some(image),
        &path,
        NSWorkspaceIconCreationOptions::empty(),
    );
    if !saved {
        eprintln!(
            "echo: could not save the theme icon onto {}; it keeps the bundled icon",
            bundle.display()
        );
        return;
    }
    if let Some(marker) = marker {
        if let Some(dir) = marker.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(marker, stamp);
    }
}
