//! Downstream-style contract tests for the supported Rust embedding API.

use std::collections::HashSet;

use redisctl_mcp::{CredentialSource, McpServerBuilder, PolicyConfig, SafetyTier, ToolsetPolicy};
use serde_json::json;
use tower_mcp::TestClient;

async fn app_client(policy: PolicyConfig) -> TestClient {
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
    client
}

async fn assert_profile_create_reaches_handler(policy: PolicyConfig) {
    let mut client = app_client(policy).await;
    let names = client
        .list_tools()
        .await
        .into_iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect::<HashSet<_>>();
    assert!(names.contains("profile_create"));

    let error = client
        .call_tool_expect_error(
            "profile_create",
            json!({
                "name": "__redisctl_policy_guard_test__",
                "profile_type": "invalid-for-policy-test"
            }),
        )
        .await
        .to_string();
    assert!(error.contains("Invalid profile type"), "{error}");
    assert!(
        !error.contains("not allowed by the active policy"),
        "{error}"
    );
}

async fn assert_tool_is_rejected(policy: PolicyConfig, tool_name: &str) {
    let mut client = app_client(policy).await;
    let names = client
        .list_tools()
        .await
        .into_iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect::<HashSet<_>>();
    assert!(!names.contains(tool_name));

    let error = client
        .call_tool_expect_error(tool_name, json!({}))
        .await
        .to_string();
    assert!(
        error.to_ascii_lowercase().contains("not found")
            || error.to_ascii_lowercase().contains("unknown")
            || error.to_ascii_lowercase().contains("unauthorized"),
        "{error}"
    );
}

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

#[tokio::test]
async fn explicit_allow_is_honored_during_handler_invocation() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::ReadOnly;
    policy.allow = vec!["profile_create".to_string()];
    assert_profile_create_reaches_handler(policy).await;
}

#[tokio::test]
async fn toolset_tier_and_allow_are_honored_during_invocation() {
    let mut app = ToolsetPolicy::default();
    app.tier = Some(SafetyTier::ReadWrite);
    let mut tier_override = PolicyConfig::default();
    tier_override.tier = SafetyTier::ReadOnly;
    tier_override.app = Some(app);
    assert_profile_create_reaches_handler(tier_override).await;

    let mut app = ToolsetPolicy::default();
    app.allow = vec!["profile_create".to_string()];
    let mut toolset_allow = PolicyConfig::default();
    toolset_allow.tier = SafetyTier::ReadOnly;
    toolset_allow.app = Some(app);
    assert_profile_create_reaches_handler(toolset_allow).await;
}

#[tokio::test]
async fn restrictive_toolset_tier_and_denies_fail_closed_end_to_end() {
    let mut app = ToolsetPolicy::default();
    app.tier = Some(SafetyTier::ReadOnly);
    let mut restrictive_toolset = PolicyConfig::default();
    restrictive_toolset.tier = SafetyTier::Full;
    restrictive_toolset.app = Some(app);
    assert_tool_is_rejected(restrictive_toolset, "profile_create").await;

    let mut global_deny = PolicyConfig::default();
    global_deny.tier = SafetyTier::Full;
    global_deny.deny = vec!["profile_validate".to_string()];
    assert_tool_is_rejected(global_deny, "profile_validate").await;

    let mut app = ToolsetPolicy::default();
    app.deny = vec!["profile_validate".to_string()];
    let mut toolset_deny = PolicyConfig::default();
    toolset_deny.tier = SafetyTier::Full;
    toolset_deny.app = Some(app);
    assert_tool_is_rejected(toolset_deny, "profile_validate").await;
}

#[tokio::test]
async fn unknown_tool_invocation_fails_closed() {
    assert_tool_is_rejected(PolicyConfig::default(), "not_a_registered_redisctl_tool").await;
}
