# Changelog

### Fix: retry Spotify playback with fresh device_id on 404
- Handles stale device_id smoothly by clearing cached id on error and refetching 

### Fix: stop re-creating tokio interval on every tick
- fixes rate limiting being caused by sync ticks from v0.2.3

### Client side volume management
- Handles Spotify volume client side instead of over API

### Cache playback bar image in Buffer
- Eliminates per-frame ratatui-image protocol rendering during playback.

### Remembers last volume level upon starting session

### Buffered playback state + Progress estimation
