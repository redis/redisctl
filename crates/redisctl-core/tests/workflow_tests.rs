//! Integration-style tests for the async `*_and_wait` workflow functions.
//!
//! These exercise the full 3-step pattern (kick off the operation, poll the
//! task/action, optionally fetch the resulting resource) against a wiremock
//! server, so the HTTP wiring and the polling loop are covered end to end.

use redis_cloud::CloudClient;
use redis_cloud::databases::DatabaseCreateRequest;
use redis_enterprise::EnterpriseClient;
use redisctl_core::CoreError;
use redisctl_core::cloud::{create_database_and_wait, delete_database_and_wait};
use redisctl_core::enterprise::flush_database_and_wait;
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn cloud_client(uri: String) -> CloudClient {
    CloudClient::builder()
        .api_key("test-key".to_string())
        .api_secret("test-secret".to_string())
        .base_url(uri)
        .build()
        .unwrap()
}

fn enterprise_client(uri: String) -> EnterpriseClient {
    EnterpriseClient::builder()
        .base_url(uri)
        .username("test-user".to_string())
        .password("test-pass".to_string())
        .insecure(true)
        .build()
        .unwrap()
}

// delete -> task -> poll-completed. The simplest 2-step workflow: no resource
// fetch at the end.
#[tokio::test]
async fn cloud_delete_database_and_wait_happy_path() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/1/databases/2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "taskId": "task-1"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "taskId": "task-1",
            "status": "processing-completed",
            "response": {"resourceId": 2}
        })))
        .mount(&mock_server)
        .await;

    let client = cloud_client(mock_server.uri());
    let result = delete_database_and_wait(&client, 1, 2, Duration::from_secs(5), None).await;

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
}

// delete -> task -> poll reports processing-error -> TaskFailed.
#[tokio::test]
async fn cloud_delete_database_and_wait_task_failure() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/subscriptions/1/databases/2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "taskId": "task-1"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "taskId": "task-1",
            "status": "processing-error",
            "response": {"error": "Something went wrong"}
        })))
        .mount(&mock_server)
        .await;

    let client = cloud_client(mock_server.uri());
    let result = delete_database_and_wait(&client, 1, 2, Duration::from_secs(5), None).await;

    match result {
        Err(CoreError::TaskFailed(msg)) => {
            assert_eq!(msg, "Something went wrong");
        }
        other => panic!("expected TaskFailed, got {other:?}"),
    }
}

// create -> task -> poll-completed (carrying a resourceId) -> fetch the new
// database. Covers the full 3-step pattern with a resource fetch at the end.
#[tokio::test]
async fn cloud_create_database_and_wait_happy_path() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/subscriptions/1/databases"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "taskId": "task-1"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/tasks/task-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "taskId": "task-1",
            "status": "processing-completed",
            "response": {"resourceId": 42}
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/subscriptions/1/databases/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "databaseId": 42,
            "name": "test-db",
            "status": "active"
        })))
        .mount(&mock_server)
        .await;

    let request = DatabaseCreateRequest::builder()
        .name("test-db")
        .memory_limit_in_gb(1.0)
        .build();

    let client = cloud_client(mock_server.uri());
    let result = create_database_and_wait(&client, 1, &request, Duration::from_secs(5), None).await;

    match result {
        Ok(db) => {
            assert_eq!(db.database_id, 42);
            assert_eq!(db.name.as_deref(), Some("test-db"));
        }
        other => panic!("expected Ok(database), got {other:?}"),
    }
}

// flush -> action -> poll-completed. Enterprise 2-step workflow, no resource
// fetch at the end.
#[tokio::test]
async fn enterprise_flush_database_and_wait_happy_path() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/bdbs/1/flush"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "action_uid": "action-1"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/actions/action-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "action_uid": "action-1",
            "name": "flush",
            "status": "completed",
            "progress": "100"
        })))
        .mount(&mock_server)
        .await;

    let client = enterprise_client(mock_server.uri());
    let result = flush_database_and_wait(&client, 1, Duration::from_secs(5), None).await;

    assert!(result.is_ok(), "expected Ok(()), got {result:?}");
}

// flush -> action -> poll reports failed -> TaskFailed surfacing the error.
#[tokio::test]
async fn enterprise_flush_database_and_wait_task_failure() {
    let mock_server = MockServer::start().await;

    Mock::given(method("PUT"))
        .and(path("/v1/bdbs/1/flush"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "action_uid": "action-1"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/actions/action-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "action_uid": "action-1",
            "name": "flush",
            "status": "failed",
            "error": "flush failed: database is locked"
        })))
        .mount(&mock_server)
        .await;

    let client = enterprise_client(mock_server.uri());
    let result = flush_database_and_wait(&client, 1, Duration::from_secs(5), None).await;

    match result {
        Err(CoreError::TaskFailed(msg)) => {
            assert_eq!(msg, "flush failed: database is locked");
        }
        other => panic!("expected TaskFailed, got {other:?}"),
    }
}
