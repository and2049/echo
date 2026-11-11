# Changelog

### Desktop App
 - Spotify playlist folders now show in the sidebar, nested as they are in Spotify and listed above echo's own folders. They are read-only: a playlist inside one can be pinned but not moved out, and echo's folders (tagged `local`) take local playlists and Spotify playlists that are not in a Spotify folder
 - Drag folders to reorder them, Spotify's and echo's alike, and drag the playlists and folders inside a Spotify folder to reorder them. The order is kept by echo and changes nothing in Spotify
 - Liked Songs and Local Music no longer carry an emoji in their names, which macOS drew in full colour regardless of theme; the Liked Songs row shows the heart icon in the accent colour instead
 - The local music folder accepts `~/Music`-style paths, and the button beside it reads Set until the typed folder is saved, then Rescan now
 - Updated librespot to the current upstream: picking a track from another Spotify app plays that track, the dealer connection honours a configured proxy, and malformed access-point or discovery packets no longer panic
 - Redesigned Settings: switches for on/off options, dropdowns for choices, a segmented Mode control, far fewer hints, and sections regrouped into General, Appearance, Audio quality, Local music and Updates. Visualizer bands and pregain dim while the setting they depend on is off
 - Removed the track numbering base setting and the `:index` command; lists always number from 1
 - Dark, light and auto modes: echo keeps one theme for dark and one for light, and `auto` follows the system on macOS, Windows and Linux. Set the mode in the theme picker (`t`), in Settings, or with `:mode auto|dark|light`; a theme you had already picked keeps its look
 - New themes pairing existing ones: chime (light echo), nimbus (dark cloud), tide (dark wave), glade (light everforest) and parchment (light gruvbox). The theme picker lists one appearance at a time, with a pair on one row and color swatches for every theme
 - Removed the crimson, cyber_citrus and neon themes; a config still naming one opens on echo
 - On macOS the app icon in the Dock and Finder now updates when the theme changes while echo is running, instead of on the launch after
