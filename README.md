<p align="center">
    <a href="">
      <picture>
        <img src="assets\echo-rs.svg" alt="ECHO-RS">
      </picture>
    </a>
</p>
<p align="center">
    <a href="README.md">English</a> |
    <a href="README.zh.md">简体中文</a> |
    <a href="README.zht.md">繁體中文</a>
</p>

echo is a terminal-based music player and Spotify client written in Rust. echo brings your local files and entire Spotify library, liked songs, playlists, and playback controls directly to your terminal with a beautiful, dynamic TUI featuring native image rendering.

![demo](demo.png)

## Features

- **Terminal Image Support**: Renders high-quality album art and playlist covers directly in your terminal (supports Kitty, Sixel, and block fallbacks).
- **Blazing Fast Liked Songs**: Uses a global caching architecture. Your entire Liked Songs library is cached locally (`~/.config/echo/cache.json`) for zero-latency, rate-limit-free scrolling, even with thousands of saved tracks.
- **Library Management**: Create, rename, delete, and organize playlists into folders.
- **Local Music Support**: Scan a local music folder, play local files, and create local playlists that can also reference Spotify tracks.
- **Responsive Playback Controls**: Full control over playback, queue, shuffle, repeat, and volume.
- **Search**: Fast global search for Spotify catalog items and scanned local tracks.

## Setup

1. **Spotify Premium**: A Spotify Premium account is required to use the Spotify Web API for playback control.
2. **Spotify Developer App**: 
   - Go to the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard/).
   - Create an app and get your `Client ID` and `Client Secret`.
   - Add `http://127.0.0.1:8888/callback` to your app's Redirect URIs.
   - Echo also uses `http://127.0.0.1:8989/login` for its internal first-party Spotify session.

### Installation

Download and run installer:
https://github.com/and2049/echo/releases

### AppImage Setup (Linux)

On Ubuntu 22.04+ the AppImage runtime requires `libfuse2`:

```bash
sudo apt-get install libfuse2
```

**Install with one command** (downloads the latest AppImage and sets up desktop integration):

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
```

To uninstall:

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --uninstall
```

**Or** if you already have the AppImage downloaded, run the included install script:

```bash
./install.sh /path/to/Echo.AppImage
```

To remove:

```bash
./install.sh --uninstall
```

### Build from Source

Clone the repository and build using Cargo:

**Linux dependencies** (Ubuntu/Debian):

```bash
sudo apt-get install -y --no-install-recommends \
  libasound2-dev libdbus-1-dev pkg-config libssl-dev
```

```bash
git clone https://github.com/and2049/echo.git
cd echo
cargo build --release
```

Run the binary:

```bash
./target/release/echo
```

On first run, echo will prompt you to enter your `Client ID` and `Client Secret`, then open your browser to authenticate with Spotify.

## Navigation & Keybindings

echo is heavily keyboard-driven. 

### Global Navigation
- `j` / `k` or `Down` / `Up`: Move down / up
- `Enter` or `z`: Select item / Open playlist / Play track
- `h` / `q` / `Esc` / `Backspace`: Go back / Close modal / Clear search
- `Tab`: Switch tabs (e.g., Playlists ↔ Albums, Search Tracks ↔ Search Albums)
- `:`: Enter Command Mode
- `/`: Search within tracklist
- `f`: Global search
- `n` / `N`: Jump to next / previous search match within a list

### Playback Controls
- `Space`: Play / Pause
- `]` / `>`: Next Track
- `[` / `<`: Previous Track
- `s`: Toggle Shuffle
- `r`: Toggle Repeat Mode (Off → Track → Context)
- `=` / `-`: Volume Up / Down (by 1%)
- `+` / `_`: Volume Up / Down (by 5%)
- `D` (Shift + d): Open Device Selection menu
- `L` (Shift + l): Toggle full-screen Synced Lyrics modal
- `Ctrl + Shift + L`: Toggle condensed Synced Lyrics view

### Track & Library Actions
- `l`: Like / Unlike the selected track
- `A` (Shift + a): Open action menu for hovered track (or currently playing if not focused in track page)
- `p`: paste a cut playlist into a folder
- `a`: Add selected track to playlist / Add selected album to library
- `q`: Add currently hovered track to Queue
- `Q` (Shift + q): Open Queue view
- `m`: Pin / Unpin a playlist
- `c`: Quick shortcut to create a new playlist
- `e`: Quick shortcut to rename a playlist or folder
- `v`: Enter Visual mode for multi-selection
- `d` (double press): Delete playlist/folder, or remove a track from your custom playlist
- `x`: Cut playlist (to move into a folder)
- `R` (Shift + r): Force refresh

## Commands
While in Command Mode (`:`), you can use the following:
- `:search <query>`: Search for tracks or albums.
- `:newplaylist <name>`: Create a new playlist.
- `:newlocalplaylist <name>`: Create a local playlist stored on this machine.
- `:localpath <absolute-folder-path>`: Set the local music folder and scan it. The path must be absolute and works on macOS, Windows, and Linux.
- `:rescanlocal`: Rescan the configured local music folder.
- `:newfolder <name>`: Create a new folder to organize playlists.
- `:delfolder`: Delete the currently selected folder.
- `:rename <name>`: Rename the currently selected playlist or folder.
- `:sort <alpha|creator>`: Sort your library.
- `:theme <theme_name>`: Switch application theme.
- `:lang <en|zh|zh-CN>`: Switch language.
- `:album`: Jump to the album of the currently selected track.
- `:queue`: Open the Queue view.
- `:vis`: Toggle the audio visualizer.
- `:visbins <number>`: Set the number of audio visualizer frequency bins (5-32).
- `:pixelate <pixels>`: Enable retro 8-bit aesthetic on album covers. Set to 0 to disable, or e.g., 16 for a pixelated look.
- `:index <number>`: Set track index base (1-indexed vs 0-indexed).
- `:quit`, `:q`, `:qa`, `:wq`: Exit the application.

## Local Music

Local support is separate from Spotify. Use `:localpath <absolute-folder-path>` to choose the folder echo should scan. Supported audio extensions are `mp3`, `wav`, `flac`, `ogg`, `m4a`, and `aac`; echo scans recursively and reads title, artist, album, duration, and artwork when available. Echo refreshes the configured local folder on startup and watches it for supported audio/artwork changes while running; `:rescanlocal` is still available as a manual fallback.

Local playlists are stored locally and are not Spotify playlists. They can contain local tracks and Spotify track references. Spotify playlists cannot contain local tracks. Local shuffle, repeat, volume, queue, and play/pause are handled by echo's local playback engine.

Embedded artwork is used when available. If a track has no embedded artwork, echo looks for folder artwork such as `cover.jpg`, `folder.jpg`, or `front.png`.

## Troubleshooting
- **Theme color rendering issues (Windows)**: Disable "Adjust indistinguishable text" in the Appearance settings of the Defaults profile. 
- **Images not rendering**: Ensure your terminal supports the Kitty image protocol or Sixel graphics (e.g., Kitty, WezTerm, Alacritty with patches). echo will fall back to block rendering if neither is supported.
- **Cache desync**: If your Liked Songs are out of sync with other devices, simply restart echo. It eagerly syncs your library in the background on startup.
- **Local file missing**: If a file was deleted or moved after scanning, run `:rescanlocal` to refresh the local library.
- **Configuration Path**: `~/.config/echo/config.toml` (holds tokens and preferences), `~/.config/echo/cache.json` (holds liked tracks), `~/.config/echo/local_library.json`, and `~/.config/echo/local_playlists.json`.
