# Build Popcorn — a lightweight local movie & TV app for macOS

You are building **Popcorn**, a small open-source desktop app that turns a local folder of movies and TV series into a Netflix/Stremio-style library and plays them inside the app.

Work through the phases below **in order**. After each phase: make sure it builds, run the tests, check the acceptance criteria, and commit. If a decision isn't covered here, pick the simplest, lightest option and note it in `docs/DECISIONS.md`. Ask me before adding any dependency that isn't listed here.

---

## 1. Hard constraints

- **Size:** the final `Popcorn.app` must be **under 50 MB** (target ~35–40 MB, DMG ~20 MB). Add a size-check script that fails the build above 50 MB.
- **Platform:** macOS on **Apple Silicon (aarch64-apple-darwin)** only for now. Minimum macOS 13. Keep platform-specific code isolated (`#[cfg(target_os = "macos")]`) so Windows and Linux can be added later.
- **Lightweight:** low idle CPU, low memory, fast startup. No Electron, no bundled browser, no Node runtime in the shipped app.
- **Offline-first:** after the first metadata fetch, the library must work fully without internet.
- **Open source:** license the project under **GPL-3.0** (because it bundles mpv/FFmpeg).
- **No secrets in the repo:** each user enters their own TMDB key in the app.

## 2. Tech stack

| Layer | Choice |
|---|---|
| App shell | **Tauri 2** (Rust) |
| UI | **Plain HTML, CSS and vanilla JS (ES modules)**: no framework, no bundler, no npm runtime deps |
| Video player | **libmpv**, embedded in the main window (Stremio-style) |
| Database | **SQLite** via `rusqlite` with the `bundled` feature |
| Metadata | **TMDB API** |

Allowed Rust crates (keep features minimal):
`tauri` (v2), `tauri-plugin-dialog`, `serde`, `serde_json`, `rusqlite` (bundled), `walkdir`, `regex`, `notify` + `notify-debouncer-full`, an HTTP client using **rustls, not OpenSSL** (prefer `ureq`, or `reqwest` with `default-features = false, features = ["rustls-tls", "json"]` if async is needed), `thiserror`, `log` + a tiny logger. For libmpv, see Phase 4.

Release profile in `Cargo.toml`:

```toml
[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

## 3. Code quality rules

- Clean, idiomatic Rust: `cargo fmt` and `cargo clippy -- -D warnings` must pass.
- No `unwrap()`/`expect()` outside tests and startup. Use a single `AppError` enum (`thiserror`) that serializes to a friendly message for the UI.
- Small, focused modules. Business logic (parsing, grouping, matching) is pure and unit-tested with no Tauri dependency.
- Frontend: small ES modules, no globals except one `state` store, CSS variables for the theme.
- README covers what Popcorn is, screenshots placeholder, how to build, how to get a TMDB key, and the unsigned-app "right-click → Open" note.

## 4. Project structure

```
popcorn/
  README.md
  LICENSE                     # GPL-3.0
  docs/DECISIONS.md
  scripts/
    build-libmpv.sh           # slim libmpv + FFmpeg build for arm64
    bundle-dylibs.sh          # copy dylibs into .app, fix install names
    check-size.sh             # fail if .app > 50 MB
  ui/
    index.html
    css/app.css
    js/
      main.js                 # router + bootstrap
      api.js                  # wrappers around Tauri invoke/listen
      state.js
      views/onboarding.js  home.js  movies.js  shows.js
            search.js  details-movie.js  details-show.js
            player.js  settings.js
      components/poster-card.js  row.js  fix-match.js  toast.js
  src-tauri/
    tauri.conf.json
    capabilities/default.json
    src/
      main.rs
      lib.rs                  # builder, plugin + command registration
      error.rs
      db/mod.rs  schema.sql  migrations.rs
      settings.rs
      library/
        mod.rs
        parse.rs              # filename → ParsedMedia (pure, heavily tested)
        group.rs              # episodes → shows (pure, tested)
        scan.rs               # walk folder, diff against DB
        watch.rs              # file watcher → incremental rescans
      tmdb/
        mod.rs  client.rs  matcher.rs  cache.rs (image download)
      player/
        mod.rs                # Player trait + commands
        mpv.rs                # libmpv wrapper
        embed_macos.rs        # native view embedding
      progress.rs
      commands.rs             # all #[tauri::command] fns
```

## 5. Data storage

Everything lives in `~/Library/Application Support/Popcorn/`:
- `popcorn.db`: SQLite
- `images/posters`, `images/backdrops`, `images/profiles`, `images/stills`: cached TMDB images

Video files are **never copied or moved**. Popcorn only reads them.

### SQLite schema (starting point)

```sql
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- A movie or a TV show
CREATE TABLE titles (
  id            INTEGER PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (kind IN ('movie','show')),
  name_key      TEXT NOT NULL,          -- normalized guessed title (+ year for movies)
  guessed_title TEXT NOT NULL,
  guessed_year  INTEGER,
  tmdb_id       INTEGER,
  match_locked  INTEGER NOT NULL DEFAULT 0,  -- 1 after manual "Fix match"
  metadata_json TEXT,                   -- trimmed TMDB response
  poster_path   TEXT,                   -- local cached file path
  backdrop_path TEXT,
  fetched_at    INTEGER,
  UNIQUE (kind, name_key)
);

-- A video file on disk (a movie file or one episode)
CREATE TABLE files (
  id        INTEGER PRIMARY KEY,
  path      TEXT NOT NULL UNIQUE,
  title_id  INTEGER NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
  season    INTEGER,
  episode   INTEGER,
  size      INTEGER NOT NULL,
  mtime     INTEGER NOT NULL,
  added_at  INTEGER NOT NULL,
  missing   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE episode_meta (
  title_id INTEGER NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
  season   INTEGER NOT NULL,
  episode  INTEGER NOT NULL,
  name     TEXT,
  overview TEXT,
  still_path TEXT,
  PRIMARY KEY (title_id, season, episode)
);

CREATE TABLE progress (
  file_id    INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
  position   REAL NOT NULL DEFAULT 0,
  duration   REAL,
  watched    INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);
```

Use a simple `user_version`-based migration runner.

---

## 6. Phases

### Phase 0: Project setup

- Create the Tauri 2 app with the structure above, `ui/` as the frontend dist folder (no build step).
- Bundle identifier `com.dizzpy.popcorn`, product name `Popcorn`.
- Dark window, minimum size 1000×650, title bar style that suits a media app (e.g. overlay/transparent title bar).
- Tight Tauri capabilities: only the commands and plugins we use. Asset protocol enabled **only** for the image cache directory.
- Add the release profile, LICENSE, README skeleton, `docs/DECISIONS.md`.

**Done when:** `cargo tauri dev` opens an empty Popcorn window; `cargo clippy` is clean.

### Phase 1: First launch + library scan

**Onboarding screen** (shown when no media folder is set):
1. "Choose your media folder" → native folder picker (`tauri-plugin-dialog`).
2. "Paste your TMDB API key" with a link to how to get one, a "Test key" button and a "Skip for now" option.
Settings are saved in the `settings` table.

**Filename parsing (`library/parse.rs`, pure function):**
- Video extensions: `mkv mp4 m4v mov avi webm ts m2ts wmv flv mpg mpeg`.
- Ignore hidden files, `._*` AppleDouble files, and files containing `sample` that are under 200 MB.
- Detect **episodes** from (case-insensitive):
  - `S01E01`, `s1e1`, `S01.E01`, `S01 E01`, `S01E01E02` (take the first episode)
  - `1x02`
  - folder patterns: `Show/Season 1/Episode 01.mkv`, `Show/S01/01 - Pilot.mkv`, `Show/Season 01/05.mkv`
  - Show name comes from the text before the episode marker; if that's empty, from the nearest parent folder that isn't a season folder.
- Everything else is a **movie**: take the title before the year (1900–2099) or before the first quality tag.
- Clean titles: dots/underscores → spaces; remove bracketed/parenthesized junk and release tags (`1080p 2160p 720p 4K UHD HDR HDR10 DV x264 x265 H.264 H.265 HEVC AV1 BluRay BRRip WEB-DL WEBRip HDTV AAC AC3 DTS DDP5.1 Atmos REMUX PROPER REPACK` and a trailing `-GROUP`); collapse spaces; title-case only if the name is all lowercase/uppercase.

Unit tests must include at least:

| Input | Expected |
|---|---|
| `Movie.Title.2025.mp4` | movie "Movie Title" (2025) |
| `The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv` | movie "The Matrix" (1999) |
| `Dune Part Two (2024) [2160p] [HDR].mkv` | movie "Dune Part Two" (2024) |
| `Show.S01E01.mkv` | episode "Show" S1E1 |
| `Show_S02E10.mkv` | episode "Show" S2E10 |
| `Show.1x02.mkv` | episode "Show" S1E2 |
| `Cyberpunk_Edgerunners_S01E01.mkv` | episode "Cyberpunk Edgerunners" S1E1 |
| `Show/Season 1/Episode 01.mkv` | episode "Show" S1E1 |
| `Show/S01/01 - Pilot.mkv` | episode "Show" S1E1 |
| `Breaking.Bad.S05E14.Ozymandias.720p.mkv` | episode "Breaking Bad" S5E14 |
| `.hidden.mkv`, `._Movie.mkv`, `movie-sample.mkv` (small) | ignored |

**Grouping (`library/group.rs`):** episodes with the same normalized show name become one show, even across different folders. Seasons and episodes are sorted numerically.

**Scanning (`library/scan.rs`):**
- Full scan on startup in a background thread; emit `scan://progress` events.
- Diff against the DB by path + size + mtime: insert new, update changed, mark deleted files `missing = 1` (hide them in the UI, don't delete progress immediately).
- **Watcher (`library/watch.rs`):** watch the folder recursively with a debounced watcher (~2 s). New or removed files trigger an incremental update and a `library://updated` event.

**Done when:** choosing a folder lists every movie and show (titles only, no metadata yet) and adding a file to the folder makes it appear without restarting. All parser tests pass.

### Phase 2: TMDB metadata + cache

- Client accepts either a v3 API key (`api_key` query param) or a v4 read access token (`Authorization: Bearer`, detect tokens starting with `eyJ`).
- **Matching:**
  - Movies: `GET /3/search/movie?query=…&year=…` (retry without year if no result).
  - Shows: `GET /3/search/tv?query=…&first_air_date_year=…` (retry without year).
  - Take the best result (exact normalized title match first, then the first result).
- **Details:**
  - Movies: `GET /3/movie/{id}?append_to_response=credits,release_dates`
  - Shows: `GET /3/tv/{id}?append_to_response=credits`, plus `GET /3/tv/{id}/season/{n}` for each season present locally (episode names, overviews, stills).
- Store a **trimmed** JSON (title, year, overview, tagline, genres, runtime, rating, certification, top 15 cast with character + profile image, director/creators, seasons) in `metadata_json`.
- **Images:** download once to the cache folder. Sizes: posters `w342`, backdrops `w1280`, profiles `w185`, stills `w300`. Base URL `https://image.tmdb.org/t/p/`. The UI loads them via `convertFileSrc`.
- Fetch in a background queue with **max 4 concurrent requests**, retry with backoff on 429/5xx. Emit `library://updated` as items complete.
- Skip titles that already have metadata unless the user clicks "Refresh metadata". Refresh automatically after 30 days if online.
- **Fix match:** a dialog with a search box → shows TMDB results (poster, title, year) → choosing one sets `tmdb_id`, `match_locked = 1` and refetches.
- No key or offline → show the local title with a generated placeholder poster (title on a colored gradient); never block the UI.

**Done when:** the library shows real posters and details, restarting with Wi-Fi off still shows everything, and Fix match works.

### Phase 3: Library UI

Design: dark, cinematic, minimal. Large posters, backdrop images with gradient fades, smooth but cheap CSS transitions (transform/opacity only). Keyboard navigable. No heavy JS; virtualize or lazy-load the poster grid (`loading="lazy"`, IntersectionObserver) so 1,000+ titles stay smooth.

Screens:
- **Sidebar or top nav:** Home, Movies, TV Shows, Search, Settings.
- **Home:** hero banner (a random or recently added title with backdrop), then rows: *Continue Watching*, *Recently Added*, *Movies*, *TV Shows*, and a few genre rows built from local metadata.
- **Movies / TV Shows:** poster grid; sort by title, year, rating, recently added; filter by genre and watched/unwatched.
- **Search:** instant local search across titles, cast names and episode names.
- **Movie details:** backdrop, poster, title, year, runtime, rating, certification, genres, tagline, overview, director, cast row with photos, file info (filename, size, resolution if known). Buttons: **Play / Resume (with time)**, **Mark watched/unwatched**, **Fix match**, **Show in Finder**.
- **Show details:** same header, plus a season selector and an episode list (still, number, name, overview, progress bar, watched tick). **Resume / Play next** button picks the right episode.
- **Settings:** media folder (change → rescan), TMDB key (change/test), Rescan now, Clear image cache, Refresh all metadata, About (version, GPL-3.0, GitHub link, and the required TMDB attribution: *"This product uses the TMDB API but is not endorsed or certified by TMDB."* with the TMDB logo).
- Back button, `Esc` and Cmd+[ go back.

**Done when:** all screens work with real data and scrolling a large library is smooth.

### Phase 4: Embedded mpv player

Architecture (same idea as Stremio's desktop shell):

```
┌──────────── Popcorn window ────────────┐
│  WebView (transparent while playing)    │ ← HTML player controls
│  Native video view (mpv renders here)   │ ← hardware decoding (VideoToolbox)
└─────────────────────────────────────────┘
        ▲ commands            │ events
        └──── Rust player module ────┘
```

Steps:
1. **Spike first.** Try `tauri-plugin-libmpv` (MPL-2.0) on macOS. Its docs list macOS as untested, so verify it quickly. If it doesn't work cleanly, write our own thin wrapper in `player/mpv.rs` + `player/embed_macos.rs`: create a native `NSView` inside the Tauri window **below** the WKWebView, make the webview transparent while the player is open, and give mpv that view (via mpv's `wid` option or the libmpv render API, whichever works reliably with the macOS backend; check the mpv docs). Document the choice in `docs/DECISIONS.md`.
2. **Fallback:** if embedding fails at runtime, open playback in a separate borderless fullscreen window using mpv's own on-screen controller, so the user can always watch. Progress tracking must still work.
3. mpv options: `hwdec=videotoolbox` (or `auto-safe`), `sub-auto=fuzzy` (auto-load `.srt/.ass/.vtt` next to the file), `keep-open=yes`, `osc=no` when embedded, sensible cache settings for local files, no config files or scripts loaded from the user's machine.
4. **Rust commands:** `player_open(file_id)`, `player_close`, `player_toggle_pause`, `player_seek(seconds, relative)`, `player_set_volume`, `player_toggle_mute`, `player_set_audio(track_id)`, `player_set_subtitle(track_id | none)`, `player_add_subtitle(path)`, `player_toggle_fullscreen`.
5. **Events to the UI:** `player://state` (playing/paused/buffering), `player://time` (position, duration; throttle to ~4/s), `player://tracks` (audio + subtitle lists with language/title), `player://ended`, `player://error`.
6. **HTML controls overlay:** title (and S/E + episode name), play/pause, seek bar with hover time, current/total time, volume, audio track menu, subtitle menu (including "Load subtitle file…"), fullscreen, back. Auto-hide after 3 s of no mouse movement; hide the cursor too.
7. **Keyboard:** Space play/pause, ←/→ seek 10 s, Shift+←/→ 60 s, ↑/↓ volume, M mute, F fullscreen, S cycle subtitles, A cycle audio, Esc exit fullscreen / close player, N next episode.

**Done when:** an MKV with HEVC video, multiple audio tracks and embedded subtitles plays smoothly inside the main window, with all controls and shortcuts working.

### Phase 5: Progress, resume, next episode

- Save position every 5 s, on pause, on seek, and on close (`progress` table).
- **Resume:** if the saved position is > 60 s and < 90% of the duration, Play becomes "Resume from 42:10", with a "Start over" option.
- **Watched:** mark watched at ≥ 90% or on end of file; manual toggle in details.
- **Next episode:** near the end of an episode show "Next episode in 10 s" with *Play now* / *Cancel*; autoplay the next episode in season order (crossing into the next season).
- **Continue Watching row:** in-progress movies, and for shows the next unwatched or in-progress episode, newest activity first.

**Done when:** closing and reopening the app resumes at the right spot and a full season can be watched start to finish without touching anything.

### Phase 6: Slim libmpv build + DMG

- `scripts/build-libmpv.sh` builds for arm64, reproducibly, with versions pinned at the top:
  - **FFmpeg** with `--disable-everything` and then only: common demuxers (matroska, mov/mp4, avi, mpegts, webm, flv, asf, mpegps), parsers and decoders for H.264, HEVC, AV1, VP9, VP8, MPEG-2, MPEG-4; audio AAC, AC3, E-AC3, DTS, MP3, FLAC, Opus, Vorbis, TrueHD, PCM; subtitles SRT/SubRip, ASS/SSA, WebVTT, MOV text, PGS, DVD sub; VideoToolbox hwaccel; `file` protocol only; swresample/swscale. No encoders (except none needed), no network, no programs, no docs.
  - **libass** with its minimal deps (freetype, fribidi, harfbuzz) for subtitles.
  - **mpv** as a shared `libmpv` with Lua, JavaScript, Vulkan, network/ytdl, cplugins and unneeded outputs disabled.
- `scripts/bundle-dylibs.sh` copies the libraries into `Popcorn.app/Contents/Frameworks`, rewrites install names to `@rpath`, and ad-hoc signs them (`codesign --force --sign -`).
- Hook this into `tauri build` (Tauri bundle `frameworks`/resources config or a post-build step).
- Build with `cargo tauri build --target aarch64-apple-darwin --bundles app,dmg`.
- `scripts/check-size.sh`: print the size breakdown (binary, each dylib, UI) and **fail if `Popcorn.app` > 50 MB**.
- Add a GitHub Actions workflow (macos-latest, arm64) that builds the DMG and attaches it to a release on tag push. Signing/notarization is optional and only runs if Apple secrets exist.

**Done when:** a fresh Mac without Homebrew can install `Popcorn.dmg`, open the app (right-click → Open) and play an MKV; the app is under 50 MB.

---

## 7. Out of scope for the MVP

Windows/Linux builds, code signing/notarization, auto-updates, trailers, multiple media folders, picture-in-picture, user accounts, streaming/torrents, moving or renaming the user's files.

## 8. Final deliverable checklist

- [ ] `Popcorn.app` < 50 MB, DMG ≈ 20 MB, arm64
- [ ] First launch: pick folder + TMDB key
- [ ] Movies and episodes detected and grouped; folder watcher works
- [ ] TMDB metadata + images cached; works offline; Fix match
- [ ] Home, Movies, TV Shows, Search, Details, Settings screens
- [ ] Embedded mpv player with controls, subtitles, audio tracks, shortcuts (+ fallback window)
- [ ] Resume, watched status, next-episode autoplay, Continue Watching
- [ ] Parser/grouping unit tests pass; clippy and fmt clean
- [ ] README, GPL-3.0 LICENSE, TMDB attribution, `docs/DECISIONS.md`
- [ ] GitHub Actions release workflow
