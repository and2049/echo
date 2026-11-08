//! Renders the app icon files, then embeds the Windows app icon as resource ID 1.
//!
//! Every file in `icons/` that a bundle or archive ships (the macOS `.icns`, the Windows
//! `.ico`, the Linux and tray PNGs) is drawn by `echo-icon` from the default theme's colors in
//! `themes/echo.toml`, so changing that theme or the icon template updates every platform's
//! icon on the next build. Files are only rewritten when their bytes change, so a build with
//! nothing to update leaves the tree clean. Other themes need no files: the macOS Dock icon is
//! rendered at runtime for whichever theme is active.
//!
//! GPUI's Windows backend loads the window/taskbar icon from the executable's own resources via
//! `LoadImageW(module, 1)` — there is no runtime icon API (`WindowOptions::icon` is X11-only).
//!
//! Deliberately does NOT embed an RT_MANIFEST: gpui's default `windows-manifest` feature already
//! embeds one, and a second copy is a duplicate-resource link error.

fn main() {
    app_icons();
    #[cfg(target_os = "windows")]
    windows_resources();
}

/// The theme whose colors the bundled icon uses.
const ICON_THEME: &str = "../../themes/echo.toml";

fn app_icons() {
    use echo_icon::{IconColors, Rgb};

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let theme_path = manifest.join(ICON_THEME);
    println!("cargo:rerun-if-changed={}", theme_path.display());
    let theme = std::fs::read_to_string(&theme_path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", theme_path.display()));
    // The base keys are plain `key = "#rrggbb"` lines; a full TOML parser would be the only
    // reason for one more build dependency.
    let color = |key: &str| {
        theme
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(k, _)| k.trim() == key)
            .and_then(|(_, v)| Rgb::from_hex(v.split('#').nth(1)?.get(..6)?))
            .unwrap_or_else(|| panic!("{} has no #rrggbb `{key}`", theme_path.display()))
    };
    let colors = IconColors {
        primary: color("primary"),
        secondary: color("secondary"),
        background: color("background"),
    };

    let png = |size: u32| {
        echo_icon::render_png(colors, size).unwrap_or_else(|e| panic!("rendering icon: {e}"))
    };
    let [p16, p32, p48, p64, p128, p256, p512, p1024] =
        [16, 32, 48, 64, 128, 256, 512, 1024].map(png);

    let icns = echo_icon::icns(&[
        (b"icp4", &p16),
        (b"icp5", &p32),
        (b"ic07", &p128),
        (b"ic08", &p256),
        (b"ic09", &p512),
        (b"ic10", &p1024),
        (b"ic11", &p32),
        (b"ic12", &p64),
        (b"ic13", &p256),
        (b"ic14", &p512),
    ]);
    let ico = echo_icon::ico(&[
        (16, &p16),
        (32, &p32),
        (48, &p48),
        (64, &p64),
        (128, &p128),
        (256, &p256),
    ]);

    let icons = manifest.join("../../icons");
    for (name, bytes) in [
        ("32x32.png", &p32),
        ("64x64.png", &p64),
        ("128x128.png", &p128),
        ("128x128@2x.png", &p256),
        ("icon.png", &p512),
        ("icon.icns", &icns),
        ("icon.ico", &ico),
    ] {
        let path = icons.join(name);
        if std::fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
            std::fs::write(&path, bytes)
                .unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_resources() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../icons/icon.ico");
    let icon = icon
        .canonicalize()
        .expect("icons/icon.ico must exist at the workspace root");
    // canonicalize() yields a \\?\-prefixed path, which rc.exe rejects.
    let icon_escaped = icon
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "\\\\");

    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let mut parts = pkg_version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .chain(std::iter::repeat(0));
    let file_version = format!(
        "{},{},{},{}",
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    );

    let rc_content = format!(
        r#"1 ICON "{icon_escaped}"

1 VERSIONINFO
FILEVERSION {file_version}
PRODUCTVERSION {file_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "echo\0"
            VALUE "FileVersion", "{pkg_version}\0"
            VALUE "ProductName", "echo\0"
            VALUE "ProductVersion", "{pkg_version}\0"
            VALUE "CompanyName", "echo\0"
            VALUE "OriginalFilename", "echo-desktop.exe\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );

    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc_path = out_dir.join("echo_resources.rc");
    std::fs::write(&rc_path, rc_content).unwrap();

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
