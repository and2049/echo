#!/bin/sh
# install.sh — Install echo as a desktop application on Linux.
#
# Usage:
#   ./install.sh                         # auto-detect AppImage or ./echo binary
#   ./install.sh /path/to/Echo.AppImage  # install a specific AppImage
#   ./install.sh /path/to/echo           # install a specific binary
#   ./install.sh --uninstall             # remove echo

set -eu

BINARY_NAME="echo"
ICON_NAME="echo"
APP_NAME="Echo"

# Resolve the directory where this script lives (for locating bundled assets)
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

# XDG user directories (default to ~/.local)
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
BIN_HOME="${HOME}/.local/bin"

ICON_DIR="$DATA_HOME/icons/hicolor"
APP_DIR="$DATA_HOME/applications"
BIN_PATH="$BIN_HOME/$BINARY_NAME"
DESKTOP_PATH="$APP_DIR/$BINARY_NAME.desktop"

usage() {
    cat <<EOF
Usage: $0 [OPTIONS] [BINARY]

Install echo as a desktop application on Linux.

Arguments:
  BINARY            Path to an AppImage or echo binary to install.
                    If omitted, auto-detects ./echo-*.AppImage or ./echo.

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
    # Refuse to run as root — installing to ~/.local as root is almost always wrong.
    if [ "$(id -u)" -eq 0 ]; then
        err "Refusing to run as root. This script installs to your user directory (~/.local)."
    fi

    # Locate the binary to install.
    SRC=""
    if [ $# -ge 1 ]; then
        SRC=$1
    elif ls "$SCRIPT_DIR"/echo-*.AppImage >/dev/null 2>&1; then
        SRC=$(ls "$SCRIPT_DIR"/echo-*.AppImage | head -n1)
    elif [ -f "$SCRIPT_DIR/$BINARY_NAME" ]; then
        SRC="$SCRIPT_DIR/$BINARY_NAME"
    else
        err "Could not find an AppImage or binary to install.
Pass the path explicitly:  $0 /path/to/Echo.AppImage"
    fi

    if [ ! -f "$SRC" ]; then
        err "File not found: $SRC"
    fi

    # Warn about libfuse2 for AppImage on Ubuntu >= 22.04
    case "$SRC" in
        *.AppImage)
            if command -v lsb_release >/dev/null 2>&1; then
                DIST=$(lsb_release -is 2>/dev/null || true)
                REL=$(lsb_release -rs 2>/dev/null || true)
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

    # Create target directories.
    mkdir -p "$BIN_HOME" "$APP_DIR"
    mkdir -p "$ICON_DIR/32x32/apps" \
             "$ICON_DIR/64x64/apps" \
             "$ICON_DIR/128x128/apps" \
             "$ICON_DIR/256x256/apps" \
             "$ICON_DIR/scalable/apps"

    # Install the binary/AppImage.
    info "Installing binary to $BIN_PATH"
    cp -f "$SRC" "$BIN_PATH"
    chmod +x "$BIN_PATH"

    # Install icons.
    # Icons live in the repo at icons/ and assets/echo-rs.svg.
    ICONS_SRC="$SCRIPT_DIR/icons"
    SVG_SRC="$SCRIPT_DIR/assets/echo-rs.svg"

    if [ -f "$ICONS_SRC/32x32.png" ]; then
        cp -f "$ICONS_SRC/32x32.png" "$ICON_DIR/32x32/apps/$ICON_NAME.png"
    fi
    if [ -f "$ICONS_SRC/64x64.png" ]; then
        cp -f "$ICONS_SRC/64x64.png" "$ICON_DIR/64x64/apps/$ICON_NAME.png"
    fi
    if [ -f "$ICONS_SRC/128x128.png" ]; then
        cp -f "$ICONS_SRC/128x128.png" "$ICON_DIR/128x128/apps/$ICON_NAME.png"
    fi
    # 128x128@2x.png is 256x256 effective — install as 256x256.
    if [ -f "$ICONS_SRC/128x128@2x.png" ]; then
        cp -f "$ICONS_SRC/128x128@2x.png" "$ICON_DIR/256x256/apps/$ICON_NAME.png"
    fi
    if [ -f "$SVG_SRC" ]; then
        cp -f "$SVG_SRC" "$ICON_DIR/scalable/apps/$ICON_NAME.svg"
    fi

    # Install .desktop file.
    info "Installing desktop entry to $DESKTOP_PATH"
    DESKTOP_SRC="$SCRIPT_DIR/assets/echo.desktop"
    if [ -f "$DESKTOP_SRC" ]; then
        cp -f "$DESKTOP_SRC" "$DESKTOP_PATH"
    else
        # Fallback: generate a minimal desktop file inline.
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

    # Refresh desktop and icon caches if the tools are available.
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
