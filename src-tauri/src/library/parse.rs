//! Turns a video file path into a guess about what it is. Pure, no I/O.

use std::path::{Component, Path};
use std::sync::LazyLock;

use regex::Regex;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "mov", "avi", "webm", "ts", "m2ts", "wmv", "flv", "mpg", "mpeg",
];
const SAMPLE_MAX_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedMedia {
    Movie {
        title: String,
        year: Option<u16>,
    },
    Episode {
        show: String,
        year: Option<u16>,
        season: u32,
        episode: u32,
    },
}

/// All patterns are constants, so a failure here is a programming error that
/// every parser test would catch.
fn pattern(source: &str) -> Regex {
    #[allow(clippy::expect_used)]
    Regex::new(source).expect("invalid built-in pattern")
}

static SXXEYY: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?i)(?:^|[^a-z0-9])s(\d{1,2})[ ._-]?e(\d{1,3})(?:[ ._-]?e\d{1,3})*(?:$|[^a-z0-9])")
});
static NXNN: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)(?:^|[^a-z0-9])(\d{1,2})x(\d{2,3})(?:$|[^a-z0-9])"));
static SEASON_DIR: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^(?:(?:season|series)[ ._-]*(\d{1,2})|s(\d{1,2})|(specials))$"));
static EPISODE_WORD: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?i)^(?:episode|ep|e)[ ._-]*(\d{1,3})(?:$|[^a-z0-9])"));
static LEADING_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"^(\d{1,3})(?:$|[ ._]*-|[ ._])"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?:19|20)\d{2}"));
static TAG: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)(?:^|[ ._\-\[\(\{])(480p|576p|720p|1080[pi]|2160p|4k|uhd|hdr10\+?|hdr|dv|x26[45]|h[ ._]?26[45]|hevc|av1|blu-?ray|brrip|bdrip|web-?dl|web-?rip|hdtv|dvdrip|aac(?:[ ._]?[257][ ._]?[01])?|e?ac-?3|dts(?:-?hd)?|ddp?\+?(?:[ ._]?[257][ ._]?[01])?|atmos|remux|proper|repack)(?:$|[ ._\-\]\)\}\[\(\{])",
    )
});
static SQUARE_GROUP: LazyLock<Regex> = LazyLock::new(|| pattern(r"\[([^\]]*)\]|\{([^}]*)\}"));
static PAREN_GROUP: LazyLock<Regex> = LazyLock::new(|| pattern(r"\(([^)]*)\)"));

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| VIDEO_EXTENSIONS.iter().any(|v| ext.eq_ignore_ascii_case(v)))
}

/// Hidden files (including `._*` AppleDouble files) and small samples.
pub fn is_ignored(file_name: &str, size: u64) -> bool {
    file_name.starts_with('.')
        || (size < SAMPLE_MAX_BYTES && file_name.to_ascii_lowercase().contains("sample"))
}

/// `path` should start at the chosen media folder (inclusive), so that folder
/// names can be used to find the show name.
pub fn parse(path: &Path) -> Option<ParsedMedia> {
    if !is_video(path) {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let folders: Vec<&str> = path
        .parent()
        .map(|parent| {
            parent
                .components()
                .filter_map(|c| match c {
                    Component::Normal(name) => name.to_str(),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    if let Some((start, season, episode)) = find_marker(stem) {
        return Some(episode_of(&stem[..start], &folders, season, episode));
    }

    if let Some(season) = folders.last().and_then(|dir| season_of(dir)) {
        let number = EPISODE_WORD
            .captures(stem)
            .or_else(|| LEADING_NUMBER.captures(stem))
            .and_then(|c| c[1].parse().ok());
        if let Some(episode) = number {
            return Some(episode_of("", &folders, season, episode));
        }
    }

    let (title, year) = split_title(stem);
    let title = if title.is_empty() {
        stem.to_string()
    } else {
        title
    };
    Some(ParsedMedia::Movie { title, year })
}

/// Finds `S01E01` or `1x02`, returning where it starts.
fn find_marker(stem: &str) -> Option<(usize, u32, u32)> {
    let captures = SXXEYY.captures(stem).or_else(|| {
        NXNN.captures(stem)
            // `5.1x264` is an audio layout followed by a codec.
            .filter(|c| !matches!(&c[2], "264" | "265"))
    })?;
    let start = captures.get(0)?.start();
    Some((start, captures[1].parse().ok()?, captures[2].parse().ok()?))
}

fn season_of(folder: &str) -> Option<u32> {
    let captures = SEASON_DIR.captures(folder.trim())?;
    match captures.get(1).or_else(|| captures.get(2)) {
        Some(number) => number.as_str().parse().ok(),
        None => Some(0), // "Specials"
    }
}

fn episode_of(before: &str, folders: &[&str], season: u32, episode: u32) -> ParsedMedia {
    let (mut show, mut year) = split_title(before);
    if show.is_empty() {
        if let Some(folder) = folders.iter().rev().find(|dir| season_of(dir).is_none()) {
            (show, year) = split_title(folder);
        }
    }
    if show.is_empty() {
        show = "Unknown Show".to_string();
    }
    ParsedMedia::Episode {
        show,
        year,
        season,
        episode,
    }
}

/// Splits a raw name into a clean title and the release year, if there is one.
fn split_title(raw: &str) -> (String, Option<u16>) {
    let text = strip_groups(raw);

    let tag_start = TAG
        .captures_iter(&text)
        .filter_map(|c| c.get(1).map(|m| m.start()))
        .find(|&start| !clean(&text[..start]).is_empty());
    let head = &text[..tag_start.unwrap_or(text.len())];

    // The last year that still leaves a title in front of it, so that
    // "2001 A Space Odyssey 1968" and "1917 2019" work.
    let year = YEAR
        .find_iter(head)
        .filter(|m| {
            let alone = |c: Option<char>| !c.is_some_and(char::is_alphanumeric);
            alone(head[..m.start()].chars().next_back()) && alone(head[m.end()..].chars().next())
        })
        .filter(|m| !clean(&head[..m.start()]).is_empty())
        .last();

    match year {
        Some(m) => (clean(&head[..m.start()]), m.as_str().parse().ok()),
        None => (clean(head), None),
    }
}

/// Drops `[..]`, `{..}` and `(..)` junk, keeping the content when it is a year.
fn strip_groups(raw: &str) -> String {
    let is_year = |s: &str| {
        let s = s.trim();
        s.len() == 4 && YEAR.is_match(s)
    };
    let without_square = SQUARE_GROUP.replace_all(raw, |c: &regex::Captures<'_>| {
        let inner = c.get(1).or_else(|| c.get(2)).map_or("", |m| m.as_str());
        if is_year(inner) {
            format!(" {} ", inner.trim())
        } else {
            " ".to_string()
        }
    });
    PAREN_GROUP
        .replace_all(&without_square, |c: &regex::Captures<'_>| {
            let whole = c.get(0);
            if is_year(&c[1]) {
                format!(" {} ", c[1].trim())
            } else if whole.is_some_and(|m| m.start() == 0) {
                // "(500) Days of Summer": a leading group is part of the title.
                c[0].to_string()
            } else {
                " ".to_string()
            }
        })
        .into_owned()
}

fn clean(raw: &str) -> String {
    let spaced = raw.replace(['.', '_'], " ");
    let joined = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = joined
        .trim_matches(|c: char| c.is_whitespace() || "-–—:,[{".contains(c))
        // An opening parenthesis can be left over when we cut at a tag.
        .trim_end_matches([' ', '(', '-']);
    fix_case(trimmed)
}

/// Title-cases names that are entirely lowercase or entirely uppercase.
fn fix_case(name: &str) -> String {
    let has_lower = name.chars().any(char::is_lowercase);
    let has_upper = name.chars().any(char::is_uppercase);
    if has_lower == has_upper {
        return name.to_string();
    }
    name.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie(title: &str, year: impl Into<Option<u16>>) -> Option<ParsedMedia> {
        Some(ParsedMedia::Movie {
            title: title.into(),
            year: year.into(),
        })
    }

    fn episode(show: &str, season: u32, episode: u32) -> Option<ParsedMedia> {
        episode_in(show, None, season, episode)
    }

    fn episode_in(show: &str, year: Option<u16>, season: u32, episode: u32) -> Option<ParsedMedia> {
        Some(ParsedMedia::Episode {
            show: show.into(),
            year,
            season,
            episode,
        })
    }

    fn check(cases: &[(&str, Option<ParsedMedia>)]) {
        for (input, expected) in cases {
            assert_eq!(&parse(Path::new(input)), expected, "input: {input}");
        }
    }

    #[test]
    fn required_movies() {
        check(&[
            ("Movie.Title.2025.mp4", movie("Movie Title", 2025)),
            (
                "The.Matrix.1999.1080p.BluRay.x264-GROUP.mkv",
                movie("The Matrix", 1999),
            ),
            (
                "Dune Part Two (2024) [2160p] [HDR].mkv",
                movie("Dune Part Two", 2024),
            ),
        ]);
    }

    #[test]
    fn required_episodes() {
        check(&[
            ("Show.S01E01.mkv", episode("Show", 1, 1)),
            ("Show_S02E10.mkv", episode("Show", 2, 10)),
            ("Show.1x02.mkv", episode("Show", 1, 2)),
            (
                "Cyberpunk_Edgerunners_S01E01.mkv",
                episode("Cyberpunk Edgerunners", 1, 1),
            ),
            ("Show/Season 1/Episode 01.mkv", episode("Show", 1, 1)),
            ("Show/S01/01 - Pilot.mkv", episode("Show", 1, 1)),
            (
                "Breaking.Bad.S05E14.Ozymandias.720p.mkv",
                episode("Breaking Bad", 5, 14),
            ),
        ]);
    }

    #[test]
    fn more_episode_forms() {
        check(&[
            ("show.s1e1.mkv", episode("Show", 1, 1)),
            ("Show.S01.E03.mkv", episode("Show", 1, 3)),
            ("Show S01 E04.mkv", episode("Show", 1, 4)),
            ("Show.S01E05E06.mkv", episode("Show", 1, 5)),
            ("Show/Season 01/05.mkv", episode("Show", 1, 5)),
            ("Show/Specials/02.mkv", episode("Show", 0, 2)),
            ("Media/Dark/Season 2/S02E03.mkv", episode("Dark", 2, 3)),
            (
                "The.Flash.2014.S01E01.mkv",
                episode_in("The Flash", Some(2014), 1, 1),
            ),
            (
                "Doctor Who (2005)/Season 1/Episode 2.mkv",
                episode_in("Doctor Who", Some(2005), 1, 2),
            ),
        ]);
    }

    #[test]
    fn tricky_movies() {
        check(&[
            (
                "2001.A.Space.Odyssey.1968.1080p.mkv",
                movie("2001 A Space Odyssey", 1968),
            ),
            ("1917.2019.mkv", movie("1917", 2019)),
            ("2012.mkv", movie("2012", None)),
            ("Spider-Man.mkv", movie("Spider-Man", None)),
            ("some_home_video.mov", movie("Some Home Video", None)),
            ("THE THING [1982].mkv", movie("The Thing", 1982)),
            (
                "Heat.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv",
                movie("Heat", None),
            ),
            (
                "(500) Days of Summer (2009).mkv",
                movie("(500) Days of Summer", 2009),
            ),
            ("Alien (Directors Cut) (1979).mkv", movie("Alien", 1979)),
            ("Wide.1920x1080.mkv", movie("Wide 1920x1080", None)),
        ]);
    }

    #[test]
    fn non_video_files_are_skipped() {
        check(&[("notes.txt", None), ("Movie.2020.srt", None)]);
    }

    #[test]
    fn ignored_files() {
        assert!(is_ignored(".hidden.mkv", 5_000_000_000));
        assert!(is_ignored("._Movie.mkv", 4096));
        assert!(is_ignored("movie-sample.mkv", 30 * 1024 * 1024));
        assert!(!is_ignored("The Sample (2020).mkv", SAMPLE_MAX_BYTES));
        assert!(!is_ignored("Movie.mkv", 10));
    }
}
