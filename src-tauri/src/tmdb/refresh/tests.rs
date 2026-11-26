use serde_json::{json, Value};

use super::testing::{FakeApi, TempDir};
use super::*;
use crate::db;
use crate::db::queries::{title_details, TitleDetails};

const SHOW: i64 = 1;
const MOVIE: i64 = 2;

fn state(dir: &TempDir) -> AppState {
    let conn = db::open_in_memory().unwrap();
    conn.execute_batch(
        "INSERT INTO titles (id, kind, name_key, guessed_title, guessed_year) VALUES
           (1, 'show', 'dark', 'Dark', NULL), (2, 'movie', 'heat 1995', 'Heat', 1995);
         INSERT INTO files (id, path, title_id, season, episode, size, mtime, added_at) VALUES
           (10, '/m/dark.s01e01.mkv', 1, 1, 1, 5, 0, 1),
           (20, '/m/heat.mkv', 2, NULL, NULL, 5, 0, 1);",
    )
    .unwrap();
    AppState::new(conn, dir.path().to_path_buf())
}

fn movie_details(title: &str, poster: &str) -> Value {
    json!({
        "title": title, "poster_path": poster,
        "credits": {"cast": [{"name": "Al", "profile_path": "/al.jpg"}]},
    })
}

fn movie_api<'a>() -> FakeApi<'a> {
    FakeApi::default()
        .json(
            "/search/movie?query=Heat&year=1995",
            json!({"results": [{"id": 5, "title": "Heat"}]}),
        )
        .json(
            "/movie/5?append_to_response=credits,release_dates",
            movie_details("Heat", "/p.jpg"),
        )
}

fn show_api<'a>(id: i64, episode_name: &str) -> FakeApi<'a> {
    FakeApi::default()
        .json(
            "/search/tv?query=Dark",
            json!({"results": [{"id": id, "name": "Dark"}]}),
        )
        .json(
            &format!("/tv/{id}?append_to_response=credits"),
            json!({"name": "Dark"}),
        )
        .json(
            &format!("/tv/{id}/season/1"),
            json!({"episodes": [{"episode_number": 1, "name": episode_name,
                                 "still_path": "/still.jpg"}]}),
        )
}

fn details(state: &AppState, id: i64) -> TitleDetails {
    title_details(&state.db(), id).unwrap()
}

fn needed(state: &AppState) -> Vec<i64> {
    store::needing_fetch(&state.db(), unix_now()).unwrap()
}

#[test]
fn a_match_is_stored_once() {
    let dir = TempDir::new("refresh-movie");
    let state = state(&dir);
    let api = movie_api();
    assert_eq!(needed(&state).len(), 2);

    let outcome = refresh(&state, &api, MOVIE, Mode::Auto).unwrap();
    assert_eq!(outcome, Outcome::Stored);
    let movie = details(&state, MOVIE);
    assert_eq!(movie.tmdb_id, Some(5));
    assert!(movie.summary.matched);
    assert!(movie.summary.poster_path.unwrap().ends_with("p.jpg"));
    assert!(movie.cast[0].profile_path.is_some());
    assert_eq!(needed(&state), [SHOW]);

    let requests = api.requests().len();
    let outcome = refresh(&state, &api, MOVIE, Mode::Auto).unwrap();
    assert_eq!(outcome, Outcome::Skipped);
    assert_eq!(api.requests().len(), requests);
}

#[test]
fn no_match_is_not_retried_until_the_key_changes() {
    let dir = TempDir::new("refresh-nomatch");
    let state = state(&dir);
    let api = FakeApi::default().json("/search/tv?query=Dark", json!({"results": []}));

    let outcome = refresh(&state, &api, SHOW, Mode::Auto).unwrap();
    assert_eq!(outcome, Outcome::NoMatch);
    assert!(!details(&state, SHOW).summary.matched);
    assert_eq!(needed(&state), [MOVIE]);

    // A new episode does not make an unmatched show worth searching again.
    state
        .db()
        .execute(
            "INSERT INTO files (path, title_id, season, episode, size, mtime, added_at)
             VALUES ('/m/dark.s01e02.mkv', 1, 1, 2, 5, 0, ?1)",
            [unix_now() + 5],
        )
        .unwrap();
    assert_eq!(needed(&state), [MOVIE]);

    store::retry_unmatched(&state.db()).unwrap();
    assert_eq!(needed(&state).len(), 2);
}

#[test]
fn metadata_goes_stale_after_thirty_days() {
    let dir = TempDir::new("refresh-age");
    let state = state(&dir);
    refresh(&state, &movie_api(), MOVIE, Mode::Auto).unwrap();
    let month = 30 * 24 * 60 * 60;
    let db = state.db();
    assert!(!store::needs_fetch(&db, MOVIE, unix_now() + month - 60).unwrap());
    assert!(store::needs_fetch(&db, MOVIE, unix_now() + month + 60).unwrap());
}

#[test]
fn a_new_episode_without_metadata_is_fetched() {
    let dir = TempDir::new("refresh-episode");
    let state = state(&dir);
    refresh(&state, &show_api(7, "Secrets"), SHOW, Mode::Auto).unwrap();
    assert_eq!(needed(&state), [MOVIE]);

    state
        .db()
        .execute(
            "INSERT INTO files (path, title_id, season, episode, size, mtime, added_at)
             VALUES ('/m/dark.s01e02.mkv', 1, 1, 2, 5, 0, ?1)",
            [unix_now() + 5],
        )
        .unwrap();
    assert!(needed(&state).contains(&SHOW));
}

#[test]
fn titles_without_files_on_disk_are_left_alone() {
    let dir = TempDir::new("refresh-missing");
    let state = state(&dir);
    state
        .db()
        .execute("UPDATE files SET missing = 1 WHERE title_id = 2", [])
        .unwrap();
    assert_eq!(needed(&state), [SHOW]);
}

#[test]
fn a_failed_refresh_keeps_what_is_cached() {
    let dir = TempDir::new("refresh-offline");
    let state = state(&dir);
    refresh(&state, &movie_api(), MOVIE, Mode::Auto).unwrap();
    store::mark_all_stale(&state.db()).unwrap();

    let offline = FakeApi::default();
    let result = refresh(&state, &offline, MOVIE, Mode::Auto);
    assert!(matches!(result, Err(AppError::Offline)));
    assert!(details(&state, MOVIE).summary.matched);
    assert!(needed(&state).contains(&MOVIE));
}

#[test]
fn a_permanent_error_is_not_retried_at_once() {
    let dir = TempDir::new("refresh-gone");
    let state = state(&dir);
    refresh(&state, &movie_api(), MOVIE, Mode::Auto).unwrap();
    store::mark_all_stale(&state.db()).unwrap();

    let gone = FakeApi::default().fail("/movie/5?append_to_response=credits,release_dates", 404);
    assert!(refresh(&state, &gone, MOVIE, Mode::Auto).is_err());
    assert!(details(&state, MOVIE).summary.matched);
    assert_eq!(needed(&state), [SHOW]);
}

#[test]
fn unavailable_images_keep_the_old_ones_and_stay_stale() {
    let dir = TempDir::new("refresh-images");
    let state = state(&dir);
    refresh(&state, &movie_api(), MOVIE, Mode::Auto).unwrap();
    let old = details(&state, MOVIE);

    let api = FakeApi::default()
        .json(
            "/movie/5?append_to_response=credits,release_dates",
            json!({"title": "Heat (new)", "poster_path": "/new.jpg",
                   "credits": {"cast": [{"name": "Al", "profile_path": "/new-al.jpg"}]}}),
        )
        .images_offline();
    refresh(&state, &api, MOVIE, Mode::Manual).unwrap();

    let new = details(&state, MOVIE);
    assert_eq!(new.summary.title, "Heat (new)");
    assert_eq!(new.summary.poster_path, old.summary.poster_path);
    assert_eq!(new.cast[0].profile_path, old.cast[0].profile_path);
    assert!(needed(&state).contains(&MOVIE));
}

#[test]
fn fix_match_replaces_the_old_match_and_locks() {
    let dir = TempDir::new("refresh-fix");
    let state = state(&dir);
    refresh(&state, &show_api(7, "Wrong"), SHOW, Mode::Auto).unwrap();

    // The new show has no data for season 1, so nothing old may survive.
    let api = FakeApi::default()
        .json(
            "/tv/8?append_to_response=credits",
            json!({"name": "Dark 2"}),
        )
        .fail("/tv/8/season/1", 404);
    refresh(&state, &api, SHOW, Mode::Fix(8)).unwrap();

    let show = details(&state, SHOW);
    assert_eq!(show.tmdb_id, Some(8));
    assert_eq!(show.summary.title, "Dark 2");
    assert_eq!(show.seasons[0].episodes[0].name, None);
    assert_eq!(show.seasons[0].episodes[0].still_path, None);
    let locked: bool = state
        .db()
        .query_row("SELECT match_locked FROM titles WHERE id = 1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(locked);

    // Later refreshes use the chosen id without searching.
    let api = show_api(8, "Right");
    refresh(&state, &api, SHOW, Mode::Manual).unwrap();
    assert!(!api.requests().iter().any(|r| r.starts_with("/search")));
    let show = details(&state, SHOW);
    assert_eq!(show.seasons[0].episodes[0].name.as_deref(), Some("Right"));
}

#[test]
fn a_failed_fix_changes_nothing() {
    let dir = TempDir::new("refresh-fix-fail");
    let state = state(&dir);
    refresh(&state, &show_api(7, "Secrets"), SHOW, Mode::Auto).unwrap();

    let api = FakeApi::default()
        .json("/tv/8?append_to_response=credits", json!({"name": "Other"}))
        .fail("/tv/8/season/1", 500);
    assert!(refresh(&state, &api, SHOW, Mode::Fix(8)).is_err());

    let show = details(&state, SHOW);
    assert_eq!(show.tmdb_id, Some(7));
    assert_eq!(show.summary.title, "Dark");
    assert_eq!(show.seasons[0].episodes[0].name.as_deref(), Some("Secrets"));
}

// These two also prove that the database is unlocked during requests: the hook
// would deadlock otherwise.

#[test]
fn a_title_deleted_mid_fetch_is_dropped() {
    let dir = TempDir::new("refresh-deleted");
    let state = state(&dir);
    let api = movie_api().during_requests(|| {
        state
            .db()
            .execute("DELETE FROM titles WHERE id = 2", [])
            .unwrap();
    });
    let outcome = refresh(&state, &api, MOVIE, Mode::Auto).unwrap();
    assert_eq!(outcome, Outcome::Skipped);
}

#[test]
fn a_fix_made_mid_fetch_wins() {
    let dir = TempDir::new("refresh-race");
    let state = state(&dir);
    let api = movie_api().during_requests(|| {
        state
            .db()
            .execute(
                "UPDATE titles SET tmdb_id = 99, match_locked = 1 WHERE id = 2",
                [],
            )
            .unwrap();
    });
    let outcome = refresh(&state, &api, MOVIE, Mode::Auto).unwrap();
    assert_eq!(outcome, Outcome::Skipped);
    assert_eq!(details(&state, MOVIE).tmdb_id, Some(99));
}

#[test]
fn clearing_images_forgets_every_path() {
    let dir = TempDir::new("refresh-clear");
    let state = state(&dir);
    refresh(&state, &movie_api(), MOVIE, Mode::Auto).unwrap();
    refresh(&state, &show_api(7, "Secrets"), SHOW, Mode::Auto).unwrap();

    store::forget_images(&mut state.db()).unwrap();

    let movie = details(&state, MOVIE);
    assert!(movie.summary.matched);
    assert_eq!(movie.summary.poster_path, None);
    assert_eq!(movie.cast[0].profile_path, None);
    let show = details(&state, SHOW);
    assert_eq!(show.seasons[0].episodes[0].still_path, None);
    assert_eq!(needed(&state).len(), 2);
}
