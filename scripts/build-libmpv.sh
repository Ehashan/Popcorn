#!/usr/bin/env bash
# Builds a slim, self-contained arm64 libmpv.2.dylib for Popcorn.
#
# Every dependency is built as a static library and linked into the one dylib, so
# the result depends only on macOS system libraries and frameworks.
# Nothing is installed outside <repo>/build/.
#
# Usage: scripts/build-libmpv.sh [--force]
# Needs: Xcode Command Line Tools, meson, ninja, pkgconf  (brew install meson ninja pkgconf)
# Output: build/prefix/lib/libmpv.2.dylib and build/prefix/share/licenses/
set -euo pipefail

# ---------------------------------------------------------------------------
# Pinned versions and SHA-256 checksums. Change a version and its hash together,
# and keep docs/THIRD_PARTY.md in step.
# ---------------------------------------------------------------------------
MPV_VERSION=0.42.0
MPV_SHA256=bcfebbfa2b65a6d88dc97ea18117831072373e38b5785a2afb874ed4df5070dd
FFMPEG_VERSION=9.0.2
FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
DAV1D_VERSION=1.5.5
DAV1D_SHA256=ca96b1d0ba3f01f55de7231d775892b64994ba061aaeeaaa4f662a390b3abf9e
LIBPLACEBO_VERSION=7.387.0
LIBPLACEBO_SHA256=3bf5d72786682c194387c33d563872f6bfe202a9816f9dde6535a31a9b2e0d74
LIBASS_VERSION=0.17.5
LIBASS_SHA256=2dca25c0e0c837ddf00b52011b3f82cac1e4ddd3ad018227806b0c2288864acc
FREETYPE_VERSION=2.15.0
FREETYPE_SHA256=93cd47eb719c325107f2d3cf087d06299e944c3b48a40b55537302304dea1dd5
FRIBIDI_VERSION=1.0.18
FRIBIDI_SHA256=1de5e8073d035388b71d0b2d452b1da1f8caeea8a7702997b401fe1b6250727f
HARFBUZZ_VERSION=15.2.0
HARFBUZZ_SHA256=e2ec49090744383cdbe7df808c1cedaafba06a97be8dc4211fe546ba585f12cb
# Build-time only (libplacebo's git submodules, not present in its archive).
GLAD_VERSION=2.0.8
GLAD_SHA256=44f06f9195427c7017f5028d0894f57eb216b0a8f7c4eda7ce883732aeb2d0fc
JINJA_VERSION=3.1.6
JINJA_SHA256=0137fb05990d35f1275a587e9aee6d56da821fc83491a0fb838183be43f66d6d
MARKUPSAFE_VERSION=3.0.4
MARKUPSAFE_SHA256=2e9ad7dd851bf45fab9f75cbff4cb493fee9979e8d8c7c9c3ee119022518edd6
# The commit libplacebo 7.387.0 pins. Headers only: libplacebo's "Vulkan is off"
# stubs still include them. No Vulkan code or library ends up in the result.
VULKAN_HEADERS_COMMIT=74d8a6cb930c68ef617b202c3ff3c59d919e086b
VULKAN_HEADERS_SHA256=75967a8288ff16044a86d1af462b96d2e75c435937efb75f2bb4102ab0382f29
# Header-only, compiled into libplacebo.
FASTFLOAT_VERSION=8.4.0
FASTFLOAT_SHA256=0bf4277827259720028bbef6c11d400f07482608bea5952b289baaef6af141a4

# ---------------------------------------------------------------------------
# Paths
# ---------------------------------------------------------------------------
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD="$ROOT/build"
DOWNLOADS="$BUILD/downloads"
SRC="$BUILD/src"
PREFIX="$BUILD/prefix"
TOOLS="$BUILD/tools"
DYLIB="$PREFIX/lib/libmpv.2.dylib"
STAMP="$PREFIX/.build-stamp"

log() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || die "this script only builds on Apple Silicon macOS"
# The autotools-style FFmpeg build does not cope with spaces in paths.
[[ "$ROOT" != *" "* ]] || die "the repository path must not contain spaces: $ROOT"

SCRIPT_HASH="$(shasum -a 256 "${BASH_SOURCE[0]}" | cut -d' ' -f1)"
if [[ "${1:-}" != "--force" && -f "$DYLIB" && -f "$STAMP" && "$(cat "$STAMP")" == "$SCRIPT_HASH" ]]; then
  log "libmpv is up to date: $DYLIB (use --force to rebuild)"
  exit 0
fi

# ---------------------------------------------------------------------------
# Hermetic environment: only the named build tools are visible, and pkg-config
# only sees our prefix, so Homebrew libraries cannot leak into the build.
# ---------------------------------------------------------------------------
rm -rf "$TOOLS" "$PREFIX" "$SRC"
mkdir -p "$DOWNLOADS" "$SRC" "$PREFIX/lib/pkgconfig" "$PREFIX/share/licenses" "$TOOLS"

for tool in meson ninja pkgconf; do
  path="$(command -v "$tool" || true)"
  [[ -n "$path" ]] || die "$tool not found. Install with: brew install meson ninja pkgconf"
  ln -s "$path" "$TOOLS/$tool"
done
# Our dependencies are static, so pkg-config must always report private libs.
# (meson's prefer_static would do that too, but it also breaks system libiconv.)
printf '#!/bin/sh\nexec "%s" --static "$@"\n' "$TOOLS/pkgconf" >"$TOOLS/pkg-config"
chmod +x "$TOOLS/pkg-config"

unset CPATH C_INCLUDE_PATH CPLUS_INCLUDE_PATH OBJC_INCLUDE_PATH LIBRARY_PATH \
  DYLD_LIBRARY_PATH DYLD_FALLBACK_LIBRARY_PATH PKG_CONFIG_PATH PKG_CONFIG_SYSROOT_DIR \
  CFLAGS CXXFLAGS CPPFLAGS OBJCFLAGS LDFLAGS PYTHONPATH SDKROOT
export PATH="$TOOLS:/usr/bin:/bin:/usr/sbin:/sbin"
export PKG_CONFIG="$TOOLS/pkg-config"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"
export MACOSX_DEPLOYMENT_TARGET=13.0
export SDKROOT
SDKROOT="$(xcrun --sdk macosx --show-sdk-path)"
export CC=clang CXX=clang++ OBJC=clang
export ZERO_AR_DATE=1 SOURCE_DATE_EPOCH=0 LC_ALL=C

# -Werror=unguarded-availability-new: fail instead of silently using an API that
# is newer than macOS 13 (it would crash on older systems).
ARCH_FLAGS="-arch arm64 -mmacosx-version-min=$MACOSX_DEPLOYMENT_TARGET"
COMMON_FLAGS="$ARCH_FLAGS -Werror=unguarded-availability-new -ffile-prefix-map=$BUILD=."
export CFLAGS="$COMMON_FLAGS" CXXFLAGS="$COMMON_FLAGS" OBJCFLAGS="$COMMON_FLAGS" LDFLAGS="$ARCH_FLAGS"
JOBS="$(sysctl -n hw.ncpu)"

# ---------------------------------------------------------------------------
# Download, verify, extract
# ---------------------------------------------------------------------------
# fetch <file name> <https url> <sha256>
fetch() {
  local file="$DOWNLOADS/$1" url="$2" sha="$3"
  if [[ ! -f "$file" ]] || ! echo "$sha  $file" | shasum -a 256 -c - >/dev/null 2>&1; then
    log "Downloading $1"
    curl --fail --location --silent --show-error \
      --proto '=https' --proto-redir '=https' --tlsv1.2 \
      --retry 5 --retry-all-errors --connect-timeout 30 \
      --output "$file.part" "$url"
    mv "$file.part" "$file"
  fi
  local actual
  actual="$(shasum -a 256 "$file" | cut -d' ' -f1)"
  if [[ "$actual" != "$sha" ]]; then
    rm -f "$file"
    die "checksum mismatch for $1: expected $sha, got $actual"
  fi
}

# unpack <archive name> <destination dir>: strips the archive's top directory.
unpack() {
  mkdir -p "$2"
  tar -xf "$DOWNLOADS/$1" -C "$2" --strip-components=1
}

GH=https://github.com
fetch "mpv-$MPV_VERSION.tar.gz" "$GH/mpv-player/mpv/archive/refs/tags/v$MPV_VERSION.tar.gz" "$MPV_SHA256"
fetch "ffmpeg-$FFMPEG_VERSION.tar.xz" "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" "$FFMPEG_SHA256"
fetch "dav1d-$DAV1D_VERSION.tar.xz" "https://download.videolan.org/pub/videolan/dav1d/$DAV1D_VERSION/dav1d-$DAV1D_VERSION.tar.xz" "$DAV1D_SHA256"
fetch "libplacebo-v$LIBPLACEBO_VERSION.tar.gz" "https://code.videolan.org/videolan/libplacebo/-/archive/v$LIBPLACEBO_VERSION/libplacebo-v$LIBPLACEBO_VERSION.tar.gz" "$LIBPLACEBO_SHA256"
fetch "libass-$LIBASS_VERSION.tar.xz" "$GH/libass/libass/releases/download/$LIBASS_VERSION/libass-$LIBASS_VERSION.tar.xz" "$LIBASS_SHA256"
fetch "freetype-$FREETYPE_VERSION.tar.xz" "https://download.savannah.gnu.org/releases/freetype/freetype-$FREETYPE_VERSION.tar.xz" "$FREETYPE_SHA256"
fetch "fribidi-$FRIBIDI_VERSION.tar.xz" "$GH/fribidi/fribidi/releases/download/v$FRIBIDI_VERSION/fribidi-$FRIBIDI_VERSION.tar.xz" "$FRIBIDI_SHA256"
fetch "harfbuzz-$HARFBUZZ_VERSION.tar.xz" "$GH/harfbuzz/harfbuzz/releases/download/$HARFBUZZ_VERSION/harfbuzz-$HARFBUZZ_VERSION.tar.xz" "$HARFBUZZ_SHA256"
fetch "glad-$GLAD_VERSION.tar.gz" "$GH/Dav1dde/glad/archive/refs/tags/v$GLAD_VERSION.tar.gz" "$GLAD_SHA256"
fetch "jinja2-$JINJA_VERSION.tar.gz" "https://files.pythonhosted.org/packages/df/bf/f7da0350254c0ed7c72f3e33cef02e048281fec7ecec5f032d4aac52226b/jinja2-$JINJA_VERSION.tar.gz" "$JINJA_SHA256"
fetch "markupsafe-$MARKUPSAFE_VERSION.tar.gz" "https://files.pythonhosted.org/packages/38/9b/e422a865e1d5d57d0e509b4e0bf1c1a70a7f6382c29a5aa428df994c8bc8/markupsafe-$MARKUPSAFE_VERSION.tar.gz" "$MARKUPSAFE_SHA256"
fetch "fast_float-$FASTFLOAT_VERSION.tar.gz" "$GH/fastfloat/fast_float/archive/refs/tags/v$FASTFLOAT_VERSION.tar.gz" "$FASTFLOAT_SHA256"

fetch "vulkan-headers-${VULKAN_HEADERS_COMMIT:0:8}.tar.gz" "$GH/KhronosGroup/Vulkan-Headers/archive/$VULKAN_HEADERS_COMMIT.tar.gz" "$VULKAN_HEADERS_SHA256"

log "Extracting"
unpack "mpv-$MPV_VERSION.tar.gz" "$SRC/mpv"
unpack "ffmpeg-$FFMPEG_VERSION.tar.xz" "$SRC/ffmpeg"
unpack "dav1d-$DAV1D_VERSION.tar.xz" "$SRC/dav1d"
unpack "libplacebo-v$LIBPLACEBO_VERSION.tar.gz" "$SRC/libplacebo"
unpack "libass-$LIBASS_VERSION.tar.xz" "$SRC/libass"
unpack "freetype-$FREETYPE_VERSION.tar.xz" "$SRC/freetype"
unpack "fribidi-$FRIBIDI_VERSION.tar.xz" "$SRC/fribidi"
unpack "harfbuzz-$HARFBUZZ_VERSION.tar.xz" "$SRC/harfbuzz"
unpack "glad-$GLAD_VERSION.tar.gz" "$SRC/libplacebo/3rdparty/glad"
unpack "jinja2-$JINJA_VERSION.tar.gz" "$SRC/libplacebo/3rdparty/jinja"
unpack "markupsafe-$MARKUPSAFE_VERSION.tar.gz" "$SRC/libplacebo/3rdparty/markupsafe"
unpack "fast_float-$FASTFLOAT_VERSION.tar.gz" "$SRC/libplacebo/3rdparty/fast_float"
unpack "vulkan-headers-${VULKAN_HEADERS_COMMIT:0:8}.tar.gz" "$SRC/libplacebo/3rdparty/Vulkan-Headers"

# The only source change. mpv 0.42.0 does not compile with Swift on and Vulkan
# off: view.swift names its Vulkan-only MetalLayer class. The system base class
# CAMetalLayer behaves the same here.
sed -i '' 's/if let metalLayer = layer as? MetalLayer {/if let metalLayer = layer as? CAMetalLayer {/' \
  "$SRC/mpv/video/out/mac/view.swift"
grep -q 'layer as? CAMetalLayer' "$SRC/mpv/video/out/mac/view.swift" ||
  die "the mpv view.swift patch no longer applies"

# meson_dep <name> [options...]: static release build installed into PREFIX.
# --auto-features=disabled means anything not explicitly enabled stays off, and
# --wrap-mode=nodownload stops meson from fetching subprojects on its own.
meson_dep() {
  local name="$1"
  shift
  log "Building $name"
  meson setup "$SRC/$name/_build" "$SRC/$name" \
    --prefix="$PREFIX" --libdir=lib --buildtype=release \
    --default-library=static --wrap-mode=nodownload --auto-features=disabled \
    -Db_ndebug=true "$@"
  ninja -C "$SRC/$name/_build" -j "$JOBS"
  meson install -C "$SRC/$name/_build" --no-rebuild >/dev/null
}

meson_dep freetype -Dzlib=system
meson_dep fribidi -Ddocs=false -Dbin=false -Dtests=false
meson_dep harfbuzz -Dfreetype=enabled -Dtests=disabled -Ddocs=disabled -Dutilities=disabled \
  -Dsubset=disabled -Draster=disabled -Dvector=disabled -Dgpu=disabled
# CoreText is the system font provider; fontconfig stays off.
meson_dep libass -Dcoretext=enabled -Dfontconfig=disabled -Dasm=enabled
meson_dep dav1d -Denable_tools=false -Denable_tests=false
# mpv requires libplacebo. OpenGL only, no Vulkan.
meson_dep libplacebo -Dvulkan=disabled -Dopengl=enabled -Dgl-proc-addr=enabled \
  -Ddovi=enabled -Ddemos=false \
  -Dc_args="$CFLAGS -I$SRC/libplacebo/3rdparty/Vulkan-Headers/include" \
  -Dcpp_args="$CXXFLAGS -I$SRC/libplacebo/3rdparty/Vulkan-Headers/include"

# ---------------------------------------------------------------------------
# FFmpeg (LGPL v2.1+: no --enable-gpl, no --enable-nonfree)
# ---------------------------------------------------------------------------
join() { local IFS=,; echo "$*"; }

# srt, ass and webvtt are needed to load external subtitle files. mpegvideo is how
# libavformat identifies the video stream inside MPEG program streams (.mpg).
DEMUXERS=(matroska mov avi mpegts flv asf mpegps mpegvideo srt ass webvtt)
# aac_latm is AAC as carried in many .ts files; mlp is the parser TrueHD uses.
PARSERS=(h264 hevc av1 vp9 vp8 mpegvideo mpeg4video aac aac_latm ac3 dca mpegaudio
  flac opus vorbis mlp dvdsub)
# "av1" is only the VideoToolbox hook (it cannot decode in software), so libdav1d
# provides AV1 on Macs without AV1 hardware (before M3).
VIDEO_DECODERS=(h264 hevc av1 libdav1d vp9 vp8 mpeg2video mpeg4)
AUDIO_DECODERS=(aac aac_latm ac3 eac3 dca mp3 mp3float flac opus vorbis truehd
  pcm_s8 pcm_u8 pcm_s16le pcm_s16be pcm_s24le pcm_s24be pcm_s32le pcm_s32be
  pcm_f32le pcm_f32be pcm_f64le pcm_f64be pcm_alaw pcm_mulaw pcm_bluray pcm_dvd)
SUBTITLE_DECODERS=(subrip srt ass ssa webvtt movtext pgssub dvdsub)
HWACCELS=(h264_videotoolbox hevc_videotoolbox vp9_videotoolbox av1_videotoolbox
  mpeg2_videotoolbox mpeg4_videotoolbox)
# Not in the app's feature list, but mpv's filter chain needs these for format
# negotiation and conversion (buffer sources and sinks are always built in).
FILTERS=(aformat anull aresample format null scale)
# Lets libavformat recover codec headers while probing MPEG-TS and similar.
BSFS=(extract_extradata)

log "Building ffmpeg"
(
  cd "$SRC/ffmpeg"
  # FFmpeg takes its flags from the configure arguments, not the environment.
  env -u CFLAGS -u CXXFLAGS -u OBJCFLAGS -u LDFLAGS ./configure \
    --prefix="$PREFIX" --libdir="$PREFIX/lib" \
    --arch=arm64 --target-os=darwin --cc=clang --cxx=clang++ --objcc=clang \
    --pkg-config="$PKG_CONFIG" --pkg-config-flags=--static \
    --extra-cflags="$COMMON_FLAGS" --extra-ldflags="$ARCH_FLAGS" \
    --enable-static --disable-shared --enable-pic \
    --disable-autodetect --disable-everything \
    --disable-programs --disable-doc --disable-network --disable-debug \
    --disable-avdevice \
    --enable-pthreads --enable-zlib --enable-videotoolbox --enable-libdav1d \
    --enable-protocol=file \
    --enable-demuxer="$(join "${DEMUXERS[@]}")" \
    --enable-parser="$(join "${PARSERS[@]}")" \
    --enable-decoder="$(join "${VIDEO_DECODERS[@]}" "${AUDIO_DECODERS[@]}" "${SUBTITLE_DECODERS[@]}")" \
    --enable-hwaccel="$(join "${HWACCELS[@]}")" \
    --enable-filter="$(join "${FILTERS[@]}")" \
    --enable-bsf="$(join "${BSFS[@]}")"

  # configure silently ignores unknown or unsatisfiable names, so check each one.
  require() { # require <KIND> <names...>
    local kind="$1" name upper
    shift
    for name in "$@"; do
      upper="$(echo "$name" | tr '[:lower:]' '[:upper:]')"
      grep -qE "^#define CONFIG_${upper}_${kind} 1$" config_components.h ||
        die "FFmpeg did not enable ${kind}: $name"
    done
  }
  require DEMUXER "${DEMUXERS[@]}"
  require PARSER "${PARSERS[@]}"
  require DECODER "${VIDEO_DECODERS[@]}" "${AUDIO_DECODERS[@]}" "${SUBTITLE_DECODERS[@]}"
  require HWACCEL "${HWACCELS[@]}"
  require FILTER "${FILTERS[@]}"
  require BSF "${BSFS[@]}"
  for off in GPL NONFREE NETWORK; do
    grep -qE "^#define CONFIG_${off} 0$" config.h || die "FFmpeg CONFIG_$off must be 0"
  done
  enabled_protocols="$(sed -nE 's/^#define CONFIG_([A-Z0-9_]+)_PROTOCOL 1$/\1/p' config_components.h | xargs)"
  [[ "$enabled_protocols" == "FILE" ]] || die "unexpected FFmpeg protocols: $enabled_protocols"
  if grep -qE '^#define CONFIG_[A-Z0-9_]+_(ENCODER|MUXER) 1$' config_components.h; then
    die "FFmpeg enabled an encoder or muxer"
  fi

  make -j "$JOBS"
  make install >/dev/null
)

# ---------------------------------------------------------------------------
# mpv: shared libmpv only
# ---------------------------------------------------------------------------
# cocoa + gl-cocoa are what videotoolbox-gl (zero-copy hardware decoding through
# the OpenGL render API) requires. Everything not listed is off, including Lua,
# JavaScript, Vulkan, cplugins, libcurl, media keys and the cocoa-cb window.
# Swift is on because mpv 0.42 does not compile with cocoa but without it. Its
# runtime is part of macOS (/usr/lib/swift), and its app code stays inert unless
# the host process is mpv's own application.
# Only the mpv_* symbols are exported, which lets the linker strip unused code
# from the static dependencies.
log "Building mpv"
meson setup "$SRC/mpv/_build" "$SRC/mpv" \
  --prefix="$PREFIX" --libdir=lib --buildtype=release \
  --default-library=shared --wrap-mode=nodownload --auto-features=disabled \
  -Db_ndebug=true \
  -Dc_link_args="$ARCH_FLAGS -lc++ -Wl,-dead_strip -Wl,-exported_symbol,_mpv_*" \
  -Dobjc_link_args="$ARCH_FLAGS -lc++ -Wl,-dead_strip -Wl,-exported_symbol,_mpv_*" \
  -Dlibmpv=true -Dcplayer=false -Dgpl=true -Dbuild-date=false -Dtests=false \
  -Dlua=disabled -Djavascript=disabled -Dcplugins=disabled -Dvulkan=disabled \
  -Dlibavdevice=disabled -Dlibcurl=disabled \
  -Dgl=enabled -Dplain-gl=enabled -Dcocoa=enabled -Dgl-cocoa=enabled \
  -Dvideotoolbox-gl=enabled -Dcoreaudio=enabled \
  -Diconv=enabled -Dzlib=enabled \
  -Dswift-build=enabled -Dswift-flags="-target arm64-apple-macosx$MACOSX_DEPLOYMENT_TARGET" \
  -Dmacos-cocoa-cb=disabled -Dmacos-media-player=disabled -Dmacos-touchbar=disabled \
  -Dmacos-10-15-4-features=enabled -Dmacos-11-features=enabled \
  -Dmacos-11-3-features=enabled -Dmacos-12-features=enabled
ninja -C "$SRC/mpv/_build" -j "$JOBS"
meson install -C "$SRC/mpv/_build" --no-rebuild >/dev/null

# ---------------------------------------------------------------------------
# Finish and verify
# ---------------------------------------------------------------------------
log "Finishing $DYLIB"
[[ -f "$DYLIB" ]] || die "mpv did not produce $DYLIB"
install_name_tool -id @rpath/libmpv.2.dylib "$DYLIB"
# The Swift toolchain adds an rpath into the build machine's developer tools.
otool -l "$DYLIB" | awk '$1=="cmd"{r=($2=="LC_RPATH")} r && $1=="path"{print $2}' |
  while IFS= read -r rpath; do
    [[ "$rpath" == /usr/lib/* ]] || install_name_tool -delete_rpath "$rpath" "$DYLIB"
  done
strip -x "$DYLIB"
codesign --force --sign - "$DYLIB"

license() { # license <component> <files relative to its source dir...>
  local name="$1" file
  shift
  mkdir -p "$PREFIX/share/licenses/$name"
  for file in "$@"; do
    cp "$SRC/$name/$file" "$PREFIX/share/licenses/$name/"
  done
}
license mpv Copyright LICENSE.GPL LICENSE.LGPL
license ffmpeg LICENSE.md COPYING.LGPLv2.1
license dav1d COPYING
license libplacebo LICENSE
license libass COPYING
license freetype LICENSE.TXT docs/FTL.TXT docs/GPLv2.TXT
license fribidi COPYING
license harfbuzz COPYING
mkdir -p "$PREFIX/share/licenses/fast_float"
cp "$SRC/libplacebo/3rdparty/fast_float/LICENSE-MIT" "$PREFIX/share/licenses/fast_float/"

# Every dependency must be part of macOS itself.
log "Verifying linkage"
otool -L "$DYLIB"
bad="$(otool -L "$DYLIB" | tail -n +2 | awk '{print $1}' |
  grep -vE '^(/usr/lib/|/System/Library/|@rpath/libmpv\.2\.dylib$)' || true)"
[[ -z "$bad" ]] || die "libmpv links to non-system libraries:
$bad"
if otool -l "$DYLIB" | grep -E '^ +(name|path) ' | grep -qE '/opt/homebrew|/usr/local'; then
  die "libmpv has a Homebrew or /usr/local load command"
fi
[[ "$(lipo -archs "$DYLIB")" == arm64 ]] || die "libmpv is not arm64-only"
minos="$(otool -l "$DYLIB" | awk '/LC_BUILD_VERSION/{f=1} f && $1=="minos"{print $2; exit}')"
[[ "$minos" == "$MACOSX_DEPLOYMENT_TARGET" ]] || die "minimum macOS is $minos, expected $MACOSX_DEPLOYMENT_TARGET"
nm -gU "$DYLIB" | grep -q ' _mpv_create$' || die "mpv_create is not exported"

echo "$SCRIPT_HASH" >"$STAMP"
log "Done: $DYLIB ($(stat -f %z "$DYLIB") bytes, minos $minos)"
