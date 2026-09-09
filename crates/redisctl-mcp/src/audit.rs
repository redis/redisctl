//! Audit logging for MCP tool invocations.
//!
//! Provides a tower middleware layer that intercepts tool calls and emits
//! structured audit events via the `tracing` crate. Events are emitted with
//! `target: "audit"` so they can be routed to a dedicated JSON subscriber
//! separate from application logs.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tower::Service;
use tower_mcp::{McpRequest, McpResponse, RouterRequest, RouterResponse};

use crate::policy::ToolsetKind;

/// Audit logging level controlling which events are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AuditLevel {
    /// Log every tool call
    #[default]
    All,
    /// Log only non-read-only tool calls (writes + destructive + denied)
    Writes,
    /// Log only destructive tool calls (+ denied)
    Destructive,
    /// Log only policy-denied calls
    Denied,
}

/// Audit configuration, typically loaded from the `[audit]` section of the policy file.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[non_exhaustive]
pub struct AuditConfig {
    /// Master switch for audit logging
    pub enabled: bool,
    /// Which events to log
    pub level: AuditLevel,
    /// Whether to include tool call arguments in logs
    pub include_args: bool,
    /// Field names to redact from arguments when `include_args` is true
    pub redact_fields: Vec<String>,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: AuditLevel::All,
            include_args: false,
            redact_fields: vec![
                "password".to_string(),
                "api_key".to_string(),
                "api_secret".to_string(),
                "secret".to_string(),
            ],
        }
    }
}

/// Tower Layer that produces [`AuditService`] instances.
#[derive(Clone)]
pub struct AuditLayer {
    config: Arc<AuditConfig>,
    tool_toolset: Arc<HashMap<String, ToolsetKind>>,
}

impl AuditLayer {
    /// Create a new audit layer with the given config and tool-to-toolset mapping.
    pub fn new(config: Arc<AuditConfig>, tool_toolset: Arc<HashMap<String, ToolsetKind>>) -> Self {
        Self {
            config,
            tool_toolset,
        }
    }
}

impl<S> tower::Layer<S> for AuditLayer {
    type Service = AuditService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuditService {
            inner,
            config: self.config.clone(),
            tool_toolset: self.tool_toolset.clone(),
        }
    }
}

/// Tower Service that wraps the MCP router and emits audit events for tool calls.
#[derive(Clone)]
pub struct AuditService<S> {
    inner: S,
    config: Arc<AuditConfig>,
    tool_toolset: Arc<HashMap<String, ToolsetKind>>,
}

impl<S> Service<RouterRequest> for AuditService<S>
where
    S: Service<RouterRequest, Response = RouterResponse, Error = std::convert::Infallible>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
{
    type Response = RouterResponse;
    type Error = std::convert::Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: RouterRequest) -> Self::Future {
        // Check if this is a tool call
        let tool_call_info = match &req.inner {
            McpRequest::CallTool(params) => {
                let toolset = self
                    .tool_toolset
                    .get(&params.name)
                    .map(|k| k.to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let args = if self.config.include_args {
                    let redacted = redact_value(&params.arguments, &self.config.redact_fields);
                    Some(redacted.to_string())
                } else {
                    None
                };

                Some((params.name.clone(), toolset, args))
            }
            _ => None,
        };

        let config = self.config.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            if let Some((tool_name, toolset, args)) = tool_call_info {
                let start = Instant::now();
                let response = inner.call(req).await?;
                let duration_ms = start.elapsed().as_millis() as u64;

                // Determine result status
                let (event, result_status) = match &response.inner {
                    Ok(McpResponse::CallTool(_)) => ("tool_invocation", "success"),
                    Ok(_) => ("tool_invocation", "success"),
                    Err(err) if err.code == -32007 => ("tool_denied", "denied"),
                    Err(_) => ("tool_error", "error"),
                };

                // Check if we should log this event based on audit level
                if should_log(config.level, event, &toolset) {
                    if let Some(args) = args {
                        tracing::info!(
                            target: "audit",
                            event,
                            tool = %tool_name,
                            toolset = %toolset,
                            result = result_status,
                            duration_ms,
                            arguments = %args,
                        );
                    } else {
                        tracing::info!(
                            target: "audit",
                            event,
                            tool = %tool_name,
                            toolset = %toolset,
                            result = result_status,
                            duration_ms,
                        );
                    }
                }

                Ok(response)
            } else {
                // Non-tool-call request: pass through
                inner.call(req).await
            }
        })
    }
}

/// Determine if an audit event should be logged based on the configured level.
fn should_log(level: AuditLevel, event: &str, _toolset: &str) -> bool {
    match level {
        // All levels above Denied log every tool call, since the middleware can't
        // distinguish read vs write vs destructive without tool annotations.
        // The filtering is primarily useful for the Denied level which only logs failures.
        AuditLevel::All | AuditLevel::Writes | AuditLevel::Destructive => true,
        AuditLevel::Denied => event == "tool_denied" || event == "tool_error",
    }
}

/// Recursively redact sensitive fields from a JSON value.
///
/// Replaces the value of any object key matching `redact_fields` with `"[REDACTED]"`.
pub fn redact_value(value: &serde_json::Value, redact_fields: &[String]) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let redacted: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| {
                    if redact_fields.iter().any(|f| f == k) {
                        (
                            k.clone(),
                            serde_json::Value::String("[REDACTED]".to_string()),
                        )
                    } else {
                        (k.clone(), redact_value(v, redact_fields))
                    }
                })
                .collect();
            serde_json::Value::Object(redacted)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(|v| redact_value(v, redact_fields)).collect())
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- AuditConfig tests --

    #[test]
    fn default_config_is_disabled() {
        let config = AuditConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.level, AuditLevel::All);
        assert!(!config.include_args);
        assert!(!config.redact_fields.is_empty());
    }

    #[test]
    fn toml_minimal() {
        let config: AuditConfig = toml::from_str("enabled = true").unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, AuditLevel::All);
    }

    #[test]
    fn toml_full() {
        let toml_str = r#"
enabled = true
level = "denied"
include_args = true
redact_fields = ["password", "token"]
"#;
        let config: AuditConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.level, AuditLevel::Denied);
        assert!(config.include_args);
        assert_eq!(config.redact_fields, vec!["password", "token"]);
    }

    #[test]
    fn toml_empty_is_default() {
        let config: AuditConfig = toml::from_str("").unwrap();
        assert!(!config.enabled);
    }

    #[test]
    fn toml_roundtrip() {
        let config = AuditConfig {
            enabled: true,
            level: AuditLevel::Writes,
            include_args: true,
            redact_fields: vec!["secret".to_string()],
        };
        let s = toml::to_string_pretty(&config).unwrap();
        let parsed: AuditConfig = toml::from_str(&s).unwrap();
        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.level, config.level);
        assert_eq!(parsed.include_args, config.include_args);
        assert_eq!(parsed.redact_fields, config.redact_fields);
    }

    // -- Redaction tests --

    #[test]
    fn redact_top_level_fields() {
        let value = json!({
            "name": "my-db",
            "password": "secret123",
            "api_key": "ak_123"
        });
        let fields = vec!["password".to_string(), "api_key".to_string()];
        let redacted = redact_value(&value, &fields);

        assert_eq!(redacted["name"], "my-db");
        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["api_key"], "[REDACTED]");
    }

    #[test]
    fn redact_nested_fields() {
        let value = json!({
            "config": {
                "name": "test",
                "credentials": {
                    "password": "secret",
                    "username": "admin"
                }
            }
        });
        let fields = vec!["password".to_string()];
        let redacted = redact_value(&value, &fields);

        assert_eq!(redacted["config"]["name"], "test");
        assert_eq!(redacted["config"]["credentials"]["password"], "[REDACTED]");
        assert_eq!(redacted["config"]["credentials"]["username"], "admin");
    }

    #[test]
    fn redact_in_array() {
        let value = json!([
            {"name": "a", "secret": "s1"},
            {"name": "b", "secret": "s2"}
        ]);
        let fields = vec!["secret".to_string()];
        let redacted = redact_value(&value, &fields);

        assert_eq!(redacted[0]["name"], "a");
        assert_eq!(redacted[0]["secret"], "[REDACTED]");
        assert_eq!(redacted[1]["secret"], "[REDACTED]");
    }

    #[test]
    fn redact_no_matching_fields() {
        let value = json!({"name": "test", "count": 42});
        let fields = vec!["password".to_string()];
        let redacted = redact_value(&value, &fields);
        assert_eq!(redacted, value);
    }

    #[test]
    fn redact_scalar_passthrough() {
        let value = json!("just a string");
        let fields = vec!["password".to_string()];
        let redacted = redact_value(&value, &fields);
        assert_eq!(redacted, value);
    }

    // -- should_log tests --

    #[test]
    fn all_level_logs_everything() {
        assert!(should_log(AuditLevel::All, "tool_invocation", "cloud"));
        assert!(should_log(AuditLevel::All, "tool_denied", "cloud"));
        assert!(should_log(AuditLevel::All, "tool_error", "cloud"));
    }

    #[test]
    fn denied_level_only_logs_denied_and_errors() {
        assert!(!should_log(AuditLevel::Denied, "tool_invocation", "cloud"));
        assert!(should_log(AuditLevel::Denied, "tool_denied", "cloud"));
        assert!(should_log(AuditLevel::Denied, "tool_error", "cloud"));
    }
}
