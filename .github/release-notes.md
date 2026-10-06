## Download

| Platform | File |
|---|---|
| macOS 11+ (Apple Silicon and Intel) | `dust-…-macos.dmg` |
| Windows 10/11 (x64) | `dust-…-windows-x64.zip` |
| Linux x86_64 / ARM64 | `dust-…-linux-x86_64.tar.gz` / `dust-…-linux-aarch64.tar.gz` |

Deezer Premium required. `SHA256SUMS.txt` lists checksums of every file.

**macOS:** open the DMG and drag dust to Applications. dust isn't notarized by Apple yet,
so the first launch is blocked: open it once, then go to **System Settings → Privacy &
Security** and click **Open Anyway**. After an update, macOS may ask once more for dust
to use your saved login in the Keychain; choose **Always Allow**.

**Windows:** unzip and run `dust.exe`. If SmartScreen says "Windows protected your PC",
click **More info → Run anyway**.

**Linux:** unpack and run `./dust`. Needs WebKitGTK 4.1, ALSA and D-Bus (on Debian/Ubuntu:
`sudo apt install libwebkit2gtk-4.1-0 libasound2t64 libdbus-1-3`). For a menu entry, copy
`dust` to `~/.local/bin`, `dust.desktop` to `~/.local/share/applications` and `dust.png`
to `~/.local/share/icons`.
