//! Hermetic tests for `redisctl init --cloud`: a wiremock CAPI plays Redis Cloud,
//! and a minimal in-test RESP responder plays the provisioned database so the
//! validation step exercises a real socket. No Docker, no network.

mod init_common;

use assert_cmd::Command;
use init_common::{fake_redis, skills_fixture};
use predicates::prelude::*;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MOCK_PASSWORD: &str = "s3cr3t-mock-pw";

fn write_cloud_profile(dir: &TempDir, api_url: &str) {
    let config = format!(
        r#"
[profiles.test]
deployment_type = "cloud"
api_key = "test-api-key"
api_secret = "test-api-secret"
api_url = "{api_url}"

default_cloud = "test"
"#
    );
    std::fs::write(dir.path().join("config.toml"), config).unwrap();
}

/// Mount the account inventory: one free Essentials sub (1) holding
/// `essentials-db` (9), one Flexible sub (2) holding `flexible-db` (42).
async fn mount_both_tiers(server: &MockServer, endpoint: &str) {
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [
                { "id": 1, "name": "free", "price": 0, "maximumDatabases": 1 }
            ]
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/1/databases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": {
                "subscriptionId": 1,
                "databases": [
                    { "databaseId": 9, "name": "essentials-db", "publicEndpoint": endpoint }
                ]
            }
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/1/databases/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": 9, "name": "essentials-db", "publicEndpoint": endpoint,
            "security": { "enableTls": false, "password": MOCK_PASSWORD }
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": 2, "name": "paid" } ]
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/subscriptions/2/databases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": [ {
                "subscriptionId": 2,
                "databases": [
                    { "databaseId": 42, "name": "flexible-db", "publicEndpoint": endpoint }
                ]
            } ]
        })))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/subscriptions/2/databases/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": 42, "name": "flexible-db", "publicEndpoint": endpoint,
            "security": { "enableTls": false, "password": MOCK_PASSWORD }
        })))
        .mount(server)
        .await;
}

/// `redisctl --config-file <cfg> init --cloud <extra..>` in `dir`, offline-safe.
fn run_init_cloud(cfg: &TempDir, dir: &std::path::Path, repo: &TempDir, extra: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    for var in [
        "REDIS_CLOUD_API_KEY",
        "REDIS_CLOUD_SECRET_KEY",
        "REDIS_CLOUD_API_URL",
        "REDISCTL_PROFILE",
    ] {
        cmd.env_remove(var);
    }
    cmd.current_dir(dir)
        // Explicitly off: an ambient key must not make test runs send telemetry.
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .arg("--config-file")
        .arg(cfg.path().join("config.toml"))
        .args(["init", "--cloud", "--no-install-cli"])
        .args(extra);
    cmd
}

#[test]
fn cloud_and_url_are_mutually_exclusive() {
    Command::cargo_bin("redisctl")
        .unwrap()
        .args(["init", "--cloud", "--url", "redis://h:1"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("--url"));
}

#[test]
fn cloud_subscription_requires_cloud() {
    Command::cargo_bin("redisctl")
        .unwrap()
        .args(["init", "--cloud-subscription", "7"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicates::str::contains("--cloud"));
}

#[tokio::test(flavor = "multi_thread")]
async fn reuse_by_name_connects_validates_and_creates_nothing() {
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    let endpoint = format!("127.0.0.1:{}", fake_redis());
    mount_both_tiers(&server, &endpoint).await;

    // A PATH shim provides redisctl-mcp, so the control-plane entry is written.
    let shim = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = shim.path().join("redisctl-mcp");
        std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path_env = format!(
        "{}:{}",
        shim.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = run_init_cloud(
        &cfg,
        project.path(),
        &repo,
        &["--name", "essentials-db", "--defaults", "--agent", "claude"],
    )
    .env("PATH", &path_env)
    .assert()
    .success()
    .stdout(predicates::str::contains(
        "database 9 in Essentials subscription 1",
    ))
    .stdout(predicates::str::contains("✓ PING"))
    .stdout(predicates::str::contains(MOCK_PASSWORD).not())
    .get_output()
    .clone();
    assert!(String::from_utf8_lossy(&output.stdout).contains("Redis Cloud (existing database)"));

    let env = std::fs::read_to_string(project.path().join(".env")).unwrap();
    assert!(
        env.contains(&format!("redis://default:{MOCK_PASSWORD}@{endpoint}")),
        "{env}"
    );

    let skill = std::fs::read_to_string(
        project
            .path()
            .join(".claude/skills/redis-project-setup/SKILL.md"),
    )
    .or_else(|_| {
        std::fs::read_to_string(
            project
                .path()
                .join(".agents/skills/redis-project-setup/SKILL.md"),
        )
    })
    .unwrap();
    assert!(skill.contains("Essentials subscription `1`"), "{skill}");
    assert!(skill.contains("database `9`"), "{skill}");
    assert!(
        skill.contains("redisctl api cloud get /fixed/subscriptions/1/databases/9"),
        "{skill}"
    );
    assert!(!skill.contains(MOCK_PASSWORD), "{skill}");

    // The shim (and so the control-plane entry) exists on unix only.
    #[cfg(unix)]
    {
        let mcp: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(project.path().join(".mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(mcp["mcpServers"]["redisctl"]["command"], "redisctl-mcp");
        assert!(mcp["mcpServers"]["redis"].is_object());
        let raw = std::fs::read_to_string(project.path().join(".mcp.json")).unwrap();
        assert!(
            !raw.contains("api-key") && !raw.contains("api_secret"),
            "{raw}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn flexible_reuse_reads_the_pro_path_into_the_skill() {
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    let endpoint = format!("127.0.0.1:{}", fake_redis());
    mount_both_tiers(&server, &endpoint).await;

    run_init_cloud(
        &cfg,
        project.path(),
        &repo,
        &["--name", "flexible-db", "--defaults", "--agent", "claude"],
    )
    .assert()
    .success()
    .stdout(predicates::str::contains(
        "database 42 in Flexible subscription 2",
    ));

    let skill = std::fs::read_to_string(
        project
            .path()
            .join(".claude/skills/redis-project-setup/SKILL.md"),
    )
    .unwrap();
    assert!(
        skill.contains("redisctl api cloud get /subscriptions/2/databases/42"),
        "{skill}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn exhausted_free_tier_fails_with_the_ways_out_writing_nothing() {
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": 3, "name": "free", "price": 0, "maximumDatabases": 1 } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/3/databases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": { "subscriptionId": 3, "databases": [
                { "databaseId": 5, "name": "someone-elses", "publicEndpoint": "h:1" }
            ] }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .mount(&server)
        .await;

    run_init_cloud(&cfg, project.path(), &repo, &["--name", "second-project"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--cloud --name someone-elses"))
        .stderr(predicates::str::contains("--cloud-subscription"));
    assert!(!project.path().join(".env").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn piped_stdin_without_a_name_lists_both_tiers_and_refuses() {
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    mount_both_tiers(&server, "h:1").await;

    run_init_cloud(&cfg, project.path(), &repo, &[])
        .assert()
        .failure()
        .stderr(predicates::str::contains("essentials-db"))
        .stderr(predicates::str::contains("Flexible subscription 2"))
        .stderr(predicates::str::contains("--cloud --name <its name>"));
    assert!(!project.path().join(".env").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_reports_the_choice_it_would_offer() {
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    mount_both_tiers(&server, "h:1").await;

    // Telemetry pointed at a second mock: a dry run must report the planned
    // database_source vocabulary, not the applied one.
    let amplitude = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/2/httpapi"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&amplitude)
        .await;
    run_init_cloud(&cfg, project.path(), &repo, &["--dry-run"])
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "test-key")
        .env(
            "REDISCTL_INIT_AMPLITUDE_URL",
            format!("{}/2/httpapi", amplitude.uri()),
        )
        .env("HOME", cfg.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "would offer \"essentials-db\", \"flexible-db\" or a new free database",
        ));
    assert!(!project.path().join(".env").exists());
    let requests = amplitude.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let props =
        requests[0].body_json::<serde_json::Value>().unwrap()["events"][0]["event_properties"]
            .clone();
    assert_eq!(props["database_source"], "Redis Cloud (planned)");
    assert_eq!(props["dry_run"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_account_creates_a_free_subscription_and_database() {
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    let endpoint = format!("127.0.0.1:{}", fake_redis());

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .up_to_n_times(3)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fixed/plans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plans": [ { "id": 12, "name": "Free", "price": 0 } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions"))
        .and(body_partial_json(json!({ "name": "redisctl-brand-new" })))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "taskId": "t-sub" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/t-sub"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "t-sub", "status": "processing-completed",
            "response": { "resourceId": 501 }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions/501/databases"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "taskId": "t-db" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/tasks/t-db"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "t-db", "status": "processing-completed",
            "response": { "resourceId": 9001 }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions/501/databases/9001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": 9001, "name": "brand-new", "publicEndpoint": endpoint,
            "security": { "enableTls": false, "password": MOCK_PASSWORD }
        })))
        .mount(&server)
        .await;
    // After provisioning, the marker subscription is listed.
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": 501, "name": "redisctl-brand-new", "price": 0, "maximumDatabases": 1 } ]
        })))
        .mount(&server)
        .await;

    run_init_cloud(
        &cfg,
        project.path(),
        &repo,
        &["--name", "brand-new", "--defaults", "--agent", "claude"],
    )
    .assert()
    .success()
    .stdout(predicates::str::contains("cloud:subscription/501"))
    .stdout(predicates::str::contains(
        "database 9001 in Essentials subscription 501",
    ))
    .stdout(predicates::str::contains("Redis Cloud (new database)"))
    .stdout(predicates::str::contains(MOCK_PASSWORD).not());
    let env = std::fs::read_to_string(project.path().join(".env")).unwrap();
    assert!(env.contains("REDIS_URL=\""), "{env}");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_explicit_cloud_flag_replaces_an_existing_env_url() {
    // Explicit beats implicit: --cloud is consent to supersede what .env carries.
    // The old value is named masked and the file ends up on the cloud database.
    let cfg = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let server = MockServer::start().await;
    write_cloud_profile(&cfg, &server.uri());
    let endpoint = format!("127.0.0.1:{}", fake_redis());
    mount_both_tiers(&server, &endpoint).await;
    std::fs::write(
        project.path().join(".env"),
        "REDIS_URL=\"redis://default:0ldpw@stale-host:1\"\n",
    )
    .unwrap();

    run_init_cloud(
        &cfg,
        project.path(),
        &repo,
        &["--name", "essentials-db", "--defaults", "--agent", "claude"],
    )
    .assert()
    .success()
    .stdout(predicate::str::contains(
        "REDIS_URL replaced (was redis://default:****@stale-host:1)",
    ))
    .stdout(predicate::str::contains("0ldpw").not())
    .stdout(predicate::str::contains("✓ PING"));
    let env = std::fs::read_to_string(project.path().join(".env")).unwrap();
    assert!(env.contains(&endpoint), "{env}");
    assert!(!env.contains("stale-host"), "{env}");
}
