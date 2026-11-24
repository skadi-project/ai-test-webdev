//! Server-side filtering and sorting of repositories.
//!
//! Keeping this logic in one place means the JSON API, the SSR fallback and the
//! unit tests all agree on what "visible" and "sorted" mean.

use std::cmp::Ordering;

use serde::Deserialize;

use crate::models::Repo;

/// Sort orders exposed through `?sort=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    /// Stars first (default).
    #[default]
    Stars,
    /// Most recently updated first.
    Updated,
    /// Alphabetical by name.
    Name,
}

impl SortBy {
    pub fn as_str(self) -> &'static str {
        match self {
            SortBy::Stars => "stars",
            SortBy::Updated => "updated",
            SortBy::Name => "name",
        }
    }

    /// Lenient parser: unknown values fall back to the default instead of erroring,
    /// so a stale bookmarked URL never breaks the page.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("updated" | "date" | "recent") => SortBy::Updated,
            Some("name" | "alpha") => SortBy::Name,
            _ => SortBy::Stars,
        }
    }
}

/// Query parameters accepted by `GET /api/repos`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepoQuery {
    /// Free-text search over name and description.
    #[serde(default)]
    pub q: Option<String>,
    /// Exact (case-insensitive) primary language match.
    #[serde(default)]
    pub language: Option<String>,
    /// Exact (case-insensitive) topic match.
    #[serde(default)]
    pub topic: Option<String>,
    /// `stars` | `updated` | `name`.
    #[serde(default)]
    pub sort: Option<String>,
    /// Overrides the configured default.
    #[serde(default)]
    pub show_forks: Option<bool>,
    /// Overrides the configured default.
    #[serde(default)]
    pub show_archived: Option<bool>,
}

/// Effective filter settings after merging query overrides with configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visibility {
    pub show_forks: bool,
    pub show_archived: bool,
}

impl Visibility {
    fn allows(&self, repo: &Repo) -> bool {
        (self.show_forks || !repo.fork) && (self.show_archived || !repo.archived)
    }
}

/// Normalizes a filter query, dropping blanks and applying visibility defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub search: Option<String>,
    pub language: Option<String>,
    pub topic: Option<String>,
    pub sort: SortBy,
    pub visibility: Visibility,
}

impl Filter {
    pub fn from_query(query: &RepoQuery, defaults: Visibility) -> Self {
        let clean = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_lowercase())
        };
        Self {
            search: clean(&query.q),
            language: clean(&query.language),
            topic: clean(&query.topic),
            sort: SortBy::parse(query.sort.as_deref()),
            visibility: Visibility {
                show_forks: query.show_forks.unwrap_or(defaults.show_forks),
                show_archived: query.show_archived.unwrap_or(defaults.show_archived),
            },
        }
    }

    /// True when the repository passes visibility, language, topic and text filters.
    pub fn matches(&self, repo: &Repo) -> bool {
        if !self.visibility.allows(repo) {
            return false;
        }
        if let Some(language) = &self.language {
            match &repo.language {
                Some(actual) if actual.eq_ignore_ascii_case(language) => {}
                _ => return false,
            }
        }
        if let Some(topic) = &self.topic {
            if !repo.topics.iter().any(|t| t.eq_ignore_ascii_case(topic)) {
                return false;
            }
        }
        if let Some(search) = &self.search {
            if !repo.search_blob().contains(search.as_str()) {
                return false;
            }
        }
        true
    }

    /// Applies the filter and returns a new, sorted `Vec`.
    ///
    /// Pinned repositories always come first, in `PINNED_REPOS` declaration order,
    /// regardless of the requested sort.
    pub fn apply(&self, repos: &[Repo], pinned_rank: &[String]) -> Vec<Repo> {
        let mut selected: Vec<Repo> = repos
            .iter()
            .filter(|repo| self.matches(repo))
            .cloned()
            .collect();
        selected.sort_by(|a, b| self.compare(a, b, pinned_rank));
        selected
    }

    fn compare(&self, a: &Repo, b: &Repo, pinned_rank: &[String]) -> Ordering {
        let rank = |repo: &Repo| pin_rank(repo, pinned_rank);
        rank(a).cmp(&rank(b)).then_with(|| match self.sort {
            SortBy::Stars => b
                .stargazers_count
                .cmp(&a.stargazers_count)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortBy::Updated => b
                .updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        })
    }
}

/// Pinned entries get rank `0..n`, everything else `usize::MAX`.
fn pin_rank(repo: &Repo, pinned_rank: &[String]) -> usize {
    if !repo.pinned {
        return usize::MAX;
    }
    pinned_rank
        .iter()
        .position(|name| {
            name.eq_ignore_ascii_case(&repo.name) || name.eq_ignore_ascii_case(&repo.full_name)
        })
        .unwrap_or(usize::MAX - 1)
}

/// Sorted list of unique language names present in `repos` (visibility-filtered).
pub fn available_languages(repos: &[Repo], visibility: Visibility) -> Vec<String> {
    collect_unique(repos, visibility, |repo| repo.language.clone())
}

/// Sorted list of unique topics present in `repos` (visibility-filtered).
pub fn available_topics(repos: &[Repo], visibility: Visibility) -> Vec<String> {
    collect_topics(repos, visibility)
}

fn collect_topics(repos: &[Repo], visibility: Visibility) -> Vec<String> {
    let mut topics: Vec<String> = Vec::new();
    for repo in repos.iter().filter(|repo| visibility.allows(repo)) {
        for topic in &repo.topics {
            if !topics.iter().any(|t| t.eq_ignore_ascii_case(topic)) {
                topics.push(topic.clone());
            }
        }
    }
    topics.sort_by_key(|t| t.to_lowercase());
    topics
}

fn collect_unique<F>(repos: &[Repo], visibility: Visibility, pick: F) -> Vec<String>
where
    F: Fn(&Repo) -> Option<String>,
{
    let mut values: Vec<String> = Vec::new();
    for repo in repos.iter().filter(|repo| visibility.allows(repo)) {
        if let Some(value) = pick(repo) {
            if !values.iter().any(|v| v.eq_ignore_ascii_case(&value)) {
                values.push(value);
            }
        }
    }
    values.sort_by_key(|v| v.to_lowercase());
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str, stars: u64, updated: &str) -> Repo {
        Repo {
            id: stars + 1,
            name: name.to_owned(),
            full_name: format!("octocat/{name}"),
            description: Some(format!("{name} project")),
            html_url: format!("https://github.com/octocat/{name}"),
            homepage: None,
            language: Some("Rust".to_owned()),
            stargazers_count: stars,
            forks_count: 0,
            topics: vec!["cli".to_owned()],
            updated_at: Some(updated.to_owned()),
            fork: false,
            archived: false,
            pinned: false,
        }
    }

    fn sample() -> Vec<Repo> {
        let mut pinned = repo("pinned-one", 1, "2023-01-01T00:00:00Z");
        pinned.pinned = true;
        let mut fork = repo("a-fork", 500, "2024-05-01T00:00:00Z");
        fork.fork = true;
        let mut archived = repo("old-thing", 700, "2020-01-01T00:00:00Z");
        archived.archived = true;
        let mut python = repo("script", 50, "2024-06-01T00:00:00Z");
        python.language = Some("Python".to_owned());
        vec![
            repo("rusty", 100, "2024-01-01T00:00:00Z"),
            pinned,
            fork,
            archived,
            python,
        ]
    }

    fn defaults() -> Visibility {
        Visibility {
            show_forks: false,
            show_archived: false,
        }
    }

    #[test]
    fn sort_by_stars_with_pins_first() {
        let filter = Filter::from_query(&RepoQuery::default(), defaults());
        let names: Vec<String> = filter
            .apply(&sample(), &["pinned-one".to_owned()])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["pinned-one", "rusty", "script"]);
    }

    #[test]
    fn forks_and_archived_hidden_by_default() {
        let filter = Filter::from_query(&RepoQuery::default(), defaults());
        let names: Vec<String> = filter
            .apply(&sample(), &[])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert!(!names.contains(&"a-fork".to_owned()));
        assert!(!names.contains(&"old-thing".to_owned()));
    }

    #[test]
    fn query_can_reveal_forks_and_archived() {
        let filter = Filter::from_query(
            &RepoQuery {
                show_forks: Some(true),
                show_archived: Some(true),
                ..Default::default()
            },
            defaults(),
        );
        let names: Vec<String> = filter
            .apply(&sample(), &[])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert!(names.contains(&"a-fork".to_owned()));
        assert!(names.contains(&"old-thing".to_owned()));
        assert_eq!(names[0], "pinned-one", "pins precede sorting");
        assert_eq!(names[1], "old-thing", "highest unpinned stars");
    }

    #[test]
    fn sort_by_updated_and_name() {
        let filter = Filter::from_query(
            &RepoQuery {
                sort: Some("updated".to_owned()),
                ..Default::default()
            },
            defaults(),
        );
        let names: Vec<String> = filter
            .apply(&sample(), &[])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["pinned-one", "script", "rusty"]);

        let by_name = Filter::from_query(
            &RepoQuery {
                sort: Some("name".to_owned()),
                ..Default::default()
            },
            defaults(),
        );
        let names: Vec<String> = by_name
            .apply(&sample(), &[])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["pinned-one", "rusty", "script"]);
    }

    #[test]
    fn unknown_sort_falls_back_to_stars() {
        assert_eq!(SortBy::parse(Some("nope")), SortBy::Stars);
        assert_eq!(SortBy::parse(None), SortBy::Stars);
        assert_eq!(SortBy::parse(Some(" UPDATED ")), SortBy::Updated);
    }

    #[test]
    fn filters_by_language_topic_and_search() {
        let by_language = Filter::from_query(
            &RepoQuery {
                language: Some("python".to_owned()),
                ..Default::default()
            },
            defaults(),
        );
        let names: Vec<String> = by_language
            .apply(&sample(), &[])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["script"]);

        let by_topic = Filter::from_query(
            &RepoQuery {
                topic: Some("CLI".to_owned()),
                ..Default::default()
            },
            defaults(),
        );
        assert_eq!(by_topic.apply(&sample(), &[]).len(), 3);

        let by_search = Filter::from_query(
            &RepoQuery {
                q: Some("  SCRIPT project ".to_owned()),
                ..Default::default()
            },
            defaults(),
        );
        assert_eq!(by_search.search.as_deref(), Some("script project"));
        let names: Vec<String> = by_search
            .apply(&sample(), &[])
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, vec!["script"]);
    }

    #[test]
    fn empty_query_does_not_filter() {
        let filter = Filter::from_query(
            &RepoQuery {
                q: Some("   ".to_owned()),
                language: Some(String::new()),
                topic: None,
                ..Default::default()
            },
            defaults(),
        );
        assert!(filter.search.is_none());
        assert!(filter.language.is_none());
        assert_eq!(filter.apply(&sample(), &[]).len(), 3);
    }

    #[test]
    fn facets_are_sorted_and_unique() {
        let languages = available_languages(&sample(), defaults());
        assert_eq!(languages, vec!["Python".to_owned(), "Rust".to_owned()]);
        let topics = available_topics(&sample(), defaults());
        assert_eq!(topics, vec!["cli".to_owned()]);
        // Forks/archived are excluded from facets too.
        let all = Visibility {
            show_forks: true,
            show_archived: true,
        };
        assert_eq!(available_languages(&sample(), all).len(), 2);
    }
}
