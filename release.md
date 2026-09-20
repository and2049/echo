# Changelog

### Desktop App
 - Resume the last session: the track that was playing comes back paused where it stopped, with its queue, shuffle and repeat, and Play, Next or Previous start it from that position
 - Keep a local play history and show it behind a Recent tab in the queue view, merged by time with Spotify's own recently played list; a play counts after 30 seconds or half the track, and every Recent row plays its song directly
 - Ctrl-click (cmd-click on macOS) picks several rows, and the whole selection can be dragged to a sidebar playlist, dropped on Liked Songs, or dropped between the rows of an editable playlist to insert it there; lists scroll while a drag hovers near their edge
 - Add Refresh to the playlist and album page menu
 - Add `:clearhistory` to forget the local play history
 - Right-click and `q` / `shift-a` now work on the Popular rows of an artist page
 - Artist pages get a Follow / Following button and discography chips (All, Albums, Singles & EPs, Appears On); `tab` cycles the chips
 - Following now needs one more Spotify permission, so the first launch after this update asks you to sign in again once
 - The followed-artists list is no longer cut off at 50 artists
 - Artist portraits that are not square are cropped to a circle instead of being squeezed into an oval; album art fills its box the same way
 - The scrollbars under the Home shelves use the seek bar's colors so they are visible, and dragging one keeps scrolling wherever the pointer wanders while the button is held

### Terminal client
 - The last session is restored paused on launch and starts again from its position
 - The queue view gains a Recent tab (`tab` switches) with the local play history
 - `q` and `A` act on the Popular rows of an artist page
 - `F` follows or unfollows the artist whose page is open and `tab` cycles its discography chips; the page title shows Following
