#!/usr/bin/env bash
# Prints where the bytes in Popcorn.app go and fails if the app is over 50 MB.
# Sizes are the sum of file sizes, in decimal megabytes (as Finder shows them).
#
# Usage: scripts/check-size.sh [path/to/Popcorn.app] [path/to/Popcorn.dmg]
set -euo pipefail

LIMIT_BYTES=50000000

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-$ROOT/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Popcorn.app}"
APP="${APP%/}"
DMG="${2:-}"

[[ -d "$APP/Contents" ]] || { echo "error: not an app bundle: $APP" >&2; exit 1; }

# Total size of the regular files under a path (symlinks are not followed).
bytes() {
  [[ -e "$1" ]] || { echo 0; return; }
  find "$1" -type f -exec stat -f %z {} + | awk '{s += $1} END {printf "%d\n", s}'
}
row() { awk -v n="$1" -v b="$2" 'BEGIN {printf "  %-34s %9.2f MB  %12d bytes\n", n, b / 1e6, b}'; }

total="$(bytes "$APP")"
binaries="$(bytes "$APP/Contents/MacOS")"
frameworks="$(bytes "$APP/Contents/Frameworks")"
resources="$(bytes "$APP/Contents/Resources")"

echo "Size breakdown: $APP"
for file in "$APP/Contents/MacOS"/*; do
  [[ -f "$file" ]] && row "binary: $(basename "$file")" "$(bytes "$file")"
done
dylibs=0
for file in "$APP/Contents/Frameworks"/*.dylib; do
  [[ -f "$file" ]] || continue
  row "dylib: $(basename "$file")" "$(bytes "$file")"
  dylibs=$((dylibs + 1))
done
((dylibs > 0)) || echo "  warning: no dylibs in Contents/Frameworks (was bundle-dylibs.sh run?)"
row "resources" "$resources"
row "other (Info.plist, signature...)" $((total - binaries - frameworks - resources))
row "TOTAL" "$total"

if [[ -d "$ROOT/ui" ]]; then
  echo "UI source (embedded in the binary, not counted twice):"
  row "ui/" "$(bytes "$ROOT/ui")"
fi

if [[ -z "$DMG" ]]; then
  # Default to the newest DMG next to the app: bundle/macos/App.app -> bundle/dmg/
  DMG="$(ls -t "$(dirname "$APP")/../dmg"/*.dmg 2>/dev/null | head -n 1 || true)"
fi
if [[ -n "$DMG" && -f "$DMG" ]]; then
  echo "Disk image: $DMG"
  row "$(basename "$DMG")" "$(stat -f %z "$DMG")"
fi

if ((total > LIMIT_BYTES)); then
  echo "FAIL: Popcorn.app is $total bytes, over the limit of $LIMIT_BYTES (50 MB)" >&2
  exit 1
fi
echo "OK: $total bytes is within the $LIMIT_BYTES byte (50 MB) limit"
