#!/bin/sh
# Build dust.app (universal: Apple Silicon + Intel) and dist/dust-<version>-macos.dmg.
#
# Signing, all optional:
#   MACOS_SIGN_IDENTITY   e.g. "Developer ID Application: Name (TEAMID)": hardened
#                         runtime + timestamp. Without it the app is ad-hoc signed and
#                         Gatekeeper asks the user to allow it once.
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD
#                         notarize and staple the DMG (needs MACOS_SIGN_IDENTITY).
set -e
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dist=dist
app=$dist/dust.app
dmg=$dist/dust-$version-macos.dmg
export MACOSX_DEPLOYMENT_TARGET=11.0

rm -rf "$app" "$dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$target" >/dev/null 2>&1 || true
  cargo build --release --locked --target "$target"
done
lipo -create -output "$app/Contents/MacOS/dust" \
  target/aarch64-apple-darwin/release/dust target/x86_64-apple-darwin/release/dust

# Icon: the artwork inset on a transparent canvas like other macOS app icons
# (836/1024, see build.rs), otherwise it looks oversized in Finder and the Dock.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
iconset=$work/dust.iconset
mkdir "$iconset"
sips -z 836 836 assets/dust-icon-pack/png/dust-1024.png --out "$work/art.png" >/dev/null
sips -p 1024 1024 "$work/art.png" --out "$work/icon.png" >/dev/null
for size in 16 32 128 256 512; do
  sips -z $size $size "$work/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double "$work/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns -o "$app/Contents/Resources/dust.icns" "$iconset"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>dust</string>
  <key>CFBundleDisplayName</key><string>dust</string>
  <key>CFBundleIdentifier</key><string>io.github.domknez.dust</string>
  <key>CFBundleExecutable</key><string>dust</string>
  <key>CFBundleIconFile</key><string>dust</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSLocalNetworkUsageDescription</key>
  <string>dust finds AirPlay speakers on your network and streams music to them.</string>
  <key>NSBonjourServices</key>
  <array>
    <string>_airplay._tcp</string>
    <string>_raop._tcp</string>
    <string>_dacp._tcp</string>
  </array>
  <key>NSHumanReadableCopyright</key><string>MIT License. Free and open source.</string>
</dict>
</plist>
PLIST

if [ -n "$MACOS_SIGN_IDENTITY" ]; then
  codesign --force --options runtime --timestamp --sign "$MACOS_SIGN_IDENTITY" "$app"
else
  codesign --force --sign - "$app"
fi
codesign --verify --strict "$app"

staging=$work/dmg
mkdir "$staging"
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -volname dust -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$dmg" >/dev/null

if [ -n "$MACOS_SIGN_IDENTITY" ]; then
  codesign --force --timestamp --sign "$MACOS_SIGN_IDENTITY" "$dmg"
  if [ -n "$APPLE_ID" ] && [ -n "$APPLE_TEAM_ID" ] && [ -n "$APPLE_APP_PASSWORD" ]; then
    xcrun notarytool submit "$dmg" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" \
      --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$dmg"
  fi
fi

echo "$dmg"
