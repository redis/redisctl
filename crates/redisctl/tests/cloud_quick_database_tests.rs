//! Mocked end-to-end tests for `cloud workflow quick-database`.
//!
//! Each test points a cloud profile at a wiremock CAPI and drives the real binary. Every
//! scenario asserts the secret-leak guard: the mock password and the `rediss://…@` URL must
//! appear in the credentials file but NEVER on stdout/stderr.

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const MOCK_PASSWORD: &str = "s3cr3t-mock-pw";
const MOCK_ENDPOINT: &str = "mock-host.example.com:12000";
const DB_NAME: &str = "quick-db-test";
const SUB_ID: i64 = 501;
const DB_ID: i64 = 9001;

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

fn run_quick_database(
    temp_dir: &TempDir,
    env_path: &std::path::Path,
) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.env_remove("REDIS_CLOUD_API_KEY");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env_remove("REDIS_CLOUD_API_URL");
    cmd.env_remove("REDISCTL_PROFILE");
    cmd.arg("--config-file")
        .arg(temp_dir.path().join("config.toml"))
        .arg("cloud")
        .arg("workflow")
        .arg("quick-database")
        .arg("--name")
        .arg(DB_NAME)
        .arg("--output-credentials")
        .arg(env_path)
        .arg("--wait-interval")
        .arg("1")
        .arg("--wait-timeout")
        .arg("30")
        .arg("-o")
        .arg("json");
    cmd.assert()
}

fn fixed_database_body() -> Value {
    json!({
        "databaseId": DB_ID,
        "name": DB_NAME,
        "region": "us-east-1",
        "publicEndpoint": MOCK_ENDPOINT,
        "security": { "enableTls": true, "password": MOCK_PASSWORD }
    })
}

async fn mock_free_plan(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/fixed/plans"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "plans": [
                { "id": 34, "name": "Standard", "price": 5 },
                { "id": 12, "name": "Free", "price": 0, "provider": "AWS", "region": "us-east-1" }
            ]
        })))
        .mount(server)
        .await;
}

async fn mock_task_completed(server: &MockServer, task_id: &str, resource_id: i64) {
    Mock::given(method("GET"))
        .and(path(format!("/tasks/{task_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": task_id,
            "status": "processing-completed",
            "response": { "resourceId": resource_id }
        })))
        .mount(server)
        .await;
}

/// Assert the leak guard: secrets in the file, never in the captured output.
fn assert_no_secret_leak(stdout: &[u8], stderr: &[u8]) {
    let out = String::from_utf8_lossy(stdout);
    let err = String::from_utf8_lossy(stderr);
    for stream in [&out, &err] {
        assert!(
            !stream.contains(MOCK_PASSWORD),
            "password leaked into output: {stream}"
        );
        assert!(
            !stream.contains("rediss://default:"),
            "connection URL leaked into output: {stream}"
        );
    }
}

#[tokio::test]
async fn fresh_create_writes_env_and_prints_schema() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // No existing subscription → fresh provisioning.
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .mount(&server)
        .await;
    mock_free_plan(&server).await;
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-sub", "status": "received"
        })))
        .mount(&server)
        .await;
    mock_task_completed(&server, "task-sub", SUB_ID).await;
    Mock::given(method("POST"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-db", "status": "received"
        })))
        .mount(&server)
        .await;
    mock_task_completed(&server, "task-db", DB_ID).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixed_database_body()))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .success()
        .get_output()
        .clone();
    assert_no_secret_leak(&output.stdout, &output.stderr);

    // Schema lock (PRD §5.2): exact key set, exact values.
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["database"]["id"], DB_ID.to_string());
    assert_eq!(report["database"]["name"], DB_NAME);
    assert_eq!(report["database"]["region"], "us-east-1");
    assert_eq!(report["database"]["plan"], "free");
    assert_eq!(report["database"]["tls"], true);
    assert_eq!(report["credentials_variable"], "REDIS_URL");
    assert_eq!(
        report["credentials_written_to"],
        env_path.display().to_string()
    );
    // Key set is locked (serializer emits keys sorted, so compare sorted).
    let mut top_keys: Vec<&str> = report
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    top_keys.sort_unstable();
    assert_eq!(
        top_keys,
        [
            "credentials_variable",
            "credentials_written_to",
            "database",
            "status"
        ]
    );
    let mut db_keys: Vec<&str> = report["database"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    db_keys.sort_unstable();
    assert_eq!(db_keys, ["id", "name", "plan", "region", "tls"]);

    // The credentials file holds the full connection string plus broken-out fields.
    let env_body = std::fs::read_to_string(&env_path).unwrap();
    assert!(env_body.contains(&format!(
        "REDIS_URL=rediss://default:{MOCK_PASSWORD}@{MOCK_ENDPOINT}"
    )));
    assert!(env_body.contains("REDIS_HOST=mock-host.example.com"));
    assert!(env_body.contains("REDIS_PORT=12000"));
    assert!(env_body.contains(&format!("REDIS_PASSWORD={MOCK_PASSWORD}")));
    assert!(env_body.contains("REDIS_USERNAME=default"));
    assert!(env_body.contains("REDIS_TLS=true"));
}

#[tokio::test]
async fn second_run_reuses_without_writes() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // Our subscription already exists, with a database. No POST mocks are mounted, so any
    // create attempt would 404 and fail the run — proving reuse performs no writes.
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": SUB_ID, "name": format!("redisctl-{DB_NAME}"), "status": "active" } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": { "subscriptionId": SUB_ID, "databases": [ fixed_database_body() ] }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixed_database_body()))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .success()
        .get_output()
        .clone();
    assert_no_secret_leak(&output.stdout, &output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(report["status"], "reused");
    assert_eq!(report["database"]["id"], DB_ID.to_string());

    let env_body = std::fs::read_to_string(&env_path).unwrap();
    assert!(env_body.contains(MOCK_PASSWORD));
}

/// The public endpoint can lag behind task completion on a fresh create. The workflow must
/// poll the database GET until it appears, not fail the first read.
#[tokio::test]
async fn waits_for_public_endpoint_to_appear() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // Reuse path: our subscription + database already exist.
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": SUB_ID, "name": format!("redisctl-{DB_NAME}"), "status": "active" } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": { "subscriptionId": SUB_ID, "databases": [ { "databaseId": DB_ID } ] }
        })))
        .mount(&server)
        .await;
    // First GET has no endpoint yet (mounted first, capped at one match); the retry succeeds.
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": DB_ID,
            "name": DB_NAME,
            "security": { "enableTls": true, "password": MOCK_PASSWORD }
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixed_database_body()))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .success()
        .get_output()
        .clone();
    assert_no_secret_leak(&output.stdout, &output.stderr);
    let env_body = std::fs::read_to_string(&env_path).unwrap();
    assert!(env_body.contains(&format!(
        "REDIS_URL=rediss://default:{MOCK_PASSWORD}@{MOCK_ENDPOINT}"
    )));
}

/// If the endpoint never appears within the timeout, that's transient (exit 3), not `unknown`.
#[tokio::test]
async fn missing_endpoint_past_timeout_is_transient() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": SUB_ID, "name": format!("redisctl-{DB_NAME}"), "status": "active" } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": { "subscriptionId": SUB_ID, "databases": [ { "databaseId": DB_ID } ] }
        })))
        .mount(&server)
        .await;
    // The endpoint never populates.
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": DB_ID,
            "name": DB_NAME,
            "security": { "enableTls": true, "password": MOCK_PASSWORD }
        })))
        .mount(&server)
        .await;

    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.env_remove("REDIS_CLOUD_API_KEY");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env_remove("REDIS_CLOUD_API_URL");
    cmd.env_remove("REDISCTL_PROFILE");
    let output = cmd
        .arg("--config-file")
        .arg(temp.path().join("config.toml"))
        .args([
            "cloud",
            "workflow",
            "quick-database",
            "--name",
            DB_NAME,
            "--wait-timeout",
            "1",
            "--wait-interval",
            "1",
            "-o",
            "json",
        ])
        .arg("--output-credentials")
        .arg(&env_path)
        .assert()
        .code(3)
        .get_output()
        .clone();
    let env: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(env["error"]["code"], "transient_api_error");
    assert_eq!(env["error"]["retryable"], true);
    assert!(!env_path.exists());
}

/// PRD §7.2 leak guard: even at max verbosity (`-vvv`), the password — which flows through
/// the CAPI response bodies on the reuse path — must not reach stdout or stderr.
#[tokio::test]
async fn no_secret_leak_at_max_verbosity() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": SUB_ID, "name": format!("redisctl-{DB_NAME}"), "status": "active" } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": { "subscriptionId": SUB_ID, "databases": [ fixed_database_body() ] }
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixed_database_body()))
        .mount(&server)
        .await;

    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.env_remove("REDIS_CLOUD_API_KEY");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env_remove("REDIS_CLOUD_API_URL");
    cmd.env_remove("REDISCTL_PROFILE");
    cmd.env_remove("RUST_LOG"); // don't let the runner's RUST_LOG change verbosity
    let output = cmd
        .arg("--config-file")
        .arg(temp.path().join("config.toml"))
        .arg("-vvv")
        .args([
            "cloud",
            "workflow",
            "quick-database",
            "--name",
            DB_NAME,
            "-o",
            "json",
        ])
        .arg("--output-credentials")
        .arg(&env_path)
        .assert()
        .success()
        .get_output()
        .clone();

    assert_no_secret_leak(&output.stdout, &output.stderr);
    // Sanity: the secret really did travel through the API responses (so the guard is meaningful).
    assert!(
        std::fs::read_to_string(&env_path)
            .unwrap()
            .contains(MOCK_PASSWORD)
    );
}

#[tokio::test]
async fn resume_after_partial_create() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // Subscription exists but has NO database (crashed between the two creates). The workflow
    // must resume by creating only the database — no subscription POST is mounted.
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [ { "id": SUB_ID, "name": format!("redisctl-{DB_NAME}"), "status": "active" } ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscription": { "subscriptionId": SUB_ID, "databases": [] }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/fixed/subscriptions/{SUB_ID}/databases")))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-db", "status": "received"
        })))
        .mount(&server)
        .await;
    mock_task_completed(&server, "task-db", DB_ID).await;
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixed_database_body()))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .success()
        .get_output()
        .clone();
    assert_no_secret_leak(&output.stdout, &output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["database"]["id"], DB_ID.to_string());
}

#[tokio::test]
async fn task_failure_surfaces_error() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .mount(&server)
        .await;
    mock_free_plan(&server).await;
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-sub", "status": "received"
        })))
        .mount(&server)
        .await;
    // The create task fails mid-flight.
    Mock::given(method("GET"))
        .and(path("/tasks/task-sub"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "task-sub",
            "status": "processing-error",
            "response": { "error": "provisioning blew up" }
        })))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .code(1)
        .get_output()
        .clone();
    assert_no_secret_leak(&output.stdout, &output.stderr);
    // In JSON mode the error envelope is on stdout (agents parse stdout).
    let env: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(env["status"], "error");
    assert_eq!(env["error"]["code"], "unknown");
    assert!(
        env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("provisioning blew up")
    );
    assert!(!env_path.exists(), "no credentials file on failure");
}

#[tokio::test]
async fn free_plan_gate_rejection_is_actionable() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .mount(&server)
        .await;
    mock_free_plan(&server).await;
    // The public CAPI rejects free-plan creation when the trusted-UA / payment gate is unmet.
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "description": "FREE_PLAN_IS_ALLOWED_ONLY_FOR_ACCOUNTS_WITH_VALID_PAYMENT_INFO"
        })))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .code(4)
        .get_output()
        .clone();
    let env: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(env["status"], "error");
    assert_eq!(env["error"]["code"], "free_db_exists");
    assert_eq!(env["error"]["retryable"], false);
    assert!(!env_path.exists());
}

/// The one-free-subscription limit is reported by the API as an *async task* failure (not a
/// synchronous 4xx), so it must still classify as `free_db_exists` (exit 4), not `unknown`.
#[tokio::test]
async fn free_sub_limit_via_task_error_is_free_db_exists() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "subscriptions": [] })))
        .mount(&server)
        .await;
    mock_free_plan(&server).await;
    Mock::given(method("POST"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-sub", "status": "received"
        })))
        .mount(&server)
        .await;
    // The create task fails asynchronously with the one-free-sub message.
    Mock::given(method("GET"))
        .and(path("/tasks/task-sub"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "task-sub",
            "status": "processing-error",
            "response": { "error": "The account already has a free plan Essentials subscription." }
        })))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .code(4)
        .get_output()
        .clone();
    let env: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(env["error"]["code"], "free_db_exists");
    assert!(!env_path.exists());
}

#[tokio::test]
async fn invalid_name_is_rejected_before_any_call() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // No mocks mounted: a valid run would hit the API, but validation must fail first.
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.env_remove("REDIS_CLOUD_API_KEY");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env_remove("REDIS_CLOUD_API_URL");
    let output = cmd
        .arg("--config-file")
        .arg(temp.path().join("config.toml"))
        .args([
            "cloud",
            "workflow",
            "quick-database",
            "--name",
            "Invalid_Name",
            "-o",
            "json",
        ])
        .arg("--output-credentials")
        .arg(&env_path)
        .assert()
        .code(2)
        .get_output()
        .clone();
    let env: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(env["status"], "error");
    assert_eq!(env["error"]["code"], "invalid_name");
    assert!(
        env["error"]["message"]
            .as_str()
            .unwrap()
            .contains("invalid database name")
    );
    assert!(!env_path.exists());
}

#[tokio::test]
async fn persistent_5xx_is_retryable_exit_3() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // The very first CAPI call (list subscriptions) fails with a persistent 503.
    Mock::given(method("GET"))
        .and(path("/fixed/subscriptions"))
        .respond_with(ResponseTemplate::new(503).set_body_json(json!({
            "description": "service temporarily unavailable"
        })))
        .mount(&server)
        .await;

    let output = run_quick_database(&temp, &env_path)
        .code(3)
        .get_output()
        .clone();
    let env: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(env["status"], "error");
    assert_eq!(env["error"]["code"], "transient_api_error");
    assert_eq!(env["error"]["retryable"], true);
    assert!(!env_path.exists());
}

/// `database-credentials`: write an EXISTING database's connection string to a file with no
/// provisioning (single GET). Report status is "existing"; the leak guard still holds.
#[tokio::test]
async fn database_credentials_writes_existing_db_without_provisioning() {
    let temp = TempDir::new().unwrap();
    let server = MockServer::start().await;
    write_cloud_profile(&temp, &server.uri());
    let env_path = temp.path().join(".env");

    // Only a GET on the database — no list/plan/create mocks, so any provisioning attempt 404s.
    Mock::given(method("GET"))
        .and(path(format!(
            "/fixed/subscriptions/{SUB_ID}/databases/{DB_ID}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixed_database_body()))
        .mount(&server)
        .await;

    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.env_remove("REDIS_CLOUD_API_KEY");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env_remove("REDIS_CLOUD_API_URL");
    cmd.env_remove("REDISCTL_PROFILE");
    let output = cmd
        .arg("--config-file")
        .arg(temp.path().join("config.toml"))
        .args([
            "cloud",
            "workflow",
            "database-credentials",
            "--subscription-id",
            &SUB_ID.to_string(),
            "--database-id",
            &DB_ID.to_string(),
            "-o",
            "json",
        ])
        .arg("--output-credentials")
        .arg(&env_path)
        .assert()
        .success()
        .get_output()
        .clone();

    assert_no_secret_leak(&output.stdout, &output.stderr);
    let report: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(report["status"], "existing");
    assert_eq!(report["database"]["id"], DB_ID.to_string());
    assert_eq!(report["database"]["name"], DB_NAME);
    assert_eq!(report["database"]["plan"], "essentials");

    let env_body = std::fs::read_to_string(&env_path).unwrap();
    assert!(env_body.contains(&format!(
        "REDIS_URL=rediss://default:{MOCK_PASSWORD}@{MOCK_ENDPOINT}"
    )));
    assert!(env_body.contains("REDIS_HOST=mock-host.example.com"));
}
