//! Downstream-style contract tests for the supported Rust embedding API.

use std::collections::HashSet;

use redisctl_mcp::{CredentialSource, McpServerBuilder, PolicyConfig, SafetyTier};
use tower_mcp::TestClient;

#[tokio::test]
async fn public_builder_installs_policy_filtering() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::ReadOnly;
    let server = McpServerBuilder::new(
        CredentialSource::Profiles(Vec::new()),
        policy,
        "embedding-api-test",
    )
    .with_tool_specs(["app"])
    .expect("app should be a valid toolset")
    .build()
    .expect("public builder should construct a router");

    let mut client = TestClient::from_router(server.into_router());
    client.initialize().await;
    let names = client
        .list_tools()
        .await
        .into_iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect::<HashSet<_>>();

    assert!(names.contains("profile_list"));
    assert!(names.contains("show_policy"));
    assert!(names.contains("list_available_tools"));
    assert!(!names.contains("profile_create"));
    assert!(!names.contains("profile_delete"));
}
