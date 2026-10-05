//! Skills must be discoverable through the public embedding API without files beside the binary.

use std::collections::HashSet;

use redisctl_mcp::{CredentialSource, McpServerBuilder, PolicyConfig, SafetyTier};
use tower_mcp::TestClient;

async fn client(policy: PolicyConfig, tools: &[&str]) -> TestClient {
    let server =
        McpServerBuilder::new(CredentialSource::Profiles(Vec::new()), policy, "skill-test")
            .with_tool_specs(tools)
            .unwrap()
            .build()
            .unwrap();
    let mut client = TestClient::from_router(server.into_router());
    client.initialize().await;
    client
}

#[tokio::test]
async fn bundled_resource_index_matches_prompts_and_readable_markdown() {
    let mut client = client(PolicyConfig::default(), &["app"]).await;
    let listed = client.list_resources().await;
    let uris = listed
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect::<HashSet<_>>();
    assert!(uris.contains("redisctl://skills"));
    let result = client.read_resource("redisctl://skills").await;
    let index: serde_json::Value =
        serde_json::from_str(result.contents[0].text.as_ref().unwrap()).unwrap();
    assert_eq!(index["schema_version"], 1);
    let skills = index["skills"].as_array().unwrap();
    assert_eq!(skills.len(), 11);
    let prompts = client.list_prompts().await;
    for skill in skills {
        let name = skill["name"].as_str().unwrap();
        let uri = skill["uri"].as_str().unwrap();
        assert!(uris.contains(uri));
        assert!(prompts.iter().any(|p| p["name"] == name));
        let content = client.read_resource(uri).await;
        let markdown = content.contents[0].text.as_ref().unwrap();
        assert!(markdown.starts_with("---\n"));
        assert!(markdown.contains(&format!("name: {name}")));
        let prompt = client.get_prompt(name, Default::default()).await;
        let prompt_json = serde_json::to_string(&prompt).unwrap();
        assert!(prompt_json.contains("redisctl://skills"));
    }
}

#[tokio::test]
async fn index_reports_only_tools_available_under_policy_and_visibility() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::ReadOnly;
    policy.deny = vec!["profile_show".to_string(), "show_policy".to_string()];
    policy.tools.exclude = vec!["profile_path".to_string()];
    let mut client = client(policy, &["app"]).await;
    let actual = client
        .list_tools()
        .await
        .into_iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    let result = client.read_resource("redisctl://skills").await;
    let index: serde_json::Value =
        serde_json::from_str(result.contents[0].text.as_ref().unwrap()).unwrap();
    let reported = index["available_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(actual, reported);
    assert!(reported.contains("profile_validate"));
    assert!(!reported.contains("profile_create"));
    assert!(!reported.contains("profile_show"));
    assert!(!reported.contains("profile_path"));
    assert!(!reported.contains("show_policy"));
}

#[tokio::test]
async fn skills_survive_disabled_app_toolset() {
    let mut policy = PolicyConfig::default();
    let mut app_policy = redisctl_mcp::ToolsetPolicy::default();
    app_policy.enabled = Some(false);
    policy.app = Some(app_policy);
    let mut client = client(policy, &["app"]).await;
    let result = client
        .read_resource("redisctl://skills/redisctl-setup")
        .await;
    assert!(
        result.contents[0]
            .text
            .as_ref()
            .unwrap()
            .contains("redisctl profile init")
    );
    assert!(
        client
            .list_prompts()
            .await
            .iter()
            .any(|p| p["name"] == "redisctl-setup")
    );
    assert!(
        !client
            .list_tools()
            .await
            .iter()
            .any(|t| t["name"] == "profile_create")
    );
}

#[tokio::test]
async fn custom_skill_resource_and_prompt_use_same_override() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("custom");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(
        path.join("SKILL.md"),
        "---\nname: data-explorer\ndescription: Replacement\n---\nOnly this custom workflow.",
    )
    .unwrap();
    let server = McpServerBuilder::new(
        CredentialSource::Profiles(Vec::new()),
        PolicyConfig::default(),
        "test",
    )
    .with_tool_specs(["app"])
    .unwrap()
    .with_skills_dir(Some(directory.path().to_path_buf()))
    .build()
    .unwrap();
    let mut client = TestClient::from_router(server.into_router());
    client.initialize().await;
    let resource = client
        .read_resource("redisctl://skills/data-explorer")
        .await;
    assert!(
        resource.contents[0]
            .text
            .as_ref()
            .unwrap()
            .contains("Only this custom workflow")
    );
    let prompt = client.get_prompt("data-explorer", Default::default()).await;
    assert!(
        serde_json::to_string(&prompt)
            .unwrap()
            .contains("Only this custom workflow")
    );
    let index = client.read_resource("redisctl://skills").await;
    let index: serde_json::Value =
        serde_json::from_str(index.contents[0].text.as_ref().unwrap()).unwrap();
    let custom = index["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "data-explorer")
        .unwrap();
    assert_eq!(custom["source"], "custom");
}

#[tokio::test]
async fn initialization_is_bounded_without_catalog_bodies_or_operator_config() {
    let mut policy = PolicyConfig::default();
    policy.deny = (0..1000)
        .map(|i| format!("operator-config-marker-{i}"))
        .collect();
    let server = McpServerBuilder::new(
        CredentialSource::Profiles(Vec::new()),
        policy,
        "operator-source-marker",
    )
    .build()
    .unwrap();
    let mut client = TestClient::from_router(server.into_router());
    let init = client.initialize().await;
    let instructions = init["instructions"].as_str().unwrap();
    assert!(instructions.len() < 2048, "{} bytes", instructions.len());
    assert!(instructions.contains("redisctl://skills"));
    assert!(instructions.contains("redisctl://skills/redisctl-setup"));
    assert!(instructions.contains("read-only"));
    assert!(instructions.contains("show_policy"));
    assert!(!instructions.contains("operator-config-marker"));
    assert!(!instructions.contains("operator-source-marker"));
    assert!(!instructions.contains("profile_validate"));
    assert!(!instructions.contains("index-advisor"));
    assert!(!instructions.contains("## 1. Inspect"));
    // All normal discovery still works; only initialization prose is reduced.
    assert!(!client.list_tools().await.is_empty());
}

#[tokio::test]
async fn canonical_resources_preserve_legacy_addresses_and_content_uris() {
    let mut client = client(PolicyConfig::default(), &["app"]).await;
    let listed = client.list_resources().await;
    for path in ["config/path", "profiles", "help"] {
        let canonical = format!("redisctl://{path}");
        let legacy = format!("redis://{path}");
        assert!(listed.iter().any(|r| r["uri"] == canonical));
        assert!(listed.iter().any(|r| r["uri"] == legacy));
        let current = client.read_resource(&canonical).await;
        let old = client.read_resource(&legacy).await;
        assert_eq!(current.contents[0].uri, canonical);
        assert_eq!(old.contents[0].uri, legacy);
        assert_eq!(current.contents[0].text, old.contents[0].text);
        assert_eq!(current.contents[0].mime_type, old.contents[0].mime_type);
    }
}

#[tokio::test]
async fn empty_profile_config_can_discover_setup_without_writes() {
    let server = McpServerBuilder::new(
        CredentialSource::Profiles(Vec::new()),
        PolicyConfig::default(),
        "bootstrap",
    )
    .with_profile_toolsets(&redisctl_core::Config::default())
    .build()
    .unwrap();
    let mut client = TestClient::from_router(server.into_router());
    client.initialize().await;
    let setup = client
        .read_resource("redisctl://skills/redisctl-setup")
        .await;
    let setup = setup.contents[0].text.as_ref().unwrap();
    assert!(setup.contains("trusted credential UI"));
    assert!(setup.contains("Do not call `profile_create` with raw credentials"));
    assert!(setup.contains("restart the MCP connection"));
    let tools = client.list_tools().await;
    assert!(tools.iter().any(|t| t["name"] == "profile_validate"));
    assert!(!tools.iter().any(|t| t["name"] == "profile_create"));
    assert!(!tools.iter().any(|t| t["name"] == "profile_delete"));
    let error = client
        .call_tool_expect_error("profile_create", serde_json::json!({}))
        .await;
    let error = error.to_string().to_ascii_lowercase();
    assert!(error.contains("unauthorized") || error.contains("not found"));
}

#[test]
fn readme_profile_example_uses_current_config_fields_and_verified_tls() {
    let readme = include_str!("../README.md");
    let example = readme
        .split("```toml\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let config: redisctl_core::Config = toml::from_str(example).unwrap();
    assert_eq!(config.default_cloud.as_deref(), Some("cloud-prod"));
    assert_eq!(config.default_enterprise.as_deref(), Some("enterprise-dev"));
    assert!(matches!(
        config.profiles["enterprise-dev"].credentials,
        redisctl_core::ProfileCredentials::Enterprise {
            insecure: false,
            ..
        }
    ));
}

#[tokio::test]
async fn skill_index_honors_toolset_overrides_and_explicit_allows() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::Full;
    let mut app = redisctl_mcp::ToolsetPolicy::default();
    app.tier = Some(SafetyTier::ReadOnly);
    app.allow = vec!["profile_create".to_string()];
    app.deny = vec!["profile_list".to_string()];
    policy.app = Some(app);
    let mut client = client(policy, &["app"]).await;
    let actual = client
        .list_tools()
        .await
        .into_iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    let result = client.read_resource("redisctl://skills").await;
    let index: serde_json::Value =
        serde_json::from_str(result.contents[0].text.as_ref().unwrap()).unwrap();
    let reported = index["available_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(actual, reported);
    assert!(reported.contains("profile_create"));
    assert!(!reported.contains("profile_list"));
    assert!(!reported.contains("profile_delete"));
}
