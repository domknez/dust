# Changelog

## Unreleased

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
