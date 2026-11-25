# Popcorn

A small, open-source macOS app that turns a local folder of movies and TV series into a
streaming-style library and plays them inside the app. It never copies, moves or renames your files.

> Early development. Apple Silicon, macOS 13 or newer.

## Screenshots

_Coming soon._

## Building

Requirements: Xcode Command Line Tools, Rust (stable) and the Tauri CLI.

```sh
cargo install tauri-cli --version "^2" --locked
cargo tauri dev
```

Checks:

```sh
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Getting a TMDB API key

1. Create a free account at <https://www.themoviedb.org>.
2. Open **Settings → API** and request a key.
3. Paste either the **API Key** or the **API Read Access Token** into Popcorn.

No key is stored in this repository. Yours stays in the app's local database.

## Opening an unsigned build

Popcorn is not code signed or notarized yet. The first time, **right-click the app → Open**.
On macOS 15 and newer, if that is not offered, go to **System Settings → Privacy & Security**
and choose **Open Anyway**.

## License

[GPL-3.0](LICENSE).

This product uses the TMDB API but is not endorsed or certified by TMDB.
