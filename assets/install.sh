#!/bin/sh
# install.sh — install echo (the desktop app and the `spotify` terminal command) on Linux and macOS.
#
#   curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- --version 0.4.6
#   curl -fsSL .../install.sh | sh -s -- --uninstall
#
# Both frontends come from one release artifact and land in one directory, which is what makes
# `spotify upgrade` able to replace them afterwards without reinstalling. See
# crates/echo-core/src/update.rs.
#
# Linux takes the portable `echo-linux-x64.tar.gz`; macOS takes the DMG, because a Mac desktop
# app has to be an .app bundle to appear in Launchpad. Either way `spotify` ends up in
# ~/.local/bin, on PATH.

set -eu

REPO="and2049/echo"
API="https://api.github.com/repos/${REPO}"
DOWNLOAD="https://github.com/${REPO}/releases/download"

BIN_HOME="${HOME}/.local/bin"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
LINUX_DIR="$DATA_HOME/echo"
ICON_DIR="$DATA_HOME/icons/hicolor"
DESKTOP_FILE="$DATA_HOME/applications/echo.desktop"

VERSION=""
NO_MODIFY_PATH=0
TMP_DIR=""

usage() {
    cat <<EOF
Install echo — the desktop app plus the \`spotify\` terminal command.

Usage: install.sh [options]

Options:
  -v, --version <version>  Install a specific release (e.g. 0.4.6)
      --no-modify-path     Don't touch your shell config, even if ~/.local/bin is not on PATH
      --uninstall          Remove echo (leaves your config in ~/.config/echo alone)
  -h, --help               Show this message

After installing, upgrade with \`spotify upgrade\` — no need to re-run this script.
EOF
}

err() { printf 'error: %s\n' "$*" >&2; exit 1; }
warn() { printf 'warning: %s\n' "$*" >&2; }
info() { printf '%s\n' "$*"; }

cleanup() {
    if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
    return 0
}

download() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$2" "$1"
    else
        err "this script needs curl or wget."
    fi
}

fetch_stdout() {
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$1"
    else
        wget -qO- "$1"
    fi
}

# --- Platform --------------------------------------------------------------

detect_os() {
    os=$(uname -s)
    case "$os" in
        Linux) OS=linux ;;
        Darwin) OS=macos ;;
        *) err "unsupported operating system: $os. Linux and macOS only — on Windows use install.ps1." ;;
    esac
}

detect_platform() {
    detect_os
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64) ARCH=x64 ;;
        arm64|aarch64) ARCH=arm64 ;;
        *) err "unsupported architecture: $arch." ;;
    esac
    # A Mac binary running under Rosetta reports x86_64 while the machine is Apple Silicon,
    # and releases are arm64-only, so ask the kernel rather than trusting uname.
    if [ "$OS" = macos ] && [ "$ARCH" = x64 ]; then
        if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
            ARCH=arm64
        fi
    fi
    if [ "$OS" = linux ] && [ "$ARCH" != x64 ]; then
        err "no Linux build for $arch — releases are x86_64 only."
    fi
    if [ "$OS" = macos ] && [ "$ARCH" != arm64 ]; then
        err "no macOS build for $arch — releases are Apple Silicon only."
    fi
}

resolve_version() {
    if [ -n "$VERSION" ]; then
        VERSION="${VERSION#v}"
        return
    fi
    # Only the tag is needed, and the response is small — no jq dependency.
    VERSION=$(fetch_stdout "$API/releases/latest" \
        | sed -n 's/.*"tag_name": *"v\{0,1\}\([^"]*\)".*/\1/p' \
        | head -n1)
    [ -n "$VERSION" ] || err "could not work out the latest version from the GitHub API."
}

# --- Install ---------------------------------------------------------------

install_linux() {
    archive="echo-linux-x64.tar.gz"
    info "Downloading $archive ($VERSION)"
    download "$DOWNLOAD/v$VERSION/$archive" "$TMP_DIR/$archive"

    mkdir -p "$TMP_DIR/unpacked"
    tar -xzf "$TMP_DIR/$archive" -C "$TMP_DIR/unpacked"
    [ -f "$TMP_DIR/unpacked/spotify" ] || err "release archive is missing the spotify binary."

    # Replaced wholesale rather than merged, so a release that drops a theme actually drops it.
    rm -rf "$LINUX_DIR"
    mkdir -p "$LINUX_DIR" "$BIN_HOME"
    cp -R "$TMP_DIR/unpacked/." "$LINUX_DIR/"
    chmod +x "$LINUX_DIR/spotify"
    ln -sf "$LINUX_DIR/spotify" "$BIN_HOME/spotify"

    if [ -f "$LINUX_DIR/echo-desktop" ]; then
        chmod +x "$LINUX_DIR/echo-desktop"
        ln -sf "$LINUX_DIR/echo-desktop" "$BIN_HOME/echo-desktop"
        install_desktop_entry
    fi

    INSTALLED_AT="$LINUX_DIR"
}

# Menu entry and icons for the desktop app. Icons ride along in the archive so this script
# needs exactly one download.
#
# The file's basename, `Icon=`, and `StartupWMClass=` all have to agree with the `app_id` the
# window sets (crates/echo-desktop/src/main.rs) — that is the string a Linux desktop matches a
# running window against to find its icon and name. Disagree and the launcher entry still looks
# right while the running window shows a blank icon labelled "Unknown".
install_desktop_entry() {
    mkdir -p "$(dirname "$DESKTOP_FILE")"
    for size in 32x32 64x64 128x128; do
        if [ -f "$LINUX_DIR/icons/$size.png" ]; then
            mkdir -p "$ICON_DIR/$size/apps"
            cp -f "$LINUX_DIR/icons/$size.png" "$ICON_DIR/$size/apps/echo.png"
        fi
    done
    if [ -f "$LINUX_DIR/icons/128x128@2x.png" ]; then
        mkdir -p "$ICON_DIR/256x256/apps"
        cp -f "$LINUX_DIR/icons/128x128@2x.png" "$ICON_DIR/256x256/apps/echo.png"
    fi

    cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Name=echo
Comment=Spotify client and music player
Exec=$LINUX_DIR/echo-desktop
Icon=echo
Terminal=false
Type=Application
Categories=Audio;Music;Player;
StartupWMClass=echo
EOF

    command -v update-desktop-database >/dev/null 2>&1 &&
        update-desktop-database "$(dirname "$DESKTOP_FILE")" 2>/dev/null || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 &&
        gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true
}

# /Applications belongs to the `admin` group, so a standard account cannot write to it. Such
# a user gets ~/Applications, which Launchpad and Spotlight index just the same.
mac_app_dir() {
    if [ -w /Applications ]; then
        printf '%s' "/Applications"
    else
        printf '%s' "$HOME/Applications"
    fi
}

install_macos() {
    dmg="echo_${VERSION}_aarch64.dmg"
    info "Downloading $dmg"
    download "$DOWNLOAD/v$VERSION/$dmg" "$TMP_DIR/$dmg"

    mount="$TMP_DIR/mnt"
    mkdir -p "$mount"
    hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mount" "$TMP_DIR/$dmg" ||
        err "could not mount $dmg."
    # shellcheck disable=SC2064  # $mount is fixed here and must expand now, not at trap time.
    trap "hdiutil detach -quiet '$mount' 2>/dev/null || true; cleanup" EXIT

    [ -d "$mount/echo.app" ] || err "$dmg does not contain echo.app."

    app_dir=$(mac_app_dir)
    mkdir -p "$app_dir"
    app="$app_dir/echo.app"
    # Staged beside the destination and swapped in, so a copy that fails partway leaves the
    # installed app untouched rather than deleted.
    staging="$app_dir/.echo-install-$$.app"
    rm -rf "$staging"
    cp -R "$mount/echo.app" "$staging" || {
        rm -rf "$staging"
        err "could not copy echo.app into $app_dir."
    }
    hdiutil detach -quiet "$mount" 2>/dev/null || true
    trap cleanup EXIT

    # Curl does not set the quarantine attribute, but a browser-downloaded DMG passed to this
    # script would; clearing it keeps Gatekeeper from refusing an ad-hoc signed bundle.
    xattr -dr com.apple.quarantine "$staging" 2>/dev/null || true

    rm -rf "$app"
    mv "$staging" "$app"

    mkdir -p "$BIN_HOME"
    ln -sf "$app/Contents/MacOS/spotify" "$BIN_HOME/spotify"

    INSTALLED_AT="$app"
}

# --- PATH ------------------------------------------------------------------

on_path() {
    case ":$PATH:" in
        *":$BIN_HOME:"*) return 0 ;;
        *) return 1 ;;
    esac
}

add_to_path() {
    if on_path; then
        return 0
    fi
    if [ "$NO_MODIFY_PATH" -eq 1 ]; then
        warn "$BIN_HOME is not on your PATH. Add it with:
  export PATH=\"$BIN_HOME:\$PATH\""
        return 0
    fi

    shell_name=$(basename "${SHELL:-sh}")
    case "$shell_name" in
        fish)
            config="${XDG_CONFIG_HOME:-$HOME/.config}/fish/config.fish"
            line="fish_add_path $BIN_HOME"
            ;;
        zsh)
            config="$HOME/.zshrc"
            line="export PATH=\"$BIN_HOME:\$PATH\""
            ;;
        *)
            config="$HOME/.bashrc"
            [ -f "$config" ] || config="$HOME/.profile"
            line="export PATH=\"$BIN_HOME:\$PATH\""
            ;;
    esac

    mkdir -p "$(dirname "$config")"
    if [ -f "$config" ] && grep -Fq "$BIN_HOME" "$config"; then
        return 0
    fi
    printf '\n# echo\n%s\n' "$line" >> "$config" ||
        { warn "could not write to $config. Add this yourself:
  $line"; return 0; }
    info "Added $BIN_HOME to your PATH in $config"
    PATH_CHANGED=1
}

# --- Uninstall -------------------------------------------------------------

uninstall() {
    detect_os
    rm -f "$BIN_HOME/spotify" "$BIN_HOME/echo-desktop"
    if [ "$OS" = macos ]; then
        # Both, because which one an install picked depends on whether the account is an admin.
        rm -rf "/Applications/echo.app" "$HOME/Applications/echo.app"
    else
        rm -rf "$LINUX_DIR"
        rm -f "$DESKTOP_FILE"
        for size in 32x32 64x64 128x128 256x256; do
            rm -f "$ICON_DIR/$size/apps/echo.png"
        done
        command -v update-desktop-database >/dev/null 2>&1 &&
            update-desktop-database "$(dirname "$DESKTOP_FILE")" 2>/dev/null || true
    fi
    info "echo has been uninstalled. Your settings in ~/.config/echo were left alone."
}

# --- Main ------------------------------------------------------------------

while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        -v|--version)
            [ $# -ge 2 ] || err "--version needs a version, e.g. --version 0.4.6"
            VERSION="$2"; shift 2 ;;
        --version=*) VERSION="${1#--version=}"; shift ;;
        --no-modify-path) NO_MODIFY_PATH=1; shift ;;
        --uninstall) uninstall; exit 0 ;;
        *) err "unknown option: $1 (try --help)" ;;
    esac
done

if [ "$(id -u)" -eq 0 ]; then
    err "refusing to run as root — echo installs into your home directory."
fi

detect_platform
resolve_version

TMP_DIR=$(mktemp -d)
trap cleanup EXIT

PATH_CHANGED=0
if [ "$OS" = macos ]; then
    install_macos
else
    install_linux
fi
add_to_path

info ""
info "echo $VERSION is installed."
info "  App:      $INSTALLED_AT"
info "  Terminal: $BIN_HOME/spotify"
info ""
if [ "$PATH_CHANGED" -eq 1 ]; then
    info "Open a new terminal, then run 'spotify' to start."
else
    info "Run 'spotify' to start."
fi
info "Later on, 'spotify upgrade' updates both the terminal client and the app."
