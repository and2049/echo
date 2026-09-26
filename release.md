# Changelog

### Liked Songs
 - Liked Songs shows the whole library instead of the first 100 songs, in both apps, straight from `~/.config/echo/liked_songs.json`
 - Keeping it current usually costs one request: new likes show up at the top, and a full re-read happens only on first use, when songs were removed on another device, or once a week
 - A full re-read goes one page a second, stops on a rate limit and waits out Spotify's retry time, and picks up where it left off after a restart
 - A failed or rate-limited sync no longer leaves hearts missing, and hearts on local files are no longer cleared by it
 - Unliking a song in echo removes it from the open list right away
