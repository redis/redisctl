//! Progress tracking and action polling for async Enterprise operations
//!
//! Enterprise API operations that are asynchronous return an `Action` which must be polled
//! until completion. This module provides utilities for that polling
//! with optional progress callbacks for UI updates.

use crate::error::{CoreError, Result};
use redis_enterprise::EnterpriseClient;
use redis_enterprise::actions::Action;
use std::time::{Duration, Instant};

/// Progress events emitted during async Enterprise operations
#[derive(Debug, Clone)]
pub enum EnterpriseProgressEvent {
    /// Action has been created/started
    Started { action_uid: String },
    /// Polling iteration with current status
    Polling {
        /// Action UID being polled.
        action_uid: String,
        /// Current status string (e.g. `"running"`, `"completed"`).
        status: String,
        /// Percent complete as reported by the API.
        ///
        /// Typed as `Option<String>` because the Redis Enterprise REST
        /// API emits this as a string (e.g. `"100"`), not a float.
        /// Callers that need a numeric value can `.parse::<f32>().ok()`.
        progress: Option<String>,
        /// Time since polling started.
        elapsed: Duration,
    },
    /// Action completed successfully
    Completed { action_uid: String },
    /// Action failed
    Failed { action_uid: String, error: String },
}

/// Callback type for Enterprise progress updates
///
/// CLI can use this to update spinners/progress bars.
/// MCP typically doesn't need this.
pub type EnterpriseProgressCallback = Box<dyn Fn(EnterpriseProgressEvent) + Send + Sync>;

/// Poll an Enterprise action until completion
///
/// # Arguments
///
/// * `client` - The Enterprise API client
/// * `action_uid` - The action UID to poll
/// * `timeout` - Maximum time to wait for completion
/// * `interval` - Time between polling attempts
/// * `on_progress` - Optional callback for progress updates
///
/// # Returns
///
/// The completed action, or an error if the action failed or timed out.
///
/// # Example
///
/// ```rust,ignore
/// use redisctl_core::enterprise::{poll_action, EnterpriseProgressEvent};
/// use std::time::Duration;
///
/// // Start an async operation (returns an action_uid)
/// let action_uid = "some-action-uid";
///
/// // Poll with progress callback
/// let completed = poll_action(
///     &client,
///     action_uid,
///     Duration::from_secs(600),
///     Duration::from_secs(5),
///     Some(Box::new(|event| {
///         match event {
///             EnterpriseProgressEvent::Polling { status, progress, elapsed, .. } => {
///                 println!("Status: {} ({:?}%) ({:.0}s)", status, progress, elapsed.as_secs());
///             }
///             EnterpriseProgressEvent::Completed { .. } => {
///                 println!("Done!");
///             }
///             _ => {}
///         }
///     })),
/// ).await?;
/// ```
pub async fn poll_action(
    client: &EnterpriseClient,
    action_uid: &str,
    timeout: Duration,
    interval: Duration,
    on_progress: Option<EnterpriseProgressCallback>,
) -> Result<Action> {
    let start = Instant::now();
    let handler = client.actions();

    emit(
        &on_progress,
        EnterpriseProgressEvent::Started {
            action_uid: action_uid.to_string(),
        },
    );

    loop {
        let elapsed = start.elapsed();
        if elapsed > timeout {
            return Err(CoreError::TaskTimeout(timeout));
        }

        let action = handler.get(action_uid).await?;
        let status = action.status.clone();

        emit(
            &on_progress,
            EnterpriseProgressEvent::Polling {
                action_uid: action_uid.to_string(),
                status: status.clone(),
                progress: action.progress.clone(),
                elapsed,
            },
        );

        match status.as_str() {
            "completed" => {
                emit(
                    &on_progress,
                    EnterpriseProgressEvent::Completed {
                        action_uid: action_uid.to_string(),
                    },
                );
                return Ok(action);
            }
            "failed" | "cancelled" => {
                let error = action
                    .error
                    .clone()
                    .unwrap_or_else(|| format!("Action {}", status));

                emit(
                    &on_progress,
                    EnterpriseProgressEvent::Failed {
                        action_uid: action_uid.to_string(),
                        error: error.clone(),
                    },
                );
                return Err(CoreError::TaskFailed(error));
            }
            // 'queued', 'starting', 'running', 'cancelling' - still in progress
            _ => {
                tokio::time::sleep(interval).await;
            }
        }
    }
}

/// Helper to emit progress events
fn emit(callback: &Option<EnterpriseProgressCallback>, event: EnterpriseProgressEvent) {
    if let Some(cb) = callback {
        cb(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(uri: String) -> EnterpriseClient {
        EnterpriseClient::builder()
            .base_url(uri)
            .username("test-user".to_string())
            .password("test-pass".to_string())
            .insecure(true)
            .build()
            .unwrap()
    }

    // An action that is already in a terminal "completed" state on the first
    // poll should return Ok immediately.
    #[tokio::test]
    async fn poll_action_immediate_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "flush",
                "status": "completed",
                "progress": "100"
            })))
            .mount(&mock_server)
            .await;

        let client = test_client(mock_server.uri());
        let result = poll_action(
            &client,
            "action-1",
            Duration::from_secs(5),
            Duration::from_millis(10),
            None,
        )
        .await;

        match result {
            Ok(action) => assert_eq!(action.status, "completed"),
            other => panic!("expected Ok(completed action), got {other:?}"),
        }
    }

    // An action that reports "running" twice before completing should keep
    // polling and ultimately return Ok.
    #[tokio::test]
    async fn poll_action_polls_then_succeeds() {
        let mock_server = MockServer::start().await;

        // Higher priority (lower number) + a call limit means the "running"
        // response is served for the first two polls, then exhausted.
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "flush",
                "status": "running",
                "progress": "50"
            })))
            .up_to_n_times(2)
            .with_priority(1)
            .mount(&mock_server)
            .await;

        // Default-priority fallback that takes over once the "running" mock is
        // exhausted.
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "flush",
                "status": "completed",
                "progress": "100"
            })))
            .mount(&mock_server)
            .await;

        let client = test_client(mock_server.uri());
        let result = poll_action(
            &client,
            "action-1",
            Duration::from_secs(5),
            Duration::from_millis(10),
            None,
        )
        .await;

        match result {
            Ok(action) => assert_eq!(action.status, "completed"),
            other => panic!("expected Ok(completed action), got {other:?}"),
        }
    }

    // A "failed" status surfaces the action's error message as TaskFailed.
    #[tokio::test]
    async fn poll_action_failure_surfaces_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "upgrade",
                "status": "failed",
                "error": "upgrade failed: version conflict"
            })))
            .mount(&mock_server)
            .await;

        let client = test_client(mock_server.uri());
        let result = poll_action(
            &client,
            "action-1",
            Duration::from_secs(5),
            Duration::from_millis(10),
            None,
        )
        .await;

        match result {
            Err(CoreError::TaskFailed(msg)) => {
                assert_eq!(msg, "upgrade failed: version conflict");
            }
            other => panic!("expected TaskFailed, got {other:?}"),
        }
    }

    // A "cancelled" status is also terminal and surfaces as TaskFailed. When no
    // error is provided, the message falls back to the status.
    #[tokio::test]
    async fn poll_action_cancelled_surfaces_as_failed() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "flush",
                "status": "cancelled"
            })))
            .mount(&mock_server)
            .await;

        let client = test_client(mock_server.uri());
        let result = poll_action(
            &client,
            "action-1",
            Duration::from_secs(5),
            Duration::from_millis(10),
            None,
        )
        .await;

        match result {
            Err(CoreError::TaskFailed(msg)) => {
                assert!(msg.contains("cancelled"), "unexpected message: {msg}");
            }
            other => panic!("expected TaskFailed, got {other:?}"),
        }
    }

    // With a 1ms timeout and a never-completing action, the first poll runs,
    // the function sleeps, and the next loop iteration trips the timeout.
    #[tokio::test]
    async fn poll_action_times_out() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "flush",
                "status": "running",
                "progress": "10"
            })))
            .mount(&mock_server)
            .await;

        let client = test_client(mock_server.uri());
        let result = poll_action(
            &client,
            "action-1",
            Duration::from_millis(1),
            Duration::from_millis(5),
            None,
        )
        .await;

        match result {
            Err(CoreError::TaskTimeout(_)) => {}
            other => panic!("expected TaskTimeout, got {other:?}"),
        }
    }

    // The progress callback must observe the lifecycle: Started, at least one
    // Polling event, and Completed.
    #[tokio::test]
    async fn poll_action_emits_progress_events() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/actions/action-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "action_uid": "action-1",
                "name": "flush",
                "status": "completed",
                "progress": "100"
            })))
            .mount(&mock_server)
            .await;

        let events: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let callback: EnterpriseProgressCallback = Box::new(move |event| {
            let label = match event {
                EnterpriseProgressEvent::Started { .. } => "started",
                EnterpriseProgressEvent::Polling { .. } => "polling",
                EnterpriseProgressEvent::Completed { .. } => "completed",
                EnterpriseProgressEvent::Failed { .. } => "failed",
            };
            sink.lock().unwrap().push(label.to_string());
        });

        let client = test_client(mock_server.uri());
        let result = poll_action(
            &client,
            "action-1",
            Duration::from_secs(5),
            Duration::from_millis(10),
            Some(callback),
        )
        .await;

        assert!(result.is_ok(), "expected Ok, got {result:?}");

        let observed = events.lock().unwrap();
        assert!(observed.contains(&"started".to_string()), "{observed:?}");
        assert!(observed.contains(&"polling".to_string()), "{observed:?}");
        assert!(observed.contains(&"completed".to_string()), "{observed:?}");
    }
}
