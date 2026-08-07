//! Embeds the Windows app icon as resource ID 1.
//!
//! GPUI's Windows backend loads the window/taskbar icon from the executable's own resources via
//! `LoadImageW(module, 1)` — there is no runtime icon API (`WindowOptions::icon` is X11-only).
//!
//! Deliberately does NOT embed an RT_MANIFEST: gpui's default `windows-manifest` feature already
//! embeds one, and a second copy is a duplicate-resource link error.

fn main() {
    #[cfg(target_os = "windows")]
    windows_resources();
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

    println!(
        "cargo:rerun-if-changed={}",
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../icons/icon.ico")
            .display()
    );
}
