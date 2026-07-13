# Changelog

### Spotify authentication recovery
- Detect expired Spotify refresh tokens and automatically start browser reauthorization.
- Preserve local music and app configuration when Spotify authentication expires or reauthorization fails.
- Add `:spotifylogin` for manually retrying Spotify authentication.

### Navigation and library controls
- Add `gg`/`G`, page, and half-page list navigation.
- Add playback-context history and `gc` navigation to the currently playing track.
- Add track sorting by title, artist, album, duration, date added, original order, and reverse order.
- Add optional relative line numbers with `:relative on|off|toggle`.
- Add configurable semantic keybindings, including modifiers and two-key sequences.

### Playback and Spotify commands
- Add source-aware seeking with `,`, `.`, `0`, and `:seek` while keeping `[` and `]` for previous and next track.
- Add mute with `M` and `:mute`, restoring the previous volume when unmuted.
- Add `:open` for Spotify track, album, artist, and playlist URLs or URIs, with clipboard input when no argument is supplied.
- Keep playback progress moving immediately while Spotify or local playback state is being confirmed.

### Fixes
- Keep local play and pause state synchronized with the playback source.
- Refresh local files without requiring repeated `:localpath` commands.

