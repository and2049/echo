# Changelog

### Desktop App
 - Add a Home page as the default view (sidebar link or `ctrl-h`, `ctrl-shift-h` on macOS): a time-of-day greeting, a quick-pick grid of Liked Songs and recent playlists and albums, and shelves for Made for you, Recently played, Your top artists, Your top songs and New releases, each card opening or playing its item
 - Redesign playlist and album pages with a hero header: large cover, type label (Public, Private or Collaborative playlist, Album, Single, Compilation), description, owner with collaborators, song count and total duration, a large play button, shuffle, sort and a More menu
 - Turn the track list into a table with sortable column headers for number, title, album, added by, date added and duration; click a header to sort, click again to flip, click the number column to restore the original order
 - Show relative dates in the date added column, an explicit badge next to the artist credits, an animated playing indicator on the current row and a play glyph when hovering a row number
 - Load whole playlists instead of the first page, and page the playlist library at 50 per request so libraries with more than 20 playlists appear in full
 - Add an Edit details dialog for owned playlists (name, description, public or private) from the sidebar menu and the page header
 - Follow and unfollow other people's playlists and save or remove albums from the page header
 - Warn when a song is already in the destination playlist, with Add anyway, Skip duplicates and Cancel
 - Filter the add-to-playlist picker and flyout by typing, and add a New playlist entry that creates and fills a playlist in one step
 - Add an All search tab with a ranked top result, a short songs list and album, artist and playlist card rows with Show all links; tabs are now pill chips
 - Remember recent searches (shown when the search box is focused and empty, removable one by one or cleared together), add a clear button to the search box and make Escape clear first and leave the box on a second press
 - Decode covers drawn larger than the sidebar at card size so Home, search and artist images are sharp, and keep mosaic playlist covers of different sizes apart in the thumbnail cache
 - Install new releases automatically in the background (on by default; Settings > Updates or `:autoupdate off`): release builds check shortly after launch and every six hours, failures are logged quietly, and a banner offers Restart now once a release is installed
