#!/bin/sh
# Release build. On macOS, signs with a local "dust dev" identity if present, so
# Keychain recognises every rebuild as the same app and stops re-prompting.
# Create the identity once with scripts/dev-cert.sh.
set -e
cd "$(dirname "$0")/.."
cargo build --release "$@"
if [ "$(uname)" = "Darwin" ] && security find-identity -v -p codesigning 2>/dev/null | grep -q '"dust dev"'; then
  codesign --force --sign "dust dev" --identifier dev.dust.app target/release/dust
  echo "signed target/release/dust with \"dust dev\""
fi
