#!/usr/bin/env bash
# Builds a slim, self-contained x86_64 libmpv-2.dll for Popcorn on Windows (MSYS2 MINGW64).
#
# Every dependency is built as a static library and linked into the one DLL, so
# the result depends only on standard Windows system DLLs.
# Nothing is installed outside <repo>/build/.
#
# Usage: scripts/build-libmpv-windows.sh [--force]
# Output: build/windows/libmpv-2.dll
set -euo pipefail

# ---------------------------------------------------------------------------
# Pinned versions and SHA-256 checksums (matching scripts/build-libmpv.sh).
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
# Build-time only (libplacebo's subprojects).
GLAD_VERSION=2.0.8
GLAD_SHA256=44f06f9195427c7017f5028d0894f57eb216b0a8f7c4eda7ce883732aeb2d0fc
JINJA_VERSION=3.1.6
JINJA_SHA256=0137fb05990d35f1275a587e9aee6d56da821fc83491a0fb838183be43f66d6d
MARKUPSAFE_VERSION=3.0.4
MARKUPSAFE_SHA256=2e9ad7dd851bf45fab9f75cbff4cb493fee9979e8d8c7c9c3ee119022518edd6
VULKAN_HEADERS_COMMIT=74d8a6cb930c68ef617b202c3ff3c59d919e086b
VULKAN_HEADERS_SHA256=75967a8288ff16044a86d1af462b96d2e75c435937efb75f2bb4102ab0382f29
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
OUT_DIR="$BUILD/windows"
OUT_DLL="$OUT_DIR/libmpv-2.dll"
STAMP="$OUT_DIR/.build-stamp"

log() { printf '\n==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

[[ "${MSYSTEM:-}" == MINGW64 ]] || die "run this script from an MSYS2 MINGW64 shell"
[[ "$ROOT" != *" "* ]] || die "the repository path must not contain spaces: $ROOT"

SCRIPT_HASH="$(sha256sum "${BASH_SOURCE[0]}" | cut -d' ' -f1)"
if [[ "${1:-}" != "--force" && -f "$OUT_DLL" && -f "$STAMP" && "$(cat "$STAMP")" == "$SCRIPT_HASH" ]]; then
  log "libmpv-2.dll is up to date: $OUT_DLL (use --force to rebuild)"
  exit 0
fi

rm -rf "$TOOLS" "$PREFIX" "$SRC"
mkdir -p "$DOWNLOADS" "$SRC" "$PREFIX/lib/pkgconfig" "$PREFIX/include" "$TOOLS" "$OUT_DIR"

for tool in gcc g++ meson ninja pkgconf nasm make cmp curl tar strip objdump; do
  command -v "$tool" >/dev/null || die "$tool not found in MINGW64 PATH"
done

# Force pkg-config to always emit static libraries, and copy static zlib/iconv
# into $PREFIX/lib so ld never picks up MSYS2's .dll.a import libraries.
printf '#!/bin/sh\nexec "%s" --static "$@"\n' "$(command -v pkgconf)" >"$TOOLS/pkg-config"
chmod +x "$TOOLS/pkg-config"
cp /mingw64/lib/libz.a "$PREFIX/lib/"
cp /mingw64/lib/libiconv.a "$PREFIX/lib/"
cp /mingw64/include/zlib.h /mingw64/include/zconf.h /mingw64/include/iconv.h "$PREFIX/include/"

export PATH="$TOOLS:/mingw64/bin:/usr/bin:/bin"
export PKG_CONFIG="$TOOLS/pkg-config"
export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig:/mingw64/lib/pkgconfig"
export CC=gcc CXX=g++
export SOURCE_DATE_EPOCH=0 LC_ALL=C

COMMON_FLAGS="-Os -ffunction-sections -fdata-sections -ffile-prefix-map=$BUILD=."
STATIC_LDFLAGS="-L$PREFIX/lib -static-libgcc -static-libstdc++ -Wl,--gc-sections"
export CFLAGS="$COMMON_FLAGS -I$PREFIX/include"
export CXXFLAGS="$COMMON_FLAGS -I$PREFIX/include"
export LDFLAGS="$STATIC_LDFLAGS"
JOBS="$(nproc)"

# ---------------------------------------------------------------------------
# Download, verify, extract
# ---------------------------------------------------------------------------
fetch() {
  local file="$DOWNLOADS/$1" url="$2" sha="$3"
  if [[ ! -f "$file" ]] || ! echo "$sha  $file" | sha256sum -c - >/dev/null 2>&1; then
    log "Downloading $1"
    curl --fail --location --silent --show-error \
      --proto '=https' --proto-redir '=https' --tlsv1.2 \
      --retry 5 --retry-all-errors --connect-timeout 30 \
      --output "$file.part" "$url"
    mv "$file.part" "$file"
  fi
  local actual
  actual="$(sha256sum "$file" | cut -d' ' -f1)"
  if [[ "$actual" != "$sha" ]]; then
    rm -f "$file"
    die "checksum mismatch for $1: expected $sha, got $actual"
  fi
}

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

# ---------------------------------------------------------------------------
# Dependencies
# ---------------------------------------------------------------------------
meson_dep() {
  local name="$1"
  shift
  log "Building $name"
  meson setup "$SRC/$name/_build" "$SRC/$name" \
    --prefix="$PREFIX" --libdir=lib --buildtype=minsize \
    --default-library=static --wrap-mode=nodownload --auto-features=disabled \
    -Db_ndebug=true "$@"
  ninja -C "$SRC/$name/_build" -j "$JOBS"
  meson install -C "$SRC/$name/_build" --no-rebuild >/dev/null
}

meson_dep freetype -Dzlib=system
meson_dep fribidi -Ddocs=false -Dbin=false -Dtests=false
meson_dep harfbuzz -Dfreetype=enabled -Dtests=disabled -Ddocs=disabled -Dutilities=disabled \
  -Dsubset=disabled -Draster=disabled -Dvector=disabled -Dgpu=disabled
meson_dep libass -Ddirectwrite=enabled -Dfontconfig=disabled -Dasm=enabled
meson_dep dav1d -Denable_tools=false -Denable_tests=false
meson_dep libplacebo -Dvulkan=disabled -Dopengl=enabled -Dgl-proc-addr=enabled \
  -Ddovi=enabled -Ddemos=false \
  -Dc_args="$CFLAGS -I$SRC/libplacebo/3rdparty/Vulkan-Headers/include" \
  -Dcpp_args="$CXXFLAGS -I$SRC/libplacebo/3rdparty/Vulkan-Headers/include"

# ---------------------------------------------------------------------------
# FFmpeg (LGPL v2.1+: no --enable-gpl, no --enable-nonfree, w32threads)
# ---------------------------------------------------------------------------
join() { local IFS=,; echo "$*"; }

DEMUXERS=(matroska mov avi mpegts flv asf mpegps mpegvideo srt ass webvtt)
PARSERS=(h264 hevc av1 vp9 vp8 mpegvideo mpeg4video aac aac_latm ac3 dca mpegaudio
  flac opus vorbis mlp dvdsub)
VIDEO_DECODERS=(h264 hevc av1 libdav1d vp9 vp8 mpeg2video mpeg4)
AUDIO_DECODERS=(aac aac_latm ac3 eac3 dca mp3 mp3float flac opus vorbis truehd
  pcm_s8 pcm_u8 pcm_s16le pcm_s16be pcm_s24le pcm_s24be pcm_s32le pcm_s32be
  pcm_f32le pcm_f32be pcm_f64le pcm_f64be pcm_alaw pcm_mulaw pcm_bluray pcm_dvd)
SUBTITLE_DECODERS=(subrip srt ass ssa webvtt movtext pgssub dvdsub)
HWACCELS=(h264_d3d11va hevc_d3d11va vp9_d3d11va av1_d3d11va mpeg2_d3d11va
  h264_dxva2 hevc_dxva2 vp9_dxva2 av1_dxva2 mpeg2_dxva2)
FILTERS=(aformat anull aresample format null scale)
BSFS=(extract_extradata)

log "Building ffmpeg"
(
  cd "$SRC/ffmpeg"
  env -u CFLAGS -u CXXFLAGS -u LDFLAGS ./configure \
    --prefix="$PREFIX" --libdir="$PREFIX/lib" \
    --arch=x86_64 --target-os=mingw32 --cc=gcc --cxx=g++ \
    --pkg-config="$PKG_CONFIG" --pkg-config-flags=--static \
    --extra-cflags="$CFLAGS" --extra-ldflags="$LDFLAGS" \
    --enable-static --disable-shared --enable-small \
    --disable-autodetect --disable-everything \
    --disable-programs --disable-doc --disable-network --disable-debug \
    --disable-avdevice \
    --enable-w32threads --enable-zlib --enable-d3d11va --enable-dxva2 --enable-libdav1d \
    --enable-protocol=file \
    --enable-demuxer="$(join "${DEMUXERS[@]}")" \
    --enable-parser="$(join "${PARSERS[@]}")" \
    --enable-decoder="$(join "${VIDEO_DECODERS[@]}" "${AUDIO_DECODERS[@]}" "${SUBTITLE_DECODERS[@]}")" \
    --enable-hwaccel="$(join "${HWACCELS[@]}")" \
    --enable-filter="$(join "${FILTERS[@]}")" \
    --enable-bsf="$(join "${BSFS[@]}")"

  make -j "$JOBS"
  make install >/dev/null
)

# ---------------------------------------------------------------------------
# mpv: shared libmpv-2.dll only
# ---------------------------------------------------------------------------
log "Building mpv"
meson setup "$SRC/mpv/_build" "$SRC/mpv" \
  --prefix="$PREFIX" --libdir=lib --bindir=bin --buildtype=minsize \
  --default-library=shared --wrap-mode=nodownload --auto-features=disabled \
  -Db_ndebug=true \
  -Dc_link_args="$STATIC_LDFLAGS" \
  -Dcpp_link_args="$STATIC_LDFLAGS" \
  -Dlibmpv=true -Dcplayer=false -Dgpl=true -Dbuild-date=false -Dtests=false \
  -Dlua=disabled -Djavascript=disabled -Dcplugins=disabled -Dvulkan=disabled \
  -Dlibavdevice=disabled -Dlibcurl=disabled \
  -Dgl=enabled -Dplain-gl=enabled -Dgl-win32=enabled \
  -Dd3d-hwaccel=enabled -Dwasapi=enabled \
  -Diconv=enabled -Dzlib=enabled
ninja -C "$SRC/mpv/_build" -j "$JOBS"
meson install -C "$SRC/mpv/_build" --no-rebuild >/dev/null

BUILT_DLL="$PREFIX/bin/libmpv-2.dll"
[[ -f "$BUILT_DLL" ]] || die "mpv did not produce $BUILT_DLL"
cp "$BUILT_DLL" "$OUT_DLL"
strip --strip-unneeded "$OUT_DLL"

log "Verifying DLL imports"
objdump -p "$OUT_DLL" | grep -i "DLL Name:"
bad="$(objdump -p "$OUT_DLL" | awk '/DLL Name:/{print $3}' |
  grep -iE '^(zlib|libiconv|libwinpthread|libgcc|libstdc\+\+|libass|libav|libplacebo|libdav1d|libfreetype|libfribidi|libharfbuzz)' || true)"
[[ -z "$bad" ]] || die "libmpv-2.dll links to non-system DLLs:
$bad"

echo "$SCRIPT_HASH" >"$STAMP"
log "Done: $OUT_DLL ($(stat -c %s "$OUT_DLL") bytes)"
