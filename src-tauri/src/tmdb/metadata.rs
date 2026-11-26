//! The trimmed metadata stored in `titles.metadata_json`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub title: String,
    pub year: Option<i64>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    pub genres: Vec<String>,
    /// Minutes.
    pub runtime: Option<u32>,
    /// 0 to 10.
    pub rating: Option<f64>,
    pub certification: Option<String>,
    pub cast: Vec<CastMember>,
    /// Directors of a movie, or creators of a show.
    pub directors: Vec<String>,
    pub seasons: Vec<SeasonInfo>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CastMember {
    pub name: String,
    pub character: Option<String>,
    /// Path of the locally cached image.
    pub profile_path: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeasonInfo {
    pub number: u32,
    pub name: Option<String>,
    pub episode_count: Option<u32>,
}
