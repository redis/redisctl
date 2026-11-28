//! `cloud subscription get` and `cloud database get` probe the flexible and
//! fixed endpoints in turn. Only a 404 means "try the other kind": an auth
//! failure or a server error must reach the user instead of being reported
//! as a missing resource.

use assert_cmd::Command;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn write_cloud_profile(temp_dir: &TempDir, api_url: &str) {
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
    std::fs::write(temp_dir.path().join("config.toml"), config).unwrap();
}

fn cmd(temp_dir: &TempDir) -> Command {
    let mut c = Command::cargo_bin("redisctl").unwrap();
    c.env_remove("REDIS_CLOUD_API_KEY");
    c.env_remove("REDIS_CLOUD_SECRET_KEY");
    c.env_remove("REDISCTL_PROFILE");
    c.arg("--config-file")
        .arg(temp_dir.path().join("config.toml"));
    c
}

async fn mount(server: &MockServer, route: &str, status: u16) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "description": format!("mock {status}")
        })))
        .mount(server)
        .await;
}

fn stderr_of(temp_dir: &TempDir, args: &[&str]) -> String {
    let output = cmd(temp_dir)
        .args(args)
        .assert()
        .failure()
        .get_output()
        .clone();
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[tokio::test]
async fn subscription_get_reports_auth_failure_not_missing_subscription() {
    let temp_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp_dir, &server.uri());
    mount(&server, "/subscriptions/42", 401).await;
    mount(&server, "/fixed/subscriptions/42", 401).await;

    let stderr = stderr_of(&temp_dir, &["cloud", "subscription", "get", "42"]);

    assert!(
        !stderr.contains("Subscription 42 not found"),
        "a 401 must not be reported as a missing subscription: {stderr}"
    );
    assert!(stderr.contains("401"), "stderr: {stderr}");
}

#[tokio::test]
async fn subscription_get_reports_server_error_from_fixed_fallback() {
    let temp_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp_dir, &server.uri());
    mount(&server, "/subscriptions/42", 404).await;
    mount(&server, "/fixed/subscriptions/42", 500).await;

    let stderr = stderr_of(&temp_dir, &["cloud", "subscription", "get", "42"]);

    assert!(
        !stderr.contains("Subscription 42 not found"),
        "a 500 must not be reported as a missing subscription: {stderr}"
    );
    assert!(stderr.contains("500"), "stderr: {stderr}");
}

#[tokio::test]
async fn subscription_get_still_reports_not_found_when_both_kinds_404() {
    let temp_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp_dir, &server.uri());
    mount(&server, "/subscriptions/42", 404).await;
    mount(&server, "/fixed/subscriptions/42", 404).await;

    let stderr = stderr_of(&temp_dir, &["cloud", "subscription", "get", "42"]);

    assert!(
        stderr.contains("Subscription 42 not found"),
        "stderr: {stderr}"
    );
}

#[tokio::test]
async fn database_get_reports_auth_failure_not_missing_database() {
    let temp_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp_dir, &server.uri());
    mount(&server, "/fixed/subscriptions/42/databases/7", 401).await;
    mount(&server, "/subscriptions/42/databases/7", 401).await;

    let stderr = stderr_of(&temp_dir, &["cloud", "database", "get", "42:7"]);

    assert!(
        !stderr.contains("Database 42:7 not found"),
        "a 401 must not be reported as a missing database: {stderr}"
    );
    assert!(stderr.contains("401"), "stderr: {stderr}");
}

#[tokio::test]
async fn database_get_reports_server_error_from_flexible_fallback() {
    let temp_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp_dir, &server.uri());
    mount(&server, "/fixed/subscriptions/42/databases/7", 404).await;
    mount(&server, "/subscriptions/42/databases/7", 500).await;

    let stderr = stderr_of(&temp_dir, &["cloud", "database", "get", "42:7"]);

    assert!(
        !stderr.contains("Database 42:7 not found"),
        "a 500 must not be reported as a missing database: {stderr}"
    );
    assert!(stderr.contains("500"), "stderr: {stderr}");
}

#[tokio::test]
async fn database_get_still_reports_not_found_when_both_kinds_404() {
    let temp_dir = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp_dir, &server.uri());
    mount(&server, "/fixed/subscriptions/42/databases/7", 404).await;
    mount(&server, "/subscriptions/42/databases/7", 404).await;

    let stderr = stderr_of(&temp_dir, &["cloud", "database", "get", "42:7"]);

    assert!(
        stderr.contains("Database 42:7 not found"),
        "stderr: {stderr}"
    );
}
