use crate::{config::Config, models::Repo};

/// How the repository list can be sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Stars,
    Updated,
    Name,
}

impl SortKey {
    /// Parse the `sort` query parameter (default: `updated`).
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::to_ascii_lowercase).as_deref() {
            Some("stars") => Self::Stars,
            Some("name") => Self::Name,
            _ => Self::Updated,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stars => "stars",
            Self::Updated => "updated",
            Self::Name => "name",
        }
    }
}

/// View rules: hide excluded/forked/archived repos, then pin the configured
/// repositories first (in `PINNED_REPOS` order).
pub fn view_repos(all: &[Repo], cfg: &Config) -> Vec<Repo> {
    let mut kept: Vec<Repo> = all
        .iter()
        .filter(|r| !cfg.excluded.iter().any(|e| e.eq_ignore_ascii_case(&r.name)))
        .filter(|r| cfg.show_forks || !r.fork)
        .filter(|r| cfg.show_archived || !r.archived)
        .cloned()
        .collect();

    let mut result: Vec<Repo> = Vec::with_capacity(kept.len());
    for pinned in &cfg.pinned {
        if let Some(index) = kept
            .iter()
            .position(|r| r.name.eq_ignore_ascii_case(pinned))
        {
            let mut repo = kept.remove(index);
            repo.pinned = true;
            result.push(repo);
        }
    }
    result.append(&mut kept);
    result
}

/// Client-side filters: free-text search over name/description/topics plus
/// exact (case-insensitive) language and topic match.
pub fn search_filter(
    repos: Vec<Repo>,
    query: Option<&str>,
    language: Option<&str>,
    topic: Option<&str>,
) -> Vec<Repo> {
    let query = query
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_ascii_lowercase);
    let language = language.map(str::trim).filter(|l| !l.is_empty());
    let topic = topic.map(str::trim).filter(|t| !t.is_empty());

    repos
        .into_iter()
        .filter(|repo| {
            if let Some(lang) = &language {
                let matches = repo
                    .language
                    .as_deref()
                    .is_some_and(|l| l.eq_ignore_ascii_case(lang));
                if !matches {
                    return false;
                }
            }
            if let Some(topic) = &topic {
                let matches = repo.topics.iter().any(|t| t.eq_ignore_ascii_case(topic));
                if !matches {
                    return false;
                }
            }
            if let Some(query) = &query {
                let in_name = repo.name.to_ascii_lowercase().contains(query);
                let in_desc = repo
                    .description
                    .as_deref()
                    .is_some_and(|d| d.to_ascii_lowercase().contains(query));
                let in_topics = repo
                    .topics
                    .iter()
                    .any(|t| t.to_ascii_lowercase().contains(query));
                if !(in_name || in_desc || in_topics) {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Sort in place; `updated` compares ISO-8601 strings (lexicographic == chronological).
pub fn sort_repos(repos: &mut [Repo], key: SortKey, ascending: bool) {
    match key {
        SortKey::Stars => {
            repos.sort_by_key(|r| std::cmp::Reverse(r.stargazers_count));
            if ascending {
                repos.reverse();
            }
        }
        SortKey::Updated => {
            repos.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
            if !ascending {
                repos.reverse();
            }
        }
        SortKey::Name => {
            repos.sort_by_key(|r| r.name.to_ascii_lowercase());
            if !ascending {
                repos.reverse();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPOS_JSON: &str = include_str!("../tests/fixtures/repos.json");

    fn fixtures() -> Vec<Repo> {
        serde_json::from_str(REPOS_JSON).expect("valid repos json")
    }

    #[test]
    fn view_rules_hide_forks_archived_and_excluded() {
        let cfg = Config::test(&[], &["legacy"], false, false);
        let viewed = view_repos(&fixtures(), &cfg);
        let names: Vec<&str> = viewed.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["hello-world"]);
    }

    #[test]
    fn view_rules_respect_flags() {
        let cfg = Config::test(&[], &[], true, true);
        let viewed = view_repos(&fixtures(), &cfg);
        assert_eq!(viewed.len(), 3);
    }

    #[test]
    fn pinned_repos_come_first_in_configured_order() {
        let cfg = Config::test(&["spoon-knife", "hello-world"], &[], true, true);
        let viewed = view_repos(&fixtures(), &cfg);
        let names: Vec<&str> = viewed.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["spoon-knife", "hello-world", "legacy"]);
    }

    #[test]
    fn pinned_missing_repo_is_ignored() {
        let cfg = Config::test(&["does-not-exist"], &[], false, false);
        let viewed = view_repos(&fixtures(), &cfg);
        assert_eq!(viewed.len(), 1);
    }

    #[test]
    fn search_matches_name_description_and_topics() {
        let cfg = Config::test(&[], &[], true, true);
        let viewed = view_repos(&fixtures(), &cfg);

        let by_topic = search_filter(viewed.clone(), Some("axum"), None, None);
        assert_eq!(by_topic.len(), 1);
        assert_eq!(by_topic[0].name, "hello-world");

        let by_desc = search_filter(viewed.clone(), Some("EXPERIMENT"), None, None);
        assert_eq!(by_desc.len(), 1);
        assert_eq!(by_desc[0].name, "legacy");

        let by_name = search_filter(viewed.clone(), Some("spoon"), None, None);
        assert_eq!(by_name.len(), 1);

        let none = search_filter(viewed.clone(), Some("zzz-no-match"), None, None);
        assert!(none.is_empty());
    }

    #[test]
    fn search_filters_by_language_and_topic() {
        let cfg = Config::test(&[], &[], true, true);
        let viewed = view_repos(&fixtures(), &cfg);

        let rust = search_filter(viewed.clone(), None, Some("rust"), None);
        assert_eq!(rust.len(), 1);

        let go = search_filter(viewed.clone(), None, Some("go"), Some("tools"));
        assert_eq!(go.len(), 1);
        assert_eq!(go[0].name, "spoon-knife");

        let empty = search_filter(viewed, None, Some("cobol"), None);
        assert!(empty.is_empty());
    }

    #[test]
    fn sorting_works_for_every_key() {
        let cfg = Config::test(&[], &[], true, true);
        let mut viewed = view_repos(&fixtures(), &cfg);

        sort_repos(&mut viewed, SortKey::Stars, false);
        assert_eq!(viewed[0].name, "hello-world");

        sort_repos(&mut viewed, SortKey::Stars, true);
        assert_eq!(viewed[0].name, "legacy");

        sort_repos(&mut viewed, SortKey::Updated, false);
        assert_eq!(viewed[0].name, "hello-world");

        sort_repos(&mut viewed, SortKey::Name, true);
        assert_eq!(viewed[0].name, "hello-world");
        assert_eq!(viewed[1].name, "legacy");
        assert_eq!(viewed[2].name, "spoon-knife");
    }

    #[test]
    fn sort_key_parses_and_defaults() {
        assert_eq!(SortKey::parse(None), SortKey::Updated);
        assert_eq!(SortKey::parse(Some("stars")), SortKey::Stars);
        assert_eq!(SortKey::parse(Some("NAME")), SortKey::Name);
        assert_eq!(SortKey::parse(Some("bogus")), SortKey::Updated);
    }
}
