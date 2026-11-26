//! The requests that turn a title into metadata and cached images. It never
//! touches the database.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::cache::{self, ImageCache, ImageKind};
use super::client::Client;
use super::matcher::{self, SearchResult};
use super::metadata::Metadata;
use crate::error::{AppError, AppResult};
use crate::library::group::TitleKind;

/// What the pipeline needs from TMDB, so that tests can stand in for it.
pub trait Api {
    fn get_json(&self, path: &str, query: &[(&str, &str)]) -> AppResult<Value>;
    fn download_image(&self, size: &str, remote: &str) -> AppResult<Vec<u8>>;
}

impl Api for Client {
    fn get_json(&self, path: &str, query: &[(&str, &str)]) -> AppResult<Value> {
        Client::get_json(self, path, query)
    }

    fn download_image(&self, size: &str, remote: &str) -> AppResult<Vec<u8>> {
        Client::download_image(self, size, remote)
    }
}

/// A poster or backdrop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// The local path.
    Cached(String),
    /// TMDB has none.
    Absent,
    /// It exists, but could not be fetched this time.
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Episode {
    pub season: u32,
    pub episode: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub still_path: Option<String>,
}

/// Everything there is to store about one match.
#[derive(Debug)]
pub struct Fetched {
    pub tmdb_id: i64,
    pub metadata: Metadata,
    pub poster: Image,
    pub backdrop: Image,
    /// Only episodes that exist locally.
    pub episodes: Vec<Episode>,
}

impl Fetched {
    /// False if an important image should be tried again soon.
    pub fn is_complete(&self) -> bool {
        self.poster != Image::Unavailable && self.backdrop != Image::Unavailable
    }
}

fn endpoint(kind: TitleKind) -> &'static str {
    match kind {
        TitleKind::Movie => "movie",
        TitleKind::Show => "tv",
    }
}

pub fn search(
    api: &impl Api,
    kind: TitleKind,
    query: &str,
    year: Option<i64>,
) -> AppResult<Vec<SearchResult>> {
    let path = format!("/search/{}", endpoint(kind));
    let year_param = match kind {
        TitleKind::Movie => "year",
        TitleKind::Show => "first_air_date_year",
    };
    let year = year.map(|year| year.to_string());
    let mut params = vec![("query", query)];
    if let Some(year) = &year {
        params.push((year_param, year));
    }
    let response = api.get_json(&path, &params)?;
    Ok(matcher::search_results(kind, &response))
}

/// The best match for a guessed title. The year is dropped if nothing matches
/// with it.
pub fn find_match(
    api: &impl Api,
    kind: TitleKind,
    title: &str,
    year: Option<i64>,
) -> AppResult<Option<i64>> {
    let mut results = search(api, kind, title, year)?;
    if results.is_empty() && year.is_some() {
        results = search(api, kind, title, None)?;
    }
    Ok(matcher::pick_best(&results, title).map(|result| result.tmdb_id))
}

/// Fetches a known TMDB entry. `local` lists the (season, episode) pairs on
/// disk. A failed request fails the whole fetch; a failed image does not.
pub fn fetch(
    api: &impl Api,
    images: &ImageCache,
    kind: TitleKind,
    tmdb_id: i64,
    local: &[(u32, u32)],
) -> AppResult<Fetched> {
    let append = match kind {
        TitleKind::Movie => "credits,release_dates",
        TitleKind::Show => "credits",
    };
    let path = format!("/{}/{tmdb_id}", endpoint(kind));
    let response = api.get_json(&path, &[("append_to_response", append)])?;
    let details = matcher::details(kind, &response).ok_or_else(unreadable)?;

    let remote_episodes = match kind {
        TitleKind::Movie => Vec::new(),
        TitleKind::Show => local_episodes(api, tmdb_id, local)?,
    };

    let mut downloader = Downloader {
        api,
        images,
        offline: false,
    };
    let poster = downloader.get(ImageKind::Poster, details.poster.as_deref());
    let backdrop = downloader.get(ImageKind::Backdrop, details.backdrop.as_deref());
    let mut metadata = details.metadata;
    for (member, remote) in metadata.cast.iter_mut().zip(&details.profiles) {
        member.profile_path = downloader.get(ImageKind::Profile, remote.as_deref()).path();
    }
    let episodes = remote_episodes
        .into_iter()
        .map(|(season, remote)| Episode {
            season,
            episode: remote.number,
            still_path: downloader
                .get(ImageKind::Still, remote.still.as_deref())
                .path(),
            name: remote.name,
            overview: remote.overview,
        })
        .collect();

    Ok(Fetched {
        tmdb_id,
        metadata,
        poster,
        backdrop,
        episodes,
    })
}

/// The error for a response that parses but is not what was asked for.
fn unreadable() -> AppError {
    AppError::BadData(serde::de::Error::custom("unexpected TMDB response"))
}

/// One request per season on disk. Seasons TMDB does not know are skipped.
fn local_episodes(
    api: &impl Api,
    tmdb_id: i64,
    local: &[(u32, u32)],
) -> AppResult<Vec<(u32, matcher::RemoteEpisode)>> {
    let mut seasons: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
    for (season, episode) in local {
        seasons.entry(*season).or_default().insert(*episode);
    }
    let mut found = Vec::new();
    for (season, wanted) in seasons {
        let response = match api.get_json(&format!("/tv/{tmdb_id}/season/{season}"), &[]) {
            Err(AppError::Tmdb(404)) => continue,
            response => response?,
        };
        found.extend(
            matcher::episodes(&response)
                .into_iter()
                .filter(|episode| wanted.contains(&episode.number))
                .map(|episode| (season, episode)),
        );
    }
    Ok(found)
}

struct Downloader<'a, A: Api> {
    api: &'a A,
    images: &'a ImageCache,
    /// Set after a network failure, so the remaining images are not each tried.
    offline: bool,
}

impl<A: Api> Downloader<'_, A> {
    fn get(&mut self, kind: ImageKind, remote: Option<&str>) -> Image {
        let Some((remote, name)) = remote.and_then(|r| Some((r, cache::file_name(r)?))) else {
            return Image::Absent;
        };
        let result = self.images.fetch(kind, name, || {
            if self.offline {
                Err(AppError::Offline)
            } else {
                self.api.download_image(kind.size(), remote)
            }
        });
        match result {
            Ok(path) => path
                .into_os_string()
                .into_string()
                .map_or(Image::Absent, Image::Cached),
            Err(error) => {
                log::warn!("could not cache an image: {error}");
                self.offline |= matches!(error, AppError::Offline);
                match error {
                    AppError::Offline | AppError::Io(_) | AppError::Tmdb(429 | 500..=599) => {
                        Image::Unavailable
                    }
                    _ => Image::Absent,
                }
            }
        }
    }
}

impl Image {
    fn path(self) -> Option<String> {
        match self {
            Self::Cached(path) => Some(path),
            Self::Absent | Self::Unavailable => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::tmdb::refresh::testing::{FakeApi, TempDir};

    #[test]
    fn the_year_is_dropped_when_nothing_matches() {
        let api = FakeApi::default()
            .json(
                "/search/tv?query=Dark&first_air_date_year=1999",
                json!({"results": []}),
            )
            .json(
                "/search/tv?query=Dark",
                json!({"results": [{"id": 7, "name": "Dark"}]}),
            );
        let found = find_match(&api, TitleKind::Show, "Dark", Some(1999)).unwrap();
        assert_eq!(found, Some(7));
        assert_eq!(api.requests().len(), 2);
    }

    #[test]
    fn only_local_seasons_and_episodes_are_fetched() {
        let api = FakeApi::default()
            .json("/tv/7?append_to_response=credits", json!({"name": "Dark"}))
            .json(
                "/tv/7/season/1",
                json!({"episodes": [
                    {"episode_number": 1, "name": "One", "still_path": "/s1.jpg"},
                    {"episode_number": 2, "name": "Two"},
                ]}),
            )
            .fail("/tv/7/season/9", 404);
        let dir = TempDir::new("fetch-show");
        let images = ImageCache::new(dir.path());
        let fetched = fetch(&api, &images, TitleKind::Show, 7, &[(1, 1), (9, 1)]).unwrap();
        assert_eq!(fetched.episodes.len(), 1);
        assert!(fetched.episodes[0].still_path.is_some());
        assert!(fetched.is_complete());
    }

    #[test]
    fn a_failing_season_fails_the_fetch() {
        let api = FakeApi::default()
            .json("/tv/7?append_to_response=credits", json!({"name": "Dark"}))
            .fail("/tv/7/season/1", 500);
        let dir = TempDir::new("fetch-fail");
        let images = ImageCache::new(dir.path());
        let result = fetch(&api, &images, TitleKind::Show, 7, &[(1, 1)]);
        assert!(matches!(result, Err(AppError::Tmdb(500))));
    }

    #[test]
    fn going_offline_stops_further_image_downloads() {
        let api = FakeApi::default()
            .json(
                "/movie/5?append_to_response=credits,release_dates",
                json!({"title": "Heat", "poster_path": "/p.jpg", "backdrop_path": "/b.jpg"}),
            )
            .images_offline();
        let dir = TempDir::new("fetch-offline");
        let images = ImageCache::new(dir.path());
        let fetched = fetch(&api, &images, TitleKind::Movie, 5, &[]).unwrap();
        assert_eq!(fetched.poster, Image::Unavailable);
        assert_eq!(fetched.backdrop, Image::Unavailable);
        assert!(!fetched.is_complete());
        assert_eq!(api.downloads(), 1);
    }
}
