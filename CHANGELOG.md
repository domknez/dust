# Changelog

## Unreleased

- Logins: a keychain that can't be read for a moment no longer leads to the other
  source's login being dropped on the next save
- Crate and history: a backup is restored only once what's there now is backed up;
  plays and Crate changes made while the sync folder is out of reach are kept and
  written once it's back; settings are saved so a crash can't leave them half written
- History: a track playing when you switch source or log out is credited to the
  source it played from; logging out of the last source pauses playback
- Back and Forward skip pages turned off in Settings since
- Playlists: edits made one after another each report back; an expired session
  while editing a playlist or a like goes to the login screen
- Playback: a network drop mid-track picks the track up where it broke off
  instead of skipping to the next; unplugged headphones or a USB DAC switch to
  the output that's there now instead of stalling; files with very large audio
  blocks no longer stall; a restored queue with an unreadable track in it starts
  on the right track
- Deezer: seeking on a CDN that ignores the requested range no longer plays
  noise; an expired session or a dropped network while starting a track says
  so instead of "isn't available in your region"
- Navidrome / Subsonic: synced lyrics with an offset are timed the right way;
  plain lyrics are still found when the server's synced lyrics lookup fails
- AirPlay: speaker remote commands are taken only from a speaker dust is
  playing to; replies from a speaker are size-checked; a volume of exactly 0
  reported by a speaker is no longer taken as full volume; a setup that fails
  halfway leaves no threads behind
- While dust is minimized or hidden: the system's now playing info follows the
  track, the queue is saved, plays are kept, and sync, backups and update checks
  carry on (before, all of that waited for the window to be shown again)
- Log in with Deezer: only deezer.com's own session cookie is taken, and a login
  window that crashes says so instead of "Login window closed"
- Updates: the download goes to a new folder only you can open
- Navidrome / Subsonic: adding the same track twice in one go adds it once
- "1 track" and "an hour ago" instead of "1 tracks" and "1 hours ago"; sync times
  older than a day say "yesterday" or how many days

## 0.31.1 (2026-11-18)

- Listening: the cards of lists made from your history grow to fit their
  description, so it no longer runs into the track count on narrow windows

## 0.31.0 (2026-11-17)

- Nix: `flake.nix` with a dev shell (`nix develop`), an installable package (`nix run`,
  `nix profile install`, NixOS) with the desktop entry and icons, and `nix flake check`
  (run by CI); see `docs/nix.md`. New cargo feature `self-update` (on by default): without
  it dust doesn't check for updates by itself, which the flake uses

## 0.30.0 (2026-11-17)

- Crate: albums say whether they're an album, EP, single, live album or
  compilation, with how many tracks and the year, in the list and in full. Albums
  saved earlier are looked up in the background
- Sleep timer: stop at the end of the album or of the queue (even on repeat), not
  only at the end of the track; also in the command palette
- Deezer: compilations no longer show up as singles in artist discographies

## 0.29.0 (2026-11-17)

- Settings is a bigger window with sections on the left: General, Playback,
  Features, Sidebar, Crate and history, Updates, Troubleshooting; on/off settings
  are switches
- Features: turn off the Crate, the play history, lyrics or AirPlay. A part turned
  off is hidden everywhere (sidebar, panel tabs, menus, shortcuts, the command
  palette) and what it saved is kept. With the history off no plays are recorded;
  with AirPlay off dust stops looking for speakers
- Sidebar: hide any of its pages but Home and Search

- Narrow windows: with the right panel open below 1280 points wide, both side
  panels narrow to leave the page room; album, playlist and artist headers use a
  smaller cover and title and show only icons on buttons that don't fit; the Crate
  page lists entries or shows one (with *Back to the list*) instead of squeezing
  both, and its *Play the new ones* button no longer runs off the page

## 0.28.1 (2026-11-17)

- Right-click an entry in the Crate (the panel tab or the page) to mark it heard
  (or new again) or remove it

## 0.28.0 (2026-11-16)

- *Listening* page (sidebar): time listened, tracks, artists and new artists over the
  last week, month, year or all time, a chart by day or month, top artists, albums
  and tracks, the artists first heard, and this week a year ago
- The play history keeps the last 15,000 plays (was 2,000) and no longer stores
  Deezer's stream tokens, so each line is half the size; the panel lists the latest
  1,000
- *new to me only* on the Flow (Radio) panel: Flow leaves out tracks already in the
  play history (unless a whole batch is heard ones, so it never runs dry)
- A lime dot on a track's cover in *up next* marks one you've never heard
- New releases (Deezer): every 12 hours dust checks the artists you follow and the 40
  you played most in the last 90 days; their new albums, singles and EPs go in the
  Crate as new, with a toast. Settings → *New releases to the Crate*
- *Play the new ones* in the Crate (page and panel tab): the new tracks in a row,
  with three tracks of each new album and artist
- Artist pages: a *Timeline* tab in the discography, every release by year, each
  marked heard or not heard (from the play history)
- Command palette (`⌘K`, or *Commands…* in the account menu): pages, playlists,
  artists you listen to, and things to do with the track playing, Flow moods and the
  player, found by typing
- Lists made from the play history, on the Listening page and in the palette: *On
  repeat*, *Found lately*, *Not heard in a while* and *Never skipped*; they open like
  playlists, to play, queue or add to a playlist of your own

## 0.27.1 (2026-11-16)

- Right-clicking the track playing in the player bar (title, artist, cover) opens its
  menu again: add to a playlist, Crate, like, go to album or artist

## 0.27.0 (2026-11-16)

- Sync the Crate and play history between computers through a folder (iCloud Drive,
  Dropbox, Syncthing, Google Drive, OneDrive or any other): Settings → *Crate and
  history*. Each computer writes its own file there and reads all of them, and the
  section shows when it last synced and on which computers
- Daily backups of the Crate and history next to the settings (the last 14), with
  *Restore a backup…*; *Copy Crate as Markdown* and *Save Crate as JSON* export it

## 0.26.1 (2026-11-16)

- Queue panel: *save as playlist* (the whole queue) sits on its own under the tabs,
  and *clear* moved next to *NEXT UP*, the tracks it removes

## 0.26.0 (2026-11-16)

- Crate: save things to check out later (tracks, albums, artists, or a note of a name
  you heard), from any source. `⌘⇧S` saves what's playing; *Save to Crate* in track
  menus, an album's ⋯ menu and on artist pages; a *crate* tab in the panel on the
  right to save and tick off what you've heard; a Crate page in the sidebar with
  filters and each entry in detail (play, add to a playlist, search for it, a note)

## 0.25.0 (2026-11-16)

- Play history: the panel on the right has *up next* and *history*. dust keeps every
  track you hear (the last 2,000, on this computer), shown newest first by day with
  the Flow or album it came from and skips marked; click one to hear it again,
  right-click for the track menu
- Lyrics open as a page in the main area instead of a tab of the queue panel, so the
  queue can stay open beside them; Back, `Esc` or the lyrics button returns

## 0.24.0 (2026-11-16)

- Flow by genre as well as by mood: *moods / genres* next to the Flow title switches
  the chips to the genres Deezer picks for you (Rock, Metal, Jazz, ...)

## 0.23.0 (2026-11-16)

- Save the track playing to a playlist from the player bar: **+** next to the heart
  opens your playlists with a field to find one by typing (`Enter` takes the first
  match), and a right-click on the track offers *Add to playlist ›*, *Add to Loved
  tracks*, *Start track mix*, *Go to artist* and *Go to album*
- `⌘D` / `Ctrl+D` adds the track playing to the playlist you added to last, also from
  the mini player
- *Add to playlist ›* everywhere lists the playlists you used last on top (*Recent*)

## 0.22.3 (2026-11-01)

- Pick several tracks in a list: click, `⌘`-click (`Ctrl`-click), `Shift`-click for a
  range, or `⌘A` / `Ctrl+A` for all (`Esc` lets go). The right-click menu then works on
  all of them: *Remove from this playlist* on your own playlists, *Play next*, *Add to
  queue*, *Add to playlist ›*. Tracks that can't be played can be picked and removed too
- Two toasts at once no longer grow until they fill the window

## 0.22.2 (2026-10-30)

- Right-click an album's name in a track list, or an album under *More by* on an album
  page: *Play next* / *Add to queue* for the whole album. Before, nothing happened

## 0.22.1 (2026-10-29)

- Pasting into text fields works (⌘V / Ctrl+V): a Deezer link into Search, a server
  address into the Navidrome dialog

## 0.22.0 (2026-10-27)

- Messages come as toasts in the bottom right corner, above the player bar, instead of a
  line of text beside the search field: a track that can't be played, a playlist
  change, a like or follow (done, or why it didn't go through). They fade after a few
  seconds (errors stay longer), stay while the pointer is on them, and × closes them

## 0.21.0 (2026-10-26)

- Explore (sidebar, `⌘7`): Deezer's genres, moods, decades, charts and *For you*, each a
  page of new releases, playlists and top artists. Genre cards on Home open them too, and
  *see all* on a row shows the whole of it
- New releases (sidebar, `⌘8`): this week's albums, the ones from artists you listen to
  first, then by genre
- Artist mix: *Mix* on an artist's page plays songs by them and artists like them (Deezer
  and music servers)
- Track mix from any track: right-click it → *Start track mix* (Deezer)

## 0.20.1 (2026-10-23)

- Updated the UI toolkit (egui 0.37) and other libraries

## 0.20.0 (2026-10-23)

- Songs follow each other without a pause: dust opens the next track about ten seconds
  before the current one ends and plays it straight after, instead of loading it once
  the last one has finished. Endless Flow and Radio fetch their next batch ahead too
- The player bar and the Now Playing widget change to the next track when you hear it
  start; on an AirPlay speaker they used to change a few seconds early
- AirPlay speakers respond more smoothly: pausing, seeking, skipping and volume changes
  no longer hold up the player while the speaker answers (up to a couple of seconds on a
  slow network), and a dragged volume slider or the sleep timer's fade sends the
  speaker only the latest level
- Fixed: errors from a Navidrome server said "Deezer" ("Couldn't reach Deezer: ..."); they
  now name the server
- Fixed: when the network dropped, dust skipped through the queue as if the tracks
  couldn't be played. It now stops at the track with the error shown; press play to
  try again. Tracks that really can't be played are still skipped
- Fixed: if playback broke down, the controls silently did nothing; dust now says so.
  Crashes are written to the log file too (what failed, where, on which thread)
- Fixed: on this computer's speakers, the start of a track (or of the audio after a
  seek) could be cut off
- Fixed: a like that didn't go through, or your playlists failing to refresh, put the
  error in place of the track list on screen until you went to another page. Errors
  now show with what failed: a like, a playlist change or a card's play button
  briefly beside the search field, the playlists in the sidebar. Artists, Albums and
  Podcasts say when they couldn't load
- Fixed: logging out or switching source while something was still loading (your
  playlists, likes, a playlist change) could let it land afterwards, on the login
  screen or the other source; what the old source was loading is now dropped

## 0.19.1 (2026-10-23)

- Fixed: the Dark choice in Settings showed a circle instead of a moon

## 0.19.0 (2026-10-23)

- The sidebar's playlist list folds away: click *PLAYLISTS* (a chevron shows which way
  it is); dust remembers it
- Fixed: with both Deezer and a Navidrome server connected, macOS asked for keychain
  access twice at start. Both logins now live in one keychain item (moved there on the
  first start, which may ask one last time for each)

## 0.18.0 (2026-10-22)

- Navidrome (and other Subsonic / OpenSubsonic servers): play your own music in dust,
  with the same pages and design. Connect a server from the account menu (*Music from*)
  or the login screen; switch between Deezer and the server there, each with its own
  queue. Home shelves and mixes, albums, artists, playlists (with editing), search, stars
  as likes, synced lyrics and play reports; MP3, FLAC, AAC/ALAC, Ogg Vorbis and WAV play
  as they are at any sample rate, other formats are converted by the server
- Navidrome Home opens with Radio, in place of Deezer's Flow: endless random songs from
  your library, or from one of its genres
- Cover art in PNG and WebP shows too (music servers send those)

## 0.17.1 (2026-10-22)

- README screenshots show the Graphite look; `DUST_DISPLAY_NAME` hides the account name
  for screenshots
- New app icon, "Grain": a lime block breaking into grains of dust on charcoal, to match
  the Graphite look

## 0.17.0 (2026-10-22)

- New look, "Graphite": charcoal (or soft grey) with one lime accent, Space Grotesk and
  JetBrains Mono, small crisp corners and hairlines. The sidebar is a compact list
  (`dust_` wordmark, shortcut hints, playlist counts), Home greets you and opens with a
  Flow panel whose moods sit in a row, Play buttons are
  lime, and the queue and lyrics panels switch with tabs
- The player bar is one row: transport, times and progress side by side; the progress
  bar is drawn as a waveform (a pattern per track, not the real audio)
- Album pages: year, track count and length, the artist and label, compact numbered rows
  with an artist column, and the artist's other releases beside the tracks
- Artist pages: a large name over a dark band with the picture on the right, popular
  tracks next to the discography as a grid with Albums / Singles / Live tabs
- `⌘1`–`⌘6` (`Ctrl+1`–`Ctrl+6`) open Home, Loved tracks, Playlists, Albums, Artists and
  Podcasts; the sidebar shows the shortcuts
- Mini player: a borderless 480 × 72 strip with the speaker it plays on and a progress
  line along the bottom; narrower, it shows just the title, times and play (previous /
  next on hover)
- "Play on": a card per output with what kind of speaker it is (e.g. "HomePod mini ·
  AirPlay 2"); the one playing carries a volume slider
- Sleep timer: a countdown ring with the time left and the choices as buttons. A timed
  sleep now fades the music out over its last 30 seconds before pausing, and the volume
  is back for the next play

## 0.16.1 (2026-10-22)

- Fixed: keyboard shortcuts in menus showed broken symbols (⌘, ⇧). Menus now show the
  shortcut on the right, as native menus do

## 0.16.0 (2026-10-22)

- Podcasts: search finds shows; a show's page lists its episodes newest first with their
  dates, to play or queue like tracks. *Podcasts* in the sidebar shows the ones you
  follow on Deezer. Episodes aren't reported as listens and can't be liked or added to
  playlists
- Settings have their own window: open it from the account menu or with `⌘,`
  (`Ctrl+,` on Windows and Linux); `Esc` or a click outside closes it. The account menu
  keeps Settings and Log out
- New look: Figtree for text, Bricolage Grotesque for titles and IBM Plex Mono for small
  labels
- Tidier player bar: the heart sits right after the track's title, and the sleep timer and
  mini player moved into a ⋯ menu. A running sleep timer shows a stopwatch with the time
  left (click it to turn the timer off)

## 0.15.0 (2026-10-22)

- Lyrics: the speech-bubble button in the player bar opens the playing track's lyrics.
  Synced lyrics follow the song (the sung line highlighted and kept in view); click a
  line to jump to it. Songwriters and copyright are shown underneath
- Sleep timer: the moon in the player bar pauses the music after 15, 30, 45, 60 or 90
  minutes, or stops at the end of the track

## 0.14.2 (2026-10-16)

- Fixed: after the Mac's network changed while playing (e.g. the dock's Ethernet dropping
  when the screen locks), starting another playlist or track could leave an AirPlay speaker
  silent until it was picked again. dust now notices it reaches the speaker over a
  different connection and reconnects
- Fixed: reconnecting to a speaker or pausing on one reported the track to Deezer as
  skipped

## 0.14.1 (2026-10-16)

- Fixed: AirPlay speakers glitched or went silent for a while when dust was in the
  background and the Mac was busy (macOS throttled dust, which it took for idle). dust now
  tells macOS it's playing (no App Nap, no idle sleep while playing), runs its audio threads
  at high priority and, if it still falls behind, re-syncs instead of sending late audio

## 0.14.0 (2026-10-15)

- Log file for troubleshooting (account menu → Show log file): tracks opened and how long
  that took, AirPlay connections, flushes and packet stats

## 0.13.0 (2026-10-14)

- Shuffle and repeat (all / one) buttons in the player bar. Shuffle mixes the upcoming
  tracks and turning it off puts the queue back in order; both settings are remembered

## 0.12.1 (2026-10-12)

- Fixed: opening some artists (e.g. Royal Blood) crashed the app on greyscale pictures

## 0.12.0 (2026-10-12)

- Playlist management: add tracks to your playlists (right-click a track, or ⋯ → Add all
  to playlist on album and playlist pages), create playlists (empty from the sidebar's +,
  from tracks, or from the queue with Save), remove tracks from your own playlists, and
  rename or delete them

## 0.11.0 (2026-10-09)

- Home: "Mixes inspired by…" — a track's mix of similar songs, starting with it; the
  daily mixes are named "Daily mix 1–5" and the monthly one "My top <month>" in English

## 0.10.3 (2026-10-08)

- Fixed: track lists twitched while hovering or scrolling them

## 0.10.2 (2026-10-08)

- Fixed: AirPlay speakers sometimes weren't found after starting dust until it was
  restarted. While no speaker (or only part of one) has been found, dust now searches
  again every few seconds

## 0.10.1 (2026-10-07)

- Fixed: the heart on a track row flickered while the pointer was over it; queue rows
  also stay highlighted over their artist names

## 0.10.0 (2026-10-07)

- Tracks that can't be played yet (e.g. an album's unreleased songs) are greyed out,
  with the date they become available on hover; Play, Shuffle and Add to queue skip
  them

## 0.9.2 (2026-10-07)

- Fixed: losing the network path to an AirPlay speaker (e.g. unplugging Ethernet so the
  Mac falls back to Wi-Fi) froze dust for up to a minute and left the speaker silent.
  dust now notices, reconnects over the current network and carries on where it was

## 0.9.1 (2026-10-07)

- Fixed: after the Mac slept, AirPlay speakers could stay missing for up to an hour.
  dust now searches again when the computer wakes, when its network changes and
  whenever you open the speaker menu

## 0.9.0 (2026-10-07)

- Back / forward between pages: ‹ › next to the search field, `⌘←` / `⌘→` (`Alt+←` /
  `Alt+→` on Windows and Linux) and the mouse's side buttons; searches come back
  with their results
- Add a whole playlist or album to the queue: *Add to queue* on its page, or
  right-click your playlists in the sidebar and on the Playlists page

## 0.8.0 (2026-10-07)

- The queue survives a restart: dust brings it back paused at the same track and
  position (and Flow keeps going if that's what was playing)

## 0.7.1 (2026-10-07)

- The search field shows when it's active: accent ring, lighter background and accent
  icon while typing, a faint ring on hover; clicking anywhere on it focuses it

## 0.7.0 (2026-10-07)

- Likes: heart tracks in any track list or in the player bar, like albums from their
  page, follow artists from theirs; synced with your Deezer account
- Albums in the sidebar: the albums you like

## 0.6.0 (2026-10-07)

- Click the track title or cover in the player bar (or the title in the mini player) to
  open its album; album names in track lists open the album too

## 0.5.1 (2026-10-07)

- Fixed: buttons that open a web page did nothing, e.g. the update button when dust
  can't install an update itself ("dust x.y.z is out") and "Update failed"

## 0.5.0 (2026-10-07)

- Paste a Deezer link into search to open it: albums, artists, playlists, tracks (opens
  their album) and share links. Handy for brand-new releases Deezer's search doesn't
  list yet

## 0.4.1 (2026-10-07)

- Fixed: dragging (or just holding) the volume slider sent the volume to AirPlay
  speakers about 60 times a second; now only real changes are sent

## 0.4.0 (2026-10-06)

- Artist pages: popular tracks, the full discography (albums, singles & EPs, live &
  compilations), playlists featuring the artist and related artists
- Click any artist name (track lists, queue, player bar, mini player, album header) to
  open their page
- Artists in the sidebar: the artists you follow on Deezer
- Search shows your own matching playlists first

## 0.3.0 (2026-10-06)

- Self-update: new releases download in the background and are checked against
  `SHA256SUMS.txt`; *Restart to update* in the sidebar swaps them in and reopens dust,
  and a downloaded update also installs when dust quits. Check automatically or from the
  account menu; `--check-update` / `--self-update` on the command line. Install 0.3.0
  by hand once; later versions arrive in the app on macOS, Windows and Linux

## 0.2.0 (2026-10-06)

- Mini player: a small always-on-top window with cover, transport and seek bar
  (player bar button, `⌘⇧M` / `Ctrl+Shift+M`)
- Fixed: volume and play/pause buttons on AirPlay speakers (e.g. Sonos) arrived about
  35 seconds late and seemed to do nothing on Macs with Docker, VPN or VM networks
- AirPlay 2: dust now reads the speaker's event channel
- Debug logs (`DUST_LOG=debug`) carry timestamps

## 0.1.0 (2026-10-06)

First release: macOS (universal), Windows x64 and Linux x86_64/ARM64.

- Deezer home page (Flow and moods, mixes, recommendations), Loved tracks, playlists,
  albums, artist top tracks, search for tracks, artists, albums and playlists
- MP3 128/320 and FLAC streaming with seeking, queue, endless Flow
- AirPlay 2 and AirPlay 1 output with discovery; speaker buttons control dust
- Media keys and the OS Now Playing widget
- Last.fm scrobbling and listening history through Deezer
- Dark and light themes, remembered speaker and volume
