//! Matching a search query against local text. Pure.

use super::group::normalize;

/// The text one title can be found by.
pub struct Document {
    pub id: i64,
    pub title: String,
    /// Cast names and episode names.
    pub extra: Vec<String>,
}

/// Ids of matching titles, title matches first. Every word of the query has to
/// appear, either in the title or together in one cast or episode name.
pub fn search(query: &str, documents: &[Document]) -> Vec<i64> {
    let query = normalize(query);
    let words: Vec<&str> = query.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return Vec::new();
    }
    let matches = |text: &str| {
        let text = normalize(text);
        words.iter().all(|word| text.contains(word))
    };
    let mut ranked: Vec<(u8, &str, i64)> = documents
        .iter()
        .filter_map(|doc| {
            let rank = if normalize(&doc.title).starts_with(&query) {
                0
            } else if matches(&doc.title) {
                1
            } else if doc.extra.iter().any(|text| matches(text)) {
                2
            } else {
                return None;
            };
            Some((rank, doc.title.as_str(), doc.id))
        })
        .collect();
    ranked.sort_unstable();
    ranked.into_iter().map(|(_, _, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_rank_before_cast_and_episodes() {
        let doc = |id, title: &str, extra: &[&str]| Document {
            id,
            title: title.into(),
            extra: extra.iter().map(|s| (*s).to_string()).collect(),
        };
        let documents = [
            doc(1, "Heat", &["Al Pacino"]),
            doc(2, "The Heat", &[]),
            doc(3, "Chef", &["Beat the Heat"]),
        ];
        assert_eq!(search("heat", &documents), [1, 2, 3]);
        assert_eq!(search("pacino al", &documents), [1]);
        assert!(search("  ", &documents).is_empty());
    }
}
