//! CLI tests for the agent-native `cloud auth` command split (non-blocking device `login`
//! that returns the code + a `status --wait` that completes). Uses a wiremock Okta issuer.

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Config with a `qa` cloud profile whose login endpoints point at the mock issuer.
fn write_config(temp: &TempDir, issuer_base: &str) {
    let config = format!(
        r#"
default_cloud = "qa"

[cloud_auth.qa]
okta_issuer = "{issuer_base}/oauth2/default"
okta_client_id = "test-client"
sm_api_url = "{issuer_base}/api/v1"
capi_url = "{issuer_base}/v1"
"#
    );
    std::fs::write(temp.path().join("config.toml"), config).unwrap();
}

fn cmd(temp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("redisctl").unwrap();
    c.env_remove("REDIS_CLOUD_API_KEY");
    c.env_remove("REDIS_CLOUD_SECRET_KEY");
    c.env_remove("REDISCTL_PROFILE");
    c.arg("--config-file").arg(temp.path().join("config.toml"));
    c
}

/// `auth login --device` (no --wait) initiates the flow, prints the code, and returns without
/// blocking — writing a pending record for `status --wait` to complete.
#[tokio::test]
async fn device_login_initiates_without_blocking() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_config(&temp, &server.uri());

    Mock::given(method("POST"))
        .and(path("/oauth2/default/v1/device/authorize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "DEV-CODE-123",
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://example.test/activate",
            "verification_uri_complete": "https://example.test/activate?user_code=WDJB-MJHT",
            "expires_in": 600,
            "interval": 5
        })))
        .mount(&server)
        .await;

    let output = cmd(&temp)
        .args([
            "cloud",
            "auth",
            "login",
            "--profile",
            "qa",
            "--device",
            "-o",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    // Returned the device code immediately (non-blocking) — no secret, no tokens.
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(report["status"], "authorization_pending");
    assert_eq!(report["user_code"], "WDJB-MJHT");
    assert_eq!(
        report["verification_uri_complete"],
        "https://example.test/activate?user_code=WDJB-MJHT"
    );

    // A pending record was written next to the config so `status --wait` can resume it.
    let pending = temp.path().join("redisctl-pending-qa.json");
    assert!(pending.exists(), "pending record should be written");
    let saved: Value = serde_json::from_slice(&std::fs::read(&pending).unwrap()).unwrap();
    // The full device authorization (carrying the device code) is persisted so `status --wait`
    // can resume polling; assert the code round-tripped without coupling to its exact nesting.
    assert!(
        saved.to_string().contains("DEV-CODE-123"),
        "pending record should carry the device authorization"
    );
    assert_eq!(saved["profile"], "qa");
}

/// `status --wait` with a pending record still awaiting approval polls until the local timeout
/// and reports `authorization_pending` (exit 0), keeping the record for a later resume.
#[tokio::test]
async fn status_wait_reports_pending_until_approved() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_config(&temp, &server.uri());

    Mock::given(method("POST"))
        .and(path("/oauth2/default/v1/device/authorize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "DEV-CODE-123",
            "user_code": "WDJB-MJHT",
            "verification_uri": "https://example.test/activate",
            "expires_in": 600,
            "interval": 1
        })))
        .mount(&server)
        .await;
    // The token endpoint keeps saying "pending" (user hasn't approved yet).
    Mock::given(method("POST"))
        .and(path("/oauth2/default/v1/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "authorization_pending"
        })))
        .mount(&server)
        .await;

    // Initiate.
    cmd(&temp)
        .args([
            "cloud",
            "auth",
            "login",
            "--profile",
            "qa",
            "--device",
            "-o",
            "json",
        ])
        .assert()
        .success();

    // Wait with a short timeout — still pending, exits 0 with a pending report.
    let output = cmd(&temp)
        .args([
            "cloud",
            "auth",
            "status",
            "--profile",
            "qa",
            "--wait",
            "--timeout",
            "1",
            "-o",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(report["status"], "authorization_pending");
    assert_eq!(report["authenticated"], false);
    // Record kept so a later `status --wait` can resume.
    assert!(temp.path().join("redisctl-pending-qa.json").exists());
}
