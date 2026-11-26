//! Choosing a search result and trimming TMDB's JSON. Pure, no I/O.
//!
//! Image paths returned here are TMDB's remote ones, such as `/abc.jpg`.

use serde_json::Value;

use super::metadata::{CastMember, Metadata, SeasonInfo};
use crate::library::group::{normalize, TitleKind};

const MAX_CAST: usize = 15;
const PREFERRED_COUNTRY: &str = "US";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub tmdb_id: i64,
    pub title: String,
    pub original_title: Option<String>,
    pub year: Option<i64>,
    pub poster: Option<String>,
}

/// A trimmed details response. `metadata` has no profile paths yet.
#[derive(Debug, Default, PartialEq)]
pub struct Details {
    pub metadata: Metadata,
    pub poster: Option<String>,
    pub backdrop: Option<String>,
    /// One entry for each member of `metadata.cast`, in the same order.
    pub profiles: Vec<Option<String>>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RemoteEpisode {
    pub number: u32,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub still: Option<String>,
}

/// TMDB names a few fields differently for movies and shows.
fn title_fields(kind: TitleKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        TitleKind::Movie => ("title", "original_title", "release_date"),
        TitleKind::Show => ("name", "original_name", "first_air_date"),
    }
}

/// A string field, treating an empty one as absent.
fn text(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn number(value: &Value, key: &str) -> Option<u32> {
    value[key].as_u64().and_then(|n| u32::try_from(n).ok())
}

fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map_or(&[], Vec::as_slice)
}

/// The year of a `YYYY-MM-DD` date.
fn year_of(value: &Value, key: &str) -> Option<i64> {
    value[key].as_str()?.get(..4)?.parse().ok()
}

pub fn search_results(kind: TitleKind, response: &Value) -> Vec<SearchResult> {
    let (title, original, date) = title_fields(kind);
    list(response, "results")
        .iter()
        .filter_map(|result| {
            Some(SearchResult {
                tmdb_id: result["id"].as_i64()?,
                title: text(result, title)?,
                original_title: text(result, original),
                year: year_of(result, date),
                poster: text(result, "poster_path"),
            })
        })
        .collect()
}

/// The first result whose title matches exactly, otherwise the first result.
pub fn pick_best<'a>(results: &'a [SearchResult], wanted: &str) -> Option<&'a SearchResult> {
    let wanted = normalize(wanted);
    // Titles with no letters or digits all normalize to nothing.
    let matches = |title: &str| !wanted.is_empty() && normalize(title) == wanted;
    results
        .iter()
        .find(|r| matches(&r.title) || r.original_title.as_deref().is_some_and(matches))
        .or_else(|| results.first())
}

/// Fails (`None`) if the response is not a details object with a title.
pub fn details(kind: TitleKind, response: &Value) -> Option<Details> {
    let (title, _, date) = title_fields(kind);
    let credits = &response["credits"];
    let (cast, profiles) = list(credits, "cast")
        .iter()
        .filter_map(|member| {
            let cast = CastMember {
                name: text(member, "name")?,
                character: text(member, "character"),
                profile_path: None,
            };
            Some((cast, text(member, "profile_path")))
        })
        .take(MAX_CAST)
        .unzip();

    let metadata = Metadata {
        title: text(response, title)?,
        year: year_of(response, date),
        overview: text(response, "overview"),
        tagline: text(response, "tagline"),
        genres: list(response, "genres")
            .iter()
            .filter_map(|genre| text(genre, "name"))
            .collect(),
        runtime: match kind {
            TitleKind::Movie => number(response, "runtime"),
            TitleKind::Show => list(response, "episode_run_time")
                .first()
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
        }
        .filter(|minutes| *minutes > 0),
        // TMDB reports 0 for titles nobody has rated.
        rating: response["vote_average"].as_f64().filter(|r| *r > 0.0),
        certification: certification(response),
        cast,
        directors: match kind {
            TitleKind::Movie => directors(credits),
            TitleKind::Show => list(response, "created_by")
                .iter()
                .filter_map(|creator| text(creator, "name"))
                .collect(),
        },
        seasons: list(response, "seasons")
            .iter()
            .filter_map(|season| {
                Some(SeasonInfo {
                    number: number(season, "season_number")?,
                    name: text(season, "name"),
                    episode_count: number(season, "episode_count"),
                })
            })
            .collect(),
    };

    Some(Details {
        metadata,
        poster: text(response, "poster_path"),
        backdrop: text(response, "backdrop_path"),
        profiles,
    })
}

fn directors(credits: &Value) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for member in list(credits, "crew") {
        if member["job"] == "Director" {
            if let Some(name) = text(member, "name").filter(|name| !names.contains(name)) {
                names.push(name);
            }
        }
    }
    names
}

/// From a movie's `release_dates`: the US certification if there is one,
/// otherwise the first country that has any.
fn certification(response: &Value) -> Option<String> {
    let countries = list(&response["release_dates"], "results");
    let of = |country: &Value| {
        list(country, "release_dates")
            .iter()
            .find_map(|release| text(release, "certification"))
    };
    countries
        .iter()
        .filter(|country| country["iso_3166_1"] == PREFERRED_COUNTRY)
        .find_map(of)
        .or_else(|| countries.iter().find_map(of))
}

/// The episodes of a season response.
pub fn episodes(response: &Value) -> Vec<RemoteEpisode> {
    list(response, "episodes")
        .iter()
        .filter_map(|episode| {
            Some(RemoteEpisode {
                number: number(episode, "episode_number")?,
                name: text(episode, "name"),
                overview: text(episode, "overview"),
                still: text(episode, "still_path"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_exact_title_beats_the_first_result() {
        let response = json!({"results": [
            {"id": 1, "title": "Heat Wave", "release_date": "1990-05-01"},
            {"id": 2, "title": "HEAT!", "release_date": "1995-12-15", "poster_path": "/h.jpg"},
            {"id": 3, "title": "Chaleur", "original_title": "The Heat", "release_date": ""},
        ]});
        let results = search_results(TitleKind::Movie, &response);
        assert_eq!(results.len(), 3);

        let best = pick_best(&results, "heat").unwrap();
        assert_eq!((best.tmdb_id, best.year), (2, Some(1995)));
        assert_eq!(best.poster.as_deref(), Some("/h.jpg"));
        assert_eq!(pick_best(&results, "the heat").unwrap().tmdb_id, 3);
        assert_eq!(pick_best(&results, "something else").unwrap().tmdb_id, 1);
        assert_eq!(results[2].year, None);
        assert_eq!(pick_best(&[], "heat"), None);
    }

    #[test]
    fn shows_use_their_own_field_names() {
        let response = json!({"results": [
            {"id": 7, "name": "Dark", "first_air_date": "2017-12-01"},
            {"name": "No id"},
        ]});
        let results = search_results(TitleKind::Show, &response);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].year, Some(2017));
    }

    #[test]
    fn trims_a_movie() {
        let cast: Vec<_> = (0..20)
            .map(|i| json!({"name": format!("Actor {i}"), "character": "", "profile_path": null}))
            .collect();
        let response = json!({
            "title": "Heat", "release_date": "1995-12-15", "overview": "A heist.",
            "tagline": "", "runtime": 170, "vote_average": 7.9,
            "genres": [{"id": 80, "name": "Crime"}],
            "poster_path": "/p.jpg", "backdrop_path": null,
            "credits": {"cast": cast, "crew": [
                {"job": "Director", "name": "Michael Mann"},
                {"job": "Producer", "name": "Someone"},
            ]},
            "release_dates": {"results": [
                {"iso_3166_1": "DE", "release_dates": [{"certification": "16"}]},
                {"iso_3166_1": "US", "release_dates": [
                    {"certification": ""}, {"certification": "R"}]},
            ]},
        });
        let details = details(TitleKind::Movie, &response).unwrap();
        let meta = &details.metadata;
        assert_eq!(meta.title, "Heat");
        assert_eq!(meta.year, Some(1995));
        assert_eq!(meta.runtime, Some(170));
        assert_eq!(meta.tagline, None);
        assert_eq!(meta.genres, ["Crime"]);
        assert_eq!(meta.directors, ["Michael Mann"]);
        assert_eq!(meta.certification.as_deref(), Some("R"));
        assert_eq!(meta.cast.len(), 15);
        assert_eq!(meta.cast[0].character, None);
        assert_eq!(details.profiles.len(), 15);
        assert_eq!(details.poster.as_deref(), Some("/p.jpg"));
        assert_eq!(details.backdrop, None);
    }

    #[test]
    fn another_country_is_used_without_a_us_certification() {
        let response = json!({"release_dates": {"results": [
            {"iso_3166_1": "FR", "release_dates": [{"certification": ""}]},
            {"iso_3166_1": "DE", "release_dates": [{"certification": "16"}]},
        ]}});
        assert_eq!(certification(&response).as_deref(), Some("16"));
        assert_eq!(certification(&json!({})), None);
    }

    #[test]
    fn trims_a_show() {
        let response = json!({
            "name": "Dark", "first_air_date": "2017-12-01", "episode_run_time": [],
            "vote_average": 0, "created_by": [{"name": "Baran bo Odar"}],
            "seasons": [{"season_number": 1, "name": "Season 1", "episode_count": 10}],
            "credits": {"cast": [{"name": "Louis", "character": "Jonas", "profile_path": "/l.jpg"}]},
        });
        let details = details(TitleKind::Show, &response).unwrap();
        assert_eq!(details.metadata.directors, ["Baran bo Odar"]);
        assert_eq!(details.metadata.runtime, None);
        assert_eq!(details.metadata.rating, None);
        assert_eq!(details.metadata.seasons[0].episode_count, Some(10));
        assert_eq!(details.profiles, [Some("/l.jpg".to_string())]);
        assert!(super::details(TitleKind::Show, &json!({"success": false})).is_none());
    }

    #[test]
    fn reads_season_episodes() {
        let response = json!({"episodes": [
            {"episode_number": 1, "name": "Secrets", "overview": "", "still_path": "/s.jpg"},
            {"name": "no number"},
        ]});
        let episodes = episodes(&response);
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].name.as_deref(), Some("Secrets"));
        assert_eq!(episodes[0].overview, None);
        assert_eq!(episodes[0].still.as_deref(), Some("/s.jpg"));
    }
}
