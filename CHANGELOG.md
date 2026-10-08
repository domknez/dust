# Changelog

## Unreleased

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
