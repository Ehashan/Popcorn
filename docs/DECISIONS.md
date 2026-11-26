# Decisions

Choices the build prompt did not cover. Each one is the simplest, lightest option.

## Phase 0

- **Repo layout:** the repository root is the `popcorn/` folder from the prompt.
- **Logger:** a ~30 line stderr logger in `logger.rs` instead of a logger crate.
- **`tauri-build`:** included as a build dependency because every Tauri 2 app needs it.
- **`withGlobalTauri`:** on, so the plain-JS UI can reach `window.__TAURI__` without npm packages.
- **Permissions:** app commands are declared in `build.rs`, so each needs an explicit grant in
  `capabilities/default.json`.
- **Data folder:** `~/Library/Application Support/Popcorn/` (as specified), not Tauri's default
  folder named after the bundle identifier. The asset protocol only covers its `images/` folder.

## Phase 1: library

- A watcher event triggers a full walk that is diffed against the database. It only stats files,
  and only changes are written.
- If the media folder cannot be read (an unplugged drive), nothing is marked missing.
- The folder picker runs in Rust, so the webview can never set the media path itself.
- Shows are grouped by normalized name only; a year in the name is used just for TMDB matching.
- The saved TMDB key is never sent back to the UI.

## Phase 2: TMDB

- A failed details or season request stores nothing, so cached data survives and Fix match is all
  or nothing. A season that returns 404 is skipped.
- Only a failed poster or backdrop leaves a title stale for retry. Cast photos and stills do not.
- `episode_meta` rows exist only for episodes on disk. A new episode triggers a refetch.
- Unmatched titles are retried after 30 days, or when a key is saved.
- Being offline, or a rejected key, empties the queue. A later scan, startup or key save refills it.
- A background result is discarded if the title changed meanwhile; a manual match wins.
- The 4-request limit covers the background queue. Search and manual actions run outside it.
- Certification is shown for movies only (US first), as the show request has no ratings.
- Fix match results load posters straight from `image.tmdb.org`, which the CSP allows.

## Phase 3: UI

- Top navigation instead of a sidebar; it doubles as the window drag area.
- The player is an overlay, so the screen underneath keeps its scroll position.
- Grids add cards in batches of 60 with lazy images. Home rows are capped at 30.
- Search matches in Rust over titles, cast names and episode names.
- WebKit reports a harmless-looking `style-src-attr` CSP violation for each `<select>` created. The
  CSP is left strict.

## Phases 4 and 5: player and progress

- libmpv is loaded with `dlopen` through hand-written FFI, adding no crates. Release builds load it
  only from the app bundle. Debug builds try `POPCORN_LIBMPV`, the bundle, then Homebrew.
- `tauri-plugin-libmpv` was rejected after reading its docs and source (it was not run): it needs
  an extra wrapper library and more dependencies, lists macOS as untested and embeds via `wid`.
- Video is drawn with the OpenGL render API into a `CAOpenGLLayer` below the webview. `wid` is not
  usable on macOS. Apple has deprecated OpenGL; that risk is accepted.
- The fallback is our own borderless window using the same renderer, with keys forwarded to mpv's
  bindings. It has no on-screen controller (the slim build has no Lua), and it does not help if
  OpenGL itself fails.
- Each file gets its own mpv core, so progress and events cannot cross files.
- The screen is kept awake with an `NSProcessInfo` activity, not mpv's `stop-screensaver`.
- No private-API Tauri feature is needed for the transparent webview.
- A file that "ends" below 90% (pulled drive, partial file) is an error, not watched.
- The next-episode countdown starts with 10 s left. Next episode always starts from the beginning.
- The subtitle picker leaves out `.sub`, which the slim build cannot load.

## Phase 6: packaging

- All of libmpv's dependencies are linked statically into one `libmpv.2.dylib` that links only
  macOS libraries.
- AV1 uses dav1d, because FFmpeg's own AV1 decoder needs hardware that Macs before M3 lack.
- FFmpeg additions beyond the spec list: external subtitle demuxers, `mpegvideo`, AAC LATM and MLP
  support, a few audio and video filters, `extract_extradata` and zlib. mpv also needs libplacebo
  (OpenGL only) and Swift for its cocoa code, with a one-line patch to build without Vulkan.
- `scripts/package.sh` makes the DMG after bundling. `cargo tauri build` alone omits libmpv.
- The size limit is 50,000,000 bytes.
