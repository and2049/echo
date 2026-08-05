# Changelog

### Audio Level Fixes
 - Fixed Spotify playback being much quieter than the Spotify desktop app at the same volume
 - New `normalisation_pregain` option (default 3.0 dB) that restores the level normalisation takes off, without clipping
 - Volume now follows the same curve for Spotify and local files, so a given percentage sounds the same on both
 - Volume is handled entirely in-app; other Spotify clients no longer override it
