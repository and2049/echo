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
- **What's New**: A feed of recent albums and singles from the artists you follow, built from cached artist data and refreshed at most every 6 hours.

## Setup

1. **Spotify Premium**: A Spotify Premium account is required to use the Spotify Web API for playback control.
2. **Spotify Developer App**: 
   - Go to the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard/).
   - Create an app and get your `Client ID` and `Client Secret`.
   - Add `http://127.0.0.1:8888/callback` to your app's Redirect URIs.
   - echo also uses `http://127.0.0.1:8989/login` for its internal first-party Spotify session.

### Installation

One command, one install: the desktop app **and** the `spotify` terminal command.

**Linux and macOS**

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
```

**Windows** (PowerShell)

```powershell
irm https://github.com/and2049/echo/releases/latest/download/install.ps1 | iex
```

Neither needs administrator rights. Both put `spotify` on your `PATH` — open a new terminal afterwards — and add the desktop app to your Start menu, Launchpad, or applications menu.

| Platform | Where it lands |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\echo` (installed from the release MSI, x64) |
| macOS | `/Applications/echo.app`, with `spotify` linked into `~/.local/bin` (Apple Silicon) |
| Linux | `~/.local/share/echo`, with both commands linked into `~/.local/bin` (x86_64) |

To pin a version or remove echo:

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --version 0.4.6
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh -s -- --uninstall
```

```powershell
& ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Version 0.4.6
& ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Uninstall
```

Uninstalling leaves your settings in `~/.config/echo` alone.

On Linux the desktop app links against a few system libraries — on Debian/Ubuntu:

```bash
sudo apt-get install libasound2 libdbus-1-3 libssl3 \
  libfontconfig1 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libx11-xcb1
```

A desktop install already has most of these. Rendering prefers Vulkan and falls back to
OpenGL, so `libvulkan1` plus your GPU's driver is worth having but is not required.

#### Updating

After the first install, echo updates itself — no reinstall, no administrator rights:

```bash
spotify upgrade          # upgrade to the latest release
spotify upgrade --check  # only report whether one is available
spotify upgrade 0.4.6    # move to a specific version
```

The desktop app does the same from **Settings → Updates → Check for updates**. Both swap the binaries and bundled themes in place and ask you to restart.

### Build from Source

Clone the repository and build using Cargo:

**Linux dependencies** (Ubuntu/Debian):

```bash
sudo apt-get install -y --no-install-recommends \
  libasound2-dev libdbus-1-dev pkg-config libssl-dev \
  libfontconfig-dev libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev
```

```bash
git clone https://github.com/and2049/echo.git
cd echo
cargo build --release
```

Run the binary:

```bash
./target/release/spotify
```

> The terminal command is `spotify` — the previous name collided with the shell builtin `echo`. Configuration and caches still live in `~/.config/echo/`, so existing setups keep working after the rename. On Windows, if the official Spotify client's directory happens to be on your `PATH`, make sure `%USERPROFILE%\.cargo\bin` (or wherever you installed this binary) comes first.

On first run, echo will prompt you to enter your `Client ID` and `Client Secret`, then open your browser to authenticate with Spotify.

## Navigation & Keybindings

echo is heavily keyboard-driven. 

### Global Navigation
- `j` / `k` or `Down` / `Up`: Move down / up
- `gg` / `G`: Jump to the first / last item
- `Ctrl-b` / `Ctrl-f` or `Page Up` / `Page Down`: Move one page
- `Ctrl-u` / `Ctrl-d`: Move half a page
- `Ctrl-l`: Clear and fully redraw the TUI
- `gc`: Jump to the currently playing track or its available context
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
- `,` / `.`: Seek backward / forward by 5 seconds
- `0`: Seek to the start of the current track
- `M` (Shift + m): Mute / restore the previous volume
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
- `T` (Shift + t): Toggle library thumbnails (cover art next to playlist / album names)
- `c`: Quick shortcut to create a new playlist
- `e`: Quick shortcut to rename a playlist or folder
- `v`: Enter Visual mode for multi-selection
- `d` (double press): Delete playlist/folder, or remove a track from your custom playlist
- `x`: Cut playlist (to move into a folder)
- `J` / `K` (Shift + j / k, desktop): Move the selected track down / up within one of your own playlists (drag-and-drop works too); requires the original sort order
- `R` (Shift + r): Force refresh

The track action menu adapts to the source. Spotify tracks support link copying, liking, and album library actions. Local tracks support copying their absolute path and revealing the file in the platform file manager. Both sources retain album/artist navigation, playlist insertion, and queue actions where applicable.

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
- `:sort <alpha|creator>`: Sort the playlist library.
- `:sort <original|title|artist|album|duration|added|reverse>`: Sort the active track list entirely in memory.
- `:seek <seconds|+seconds|-seconds>`: Seek to an absolute position or by a relative offset.
- `:sleep <30m|1h|off>`: Pause playback after a delay (sleep timer).
- `:mute`: Mute playback or restore the previous volume.
- `:open [spotify-url-or-uri]`: Open a Spotify track, album, artist, or playlist. With no argument, read it from the clipboard.
- `:relative <on|off|toggle>`: Configure Vim-style relative line numbers in track lists.
- `:redraw`: Clear and fully redraw the TUI after unexpected terminal output.
- `:theme <theme_name>`: Switch application theme.
- `:lang <en|zh|zh-CN>`: Switch language.
- `:album`: Jump to the album of the currently selected track.
- `:queue`: Open the Queue view.
- `:vis`: Toggle the audio visualizer.
- `:visbins <number>`: Set the number of audio visualizer frequency bins (5-32).
- `:pixelate <pixels>`: Enable retro 8-bit aesthetic on album covers. Set to 0 to disable, or e.g., 16 for a pixelated look.
- `:thumbs [on|off]`: Toggle cover-art thumbnails in the library sidebar. Covers are cached in `~/.config/echo/thumbs/` so they load instantly on later launches.
- `:index <number>`: Set track index base (1-indexed vs 0-indexed).
- `:quit`, `:q`, `:qa`, `:wq`: Exit the application.

## Custom Keybindings

Add a `keybindings` table under `[library]` in `~/.config/echo/config.toml` to override or add semantic mappings. Single keys, modifier keys such as `ctrl-f`, and two-key sequences are supported. Unmapped keys keep echo's defaults.

```toml
[library.keybindings]
"s d" = "sort_duration"
"s a" = "sort_artist"
"ctrl-j" = "half_page_down"
"ctrl-k" = "half_page_up"
";" = "seek_forward"
```

Available actions are `first`, `last`, `page_up`, `page_down`, `half_page_up`, `half_page_down`, `current_context`, `play_pause`, `next`, `previous`, `shuffle`, `repeat`, `seek_backward`, `seek_forward`, `seek_start`, `mute`, `sort_original`, `sort_title`, `sort_artist`, `sort_album`, `sort_duration`, `sort_added`, `reverse_tracks`, `redraw`, and `toggle_thumbnails`.

Track sorting and navigation operate on already-loaded data. They do not issue Spotify requests. Navigation history retains up to 20 in-memory views so returning to a previous track list normally does not refetch it.

## Themes

Themes live in `themes/*.toml` as a flat list: nine base colors followed by the twelve derived colors the desktop app paints, every one explicit with a comment saying what it drives. Edit values freely, or change base colors and run `python themes/generate_desktop.py` to recompute the derived ones. Derived keys are optional — a missing key is computed with the formula named in its comment, and a `[desktop]` table is also accepted for overrides. To iterate visually, `python tools/theme-preview/serve.py` opens a live mock of the desktop window in the browser that repaints on every save — no rebuild needed. Colors can be edited in either direction: change the toml in your editor, or click any color in the preview's legend to adjust it with a picker that writes straight back to the file (its "recompute derived" button re-runs the generator for the current theme).

## Audio Quality

echo streams at 320 kbps and applies volume normalisation, matching the Spotify desktop app's defaults. These live under `[library]` in `~/.config/echo/config.toml` and take effect on the next launch.

```toml
[library]
bitrate = 320               # 96, 160, or 320
normalisation = true        # Even out loudness between tracks, like the Spotify app.
normalisation_pregain = 3.0 # dB added back after normalisation. Raise if playback is too quiet.
```

Normalisation attenuates each track by its ReplayGain value, which is typically several dB on modern masters. `normalisation_pregain` adds that headroom back so playback lands at a comparable level to the Spotify app. The gain is applied ahead of librespot's dynamic limiter, so raising it does not clip. Setting `normalisation = false` skips the gain stage entirely for bit-exact full-scale output, at the cost of loudness jumps between tracks.

Volume is applied entirely client-side — Spotify streams arrive at full scale, and echo attenuates them itself, so the device's volume slider in other Spotify clients is inactive. Both Spotify and local playback use the same cubic volume curve, so a given percentage sounds the same whichever source is playing, and 100% is unity gain on both.

echo opens the output device as stereo at 44.1 kHz — librespot's native rate, so no resampling — whenever the device supports it. Devices that do not offer 44.1 kHz (most Windows endpoints default to 48 kHz) fall back to the device's own default rate.

The endpoint actually opened is written to `echo-debug-audio-spotify.log` in the working directory, and `echo-debug-audio-local.log` for local files:

```
device=Headphones (WH-1000XM5) channels=2 sample_rate=48000 format=F32
```

## Local Music

Local support is separate from Spotify. Use `:localpath <absolute-folder-path>` to choose the folder echo should scan. Supported audio extensions are `mp3`, `wav`, `flac`, `ogg`, `m4a`, and `aac`; echo scans recursively and reads title, artist, album, duration, and artwork when available. echo refreshes the configured local folder on startup and watches it for supported audio/artwork changes while running; `:rescanlocal` is still available as a manual fallback.

Local playlists are stored locally and are not Spotify playlists. They can contain local tracks and Spotify track references. Spotify playlists cannot contain local tracks. Local shuffle, repeat, volume, queue, and play/pause are handled by echo's local playback engine.

Embedded artwork is used when available. If a track has no embedded artwork, echo looks for folder artwork such as `cover.jpg`, `folder.jpg`, or `front.png`.

## Troubleshooting
- **Theme color rendering issues (Windows)**: Disable "Adjust indistinguishable text" in the Appearance settings of the Defaults profile. 
- **Images not rendering**: Cover art is drawn with half-block cells and needs nothing from the terminal beyond truecolor support, which every modern terminal has.
- **Cache desync**: If your Liked Songs are out of sync with other devices, simply restart echo. It eagerly syncs your library in the background on startup.
- **Local file missing**: If a file was deleted or moved after scanning, run `:rescanlocal` to refresh the local library.
- **Audio sounds mono or muffled (Bluetooth headsets)**: Windows exposes a Bluetooth headset as two output devices — a stereo "Headphones" (A2DP) endpoint, and a mono "Hands-Free" (HFP) endpoint capped at 16 kHz. Windows switches to Hands-Free whenever an application opens the microphone. Check `echo-debug-audio-spotify.log`: if it reports `channels=1`, quit whatever is holding the mic and select the stereo endpoint as your default output device.
- **Configuration Path**: `~/.config/echo/config.toml` (holds tokens and preferences), `~/.config/echo/cache.json` (holds liked tracks), `~/.config/echo/local_library.json`, and `~/.config/echo/local_playlists.json`.
