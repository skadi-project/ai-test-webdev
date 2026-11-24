pub mod cache;
pub mod config;
pub mod error;
pub mod github;
pub mod models;
pub mod routes;

#[cfg(test)]
mod tests {
    use super::models::Profile;
    #[test]
    fn profile_fixture_deserializes() {
        let profile: Profile = serde_json::from_str(include_str!("../tests/fixtures/profile.json"))
            .expect("valid fixture");
        assert_eq!(profile.login, "octocat");
    }
}
