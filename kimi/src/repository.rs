use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
};

use crate::models::{LanguageMap, LanguageStat, Repository, RepositoryQuery, RepositorySort};

pub fn apply_preferences(
    mut repositories: Vec<Repository>,
    pinned: &[String],
    excluded: &HashSet<String>,
    show_forks: bool,
    show_archived: bool,
) -> Vec<Repository> {
    repositories.retain(|repository| {
        let name = repository.name.to_ascii_lowercase();
        !excluded.contains(&name)
            && (show_forks || !repository.fork)
            && (show_archived || !repository.archived)
    });

    for repository in &mut repositories {
        repository.pinned = pinned.contains(&repository.name.to_ascii_lowercase());
    }

    repositories.sort_by(|left, right| {
        pin_order(left, right, pinned)
            .then_with(|| right.stargazers_count.cmp(&left.stargazers_count))
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
    });
    repositories
}

pub fn is_excluded(repository: &Repository, excluded: &HashSet<String>) -> bool {
    excluded.contains(&repository.name.to_ascii_lowercase())
}

pub fn filter_and_sort(repositories: &[Repository], query: &RepositoryQuery) -> Vec<Repository> {
    let search = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase);

    let mut filtered: Vec<_> = repositories
        .iter()
        .filter(|repository| {
            search.as_ref().is_none_or(|needle| {
                repository.name.to_ascii_lowercase().contains(needle)
                    || repository
                        .description
                        .as_deref()
                        .is_some_and(|description| {
                            description.to_ascii_lowercase().contains(needle)
                        })
            })
        })
        .filter(|repository| {
            query.language.as_deref().is_none_or(|language| {
                repository
                    .language
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(language))
            })
        })
        .filter(|repository| {
            query.topic.as_deref().is_none_or(|topic| {
                repository
                    .topics
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(topic))
            })
        })
        .cloned()
        .collect();

    let pin_positions: HashMap<_, _> = repositories
        .iter()
        .enumerate()
        .map(|(index, repository)| (repository.name.as_str(), index))
        .collect();
    filtered.sort_by(|left, right| {
        right
            .pinned
            .cmp(&left.pinned)
            .then_with(|| {
                if left.pinned && right.pinned {
                    pin_positions
                        .get(left.name.as_str())
                        .cmp(&pin_positions.get(right.name.as_str()))
                } else {
                    match query.sort {
                        RepositorySort::Stars => right
                            .stargazers_count
                            .cmp(&left.stargazers_count)
                            .then_with(|| right.updated_at.cmp(&left.updated_at)),
                        RepositorySort::Updated => right.updated_at.cmp(&left.updated_at),
                        RepositorySort::Name => Ordering::Equal,
                    }
                }
            })
            .then_with(|| {
                left.name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase())
            })
    });
    filtered
}

pub fn aggregate_languages(maps: impl IntoIterator<Item = LanguageMap>) -> Vec<LanguageStat> {
    let mut totals = LanguageMap::new();
    for map in maps {
        for (language, bytes) in map {
            *totals.entry(language).or_insert(0) += bytes;
        }
    }

    let total_bytes: u64 = totals.values().sum();
    if total_bytes == 0 {
        return Vec::new();
    }

    let mut statistics: Vec<_> = totals
        .into_iter()
        .map(|(name, bytes)| LanguageStat {
            name,
            bytes,
            percentage: (bytes as f64 * 100.0 / total_bytes as f64 * 10.0).round() / 10.0,
        })
        .collect();
    statistics.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.name.cmp(&right.name))
    });
    statistics
}

fn pin_order(left: &Repository, right: &Repository, pinned: &[String]) -> Ordering {
    let index = |repository: &Repository| {
        pinned
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&repository.name))
            .unwrap_or(usize::MAX)
    };
    index(left).cmp(&index(right))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use chrono::{TimeZone, Utc};

    use super::{aggregate_languages, apply_preferences, filter_and_sort};
    use crate::models::{LanguageMap, Repository, RepositoryQuery, RepositorySort};

    fn repository(name: &str, stars: u64, day: u32) -> Repository {
        Repository {
            name: name.into(),
            description: Some(format!("{name} description")),
            html_url: format!("https://github.com/example/{name}"),
            homepage: None,
            language: Some("Rust".into()),
            stargazers_count: stars,
            forks_count: 0,
            topics: vec!["cli".into()],
            updated_at: Utc
                .with_ymd_and_hms(2026, 1, day, 0, 0, 0)
                .single()
                .expect("valid test date"),
            fork: false,
            archived: false,
            pinned: false,
        }
    }

    #[test]
    fn preferences_hide_and_pin_repositories() {
        let mut fork = repository("fork", 100, 1);
        fork.fork = true;
        let mut archived = repository("archived", 100, 1);
        archived.archived = true;
        let repositories = vec![
            repository("beta", 2, 3),
            fork,
            archived,
            repository("alpha", 1, 1),
        ];

        let result = apply_preferences(
            repositories,
            &["alpha".into()],
            &HashSet::new(),
            false,
            false,
        );
        assert_eq!(
            result
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta"]
        );
        assert!(result[0].pinned);
    }

    #[test]
    fn search_filter_and_sort_work_together() {
        let repositories = vec![repository("Beta", 2, 3), repository("alpha", 10, 1)];
        let query = RepositoryQuery {
            q: Some("ALPHA".into()),
            language: Some("rust".into()),
            topic: Some("CLI".into()),
            sort: RepositorySort::Name,
        };
        let result = filter_and_sort(&repositories, &query);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "alpha");
    }

    #[test]
    fn languages_are_aggregated_and_sorted() {
        let first = LanguageMap::from([("Rust".into(), 75), ("Shell".into(), 25)]);
        let second = LanguageMap::from([("Rust".into(), 25)]);
        let result = aggregate_languages([first, second]);
        assert_eq!(result[0].name, "Rust");
        assert_eq!(result[0].bytes, 100);
        assert_eq!(result[0].percentage, 80.0);
    }
}
