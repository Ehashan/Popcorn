# Third-party components

Popcorn is released under [GPL-3.0](../LICENSE). `Popcorn.app` ships
`Contents/Frameworks/libmpv.2.dylib`, which `scripts/build-libmpv.sh` builds by statically
linking the components below. Their license texts are inside the app in
`Contents/Resources/licenses/`.

## Linked into libmpv

| Component | Version | License | Source |
|---|---|---|---|
| mpv | 0.42.0 | GPL-2.0-or-later (built with `gpl=true`) | <https://github.com/mpv-player/mpv/archive/refs/tags/v0.42.0.tar.gz> |
| FFmpeg | 9.0.2 | LGPL-2.1-or-later (no GPL or nonfree parts) | <https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz> |
| dav1d | 1.5.5 | BSD-2-Clause | <https://download.videolan.org/pub/videolan/dav1d/1.5.5/dav1d-1.5.5.tar.xz> |
| libplacebo | 7.387.0 | LGPL-2.1-or-later | <https://code.videolan.org/videolan/libplacebo/-/archive/v7.387.0/libplacebo-v7.387.0.tar.gz> |
| libass | 0.17.5 | ISC | <https://github.com/libass/libass/releases/download/0.17.5/libass-0.17.5.tar.xz> |
| FreeType | 2.15.0 | FreeType License (FTL) or GPL-2.0-or-later | <https://download.savannah.gnu.org/releases/freetype/freetype-2.15.0.tar.xz> |
| FriBidi | 1.0.18 | LGPL-2.1-or-later | <https://github.com/fribidi/fribidi/releases/download/v1.0.18/fribidi-1.0.18.tar.xz> |
| HarfBuzz | 15.2.0 | MIT ("Old MIT") | <https://github.com/harfbuzz/harfbuzz/releases/download/15.2.0/harfbuzz-15.2.0.tar.xz> |
| fast_float (headers, via libplacebo) | 8.4.0 | MIT, Apache-2.0 or BSL-1.0 (MIT used) | <https://github.com/fastfloat/fast_float/archive/refs/tags/v8.4.0.tar.gz> |

zlib, libiconv, the Swift runtime and Apple frameworks are part of macOS and are not shipped.

## Build time only (no code of their own is shipped)

| Component | Version | License | Source |
|---|---|---|---|
| glad (generates the OpenGL loader) | 2.0.8 | MIT; generated code also under the Khronos specification licenses | <https://github.com/Dav1dde/glad/archive/refs/tags/v2.0.8.tar.gz> |
| Jinja | 3.1.6 | BSD-3-Clause | <https://pypi.org/project/Jinja2/3.1.6/> |
| MarkupSafe | 3.0.4 | BSD-3-Clause | <https://pypi.org/project/MarkupSafe/3.0.4/> |
| Vulkan-Headers (needed to compile libplacebo with Vulkan off) | commit 74d8a6cb | Apache-2.0 or MIT | <https://github.com/KhronosGroup/Vulkan-Headers/tree/74d8a6cb930c68ef617b202c3ff3c59d919e086b> |

Rust crates compiled into the app are listed with exact versions in `src-tauri/Cargo.lock`.

## Source offer

The libraries are built from the source archives linked above. The only change is a one-line
compile fix to mpv's `video/out/mac/view.swift`, applied by the build script. Exact checksums,
configure options and the whole build procedure are in
[`scripts/build-libmpv.sh`](../scripts/build-libmpv.sh), so this repository at a release tag,
together with those archives, is the corresponding source of that release's binaries.

If one of the links stops working, or you cannot get the source for a release, open an issue on
the Popcorn repository.
