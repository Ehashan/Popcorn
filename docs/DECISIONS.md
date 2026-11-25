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
