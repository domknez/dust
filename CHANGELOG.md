# Changelog

## Unreleased

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
