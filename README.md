# dust

Blazingly fast, ultra-lightweight Deezer desktop client in Rust, with AirPlay output.
Inspired by [SpotLight](https://github.com/dddevid/SpotLight).

| | dust | Deezer desktop |
|---|---|---|
| Binary | ~7 MB | 300+ MB |
| Idle memory | ~55 MB | 400+ MB |
| Idle CPU | ~0% (UI sleeps when untouched) | |

## Features

- Search, Flow, Loved tracks, your playlists
- MP3 128 / MP3 320 / FLAC (FLAC needs a HiFi plan), streamed and decoded on the fly, nothing written to disk
- Seek (HTTP range jumps for MP3)
- **AirPlay** output (RAOP), with mDNS discovery; works with Sonos, AirPort Express,
  Apple TV, shairport-sync and most AirPlay speakers
- Credentials in the OS keychain (macOS Keychain, Windows Credential Manager, Secret Service)
- Runs on macOS, Linux, Windows

## Build

```sh
cargo build --release
./target/release/dust
```

Linux needs ALSA and D-Bus headers: `sudo apt install libasound2-dev libdbus-1-dev pkg-config`.

## Log in

Click **Log in with Deezer**: a small window shows deezer.com's own login page (OS
webview, incognito). Once you are signed in, dust picks up the session and closes it.
The session token (`arl` cookie) goes to your OS keychain; "Log out" removes it.
Deezer Premium required.

Fallback: "Paste ARL cookie instead" takes the `arl` value from a logged-in browser
(DevTools → Application → Cookies). Treat it like a password; "Log out of all devices"
in Deezer's account settings revokes it.

Linux builds need `libwebkit2gtk-4.1-dev` for the login window, or build without it:
`cargo build --release --no-default-features`.

## AirPlay

Pick a device in the output menu next to the volume slider. dust speaks AirPlay 1
(RAOP) with unencrypted ALAC, so it works with receivers that advertise `et=0`.
Devices that need an AirPlay password or only accept AirPlay 2 pairing (e.g. a Mac's
own "AirPlay Receiver", HomePod) show up but are not supported yet. Expect about 2 s
of latency on AirPlay, same as iTunes.

Check an output without logging in:

```sh
dust --tone              # 4 s test tone on the local sound card
DUST_DEBUG=1 dust --tone "Living Room"   # ...on an AirPlay device, with packet stats
```

## Shortcuts

- `Space`: play/pause
- Double-click a track: play from there

## Notes

Uses Deezer's private web API. For personal use with your own subscription; this may
break or conflict with Deezer's terms of service.
