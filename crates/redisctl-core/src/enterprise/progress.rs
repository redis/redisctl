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
