# Changelog

### Desktop App
 - Add a fullscreen now-playing view (shift-F, escape to leave): cover and title on the left, lyrics centered on the right with the current line pinned and gliding up one row at a time, seek bar and transport controls alongside
 - Derive every color in the immersive view from the album cover: a deep or pale tint of the cover's primary as the base, text and accent pulled to stay readable on it, a light layout for light covers and a dark one for dark covers
 - Widen one-hue and grayscale covers with analogous or hashed tints so monochrome albums still get a colorful backdrop, stable per cover; tracks without art use the theme's accents
 - Animate the backdrop whenever the view is up, paused or not: a 30 s seamless loop crossfaded on the GPU from prebuilt keyframes at 20 fps, no per-frame pixel work, honoring reduce-motion
 - Add an immersive backdrop setting (Settings > Appearance, or `:backdrop <name>`) with five modes: Lights (orbiting blurred discs), Mesh (flowing gradient mesh), Aurora (rippling curtains), Vinyl (turning record), Nebula (ray-marched field after a twigl shader)
 - Build backdrop keyframes off the UI thread on a track or mode change, showing a still frame until they land, and release replaced textures so the atlas no longer grows per cover
 - Move the queue button into the immersive view's top-right slots so the toggle and settings stay under the pointer; the titlebar goes transparent with cover-derived glyphs
