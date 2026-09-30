# dust

**A free and open source, ultra-lightweight Deezer desktop client written in Rust — with
AirPlay 2 streaming built in.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Free and open source](https://img.shields.io/badge/free%20%26%20open%20source-yes-brightgreen.svg)
![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg)

dust is free software: free to use, study, modify and share under the [MIT license](LICENSE).
No ads, no telemetry, no account with us — just your Deezer subscription.
Inspired by [SpotLight](https://github.com/dddevid/SpotLight).

| | dust | Deezer desktop |
|---|---|---|
| Binary | ~8 MB | 300+ MB |
| Idle memory | ~80 MB (incl. cover art cache) | 400+ MB |
| Idle CPU | ~0% (UI sleeps when untouched) | |

## Features

- **Music:** your Flow, Loved tracks, playlists (list and grid), search
- **Quality:** MP3 128 / MP3 320 / FLAC (FLAC needs a HiFi plan), streamed and decoded
  on the fly — nothing is written to disk
- **Seeking** (HTTP range jumps for MP3), shuffle, queue auto-advance
- **AirPlay 2 and AirPlay 1** output with automatic discovery — Sonos (incl. Era 100/300),
  HomePod-class receivers, Apple TV, AirPort Express, shairport-sync and most AirPlay speakers
- **Speaker buttons work:** volume and play/pause/skip on the speaker control dust
- **Modern UI:** Tidal-style layout with cover art, soft dark and light themes (or follow
  the system), Inter typeface
- **Secure sign-in:** "Log in with Deezer" opens Deezer's own login page; the session is
  kept in the OS keychain (macOS Keychain, Windows Credential Manager, Secret Service)
- Runs on macOS, Linux and Windows

## Build

```sh
cargo build --release
./target/release/dust
```

On macOS, run `scripts/dev-cert.sh` once and build with `scripts/build.sh`: it signs
builds with a local "dust dev" identity so Keychain stops asking for access after every
rebuild. (Published releases should be signed with an Apple Developer ID instead.)

Linux needs ALSA, D-Bus and WebKitGTK headers:
`sudo apt install libasound2-dev libdbus-1-dev libwebkit2gtk-4.1-dev pkg-config`.
Without the login window (no WebKitGTK): `cargo build --release --no-default-features`.

## Log in

Click **Log in with Deezer**: a small window shows deezer.com's own login page (OS
webview, private session). Once you are signed in, dust picks up the session and closes
it. The session token (`arl` cookie) is stored in your OS keychain; **Log out** in the
account menu removes it. Deezer Premium required. Email/password sign-in works; Google
and Apple sign-in may refuse embedded browsers.

Fallback: "Paste ARL cookie instead" takes the `arl` value from a logged-in browser
(DevTools → Application → Cookies). Treat it like a password; "Log out of all devices"
in Deezer's account settings revokes it.

## Settings

Click your name at the bottom of the sidebar:

- **Appearance:** System, Dark or Light
- **Streaming quality:** MP3 128, MP3 320 or FLAC

Choices are saved in `settings.conf` in your config directory
(`~/Library/Application Support/dust` on macOS, `%APPDATA%\dust` on Windows,
`~/.config/dust` on Linux).

## AirPlay

Pick a speaker from the AirPlay button next to the volume slider. Expect about 2 s of
latency, the same as other AirPlay senders.

- **AirPlay 2** receivers (most speakers from recent years, e.g. Sonos Era): dust pairs
  transiently (HomeKit, no PIN), encrypts control and audio, and runs a small PTP clock
  that the speaker follows. PTP uses UDP ports 319/320: fine without root on macOS and
  Windows; on Linux grant `CAP_NET_BIND_SERVICE`
  (`sudo setcap cap_net_bind_service=+ep target/release/dust`), otherwise dust falls back
  to NTP timing, which many AirPlay 2 speakers accept but won't play.
- **AirPlay 1** receivers with unencrypted audio (`et=0`) use the classic RAOP path.
- Not supported yet: receivers that need an AirPlay password or PIN pairing.

Speaker hardware buttons reach dust through DACP (an mDNS-advertised remote-control
service), so volume changes on the speaker move dust's slider.

Check an output without logging in:

```sh
dust --tone                                # 4 s test tone on the local sound card
DUST_DEBUG=1 dust --tone "Living Room"     # ...on an AirPlay speaker, with packet stats
```

## Shortcuts

- `Space`: play/pause
- Double-click a track, or click its number: play from there

## Credits

- AirPlay 2 pairing, encryption and PTP timing follow the approach of
  [OwnTone](https://github.com/owntone/owntone-server) (its PTP library, libairptp, is MIT)
  and were validated against [shairport-sync](https://github.com/mikebrady/shairport-sync).
- [Inter](https://rsms.me/inter/) typeface by Rasmus Andersson, SIL Open Font License
  ([assets/fonts/OFL-Inter.txt](assets/fonts/OFL-Inter.txt)).
- Built with [egui](https://github.com/emilk/egui), [symphonia](https://github.com/pdeljanov/Symphonia),
  [cpal](https://github.com/RustAudio/cpal) and [wry](https://github.com/tauri-apps/wry).

## License

dust is free and open source software, released under the [MIT license](LICENSE).
Contributions are welcome.

## Disclaimer

dust is an unofficial client and is not affiliated with or endorsed by Deezer. It uses
Deezer's private web API with your own subscription, for personal use; it may break at
any time or conflict with Deezer's terms of service.
