#!/usr/bin/env bash
# Builds the distributable: Popcorn.app with libmpv inside, and a DMG made from it.
# Tauri's own DMG step is skipped because it would run before libmpv is bundled.
#
# Usage: scripts/package.sh [extra "cargo tauri build" arguments]
# Env:   CODESIGN_IDENTITY  optional Developer ID identity (default: ad-hoc)
# Needs: Rust, tauri-cli v2, and the build tools of scripts/build-libmpv.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPTS="$ROOT/scripts"
TARGET=aarch64-apple-darwin
BUNDLE="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}/$TARGET/release/bundle"
APP="$BUNDLE/macos/Popcorn.app"

export MACOSX_DEPLOYMENT_TARGET=13.0

# Does nothing if the library is already built from the current script.
"$SCRIPTS/build-libmpv.sh"

(cd "$ROOT" && cargo tauri build --target "$TARGET" --bundles app "$@")

"$SCRIPTS/bundle-dylibs.sh" "$APP"

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
DMG="$BUNDLE/dmg/Popcorn_${VERSION}_aarch64.dmg"
mkdir -p "$BUNDLE/dmg"
rm -f "$DMG"

# Check the app before spending time on the disk image.
"$SCRIPTS/check-size.sh" "$APP"

STAGING="$(mktemp -d "${TMPDIR:-/tmp}/popcorn-dmg.XXXXXX")"
trap 'rm -rf "$STAGING"' EXIT
ditto "$APP" "$STAGING/Popcorn.app"
ln -s /Applications "$STAGING/Applications"
# ULMO (LZMA) is much smaller than UDZO and is readable since macOS 10.15.
hdiutil create -volname Popcorn -srcfolder "$STAGING" -fs HFS+ -format ULMO -ov "$DMG"
hdiutil verify "$DMG"

if [[ -n "${CODESIGN_IDENTITY:-}" && "$CODESIGN_IDENTITY" != "-" ]]; then
  codesign --force --sign "$CODESIGN_IDENTITY" --timestamp "$DMG"
fi

echo
"$SCRIPTS/check-size.sh" "$APP" "$DMG"
