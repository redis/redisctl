//! Hermetic tests for init telemetry: a wiremock endpoint plays Amplitude, HOME is
//! redirected so the device id never touches the real cache, and the RESP
//! responder lets runs succeed offline. Unix-only: the id-file isolation relies on
//! $HOME.
#![cfg(unix)]

mod init_common;

use assert_cmd::Command;
use init_common::{fake_redis, skills_fixture};
use predicates::prelude::*;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// `redisctl init --url <local fake> ...` with telemetry pointed at `server`.
fn run_init(
    project: &TempDir,
    home: &TempDir,
    repo: &TempDir,
    endpoint: &str,
    server: Option<&MockServer>,
    extra: &[&str],
) -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.current_dir(project.path())
        .env("HOME", home.path())
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .env_remove("DO_NOT_TRACK")
        .env_remove("REDISCTL_INIT_TELEMETRY")
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "test-key")
        .args([
            "init",
            "--url",
            &format!("redis://default:s3cret@{endpoint}"),
            "--name",
            "s3cret-name",
            "--defaults",
            "--no-install-cli",
            "--agent",
            "claude",
        ])
        .args(extra);
    if let Some(server) = server {
        cmd.env(
            "REDISCTL_INIT_AMPLITUDE_URL",
            format!("{}/2/httpapi", server.uri()),
        );
    }
    cmd
}

async fn amplitude() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/2/httpapi"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_one_event_and_never_the_values() {
    let (project, home, repo) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        skills_fixture(),
    );
    let server = amplitude().await;
    let endpoint = format!("127.0.0.1:{}", fake_redis());

    run_init(&project, &home, &repo, &endpoint, Some(&server), &[])
        .assert()
        .success()
        .stderr(predicate::str::contains("Anonymous usage data"));

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["api_key"], "test-key");
    let event = &body["events"][0];
    assert_eq!(event["event_type"], "cli_run");
    assert_eq!(event["ip"], "0.0.0.0");
    let props = &event["event_properties"];
    assert_eq!(props["outcome"], "success");
    assert_eq!(props["exit_code"], 0);
    assert_eq!(props["url_provided"], true);
    assert_eq!(props["name_provided"], true);
    assert_eq!(props["first_run"], true);
    assert_eq!(props["interactive"], false);
    // Values never travel: not the URL, the password, the name, or any path.
    let raw = String::from_utf8_lossy(&requests[0].body);
    assert!(!raw.contains("s3cret"), "{raw}");
    assert!(!raw.contains(project.path().to_str().unwrap()), "{raw}");

    // Second run: same device id, no notice, first_run false.
    run_init(&project, &home, &repo, &endpoint, Some(&server), &[])
        .assert()
        .success()
        .stderr(predicate::str::contains("Anonymous usage data").not());
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let second: serde_json::Value = requests[1].body_json().unwrap();
    assert_eq!(
        second["events"][0]["device_id"],
        body["events"][0]["device_id"]
    );
    assert_eq!(second["events"][0]["event_properties"]["first_run"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn every_opt_out_sends_nothing() {
    let (project, home, repo) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        skills_fixture(),
    );
    let server = amplitude().await;
    let endpoint = format!("127.0.0.1:{}", fake_redis());

    run_init(
        &project,
        &home,
        &repo,
        &endpoint,
        Some(&server),
        &["--no-telemetry"],
    )
    .assert()
    .success();
    run_init(&project, &home, &repo, &endpoint, Some(&server), &[])
        .env("REDISCTL_INIT_TELEMETRY", "0")
        .assert()
        .success();
    run_init(&project, &home, &repo, &endpoint, Some(&server), &[])
        .env("DO_NOT_TRACK", "1")
        .assert()
        .success();
    // An exported-but-empty key is the off switch; a compiled-in key must not
    // resurface, and the debug echo says why nothing was shared.
    run_init(&project, &home, &repo, &endpoint, Some(&server), &[])
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .env("REDISCTL_INIT_TELEMETRY_DEBUG", "1")
        .assert()
        .success()
        .stderr(predicate::str::contains("nothing shared"));

    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_run_reports_step_and_exit_code() {
    let (project, home, repo) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        skills_fixture(),
    );
    let server = amplitude().await;
    // A port with nothing listening: validation fails after apply.
    let dead = std::net::TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    run_init(
        &project,
        &home,
        &repo,
        &format!("127.0.0.1:{dead}"),
        Some(&server),
        &[],
    )
    .assert()
    .failure();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let props =
        requests[0].body_json::<serde_json::Value>().unwrap()["events"][0]["event_properties"]
            .clone();
    assert_eq!(props["outcome"], "failed");
    assert_eq!(props["failed_step"], "validate");
    assert_eq!(props["exit_code"], 10);
}
