use std::collections::HashSet;

use github_portfolio::{
    models::{LanguageMap, Profile, Repository, RepositoryQuery, RepositorySort},
    repository::{aggregate_languages, apply_preferences, filter_and_sort},
};

fn repositories() -> Vec<Repository> {
    serde_json::from_str(include_str!("fixtures/repositories.json"))
        .expect("repository fixture must be valid")
}

fn query(sort: RepositorySort) -> RepositoryQuery {
    RepositoryQuery {
        q: None,
        language: None,
        topic: None,
        sort,
    }
}

#[test]
fn parses_github_profile_and_normalizes_empty_blog() {
    let profile: Profile = serde_json::from_str(include_str!("fixtures/profile.json"))
        .expect("profile fixture must be valid");
    assert_eq!(profile.login, "octocat");
    assert_eq!(profile.followers, 1200);
    assert_eq!(profile.blog, None);
}

#[test]
fn parses_repositories_and_defaults_missing_topics() {
    let repositories = repositories();
    assert_eq!(repositories.len(), 5);
    assert_eq!(repositories[0].name, "atlas");
    assert_eq!(
        repositories[0].updated_at.to_rfc3339(),
        "2026-11-20T12:30:00+00:00"
    );
    assert_eq!(repositories[1].homepage, None);
    assert!(repositories[3].topics.is_empty());
    assert!(repositories[3].fork);
}

#[test]
fn preferences_exclude_pin_and_hide_forks_and_archived() {
    let visible = apply_preferences(
        repositories(),
        &["atlas".into(), "beacon".into()],
        &HashSet::from(["secret-lab".into()]),
        false,
        false,
    );
    let names: Vec<_> = visible.iter().map(|repo| repo.name.as_str()).collect();
    assert_eq!(names, ["atlas", "beacon"]);
    assert!(visible.iter().all(|repo| repo.pinned));

    let sorted = filter_and_sort(&visible, &query(RepositorySort::Stars));
    let sorted_names: Vec<_> = sorted.iter().map(|repo| repo.name.as_str()).collect();
    assert_eq!(sorted_names, ["atlas", "beacon"]);
}

#[test]
fn filtering_is_case_insensitive_and_sorting_is_deterministic() {
    let visible = apply_preferences(
        repositories(),
        &[],
        &HashSet::from(["secret-lab".into()]),
        false,
        false,
    );

    let mut stars_query = query(RepositorySort::Stars);
    stars_query.language = Some("rust".into());
    stars_query.topic = Some("CLI".into());
    stars_query.q = Some("CARTOGRAPHY".into());
    let rust = filter_and_sort(&visible, &stars_query);
    assert_eq!(rust.len(), 1);
    assert_eq!(rust[0].name, "atlas");

    let by_name = filter_and_sort(&visible, &query(RepositorySort::Name));
    let names: Vec<_> = by_name.iter().map(|repo| repo.name.as_str()).collect();
    assert_eq!(names, ["atlas", "beacon"]);

    let by_update = filter_and_sort(&visible, &query(RepositorySort::Updated));
    assert_eq!(by_update[0].name, "beacon");
}

#[test]
fn parses_and_aggregates_language_payloads() {
    let languages: LanguageMap = serde_json::from_str(include_str!("fixtures/languages.json"))
        .expect("languages fixture must be valid");
    let other = LanguageMap::from([("Rust".into(), 25_000), ("TypeScript".into(), 50_000)]);
    let stats = aggregate_languages([languages, other]);

    assert_eq!(stats[0].name, "Rust");
    assert_eq!(stats[0].bytes, 150_000);
    assert_eq!(stats[1].name, "TypeScript");
    assert!(stats
        .iter()
        .any(|item| item.name == "Dockerfile" && item.bytes == 1200));
}
