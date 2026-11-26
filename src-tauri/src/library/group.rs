//! Groups parsed files into movies and shows. Pure, no I/O.

use std::collections::BTreeMap;

use super::parse::ParsedMedia;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TitleKind {
    Movie,
    Show,
}

impl TitleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Show => "show",
        }
    }
}

/// What a single file belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub kind: TitleKind,
    pub name_key: String,
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TitleGroup<T> {
    pub kind: TitleKind,
    pub name_key: String,
    pub title: String,
    pub year: Option<u16>,
    /// Sorted by season, then episode.
    pub files: Vec<(Identity, T)>,
}

/// Lowercase words only, so punctuation and casing never split a show in two.
pub fn normalize(name: &str) -> String {
    let unified = name
        .to_lowercase()
        .replace('&', " and ")
        .replace(['\'', '’'], "");
    unified
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn identify(media: &ParsedMedia) -> Identity {
    match media {
        ParsedMedia::Movie { title, year } => {
            let mut name_key = normalize(title);
            if let Some(year) = year {
                name_key = format!("{name_key} {year}");
            }
            Identity {
                kind: TitleKind::Movie,
                name_key,
                title: title.clone(),
                year: *year,
                season: None,
                episode: None,
            }
        }
        ParsedMedia::Episode {
            show,
            year,
            season,
            episode,
        } => Identity {
            kind: TitleKind::Show,
            name_key: normalize(show),
            title: show.clone(),
            year: *year,
            season: Some(*season),
            episode: Some(*episode),
        },
    }
}

/// Files with the same kind and name key become one title, whatever folder
/// they are in. The first file seen decides the display title.
pub fn group<T>(items: impl IntoIterator<Item = (ParsedMedia, T)>) -> Vec<TitleGroup<T>> {
    let mut groups: BTreeMap<(TitleKind, String), TitleGroup<T>> = BTreeMap::new();
    for (media, payload) in items {
        let identity = identify(&media);
        let entry = groups
            .entry((identity.kind, identity.name_key.clone()))
            .or_insert_with(|| TitleGroup {
                kind: identity.kind,
                name_key: identity.name_key.clone(),
                title: identity.title.clone(),
                year: identity.year,
                files: Vec::new(),
            });
        entry.year = entry.year.or(identity.year);
        entry.files.push((identity, payload));
    }
    let mut result: Vec<_> = groups.into_values().collect();
    for title in &mut result {
        title.files.sort_by_key(|(id, _)| (id.season, id.episode));
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::library::parse::parse;

    fn grouped(paths: &[&'static str]) -> Vec<TitleGroup<&'static str>> {
        group(
            paths
                .iter()
                .filter_map(|p| parse(Path::new(p)).map(|media| (media, *p))),
        )
    }

    #[test]
    fn episodes_across_folders_become_one_show() {
        let titles = grouped(&[
            "A/Breaking.Bad.S01E01.mkv",
            "B/breaking bad/Season 2/Episode 01.mkv",
            "C/Breaking_Bad_1x02.mkv",
        ]);
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0].title, "Breaking Bad");
        assert_eq!(titles[0].files.len(), 3);
    }

    #[test]
    fn seasons_and_episodes_sort_numerically() {
        let titles = grouped(&[
            "Show.S10E01.mkv",
            "Show.S02E10.mkv",
            "Show.S02E09.mkv",
            "Show.S01E01.mkv",
        ]);
        let order: Vec<_> = titles[0].files.iter().map(|(_, path)| *path).collect();
        assert_eq!(
            order,
            [
                "Show.S01E01.mkv",
                "Show.S02E09.mkv",
                "Show.S02E10.mkv",
                "Show.S10E01.mkv"
            ]
        );
    }

    #[test]
    fn movies_with_different_years_stay_apart() {
        let titles = grouped(&["Dune.1984.mkv", "Dune.2021.mkv", "dune (2021).mp4"]);
        assert_eq!(titles.len(), 2);
    }

    #[test]
    fn normalize_ignores_punctuation() {
        assert_eq!(
            normalize("Marvel's Agents of S.H.I.E.L.D."),
            "marvels agents of s h i e l d"
        );
        assert_eq!(normalize("Law & Order"), normalize("law and order"));
    }
}
