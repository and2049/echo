# Changelog

### Browse tab
New tab in library for Top Tracks, Recently Played, Followed Artists

### Artist Pages
Artists pages displaying album list
- Available through global search and Followed Artists

### Caching and refresh
Reduce API calls to reduce change of rate limiting and using background refreshes to keep up to date.
- User playlist list: cached list displays immediately; refreshes in background if older than 15 minutes.
- Saved albums list: same 15-minute soft refresh behavior.
- Playlist track lists: cached tracks display immediately; refreshes in background if older than 15 minutes.
- Album track lists: cached tracks display immediately; refreshes in background if older than 6 hours.
- Manual library refresh: uppercase R on the Library view refreshes playlists and saved albums.
