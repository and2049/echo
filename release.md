# Changelog

### Desktop App
 - Right-click and `q` / `shift-a` now work on the song rows of the search page, on the All overview as well as the Songs tab
 - Artist portraits and covers that are taller than they are wide no longer stretch into ovals or push into the title row; every picture box is centre-cropped to a square
 - Song rows on the search page select on a single click and play on a double click, like every other list
 - Recent searches drop down under the search box while it has focus instead of replacing the page, and narrow to the entries containing what you type; clicking one runs it again, and the remove and clear buttons keep the box focused

### Liked Songs
 - Liked Songs shows the whole library instead of the first 100 songs, in both apps, straight from `~/.config/echo/liked_songs.json`
 - Keeping it current usually costs one request: new likes show up at the top, and a full re-read happens only on first use, when songs were removed on another device, or once a week
 - A full re-read goes one page a second, stops on a rate limit and waits out Spotify's retry time, and picks up where it left off after a restart
 - A failed or rate-limited sync no longer leaves hearts missing, and hearts on local files are no longer cleared by it
 - Unliking a song in echo removes it from the open list right away
