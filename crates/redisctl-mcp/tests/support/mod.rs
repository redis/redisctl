#![cfg(feature = "enterprise")]

use std::sync::Arc;

use redis_enterprise::EnterpriseClient;
use redisctl_mcp::state::AppState;

/// Returns true when the local Enterprise demo cluster is reachable.
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
pub fn enterprise_state() -> Arc<AppState> {
    Arc::new(AppState::with_enterprise_client(enterprise_client()))
}
