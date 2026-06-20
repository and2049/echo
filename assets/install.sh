#!/bin/sh
# install.sh — Install echo as a desktop application on Linux.
#
# Usage:
#   ./install.sh                         # auto-detect AppImage or ./echo binary
#   ./install.sh /path/to/Echo.AppImage  # install a specific AppImage
#   ./install.sh /path/to/echo           # install a specific binary
#   ./install.sh --uninstall             # remove echo
#
# Remote (curl) usage:
#   curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
#   curl -fsSL .../install.sh | sh -s -- /path/to/Echo.AppImage
#   curl -fsSL .../install.sh | sh -s -- --uninstall

set -eu

BINARY_NAME="echo"
ICON_NAME="echo"
APP_NAME="Echo"
REPO="and2049/echo"
RAW_BASE="https://raw.githubusercontent.com/${REPO}/main"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

find_repo_root() {
    dir=$1
    while [ "$dir" != "/" ]; do
        if [ -f "$dir/assets/echo.desktop" ] && [ -d "$dir/icons" ]; then
            echo "$dir"
            return 0
        fi
        dir=$(dirname "$dir")
    done
    return 1
}

REPO_ROOT=$(find_repo_root "$SCRIPT_DIR") || true
IS_LOCAL=0
if [ -n "$REPO_ROOT" ]; then
    IS_LOCAL=1
fi

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_HOME="${HOME}/.local/bin"

ICON_DIR="$DATA_HOME/icons/hicolor"
APP_DIR="$DATA_HOME/applications"
BIN_PATH="$BIN_HOME/$BINARY_NAME"
DESKTOP_PATH="$APP_DIR/$BINARY_NAME.desktop"

TMP_DIR=""
REMOTE=0

usage() {
    cat <<EOF
Usage: $0 [OPTIONS] [BINARY]

Install echo as a desktop application on Linux.

Arguments:
  BINARY            Path to an AppImage or echo binary to install.
                    If omitted, auto-detects ./echo-*.AppImage or ./echo,
                    or downloads the latest AppImage from GitHub releases.

Options:
  --uninstall       Remove echo and its desktop integration files.
  -h, --help        Show this help message.
EOF
}

err() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

warn() {
    printf 'warning: %s\n' "$*" >&2
}

info() {
    printf '%s\n' "$*"
}

cleanup() {
    if [ -n "$TMP_DIR" ] && [ -d "$TMP_DIR" ]; then
        rm -rf "$TMP_DIR"
    fi
}

download() {
    url=$1
    dest=$2
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$dest" "$url"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$dest" "$url"
    else
        err "Neither curl nor wget is installed. Please install one and retry."
    fi
}

setup_remote_assets() {
    if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
        err "Remote mode requires curl or wget. Please install one and retry."
    fi

    REMOTE=1
    TMP_DIR=$(mktemp -d)
    trap cleanup EXIT

    mkdir -p "$TMP_DIR/icons" "$TMP_DIR/assets"

    info "Downloading icons and desktop entry..."
    download "$RAW_BASE/icons/32x32.png"        "$TMP_DIR/icons/32x32.png"        || true
    download "$RAW_BASE/icons/64x64.png"        "$TMP_DIR/icons/64x64.png"        || true
    download "$RAW_BASE/icons/128x128.png"      "$TMP_DIR/icons/128x128.png"      || true
    download "$RAW_BASE/icons/128x128@2x.png"   "$TMP_DIR/icons/128x128@2x.png"   || true
    download "$RAW_BASE/assets/echo-rs.svg"     "$TMP_DIR/assets/echo-rs.svg"     || true
    download "$RAW_BASE/assets/echo.desktop"    "$TMP_DIR/assets/echo.desktop"    || err "Failed to download desktop entry."

    REPO_ROOT="$TMP_DIR"
}

download_latest_appimage() {
    info "Finding latest echo AppImage from GitHub releases..."
    API_URL="https://api.github.com/repos/${REPO}/releases/latest"
    API_JSON="$TMP_DIR/release.json"
    download "$API_URL" "$API_JSON" || err "Failed to query GitHub releases API."

    APPIMAGE_URL=$(grep "browser_download_url.*AppImage" "$API_JSON" | head -n1 | cut -d'"' -f4)

    if [ -z "$APPIMAGE_URL" ]; then
        err "Could not find an AppImage in the latest release."
    fi

    info "Downloading $APPIMAGE_URL"
    download "$APPIMAGE_URL" "$REPO_ROOT/echo-latest.AppImage"
    echo "$REPO_ROOT/echo-latest.AppImage"
}

# --- Uninstall -------------------------------------------------------------

uninstall() {
    info "Removing echo desktop integration..."
    rm -f "$BIN_PATH"
    rm -f "$DESKTOP_PATH"
    rm -f "$ICON_DIR/32x32/apps/$ICON_NAME.png"
    rm -f "$ICON_DIR/64x64/apps/$ICON_NAME.png"
    rm -f "$ICON_DIR/128x128/apps/$ICON_NAME.png"
    rm -f "$ICON_DIR/256x256/apps/$ICON_NAME.png"
    rm -f "$ICON_DIR/scalable/apps/$ICON_NAME.svg"

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$APP_DIR" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true
    fi

    info "echo has been uninstalled."
}

# --- Install ---------------------------------------------------------------

install() {
    if [ "$(id -u)" -eq 0 ]; then
        err "Refusing to run as root. This script installs to your user directory (~/.local)."
    fi

    if [ "$IS_LOCAL" -eq 0 ]; then
        setup_remote_assets
    fi

    SRC=""
    if [ $# -ge 1 ]; then
        SRC=$1
    elif [ "$IS_LOCAL" -eq 1 ]; then
        if ls "$REPO_ROOT"/echo-*.AppImage >/dev/null 2>&1; then
            SRC=$(ls "$REPO_ROOT"/echo-*.AppImage | head -n1)
        elif [ -f "$REPO_ROOT/$BINARY_NAME" ]; then
            SRC="$REPO_ROOT/$BINARY_NAME"
        else
            err "Could not find an AppImage or binary to install.
Pass the path explicitly:  $0 /path/to/Echo.AppImage"
        fi
    elif [ "$REMOTE" -eq 1 ]; then
        SRC=$(download_latest_appimage)
    else
        err "Could not find an AppImage or binary to install.
Pass the path explicitly:  $0 /path/to/Echo.AppImage"
    fi

    if [ ! -f "$SRC" ]; then
        err "File not found: $SRC"
    fi

    case "$SRC" in
        *.AppImage)
            if command -v lsb_release >/dev/null 2>&1; then
                DIST=$(lsb_release -is 2>/dev/null || true)
                case "$DIST" in
                    Ubuntu|LinuxMint|Pop)
                        if ! ldconfig -p 2>/dev/null | grep -q libfuse.so.2; then
                            warn "AppImage requires libfuse2, which is not installed.
Install it with:  sudo apt-get install libfuse2
(On Ubuntu 24.04+ the package may be named libfuse2t64.)"
                        fi
                        ;;
                esac
            fi
            ;;
    esac

    mkdir -p "$BIN_HOME" "$APP_DIR"
    mkdir -p "$ICON_DIR/32x32/apps" \
             "$ICON_DIR/64x64/apps" \
             "$ICON_DIR/128x128/apps" \
             "$ICON_DIR/256x256/apps" \
             "$ICON_DIR/scalable/apps"

    info "Installing binary to $BIN_PATH"
    cp -f "$SRC" "$BIN_PATH"
    chmod +x "$BIN_PATH"

    ICONS_SRC="$REPO_ROOT/icons"
    SVG_SRC="$REPO_ROOT/assets/echo-rs.svg"

    if [ -f "$ICONS_SRC/32x32.png" ]; then
        cp -f "$ICONS_SRC/32x32.png" "$ICON_DIR/32x32/apps/$ICON_NAME.png"
    fi
    if [ -f "$ICONS_SRC/64x64.png" ]; then
        cp -f "$ICONS_SRC/64x64.png" "$ICON_DIR/64x64/apps/$ICON_NAME.png"
    fi
    if [ -f "$ICONS_SRC/128x128.png" ]; then
        cp -f "$ICONS_SRC/128x128.png" "$ICON_DIR/128x128/apps/$ICON_NAME.png"
    fi
    if [ -f "$ICONS_SRC/128x128@2x.png" ]; then
        cp -f "$ICONS_SRC/128x128@2x.png" "$ICON_DIR/256x256/apps/$ICON_NAME.png"
    fi
    if [ -f "$SVG_SRC" ]; then
        cp -f "$SVG_SRC" "$ICON_DIR/scalable/apps/$ICON_NAME.svg"
    fi

    info "Installing desktop entry to $DESKTOP_PATH"
    DESKTOP_SRC="$REPO_ROOT/assets/echo.desktop"
    if [ -f "$DESKTOP_SRC" ]; then
        cp -f "$DESKTOP_SRC" "$DESKTOP_PATH"
    else
        cat > "$DESKTOP_PATH" <<EOF
[Desktop Entry]
Name=$APP_NAME
Comment=Terminal-based Spotify client and music player
Exec=$BINARY_NAME
Icon=$ICON_NAME
Terminal=true
Type=Application
Categories=Audio;Music;Player;
EOF
    fi

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$APP_DIR" 2>/dev/null || true
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -f -t "$ICON_DIR" 2>/dev/null || true
    fi

    info ""
    info "echo has been installed!"
    info "  Binary:   $BIN_PATH"
    info "  Launcher: $DESKTOP_PATH"
    info ""
    info "Search for '$APP_NAME' in your applications menu, or run '$BINARY_NAME' from a terminal."
    info "Make sure $BIN_HOME is in your PATH (it usually is on modern Linux distributions)."
    if ! echo "$PATH" | tr ':' '\n' | grep -qx "$BIN_HOME"; then
        warn "$BIN_HOME is not in your PATH. Add it with:
  export PATH=\"$BIN_HOME:\$PATH\""
    fi
}

# --- Argument parsing ------------------------------------------------------

if [ $# -eq 0 ]; then
    install
    exit 0
fi

case "$1" in
    --uninstall)
        uninstall
        ;;
    -h|--help)
        usage
        ;;
    --*)
        err "Unknown option: $1
Run '$0 --help' for usage."
        ;;
    *)
        install "$1"
        ;;
esac
