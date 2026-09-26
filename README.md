<p align="center">
    <a href="">
      <picture>
        <img src="assets\echo-rs.svg" alt="ECHO-RS">
      </picture>
    </a>
</p>
<p align="center">
    <a href="README.md">English</a> |
    <a href="README.zh.md">简体中文</a>
</p>

echo is a native desktop music player and Spotify client written in Rust. echo brings your entire Spotify library — liked songs, playlists, albums, and the artists you follow — plus your local music files into one fast, keyboard-friendly app, with full playback control, synced lyrics, and dynamic theming.

![echo desktop app](assets/echo-desktop.png)

## Features

- **Native desktop app**: Built on GPUI (Zed's UI framework) for a fast, GPU-accelerated interface that runs on Windows, macOS, and Linux. Drive it with the mouse or entirely from the keyboard.
  The Home page brings together quick picks, recent listening, top artists and songs, and new releases; open it with `ctrl-h` (`ctrl-shift-h` on macOS).
- **Your whole library**: Playlists, albums, and followed artists in one sidebar, with Top Tracks, Recently Played, and Top Artists views.
- **Full playback control**: Play/pause, next/previous, seek, shuffle, repeat, volume, queue, and device switching from the now-playing bar.
- **Synced lyrics**: Time-synced lyrics inline in the player bar or as a full-screen view.
- **What's New**: A feed of recent albums and singles from the artists you follow, refreshed at most every 6 hours.
- **Blazing fast Liked Songs**: Your entire Liked Songs library is cached locally (`~/.config/echo/liked_songs.json`) for zero-latency scrolling, even with thousands of saved tracks. Keeping it current usually takes a single request; a full re-read is paced, resumable, and backs off when Spotify rate-limits.
- **Library management**: Create, rename, delete, and organize playlists into folders; reorder tracks in your own playlists.
- **Local music support**: Scan a local music folder, play local files, and create local playlists that can also reference Spotify tracks.
- **Search**: Fast global search (`ctrl-k`) across the Spotify catalog and your scanned local tracks.
- **Dynamic theming**: Ship-with themes plus live theme editing — see [Themes](#themes).

## Setup

1. **Spotify Premium**: A Spotify Premium account is required to use the Spotify Web API for playback control.
2. **Spotify Developer App**: 
   - Go to the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard/).
   - Create an app and get your `Client ID` and `Client Secret`.
   - Add `http://127.0.0.1:8888/callback` to your app's Redirect URIs.
   - echo also uses `http://127.0.0.1:8989/login` for its internal first-party Spotify session.

### Installation

**Linux and macOS**

```bash
curl -fsSL https://github.com/and2049/echo/releases/latest/download/install.sh | sh
```

**Windows** (PowerShell)

```powershell
irm https://github.com/and2049/echo/releases/latest/download/install.ps1 | iex
```

Neither needs administrator rights. Both add echo to your Start menu, Launchpad, or applications menu.

| Platform | Where it lands |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Programs\echo` (installed from the release MSI, x64) |
| macOS | `/Applications/echo.app` (Apple Silicon) |
| Linux | `~/.local/share/echo`, with `echo-desktop` linked into `~/.local/bin` (x86_64) |

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

After the first install, echo updates itself — no reinstall, no administrator rights. New releases install automatically in the background; turn this off in Settings or with `:autoupdate off`, and check by hand from **Settings → Updates → Check for updates**. An update swaps the app and its bundled themes in place and asks you to restart.

> Earlier releases also shipped a `spotify` terminal client. It has been removed; updating from one of those releases deletes it along with its `~/.local/bin` link.

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
cargo run --release
```

The binary lands at `./target/release/echo-desktop`.

On first run, echo will prompt you to enter your `Client ID` and `Client Secret`, then open your browser to authenticate with Spotify.

## Usage

echo works with the mouse — click a playlist, artist, or track to open it, use the controls in the now-playing bar, and drag tracks to reorder your own playlists. It's also fully keyboard-driven. Press `?` at any time for the in-app shortcut overlay, `ctrl-,` for settings, and `t` to switch themes.

Closing the window keeps echo playing: on Windows and Linux it hides to a tray icon (click it to bring the window back, or pick Quit), on macOS it stays in the Dock. Turn that off with `:tray off` or Settings → Window; `ctrl-q` always quits. Launching echo again brings the running one forward instead of starting a second copy.

### Navigation
- `j` / `k` or `↓` / `↑`: Move down / up
- `gg` / `G`: Jump to the first / last item
- `ctrl-b` / `ctrl-f` or `Page Up` / `Page Down`: Move one page
- `ctrl-u` / `ctrl-d`: Move half a page
- `gc`: Jump to the currently playing track or its context
- `enter` or `z`: Open the selected item / play the selected track
- `h` / `esc`: Go back / close a panel
- `←` / `→`: Move focus between the sidebar and the main pane; `backspace` also returns to the sidebar
- `alt-←` / `alt-→`: History back / forward
- `tab`: Switch tabs (e.g. search results, artist discography)
- `ctrl-h` (`ctrl-shift-h` on macOS): Home
- `ctrl-\`: Show / hide the sidebar

### Playback
- `space`: Play / pause
- `]` / `[` (or `ctrl-→` / `ctrl-←`): Next / previous track
- `.` / `,` (or `shift-→` / `shift-←`): Seek forward / backward 5 seconds
- `0`: Seek to the start of the track
- `=` / `-`: Volume up / down 1%; `+` / `_` for 5%
- `shift-M`: Mute / restore the previous volume
- `s`: Toggle shuffle
- `r`: Cycle repeat (off → track → context)
- `shift-D`: Device menu
- `shift-L`: Synced lyrics panel
- `ctrl-shift-L`: Condensed lyrics in the player bar
- `shift-F`: Immersive view

### Library
- `l`: Like / unlike the selected track
- `a`: Add the selected track to a playlist, or the selected album to your library
- `shift-A`: Action menu for the selected (or currently playing) track
- `q`: Add the selected track to the queue
- `shift-Q`: Open the queue
- `m`: Pin / unpin a playlist
- `c` / `e`: Create / rename a playlist or folder
- `v`: Visual mode for selecting a range
- `dd`: Delete a playlist or folder, or remove a track from your own playlist
- `shift-J` / `shift-K`: Move the selected track down / up within your own playlist (drag-and-drop works too); requires the original sort order
- `shift-R`: Force refresh

### Finding things
- `ctrl-k`: Global search
- `f`: Search from the command bar
- `/`: Filter the current list
- `n` / `shift-N`: Next / previous match
- `:`: Command bar — see [Commands](#commands)

The track action menu adapts to the source. Spotify tracks support link copying, liking, and album library actions. Local tracks support copying their absolute path and revealing the file in the platform file manager. Both sources retain album/artist navigation, playlist insertion, and queue actions where applicable.

## Commands
The `:` command bar accepts the following:
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
- `:theme <theme_name>`: Switch application theme.
- `:lang <en|zh|zh-CN>`: Switch language.
- `:album`: Jump to the album of the currently selected track.
- `:queue`: Open the Queue view.
- `:clearqueue`: Clear the manually queued tracks (only while playing on this device).
- `:clearhistory`: Forget the local play history.
- `:range <short|medium|long>`: Time range for Top Tracks and Top Artists.
- `:spotifylogin`: Re-authenticate with Spotify.
- `:vis`: Toggle the audio visualizer.
- `:visbins <number>`: Set the number of audio visualizer frequency bins (5-32).
- `:pixelate <pixels>`: Enable retro 8-bit aesthetic on album covers. Set to 0 to disable, or e.g., 16 for a pixelated look.
- `:backdrop <lights|mesh|aurora|vinyl|nebula>`: Pick the moving picture behind the immersive view (also in Settings).
- `:tray [on|off]`: Whether the close button hides echo to the tray instead of quitting (also in Settings).
- `:autoupdate [on|off]`: Install new releases automatically (also in Settings).
- `:index <number>`: Set track index base (1-indexed vs 0-indexed).
- `:quit`, `:q`, `:qa`, `:wq`: Exit the application.

Track sorting and navigation operate on already-loaded data. They do not issue Spotify requests. Navigation history retains up to 20 in-memory views so returning to a previous track list normally does not refetch it.

## Themes

Themes live in `themes/*.toml` as a flat list: nine base colors followed by the twelve derived colors the app paints, every one explicit with a comment saying what it drives. Edit values freely, or change base colors and run `python themes/generate_desktop.py` to recompute the derived ones. Derived keys are optional — a missing key is computed with the formula named in its comment, and a `[desktop]` table is also accepted for overrides. To iterate visually, `python tools/theme-preview/serve.py` opens a live mock of the desktop window in the browser that repaints on every save — no rebuild needed. Colors can be edited in either direction: change the toml in your editor, or click any color in the preview's legend to adjust it with a picker that writes straight back to the file (its "recompute derived" button re-runs the generator for the current theme).

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
- **Cache desync**: Likes from other devices show up on startup or when you open Liked Songs (checked at most every 15 minutes). Songs unliked elsewhere are caught at the same point and trigger a background re-read of the library; a rate-limited re-read picks up where it stopped. Deleting `~/.config/echo/liked_songs.json` while echo is closed forces a full re-read.
- **Local file missing**: If a file was deleted or moved after scanning, run `:rescanlocal` to refresh the local library.
- **Audio sounds mono or muffled (Bluetooth headsets)**: Windows exposes a Bluetooth headset as two output devices — a stereo "Headphones" (A2DP) endpoint, and a mono "Hands-Free" (HFP) endpoint capped at 16 kHz. Windows switches to Hands-Free whenever an application opens the microphone. Check `echo-debug-audio-spotify.log`: if it reports `channels=1`, quit whatever is holding the mic and select the stereo endpoint as your default output device.
- **Configuration Path**: `~/.config/echo/config.toml` (holds tokens and preferences), `~/.config/echo/cache.json` (holds library caches and liked-state hearts), `~/.config/echo/liked_songs.json` (holds the Liked Songs list), `~/.config/echo/local_library.json`, and `~/.config/echo/local_playlists.json`.
