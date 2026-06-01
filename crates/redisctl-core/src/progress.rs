//! Progress tracking and task polling for async Cloud operations
//!
//! Cloud API operations return a `TaskStateUpdate` which must be polled
//! until completion. This module provides utilities for that polling
//! with optional progress callbacks for UI updates.

use crate::error::{CoreError, Result};
use redis_cloud::tasks::TaskStateUpdate;
use redis_cloud::{CloudClient, TaskHandler};
use std::time::{Duration, Instant};

/// Progress events emitted during async operations
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Task has been created/started
    Started { task_id: String },
    /// Polling iteration with current status
    Polling {
        task_id: String,
        status: String,
        elapsed: Duration,
    },
    /// Task completed successfully
    Completed {
        task_id: String,
        resource_id: Option<i32>,
    },
    /// Task failed
    Failed { task_id: String, error: String },
}

/// Callback type for progress updates
///
/// CLI can use this to update spinners/progress bars.
/// MCP typically doesn't need this.
pub type ProgressCallback = Box<dyn Fn(ProgressEvent) + Send + Sync>;

/// Poll a Cloud task until completion
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `task_id` - The task ID to poll
/// * `timeout` - Maximum time to wait for completion
/// * `interval` - Time between polling attempts
/// * `on_progress` - Optional callback for progress updates
///
/// # Returns
///
/// The completed task response, or an error if the task failed or timed out.
///
/// # Example
///
/// ```rust,ignore
/// use redisctl_core::{poll_task, ProgressEvent};
/// use std::time::Duration;
///
/// // Create a database (returns TaskStateUpdate)
/// let task = handler.create(subscription_id, &request).await?;
/// let task_id = task.task_id.unwrap();
///
/// // Poll with progress callback
/// let completed = poll_task(
///     &client,
///     &task_id,
///     Duration::from_secs(600),
///     Duration::from_secs(10),
///     Some(Box::new(|event| {
///         match event {
///             ProgressEvent::Polling { status, elapsed, .. } => {
///                 println!("Status: {} ({:.0}s)", status, elapsed.as_secs());
///             }
///             ProgressEvent::Completed { resource_id, .. } => {
///                 println!("Done! Resource ID: {:?}", resource_id);
///             }
///             _ => {}
///         }
///     })),
/// ).await?;
/// ```
pub async fn poll_task(
    client: &CloudClient,
    task_id: &str,
    timeout: Duration,
    interval: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<TaskStateUpdate> {
    let start = Instant::now();
    let handler = TaskHandler::new(client.clone());

    emit(
        &on_progress,
        ProgressEvent::Started {
            task_id: task_id.to_string(),
        },
    );

    loop {
        let elapsed = start.elapsed();
        if elapsed > timeout {
            return Err(CoreError::TaskTimeout(timeout));
        }

        let task = handler.get_task_by_id(task_id.to_string()).await?;
        let status = task.status.clone().unwrap_or_default();

        emit(
            &on_progress,
            ProgressEvent::Polling {
                task_id: task_id.to_string(),
                status: status.clone(),
                elapsed,
            },
        );

        // Check for terminal states (case-insensitive)
        match status.to_lowercase().as_str() {
            // Success states
            "processing-completed" | "completed" | "complete" | "succeeded" | "success" => {
                let resource_id = task.response.as_ref().and_then(|r| r.resource_id);
                emit(
                    &on_progress,
                    ProgressEvent::Completed {
                        task_id: task_id.to_string(),
                        resource_id,
                    },
                );
                return Ok(task);
            }
            // Failure states
            "processing-error" | "failed" | "error" => {
                let error = task
                    .response
                    .as_ref()
                    .and_then(|r| r.error_message())
                    .unwrap_or_else(|| format!("Task failed with status: {}", status));

                emit(
                    &on_progress,
                    ProgressEvent::Failed {
                        task_id: task_id.to_string(),
                        error: error.clone(),
                    },
                );
                return Err(CoreError::TaskFailed(error));
            }
            // Cancelled state
            "cancelled" => {
                emit(
                    &on_progress,
                    ProgressEvent::Failed {
                        task_id: task_id.to_string(),
                        error: "Task was cancelled".to_string(),
                    },
                );
                return Err(CoreError::TaskFailed("Task was cancelled".to_string()));
            }
            _ => {
                // Still processing, wait and try again
                tokio::time::sleep(interval).await;
            }
        }
    }
}

/// Helper to emit progress events
fn emit(callback: &Option<ProgressCallback>, event: ProgressEvent) {
    if let Some(cb) = callback {
        cb(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_client(uri: String) -> CloudClient {
        CloudClient::builder()
            .api_key("test-key".to_string())
            .api_secret("test-secret".to_string())
            .base_url(uri)
            .build()
            .unwrap()
    }

    // Regression: a failed task whose `response.error` is a structured object
    // (e.g. a failed database backup) must surface the human-readable
    // description, not fail to deserialize. See redis-cloud-rs
    // ProcessorResponse::error_message.
    #[tokio::test]
    async fn poll_task_surfaces_object_error_description() {
        let mock_server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/tasks/task-backup"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "taskId": "task-backup",
                "commandType": "DATABASE_BACKUP",
                "status": "processing-error",
                "response": {
                    "error": {
                        "type": "BACKUP_FAILED",
                        "status": "400 BAD_REQUEST",
                        "description": "Remote backup location is not configured"
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = test_client(mock_server.uri());
        let result = poll_task(
            &client,
            "task-backup",
            Duration::from_secs(5),
            Duration::from_millis(10),
            None,
        )
        .await;

        match result {
            Err(CoreError::TaskFailed(msg)) => {
                assert_eq!(msg, "Remote backup location is not configured");
            }
            other => panic!("expected TaskFailed with description, got {other:?}"),
        }
    }
}
