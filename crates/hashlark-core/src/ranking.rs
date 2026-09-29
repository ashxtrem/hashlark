// SPDX-License-Identifier: GPL-3.0-or-later

//! Relevance scoring and result ordering.

use std::cmp::Ordering;

use time::OffsetDateTime;

use crate::model::{MergedResult, SortOrder};

const TITLE_WEIGHT: f32 = 0.55;
const SEEDERS_WEIGHT: f32 = 0.35;
const RECENCY_WEIGHT: f32 = 0.10;
/// Recency halves roughly every 1.4 years (e-folding time of two years).
const RECENCY_DAYS: f32 = 730.0;

/// Lowercased alphanumeric words of `s`.
pub fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// How well `title` matches `query`, in `0.0..=1.0`.
///
/// Mostly recall (share of query words found in the title), plus precision
/// (share of title words that are query words) so that titles padded with
/// unrelated words rank below close matches, plus a little fuzzy similarity.
pub fn title_match(query: &str, title: &str) -> f32 {
    let query = tokens(query);
    let title = tokens(title);
    if query.is_empty() || title.is_empty() {
        return 0.0;
    }
    let recall =
        query.iter().filter(|word| title.contains(word)).count() as f32 / query.len() as f32;
    let precision =
        title.iter().filter(|word| query.contains(word)).count() as f32 / title.len() as f32;
    let similarity = strsim::jaro_winkler(&query.join(" "), &title.join(" ")) as f32;
    0.6 * recall + 0.3 * precision + 0.1 * similarity
}

/// Relevance score in `0.0..=1.0` from title match, seeders and recency.
pub fn score(query: &str, result: &MergedResult, now: OffsetDateTime) -> f32 {
    let title = title_match(query, &result.primary.title);
    let seeders = result
        .seeders
        .map_or(0.0, |s| ((s as f32 + 1.0).log10() / 4.0).min(1.0));
    let recency = result.primary.published.map_or(0.0, |published| {
        let days = (now - published).whole_days().max(0) as f32;
        (-days / RECENCY_DAYS).exp()
    });
    TITLE_WEIGHT * title + SEEDERS_WEIGHT * seeders + RECENCY_WEIGHT * recency
}

/// Sorts `results` in place. Unknown values sort last; ties fall back to
/// relevance.
pub fn sort(results: &mut [MergedResult], order: SortOrder) {
    results.sort_by(|a, b| {
        let primary = match order {
            SortOrder::Relevance => Ordering::Equal,
            SortOrder::Title => a
                .primary
                .title
                .to_lowercase()
                .cmp(&b.primary.title.to_lowercase()),
            SortOrder::Seeders => desc_option(a.seeders, b.seeders),
            SortOrder::Peers => desc_option(a.leechers, b.leechers),
            SortOrder::Size => desc_option(a.primary.size_bytes, b.primary.size_bytes),
            SortOrder::Date => desc_option(a.primary.published, b.primary.published),
        };
        primary
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| desc_option(a.seeders, b.seeders))
    });
}

/// Descending order with `None` after every `Some`.
fn desc_option<T: Ord>(a: Option<T>, b: Option<T>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use time::Duration;

    use super::*;
    use crate::model::SearchResult;

    fn merged(title: &str, seeders: Option<u32>, size: Option<u64>) -> MergedResult {
        MergedResult {
            id: title.into(),
            primary: SearchResult {
                size_bytes: size,
                ..SearchResult::new("p".into(), title)
            },
            sources: vec!["p".into()],
            seeders,
            leechers: None,
            score: 0.0,
        }
    }

    #[test]
    fn exact_title_beats_partial_and_unrelated() {
        let q = "night of the living dead";
        let exact = title_match(q, "Night of the Living Dead");
        let longer = title_match(q, "Night Of The Living Dead (1968) Restored 1080p");
        let partial = title_match(q, "Dawn of the Dead");
        let unrelated = title_match(q, "Ubuntu 24.04");
        assert!(exact > longer, "{exact} > {longer}");
        assert!(longer > partial, "{longer} > {partial}");
        assert!(partial > unrelated, "{partial} > {unrelated}");
        assert!((0.99..=1.0).contains(&exact));
        assert_eq!(title_match("", "anything"), 0.0);
    }

    #[test]
    fn close_old_title_beats_padded_new_one() {
        // Regression: with no seeder data, recency used to outweigh extra
        // unrelated words in the title.
        let now = OffsetDateTime::now_utc();
        let mut exact = merged("Night Of The Living Dead", None, None);
        exact.primary.published = Some(now - Duration::days(730));
        let mut padded = merged(
            "NIGHT OF THE LIVING DEAD REMIX Live On Stage 98 Mins",
            None,
            None,
        );
        padded.primary.published = Some(now - Duration::days(7));

        let q = "night of the living dead";
        assert!(score(q, &exact, now) > score(q, &padded, now));
    }

    #[test]
    fn seeders_and_recency_raise_score() {
        let now = OffsetDateTime::now_utc();
        let few = merged("ubuntu", Some(1), None);
        let many = merged("ubuntu", Some(5000), None);
        assert!(score("ubuntu", &many, now) > score("ubuntu", &few, now));

        let mut old = merged("ubuntu", None, None);
        old.primary.published = Some(now - Duration::days(3650));
        let mut new = merged("ubuntu", None, None);
        new.primary.published = Some(now - Duration::days(10));
        assert!(score("ubuntu", &new, now) > score("ubuntu", &old, now));
    }

    #[test]
    fn sort_orders_put_unknown_last() {
        let mut results = vec![
            merged("a", None, Some(10)),
            merged("b", Some(5), None),
            merged("c", Some(50), Some(5)),
        ];
        results[0].score = 0.9;

        sort(&mut results, SortOrder::Seeders);
        assert_eq!(ids(&results), ["c", "b", "a"]);

        sort(&mut results, SortOrder::Size);
        assert_eq!(ids(&results), ["a", "c", "b"]);

        sort(&mut results, SortOrder::Relevance);
        assert_eq!(ids(&results)[0], "a");

        for r in &mut results {
            r.leechers = match r.id.as_str() {
                "a" => Some(1),
                "b" => Some(9),
                _ => None,
            };
        }
        sort(&mut results, SortOrder::Peers);
        assert_eq!(ids(&results), ["b", "a", "c"]);

        sort(&mut results, SortOrder::Title);
        assert_eq!(ids(&results), ["a", "b", "c"]);
    }

    fn ids(results: &[MergedResult]) -> Vec<&str> {
        results.iter().map(|r| r.id.as_str()).collect()
    }
}
