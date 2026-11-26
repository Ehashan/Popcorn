# UI ↔ Rust contract

The UI uses `window.__TAURI__.core.invoke`, `.core.convertFileSrc` and `.event.listen`
(`withGlobalTauri`). Command **arguments are camelCase** in JS; **returned fields are snake_case**.
A failed command rejects with a friendly message **string**. All `*_path` image fields are
absolute local paths (load with `convertFileSrc`) or `null`. Field-level types are defined by the
structs in `src-tauri/src/db/queries.rs`, which is the source of truth.

## Library commands (implemented)

| Command | Args | Returns |
|---|---|---|
| `app_info` | | `{version}` |
| `get_settings` | | `{media_folder: string\|null, has_tmdb_key, onboarded}` |
| `pick_media_folder` | | `string\|null` (null = cancelled). Saves it and starts a scan |
| `finish_onboarding` | | |
| `set_tmdb_key` | `key` | empty string removes the key. The saved key is never returned |
| `test_tmdb_key` | `key: string\|null` | resolves if accepted; null tests the saved key |
| `rescan` | | |
| `list_titles` | | `TitleSummary[]`, sorted by title |
| `get_title` | `id` | `TitleDetails` (all `TitleSummary` fields plus details, `files`, `seasons`, `play`) |
| `continue_watching` | | `ContinueItem[]`, newest first. `position` is 0 for a not-yet-started next episode |
| `search_library` | `query` | title ids, best first |
| `set_watched` | `fileId, watched` | |
| `set_title_watched` | `titleId, watched` | |
| `reveal_file` | `fileId` | |
| `open_external` | `target`: `tmdb_api` \| `tmdb` \| `github` | |

## TMDB commands (owned by the TMDB module)

| Command | Args | Returns |
|---|---|---|
| `tmdb_search` | `kind: "movie"\|"show", query` | `[{tmdb_id, title, year: number\|null, poster_url: string\|null}]` (`poster_url` is a full `https://image.tmdb.org` URL) |
| `fix_match` | `titleId, tmdbId` | resolves once the new metadata is stored |
| `refresh_title` | `titleId` | resolves once refetched |
| `refresh_all_metadata` | | queues everything, returns at once |
| `clear_image_cache` | | deletes cached images and queues a refetch |

Saving a TMDB key (`set_tmdb_key`) and finishing a scan queue any missing or stale metadata.

## Player commands (owned by the player module)

| Command | Args | Returns |
|---|---|---|
| `player_open` | `fileId, startAt: number\|null` (seconds; null = from the beginning) | `{embedded: bool, item: PlaybackItem}` |
| `player_close` | | |
| `player_toggle_pause` | | |
| `player_seek` | `seconds, relative` | |
| `player_set_volume` | `volume` (0–100) | |
| `player_toggle_mute` | | |
| `player_set_audio` | `trackId` | |
| `player_set_subtitle` | `trackId: number\|null` | null = off |
| `player_add_subtitle` | | opens a native file picker in Rust, then loads the chosen file |
| `player_toggle_fullscreen` | | `bool`, the new fullscreen state |
| `player_exit_fullscreen` | | `bool`, true if the window was fullscreen |

`player_open` while already playing switches files (used for next episode). Resume position is
chosen by the UI. Rust saves progress by itself; the UI never does.

## Events

| Event | Payload |
|---|---|
| `scan://progress` | `{found, done, error: string\|null}` |
| `library://updated` | none. Reload whatever is on screen |
| `player://state` | `{state: "playing"\|"paused"\|"buffering", volume, muted}` (sent on any of these changing) |
| `player://time` | `{position, duration}` at most ~4 per second |
| `player://tracks` | `{audio: Track[], subtitles: Track[]}` with `Track = {id, title, lang, selected}` (`title`/`lang` may be null) |
| `player://ended` | `{file_id}` at end of file (the last frame stays up) |
| `player://error` | `{message}` |
| `player://closed` | `{file_id}` when playback closes from the native side (fallback window) |

While the embedded player is open the UI adds `class="playing"` to `<html>` and must make
`html`, `body` and everything outside the player controls transparent or hidden, because the
video is drawn underneath the webview. When `embedded` is false, video is in a separate window
and the UI shows a plain "Playing in a separate window" screen with a Stop button.
