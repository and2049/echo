# Changelog

### Perf: Startup exponential backoff
- Reduces Spotify API calls on startup by using an exponential backoff loop while waiting for the daemon to authenticate.

### Feat: Play/Toggle debouncing & In-flight guarding
- Implements a 300ms key-repeat debounce for Play/Pause to prevent API spam from OS media keys or held keys.
- Adds an in-flight guard to prevent concurrent playback requests and race conditions, while correctly letting Pause requests bypass the guard.
- Perfectly syncs the optimistic TUI state by reverting visual changes if a background request is debounced or dropped.

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
