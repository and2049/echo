# Changelog

### Desktop App
 - Dark, light and auto modes: echo keeps one theme for dark and one for light, and `auto` follows the system on macOS, Windows and Linux. Set the mode in the theme picker (`t`), in Settings, or with `:mode auto|dark|light`; a theme you had already picked keeps its look
 - New themes pairing existing ones: chime (light echo), nimbus (dark cloud), tide (dark wave), glade (light everforest) and parchment (light gruvbox). The theme picker lists one appearance at a time, with a pair on one row and color swatches for every theme
 - Removed the crimson, cyber_citrus and neon themes; a config still naming one opens on echo
 - On macOS the app icon in the Dock and Finder now updates when the theme changes while echo is running, instead of on the launch after
 - On macOS the themed app icon now stays in the Dock, Finder and Launchpad after echo quits, and is redrawn on the next launch if the theme changed while echo was closed
