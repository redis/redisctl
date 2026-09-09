#![allow(dead_code)]

use std::sync::Arc;

#[cfg(feature = "enterprise")]
use redis_enterprise::EnterpriseClient;
use redisctl_mcp::AppState;
#[cfg(feature = "database")]
use redisctl_mcp::CredentialSource;

/// Wrap an AppState with an explicit read-only policy.
pub fn state_readonly(mut state: AppState) -> Arc<AppState> {
    state.set_test_policy(AppState::test_policy());
    Arc::new(state)
}

/// Wrap an AppState with an explicit read-write policy.
pub fn state_write(mut state: AppState) -> Arc<AppState> {
    state.set_test_policy(AppState::test_write_policy());
    Arc::new(state)
}

/// Wrap an AppState with an explicit full policy.
pub fn state_full(mut state: AppState) -> Arc<AppState> {
    state.set_test_policy(AppState::test_full_policy());
    Arc::new(state)
}

#[cfg(feature = "database")]
fn database_state(url: String) -> AppState {
    AppState::new(
        CredentialSource::Profiles(vec![]),
        AppState::test_policy(),
        Some(url),
        false,
        None,
    )
    .expect("database test state should be valid")
}

/// Build a direct-Redis test state with read-only access.
#[cfg(feature = "database")]
pub fn database_state_readonly(url: String) -> Arc<AppState> {
    state_readonly(database_state(url))
}

/// Build a direct-Redis test state with read-write access.
#[cfg(feature = "database")]
pub fn database_state_write(url: String) -> Arc<AppState> {
    state_write(database_state(url))
}

/// Build a direct-Redis test state with full access.
#[cfg(feature = "database")]
pub fn database_state_full(url: String) -> Arc<AppState> {
    state_full(database_state(url))
}

/// Returns true when the local Enterprise demo cluster is reachable.
#[cfg(feature = "enterprise")]
pub fn docker_available() -> bool {
    std::process::Command::new("curl")
        .args([
            "-k",
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-u",
            "admin@redis.local:Redis123!",
            "https://localhost:9443/v1/cluster",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "200")
        .unwrap_or(false)
}

/// Build an EnterpriseClient from the standard Docker demo env vars.
#[cfg(feature = "enterprise")]
pub fn enterprise_client() -> EnterpriseClient {
    EnterpriseClient::builder()
        .base_url("https://localhost:9443")
        .username("admin@redis.local")
        .password("Redis123!")
        .insecure(true)
        .build()
        .expect("Failed to build Enterprise client")
}

/// Build an AppState pre-wired with the Docker demo Enterprise client.
#[cfg(feature = "enterprise")]
pub fn enterprise_state() -> Arc<AppState> {
    state_readonly(AppState::with_enterprise_client(enterprise_client()))
}

/// Build a full-tier AppState for bounded Enterprise Docker write tests.
#[cfg(feature = "enterprise")]
pub fn enterprise_state_full() -> Arc<AppState> {
    state_full(AppState::with_enterprise_client(enterprise_client()))
}
