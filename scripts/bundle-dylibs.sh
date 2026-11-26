#!/usr/bin/env bash
# Puts libmpv (and any non-system library it needs) into Popcorn.app/Contents/Frameworks,
# makes the install names relocatable, and signs the dylibs and then the app.
# Safe to run repeatedly on the same app.
#
# Usage: scripts/bundle-dylibs.sh [path/to/Popcorn.app]
# Env:   LIBMPV_DIR         where libmpv.2.dylib was built (default build/prefix/lib)
#        CODESIGN_IDENTITY  a "Developer ID Application: ..." identity. Default "-" (ad-hoc).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-$ROOT/src-tauri/target/aarch64-apple-darwin/release/bundle/macos/Popcorn.app}"
APP="${APP%/}"
LIBDIR="${LIBMPV_DIR:-$ROOT/build/prefix/lib}"
LICENSES="$LIBDIR/../share/licenses"
IDENTITY="${CODESIGN_IDENTITY:--}"
FRAMEWORKS="$APP/Contents/Frameworks"

die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[[ -d "$APP/Contents/MacOS" ]] || die "not an app bundle: $APP"
[[ -f "$LIBDIR/libmpv.2.dylib" ]] || die "$LIBDIR/libmpv.2.dylib not found. Run scripts/build-libmpv.sh first"

# Install names a Mach-O file links to, without its own id.
deps_of() {
  local id
  id="$(otool -D "$1" | tail -n +2)"
  otool -L "$1" | tail -n +2 | awk '{print $1}' | grep -vxF -- "${id:-<none>}" || true
}

is_system() { [[ "$1" == /usr/lib/* || "$1" == /System/Library/* ]]; }

mkdir -p "$FRAMEWORKS"
rm -f "$FRAMEWORKS"/*.dylib

# Copy libmpv, then everything non-system it depends on, breadth first. With the
# static build from build-libmpv.sh there is only libmpv itself.
queue=(libmpv.2.dylib)
i=0
while ((i < ${#queue[@]})); do
  name="${queue[$i]}"
  i=$((i + 1))
  dest="$FRAMEWORKS/$name"
  cp -L "$LIBDIR/$name" "$dest"
  chmod 755 "$dest"
  install_name_tool -id "@rpath/$name" "$dest"

  while IFS= read -r dep; do
    [[ -n "$dep" ]] || continue
    is_system "$dep" && continue
    base="$(basename "$dep")"
    case "$dep" in
      @rpath/* | @loader_path/*) ;;
      "$LIBDIR"/*) ;;
      *) die "$name depends on $dep, which is neither a system library nor part of $LIBDIR" ;;
    esac
    [[ -f "$LIBDIR/$base" ]] || die "$name depends on $dep but $LIBDIR/$base does not exist"
    install_name_tool -change "$dep" "@loader_path/$base" "$dest"
    [[ " ${queue[*]} " == *" $base "* ]] || queue+=("$base")
  done < <(deps_of "$dest")
done

# Drop build-machine rpaths: dependencies resolve through @loader_path only.
for dylib in "$FRAMEWORKS"/*.dylib; do
  while IFS= read -r rpath; do
    case "$rpath" in
      @* | /usr/lib/*) ;;
      *) install_name_tool -delete_rpath "$rpath" "$dylib" ;;
    esac
  done < <(otool -l "$dylib" | awk '$1=="cmd"{r=($2=="LC_RPATH")} r && $1=="path"{print $2}')
done

# License texts for the bundled components travel with the app (GPL requirement).
DOCS="$APP/Contents/Resources/licenses"
rm -rf "$DOCS"
mkdir -p "$DOCS"
cp "$ROOT/LICENSE" "$DOCS/Popcorn-GPL-3.0.txt"
cp "$ROOT/docs/THIRD_PARTY.md" "$DOCS/"
[[ -d "$LICENSES" ]] && cp -R "$LICENSES/." "$DOCS/"

# Sign inside-out: dylibs first, then the app, which seals them.
sign_args=(--force --sign "$IDENTITY")
if [[ "$IDENTITY" != "-" ]]; then
  # Needed for notarization.
  sign_args+=(--options runtime --timestamp)
fi
for dylib in "$FRAMEWORKS"/*.dylib; do
  codesign "${sign_args[@]}" "$dylib"
done
codesign "${sign_args[@]}" "$APP"
codesign --verify --deep --strict "$APP" || die "signature verification failed"

# Nothing in the app may point outside the app or the operating system.
failed=0
for file in "$APP/Contents/MacOS"/* "$FRAMEWORKS"/*.dylib; do
  while IFS= read -r dep; do
    [[ -n "$dep" ]] || continue
    is_system "$dep" && continue
    case "$dep" in
      @loader_path/* | @executable_path/* | @rpath/*) ;;
      *)
        echo "error: $file links to $dep" >&2
        failed=1
        ;;
    esac
  done < <(deps_of "$file")
done
((failed == 0)) || die "the app still has external library references"

echo "Bundled into $FRAMEWORKS:"
for dylib in "$FRAMEWORKS"/*.dylib; do
  echo "  $(basename "$dylib") ($(stat -f %z "$dylib") bytes)"
done
