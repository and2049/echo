//! Self-upgrade: replacing an installed echo with a newer GitHub release.
//!
//! There is exactly one install shape — `echo-desktop` and `spotify` side by side in a directory
//! the user owns, with `themes/` beside them — so there is exactly one thing to swap: the
//! `echo-<os>-<arch>.tar.gz` archive of that directory. The installers (`assets/install.sh`,
//! `assets/install.ps1`) lay that shape down; every upgrade after it happens here, with no
//! installer UI involved.
//!
//! Two details drive the design:
//!
//! - Both frontends live in the *same* directory on every platform (see
//!   `echo-desktop/Cargo.toml`), so one archive updates both at once.
//! - Themes are loaded from `<exe dir>/themes` (see [`crate::config`]), so an archive of bare
//!   binaries would leave a release's theme changes behind. The archives carry `themes/` too and
//!   the directory is swapped along with the binaries.

use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

/// Owner/name of the repository releases are pulled from. Matches the install scripts.
pub const REPO: &str = "and2049/echo";

/// The version in the committed `Cargo.toml`. CI rewrites it per release, so a binary still
/// reporting this was built from a working tree.
pub const DEV_VERSION: &str = "0.1.0";

/// GitHub rejects API requests that arrive without one.
const USER_AGENT: &str = concat!("echo/", env!("CARGO_PKG_VERSION"));

/// Network calls are bounded so a black-holed connection cannot hang `spotify upgrade`.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// True unless this binary came out of the release workflow.
///
/// Two independent signals, because getting this wrong means a `cargo run` overwrites itself
/// with a downloaded release: the publish workflow sets `ECHO_RELEASE`, and it also rewrites the
/// version away from [`DEV_VERSION`]. Either one missing means "not a release".
pub fn is_dev_build() -> bool {
    option_env!("ECHO_RELEASE").is_none() || current_version() == DEV_VERSION
}

/// Where to send someone whose install cannot be upgraded in place.
pub fn releases_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("development build ({0}) — install a release before upgrading")]
    DevBuild(&'static str),
    #[error("could not reach GitHub: {0}")]
    Network(String),
    #[error("no release build for {0}")]
    UnsupportedPlatform(String),
    #[error("release v{version} has no asset named {asset}")]
    MissingAsset { version: String, asset: String },
    #[error("could not tell how echo was installed (looked in {0})")]
    UnknownInstall(PathBuf),
    // Installs are per-user by construction, so this means something moved echo somewhere the
    // user cannot write — reinstalling puts it back where upgrades work.
    #[error("{0} is not writable — reinstall echo from {1}")]
    NotWritable(PathBuf, String),
    #[error("download was incomplete or corrupt — try again")]
    CorruptDownload,
    #[error("{0}")]
    Io(String),
}

fn net(error: reqwest::Error) -> UpdateError {
    UpdateError::Network(error.to_string())
}

fn io(error: std::io::Error) -> UpdateError {
    UpdateError::Io(error.to_string())
}

fn client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(net)
}

// --- Release metadata ------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

impl Release {
    /// Tags are `v1.2.3`; versions are `1.2.3`.
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }

    fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

/// True when `candidate` supersedes `current`.
///
/// Falls back to string inequality for versions semver cannot parse: offering an upgrade that
/// turns out to be a no-op beats stranding someone on an old build.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(current),
    ) {
        (Ok(candidate), Ok(current)) => candidate > current,
        _ => candidate != current,
    }
}

async fn fetch_release(url: String) -> Result<Release, UpdateError> {
    let response = client()?.get(url).send().await.map_err(net)?;
    // Unauthenticated API calls are rate limited to 60/hour per IP, which is easy to hit
    // behind CGNAT and worth naming rather than reporting as a bare 403.
    if response.status() == reqwest::StatusCode::FORBIDDEN
        && response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value == "0")
    {
        return Err(UpdateError::Network(
            "GitHub API rate limit reached — try again later".into(),
        ));
    }
    response
        .error_for_status()
        .map_err(net)?
        .json::<Release>()
        .await
        .map_err(net)
}

pub async fn latest_release() -> Result<Release, UpdateError> {
    fetch_release(format!(
        "https://api.github.com/repos/{REPO}/releases/latest"
    ))
    .await
}

/// Fetch a specific tag rather than whichever release is latest.
pub async fn release_for(version: &str) -> Result<Release, UpdateError> {
    let tag = format!("v{}", version.trim_start_matches('v'));
    fetch_release(format!(
        "https://api.github.com/repos/{REPO}/releases/tags/{tag}"
    ))
    .await
}

// --- Working out what is installed ----------------------------------------

/// The `<os>-<arch>` fragment in archive names, or `None` on a platform CI does not build.
///
/// macOS is arm64-only because the release runner is `macos-15`; an Intel Mac gets a clear
/// "no build" error rather than an arm64 binary it cannot execute.
pub fn platform_target() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("macos", "aarch64") => "darwin-arm64",
        ("windows", "x86_64") => "windows-x64",
        _ => return None,
    })
}

fn require_target() -> Result<&'static str, UpdateError> {
    platform_target().ok_or_else(|| {
        UpdateError::UnsupportedPlatform(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Directory replacements are staged in and renamed into.
    pub dir: PathBuf,
    /// Absolute paths of binaries to replace. File names double as archive member names.
    pub targets: Vec<PathBuf>,
    /// Theme directory to replace wholesale, when this install has one next to the binaries.
    pub themes: Option<PathBuf>,
}

impl Plan {
    /// The one asset an upgrade ever pulls. Unversioned in the name, so `releases/latest`
    /// download URLs stay stable.
    pub fn asset_name(&self) -> Result<String, UpdateError> {
        Ok(format!("echo-{}.tar.gz", require_target()?))
    }
}

fn bin_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }
}

/// Resolve what to replace from the path of the running process.
///
/// Split out from [`plan`] so it is testable without a matching install on disk: `exists`
/// answers "is there a file at this path".
fn resolve(exe: &Path, exists: impl Fn(&Path) -> bool) -> Result<Plan, UpdateError> {
    let dir = parent_of(exe)?;

    // An install normally holds both binaries, but only what is actually on disk gets replaced:
    // extracting `echo-desktop` over a directory that never had it would leave a stray copy
    // outside whatever the user's launcher points at.
    let targets: Vec<PathBuf> = ["spotify", "echo-desktop"]
        .into_iter()
        .map(|stem| dir.join(bin_name(stem)))
        .filter(|path| exists(path))
        .collect();
    if targets.is_empty() {
        return Err(UpdateError::UnknownInstall(dir));
    }

    // Mirrors `config::app_theme_dirs`: themes sit beside the binary, except in a macOS
    // bundle where they live in Contents/Resources.
    let bundled_themes = dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name == "MacOS")
        .and_then(|_| dir.parent())
        .map(|contents| contents.join("Resources").join("themes"));
    let themes = bundled_themes
        .or_else(|| Some(dir.join("themes")))
        .filter(|path| exists(path));

    Ok(Plan {
        dir,
        targets,
        themes,
    })
}

/// Inspect the running install and decide what to replace.
///
/// Proves the install directory is writable before anything is downloaded, so someone on a
/// system-owned path finds out immediately rather than after a 25 MB transfer.
pub fn plan() -> Result<Plan, UpdateError> {
    // Resolves symlinks, so the `~/.local/bin/spotify` link the installers leave behind lands
    // on the real install directory rather than on a directory holding nothing but links.
    let exe = std::env::current_exe().map_err(io)?;
    let plan = resolve(&exe, |path| path.exists())?;
    probe_writable(&plan.dir)?;
    // The theme directory is replaced by renaming it, which needs write permission on its
    // parent, not on the directory itself — and in a macOS bundle that parent is
    // Contents/Resources, which the check above never touched.
    if let Some(parent) = plan.themes.as_deref().and_then(Path::parent)
        && parent != plan.dir
    {
        probe_writable(parent)?;
    }
    Ok(plan)
}

fn parent_of(path: &Path) -> Result<PathBuf, UpdateError> {
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| UpdateError::UnknownInstall(path.to_path_buf()))
}

fn probe_writable(dir: &Path) -> Result<(), UpdateError> {
    let probe = dir.join(format!(".echo-write-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(_) => Err(UpdateError::NotWritable(
            dir.to_path_buf(),
            releases_url(),
        )),
    }
}

/// Delete `*.old` files left behind by a previous upgrade.
///
/// Windows cannot unlink the image of a running process, so [`apply`] leaves its backups in
/// place. Both frontends call this at startup, when nothing holds them open any more.
pub fn sweep_backups() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    for stem in ["spotify", "echo-desktop"] {
        let mut name = std::ffi::OsString::from(bin_name(stem));
        name.push(".old");
        let _ = std::fs::remove_file(dir.join(name));
    }
}

// --- Checking -------------------------------------------------------------

pub enum Check {
    UpToDate,
    Available(Release),
}

/// Ask GitHub whether anything newer exists. Does not touch the filesystem.
pub async fn check() -> Result<Check, UpdateError> {
    if is_dev_build() {
        return Err(UpdateError::DevBuild(current_version()));
    }
    let release = latest_release().await?;
    if is_newer(release.version(), current_version()) {
        Ok(Check::Available(release))
    } else {
        Ok(Check::UpToDate)
    }
}

// --- Downloading ----------------------------------------------------------

/// A downloaded release, unpacked and waiting to be renamed into place.
pub struct Staged {
    /// Temp directory inside the install dir, so every rename stays on one filesystem.
    staging: PathBuf,
    /// `(staged path, destination)` pairs. Either side may be a directory (`themes/`).
    moves: Vec<(PathBuf, PathBuf)>,
    dir: PathBuf,
    pub version: String,
}

impl Drop for Staged {
    fn drop(&mut self) {
        // Covers both the happy path and a caller that downloads then bails.
        let _ = std::fs::remove_dir_all(&self.staging);
    }
}

/// Download the asset this plan needs and unpack it into a staging directory.
///
/// `on_progress` receives 0-100 as the transfer proceeds. Nothing installed is touched here —
/// the archive is fully validated first, so a corrupt download cannot leave a half-swapped
/// install behind.
pub async fn download(
    plan: Plan,
    release: &Release,
    mut on_progress: impl FnMut(u8),
) -> Result<Staged, UpdateError> {
    let version = release.version().to_string();
    let asset_name = plan.asset_name()?;
    let asset = release
        .asset(&asset_name)
        .ok_or_else(|| UpdateError::MissingAsset {
            version: version.clone(),
            asset: asset_name.clone(),
        })?;

    let bytes = fetch(asset, &mut on_progress).await?;

    let staging = plan.dir.join(format!(".echo-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir(&staging).map_err(io)?;

    // Held from here on so every early return cleans the staging directory up.
    let mut staged = Staged {
        staging: staging.clone(),
        moves: Vec::new(),
        dir: plan.dir.clone(),
        version,
    };

    staged.moves = unpack(&bytes, &staging, &plan)?;

    Ok(staged)
}

async fn fetch(asset: &Asset, on_progress: &mut impl FnMut(u8)) -> Result<Vec<u8>, UpdateError> {
    let mut response = client()?
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(net)?
        .error_for_status()
        .map_err(net)?;

    let expected = response.content_length().unwrap_or(asset.size);
    let total = expected.max(1);
    let mut bytes: Vec<u8> = Vec::with_capacity(total as usize);
    let mut last_reported = u8::MAX;

    while let Some(chunk) = response.chunk().await.map_err(net)? {
        bytes.extend_from_slice(&chunk);
        let percent = ((bytes.len() as u64 * 100) / total).min(100) as u8;
        if percent != last_reported {
            last_reported = percent;
            on_progress(percent);
        }
    }

    // A connection dropped mid-transfer yields a short body with no error of its own.
    if expected > 0 && bytes.len() as u64 != expected {
        return Err(UpdateError::CorruptDownload);
    }

    Ok(bytes)
}

/// Unpack the members this plan needs out of a `.tar.gz`.
///
/// Archive paths are untrusted: anything absolute, containing `..`, or outside the expected
/// members is skipped, so a crafted archive cannot write outside the staging directory.
fn unpack(
    archive: &[u8],
    staging: &Path,
    plan: &Plan,
) -> Result<Vec<(PathBuf, PathBuf)>, UpdateError> {
    let wanted: Vec<(std::ffi::OsString, &PathBuf)> = plan
        .targets
        .iter()
        .filter_map(|target| target.file_name().map(|name| (name.to_os_string(), target)))
        .collect();

    let mut decoder = flate2::read::GzDecoder::new(archive);
    {
        let mut tar = tar::Archive::new(&mut decoder);
        for entry in tar.entries().map_err(io)? {
            let mut entry = entry.map_err(io)?;
            let path = entry.path().map_err(io)?.into_owned();
            let Some(relative) = safe_relative(&path) else {
                continue;
            };
            let first = relative.components().next();
            let keep = match first {
                Some(Component::Normal(name)) => {
                    wanted.iter().any(|(wanted, _)| wanted == name)
                        || (plan.themes.is_some() && name == "themes")
                }
                _ => false,
            };
            if !keep {
                continue;
            }
            let destination = staging.join(&relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(io)?;
            }
            entry.unpack(&destination).map_err(io)?;
        }
    }

    // `tar` stops at the end-of-archive marker and never reads the gzip trailer, so flate2's
    // CRC32 is only checked if the rest of the stream is drained. Without this a truncated
    // download unpacks "successfully" with a short final file.
    std::io::copy(&mut decoder, &mut std::io::sink()).map_err(|_| UpdateError::CorruptDownload)?;

    // Validate everything before the caller is allowed to swap anything in.
    let mut moves = Vec::new();
    for (name, target) in &wanted {
        let staged = staging.join(name);
        let usable = std::fs::metadata(&staged).is_ok_and(|meta| meta.is_file() && meta.len() > 0);
        if !usable {
            return Err(UpdateError::Io(format!(
                "release archive is missing {}",
                name.to_string_lossy()
            )));
        }
        set_executable(&staged);
        moves.push((staged, (*target).clone()));
    }
    if let Some(themes) = &plan.themes {
        let staged = staging.join("themes");
        if staged.is_dir() {
            moves.push((staged, themes.clone()));
        }
        // A release without a themes/ member simply leaves the installed themes alone.
    }

    Ok(moves)
}

/// Reject absolute paths and `..`, and strip a leading `./`.
fn safe_relative(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

// --- Applying -------------------------------------------------------------

/// Rename the staged payload over the installed one, returning the version now installed.
///
/// Every replacement is a rename within one directory, so each entry flips atomically. The old
/// entry is moved aside rather than overwritten: on Windows that is the only way to replace a
/// running `.exe`, and everywhere it leaves something to roll back to if a later entry fails.
pub fn apply(staged: Staged) -> Result<String, UpdateError> {
    let mut done: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();

    for (source, dest) in &staged.moves {
        let backup = backup_path(dest);
        // A previous upgrade on Windows may have left this behind; it only becomes deletable
        // once the process that was holding it has exited.
        remove_any(&backup);

        let saved = if dest.exists() {
            match rename_with_retry(dest, &backup) {
                Ok(()) => Some(backup),
                Err(error) => {
                    rollback(&done);
                    return Err(io(error));
                }
            }
        } else {
            None
        };

        if let Err(error) = rename_with_retry(source, dest) {
            if let Some(saved) = &saved {
                let _ = std::fs::rename(saved, dest);
            }
            rollback(&done);
            return Err(io(error));
        }

        set_executable(dest);
        done.push((dest.clone(), saved));
    }

    // Unix can unlink a running binary — the process keeps its inode — so backups go now.
    // Windows holds a lock until exit, so they stay and `sweep_backups` clears them next launch.
    if !cfg!(windows) {
        for (_, backup) in &done {
            if let Some(backup) = backup {
                remove_any(backup);
            }
        }
    }

    resign_if_bundled(&staged.dir, &staged.moves);
    Ok(staged.version.clone())
}

fn rollback(done: &[(PathBuf, Option<PathBuf>)]) {
    for (dest, backup) in done.iter().rev() {
        if let Some(backup) = backup {
            remove_any(dest);
            let _ = std::fs::rename(backup, dest);
        }
    }
}

/// Windows briefly reports a sharing violation while a virus scanner holds a freshly written
/// file. A few short retries turn that from a failed upgrade into a slightly slower one.
fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut attempt = 0;
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) => {
                if !cfg!(windows) || attempt >= 4 {
                    return Err(error);
                }
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(150 * attempt));
            }
        }
    }
}

fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

fn backup_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    dest.with_file_name(name)
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if path.is_file() {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

/// Re-sign an app bundle whose binaries were just swapped.
///
/// cargo-packager does not sign this bundle (there is no `[package.metadata.packager.macos]`
/// section and CI does no notarization), so in practice the only signature is the ad-hoc one
/// the linker embeds per binary, which survives the copy. This is belt-and-braces for the case
/// where a bundle seal does exist, and is best effort: if `codesign` is absent or fails, the
/// DMG remains the way out.
#[cfg(target_os = "macos")]
fn resign_if_bundled(dir: &Path, moves: &[(PathBuf, PathBuf)]) {
    // .../echo.app/Contents/MacOS -> .../echo.app
    let Some(bundle) = dir
        .parent()
        .and_then(Path::parent)
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
    else {
        return;
    };
    // Nested executables must be signed before the bundle that contains them.
    for (_, dest) in moves.iter().filter(|(_, dest)| dest.is_file()) {
        let _ = std::process::Command::new("codesign")
            .args(["--force", "--sign", "-"])
            .arg(dest)
            .status();
    }
    let _ = std::process::Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(bundle)
        .status();
}

#[cfg(not(target_os = "macos"))]
fn resign_if_bundled(_dir: &Path, _moves: &[(PathBuf, PathBuf)]) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_names_lose_their_v() {
        let release = Release {
            tag_name: "v0.4.5".into(),
            assets: Vec::new(),
        };
        assert_eq!(release.version(), "0.4.5");
    }

    #[test]
    fn newer_versions_are_ordered_by_semver_not_string() {
        assert!(is_newer("0.4.10", "0.4.9"));
        assert!(is_newer("0.5.0", "0.4.9"));
        assert!(!is_newer("0.4.5", "0.4.5"));
        // A local build ahead of the published release is not an upgrade.
        assert!(!is_newer("0.4.5", "0.5.0"));
    }

    #[test]
    fn unparseable_versions_fall_back_to_inequality() {
        assert!(is_newer("nightly", "0.4.5"));
        assert!(!is_newer("nightly", "nightly"));
    }

    #[test]
    fn the_asset_is_the_combined_archive_for_this_platform() {
        let plan = Plan {
            dir: PathBuf::from("/opt/echo"),
            targets: Vec::new(),
            themes: None,
        };
        if let Some(target) = platform_target() {
            assert_eq!(
                plan.asset_name().unwrap(),
                format!("echo-{target}.tar.gz")
            );
        }
    }

    #[test]
    fn backups_keep_the_original_extension() {
        // `with_extension` would turn spotify.exe into spotify.old and orphan the real binary.
        assert_eq!(
            backup_path(Path::new("/opt/echo/spotify.exe")),
            PathBuf::from("/opt/echo/spotify.exe.old")
        );
        assert_eq!(
            backup_path(Path::new("/opt/echo/spotify")),
            PathBuf::from("/opt/echo/spotify.old")
        );
    }

    #[test]
    fn archive_paths_that_escape_are_rejected() {
        assert_eq!(safe_relative(Path::new("./spotify")).as_deref(), Some(Path::new("spotify")));
        assert_eq!(
            safe_relative(Path::new("themes/echo.toml")).as_deref(),
            Some(Path::new("themes/echo.toml"))
        );
        assert!(safe_relative(Path::new("../../etc/passwd")).is_none());
        assert!(safe_relative(Path::new("/etc/passwd")).is_none());
        assert!(safe_relative(Path::new("./")).is_none());
    }

    // `resolve` is the whole install-shape decision, so it is exercised directly rather than
    // against a real install.
    fn present(paths: &[&str]) -> impl Fn(&Path) -> bool + use<> {
        let owned: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        move |path: &Path| owned.iter().any(|known| known == path)
    }

    #[test]
    fn both_binaries_side_by_side_are_both_replaced() {
        let dir = if cfg!(windows) { "C:/echo" } else { "/opt/echo" };
        let exe = format!("{dir}/{}", bin_name("spotify"));
        let desktop = format!("{dir}/{}", bin_name("echo-desktop"));
        let plan = resolve(Path::new(&exe), present(&[&exe, &desktop])).unwrap();
        assert_eq!(
            plan.targets,
            vec![PathBuf::from(&exe), PathBuf::from(&desktop)]
        );
    }

    #[test]
    fn only_the_binaries_actually_installed_are_replaced() {
        // Someone who kept just the TUI out of the archive gets the same archive back, with
        // `echo-desktop` left out of it rather than dropped beside a binary they never had.
        let dir = if cfg!(windows) { "C:/echo" } else { "/opt/echo" };
        let exe = format!("{dir}/{}", bin_name("spotify"));
        let themes = format!("{dir}/themes");
        let plan = resolve(Path::new(&exe), present(&[&exe, &themes])).unwrap();
        assert_eq!(plan.targets, vec![PathBuf::from(&exe)]);
        assert_eq!(plan.themes, Some(PathBuf::from(themes)));
    }

    #[test]
    fn an_install_with_neither_binary_is_not_upgradeable() {
        let dir = if cfg!(windows) { "C:/echo" } else { "/opt/echo" };
        let exe = format!("{dir}/something-else");
        let error = resolve(Path::new(&exe), present(&[])).unwrap_err();
        assert!(matches!(error, UpdateError::UnknownInstall(_)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_mac_bundle_finds_themes_in_resources() {
        // install.sh drops echo.app in /Applications, so the upgrade target is the bundle's
        // own MacOS directory — themes are one level up in Resources, not beside the binaries.
        let exe = "/Applications/echo.app/Contents/MacOS/spotify";
        let desktop = "/Applications/echo.app/Contents/MacOS/echo-desktop";
        let themes = "/Applications/echo.app/Contents/Resources/themes";
        let plan = resolve(Path::new(exe), present(&[exe, desktop, themes])).unwrap();
        assert_eq!(plan.themes, Some(PathBuf::from(themes)));
    }

    /// Build a `.tar.gz` the way the publish workflow does: members at the archive root.
    fn build_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        for (name, contents) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, *contents).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "echo-update-test-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The whole swap, end to end: unpack an archive and rename it over a live install.
    #[test]
    fn unpack_then_apply_replaces_binaries_and_themes() {
        let dir = scratch_dir("roundtrip");
        let binary = dir.join(bin_name("spotify"));
        std::fs::write(&binary, b"old binary").unwrap();
        std::fs::create_dir(dir.join("themes")).unwrap();
        std::fs::write(dir.join("themes").join("echo.toml"), b"old theme").unwrap();

        let archive = build_archive(&[
            (bin_name("spotify").as_str(), b"new binary"),
            ("themes/echo.toml", b"new theme"),
            ("themes/added.toml", b"added theme"),
        ]);

        let plan = resolve(&binary, |path| path.exists()).unwrap();
        assert_eq!(plan.themes, Some(dir.join("themes")));

        let staging = dir.join(".echo-update-test");
        std::fs::create_dir(&staging).unwrap();
        let moves = unpack(&archive, &staging, &plan).unwrap();
        // Nothing installed may be touched until the archive has fully validated.
        assert_eq!(std::fs::read(&binary).unwrap(), b"old binary");

        let staged = Staged {
            staging: staging.clone(),
            moves,
            dir: dir.clone(),
            version: "9.9.9".into(),
        };
        assert_eq!(apply(staged).unwrap(), "9.9.9");

        assert_eq!(std::fs::read(&binary).unwrap(), b"new binary");
        assert_eq!(
            std::fs::read(dir.join("themes").join("echo.toml")).unwrap(),
            b"new theme"
        );
        // The theme directory is replaced wholesale, so new files in a release arrive too.
        assert!(dir.join("themes").join("added.toml").exists());
        assert!(!staging.exists(), "staging directory should be cleaned up");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The publish workflow runs `tar -czf … -C pkg .`, which prefixes every member with `./`
    /// and emits a bare `./` entry of its own. Both must survive the path guard.
    #[test]
    fn archives_in_the_shape_ci_produces_unpack() {
        let dir = scratch_dir("ci-shape");
        let binary = dir.join(bin_name("spotify"));
        std::fs::write(&binary, b"old binary").unwrap();
        std::fs::create_dir(dir.join("themes")).unwrap();
        std::fs::write(dir.join("themes").join("echo.toml"), b"old theme").unwrap();

        let archive = build_archive(&[
            ("./", b""),
            (format!("./{}", bin_name("spotify")).as_str(), b"new binary"),
            ("./themes/echo.toml", b"new theme"),
        ]);

        let plan = resolve(&binary, |path| path.exists()).unwrap();
        let staging = dir.join(".echo-update-test");
        std::fs::create_dir(&staging).unwrap();
        let moves = unpack(&archive, &staging, &plan).unwrap();

        let staged = Staged {
            staging,
            moves,
            dir: dir.clone(),
            version: "9.9.9".into(),
        };
        apply(staged).unwrap();
        assert_eq!(std::fs::read(&binary).unwrap(), b"new binary");
        assert_eq!(
            std::fs::read(dir.join("themes").join("echo.toml")).unwrap(),
            b"new theme"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_archive_is_rejected_rather_than_half_unpacked() {
        let dir = scratch_dir("truncated");
        let binary = dir.join(bin_name("spotify"));
        std::fs::write(&binary, b"old binary").unwrap();

        let archive = build_archive(&[(bin_name("spotify").as_str(), b"new binary")]);
        // Losing the gzip trailer is exactly what a dropped connection looks like. `tar` alone
        // would not notice, because it stops at the end-of-archive marker.
        let truncated = &archive[..archive.len() - 8];

        let plan = resolve(&binary, |path| path.exists()).unwrap();
        let staging = dir.join(".echo-update-test");
        std::fs::create_dir(&staging).unwrap();
        assert!(matches!(
            unpack(truncated, &staging, &plan),
            Err(UpdateError::CorruptDownload)
        ));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_archive_missing_a_wanted_binary_is_rejected() {
        let dir = scratch_dir("missing");
        let binary = dir.join(bin_name("spotify"));
        std::fs::write(&binary, b"old binary").unwrap();

        let archive = build_archive(&[("themes/echo.toml", b"only themes")]);
        let plan = resolve(&binary, |path| path.exists()).unwrap();
        let staging = dir.join(".echo-update-test");
        std::fs::create_dir(&staging).unwrap();

        let error = unpack(&archive, &staging, &plan).unwrap_err();
        assert!(matches!(error, UpdateError::Io(_)), "got {error:?}");
        assert_eq!(std::fs::read(&binary).unwrap(), b"old binary");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Serve one canned HTTP response and return the URL to hit. `declared_len` is what the
    /// `Content-Length` header claims, which is not always what gets written.
    fn serve_once(body: Vec<u8>, declared_len: usize) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut discard = [0u8; 1024];
            let _ = stream.read(&mut discard);
            let _ = stream.write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {declared_len}\r\n\r\n").as_bytes(),
            );
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        });
        format!("http://127.0.0.1:{port}/asset.tar.gz")
    }

    #[tokio::test]
    async fn a_download_streams_through_to_completion_and_reports_progress() {
        let body: Vec<u8> = (0..64_000u32).map(|byte| byte as u8).collect();
        let asset = Asset {
            name: "asset.tar.gz".into(),
            browser_download_url: serve_once(body.clone(), body.len()),
            size: body.len() as u64,
        };

        let mut seen = Vec::new();
        let fetched = fetch(&asset, &mut |percent| seen.push(percent)).await.unwrap();

        assert_eq!(fetched, body);
        assert_eq!(seen.last(), Some(&100), "progress should reach 100%");
        assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "monotonic");
    }

    #[tokio::test]
    async fn a_short_body_is_not_accepted_as_a_complete_download() {
        // A connection dropped mid-transfer: the header promises more than arrives.
        let body = vec![7u8; 1_000];
        let asset = Asset {
            name: "asset.tar.gz".into(),
            browser_download_url: serve_once(body, 10_000),
            size: 10_000,
        };
        assert!(fetch(&asset, &mut |_| {}).await.is_err());
    }

    #[test]
    fn a_release_payload_deserializes_and_looks_assets_up_by_name() {
        let release: Release = serde_json::from_str(
            r#"{"tag_name":"v0.4.5","assets":[
                {"name":"a.tar.gz","browser_download_url":"https://x/a","size":12},
                {"name":"b.tar.gz","browser_download_url":"https://x/b"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(release.assets.len(), 2);
        assert_eq!(release.asset("a.tar.gz").unwrap().size, 12);
        assert_eq!(release.asset("b.tar.gz").unwrap().size, 0);
        assert!(release.asset("missing").is_none());
    }
}
