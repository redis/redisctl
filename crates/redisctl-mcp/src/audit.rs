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
use tower_mcp::{
    CallToolResult, McpRequest, McpResponse, RouterRequest, RouterResponse, ToolAnnotationsMap,
};

use crate::policy::{ToolSafety, ToolsetKind, is_policy_denied_message};

/// Audit logging level controlling which events are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AuditLevel {
    /// Log every tool call
    #[default]
    All,
    /// Log only non-read-only tool calls (writes + destructive + denied)
    #[serde(alias = "mutations")]
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
                "db_password".to_string(),
                "global_password".to_string(),
                "api_key".to_string(),
                "api_secret".to_string(),
                "access_secret_key".to_string(),
                "console_password".to_string(),
                "bind_pass".to_string(),
                "license_key".to_string(),
                "secret".to_string(),
                "token".to_string(),
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
                    let redacted = redact_arguments(
                        &params.name,
                        &params.arguments,
                        &self.config.redact_fields,
                    );
                    Some(redacted.to_string())
                } else {
                    None
                };

                let annotations = req.extensions.get::<ToolAnnotationsMap>();
                let annotated_safety = annotations
                    .map(|annotations| {
                        ToolSafety::from_hints(
                            annotations.is_read_only(&params.name),
                            annotations.is_destructive(&params.name),
                        )
                    })
                    .unwrap_or(ToolSafety::Destructive);
                let safety = audit_safety(&params.name, &params.arguments, annotated_safety);

                Some((params.name.clone(), toolset, args, safety))
            }
            _ => None,
        };

        let config = self.config.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            if let Some((tool_name, toolset, args, safety)) = tool_call_info {
                let start = Instant::now();
                let response = inner.call(req).await?;
                let duration_ms = start.elapsed().as_millis() as u64;

                // Determine result status
                let (event, result_status) = match &response.inner {
                    Ok(McpResponse::CallTool(result))
                        if result.is_error && is_policy_denied_result(result) =>
                    {
                        ("tool_denied", "denied")
                    }
                    Ok(McpResponse::CallTool(result)) if result.is_error => ("tool_error", "error"),
                    Ok(McpResponse::CallTool(_)) => ("tool_invocation", "success"),
                    Ok(_) => ("tool_invocation", "success"),
                    Err(err) if err.code == -32007 => ("tool_denied", "denied"),
                    Err(_) => ("tool_error", "error"),
                };

                // Check if we should log this event based on audit level
                if should_log(config.level, event, safety) {
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

fn is_policy_denied_result(result: &CallToolResult) -> bool {
    result.first_text().is_some_and(|message| {
        message
            .char_indices()
            .any(|(index, _)| is_policy_denied_message(&message[index..]))
    })
}

fn audit_safety(
    tool_name: &str,
    arguments: &serde_json::Value,
    annotated_safety: ToolSafety,
) -> ToolSafety {
    #[cfg(not(feature = "database"))]
    let _ = arguments;

    match tool_name {
        #[cfg(feature = "database")]
        "redis_command" => classify_redis_command(arguments),
        #[cfg(feature = "database")]
        "redis_bulk_load" => classify_redis_command_batch(arguments),
        #[cfg(feature = "database")]
        "redis_alias_set" => annotated_safety.max(classify_redis_command_batch(arguments)),
        // The stored commands are not present in this invocation, so the audit
        // layer cannot classify them without reaching into application state.
        "redis_alias_run" => ToolSafety::Destructive,
        _ => annotated_safety,
    }
}

#[cfg(feature = "database")]
fn classify_redis_command(arguments: &serde_json::Value) -> ToolSafety {
    let Some(arguments) = arguments.as_object() else {
        return ToolSafety::Destructive;
    };
    let Some(command) = arguments.get("command").and_then(serde_json::Value::as_str) else {
        return ToolSafety::Destructive;
    };
    let Some(args) = optional_string_array(arguments.get("args")) else {
        return ToolSafety::Destructive;
    };

    crate::tools::redis::command_safety::classify_command_safety(command, &args)
        .unwrap_or(ToolSafety::Destructive)
}

#[cfg(feature = "database")]
fn classify_redis_command_batch(arguments: &serde_json::Value) -> ToolSafety {
    let Some(commands) = arguments
        .as_object()
        .and_then(|arguments| arguments.get("commands"))
        .and_then(serde_json::Value::as_array)
    else {
        return ToolSafety::Destructive;
    };

    let Some(commands) = commands
        .iter()
        .map(|command| {
            command
                .as_object()
                .and_then(|command| command.get("args"))
                .and_then(required_string_array)
        })
        .collect::<Option<Vec<_>>>()
    else {
        return ToolSafety::Destructive;
    };

    crate::tools::redis::command_safety::classify_batch_safety(commands.iter().map(Vec::as_slice))
        .unwrap_or(ToolSafety::Destructive)
}

#[cfg(feature = "database")]
fn optional_string_array(value: Option<&serde_json::Value>) -> Option<Vec<String>> {
    match value {
        None => Some(Vec::new()),
        Some(value) => required_string_array(value),
    }
}

#[cfg(feature = "database")]
fn required_string_array(value: &serde_json::Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|value| value.as_str().map(ToOwned::to_owned))
        .collect()
}

/// Determine if an audit event should be logged based on the configured level.
fn should_log(level: AuditLevel, event: &str, safety: ToolSafety) -> bool {
    if event == "tool_denied" {
        return true;
    }

    match level {
        AuditLevel::All => true,
        AuditLevel::Writes => safety != ToolSafety::ReadOnly,
        AuditLevel::Destructive => safety == ToolSafety::Destructive,
        AuditLevel::Denied => false,
    }
}

const ALWAYS_REDACT_FIELDS: &[&str] = &[
    "accesskeyid",
    "accesssecretkey",
    "apikey",
    "apisecret",
    "bindpass",
    "clientsecret",
    "consolepassword",
    "credential",
    "dbpassword",
    "globalpassword",
    "licensekey",
    "password",
    "privatekey",
    "refreshtoken",
    "secret",
    "secretkey",
    "token",
    "accesstoken",
];

fn normalize_field_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn is_sensitive_field(name: &str, redact_fields: &[String]) -> bool {
    let normalized = normalize_field_name(name);
    ALWAYS_REDACT_FIELDS.contains(&normalized.as_str())
        || redact_fields
            .iter()
            .any(|field| normalize_field_name(field) == normalized)
}

fn sanitize_redis_url(value: &str) -> String {
    let trimmed = value.trim();
    let lowercase = trimmed.to_ascii_lowercase();
    let supported = ["redis://", "rediss://", "redis+unix://", "unix://"]
        .iter()
        .any(|prefix| lowercase.starts_with(prefix));
    if !supported {
        return value.to_string();
    }

    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return "[REDACTED]".to_string();
    };

    let mut changed = false;
    if !parsed.username().is_empty() {
        if parsed.set_username("redacted").is_err() {
            return "[REDACTED]".to_string();
        }
        changed = true;
    }
    if parsed.password().is_some() {
        if parsed.set_password(Some("redacted")).is_err() {
            return "[REDACTED]".to_string();
        }
        changed = true;
    }
    if parsed.fragment().is_some() {
        parsed.set_fragment(Some("redacted"));
        changed = true;
    }

    let query = parsed
        .query_pairs()
        .map(|(key, value)| {
            let sensitive = is_sensitive_field(&key, &[])
                || matches!(
                    normalize_field_name(&key).as_str(),
                    "pass" | "user" | "username"
                );
            if sensitive {
                changed = true;
                (key.into_owned(), "redacted".to_string())
            } else {
                (key.into_owned(), value.into_owned())
            }
        })
        .collect::<Vec<_>>();
    if changed && parsed.query().is_some() {
        parsed.query_pairs_mut().clear().extend_pairs(
            query
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
    }

    if changed {
        parsed.to_string()
    } else {
        value.to_string()
    }
}

fn safe_command_identity(value: &serde_json::Value) -> serde_json::Value {
    let Some(command) = value.as_str() else {
        return serde_json::Value::String("[REDACTED]".to_string());
    };
    let command = command.trim();
    if command.is_empty() || command.chars().any(char::is_whitespace) {
        serde_json::Value::String("[REDACTED]".to_string())
    } else {
        serde_json::Value::String(command.to_ascii_uppercase())
    }
}

fn redact_array_operands(array: &mut [serde_json::Value], command_is_first: bool) {
    let first_operand = usize::from(command_is_first);
    if command_is_first && !array.is_empty() {
        array[0] = safe_command_identity(&array[0]);
    }
    for value in array.iter_mut().skip(first_operand) {
        *value = serde_json::Value::String("[REDACTED]".to_string());
    }
}

fn redacted_marker() -> serde_json::Value {
    serde_json::Value::String("[REDACTED]".to_string())
}

fn redact_packed_command(command: &mut serde_json::Value) {
    let serde_json::Value::Object(command) = command else {
        *command = redacted_marker();
        return;
    };

    match command.get_mut("args") {
        Some(serde_json::Value::Array(values)) => redact_array_operands(values, true),
        Some(args) => *args = redacted_marker(),
        None => {
            // The entry cannot be interpreted as a packed command. Remove all
            // of it rather than retaining an unknown field that may be an
            // incorrectly-shaped command operand.
            command.clear();
            command.insert("args".to_string(), redacted_marker());
        }
    }
}

fn redact_arguments(
    tool_name: &str,
    value: &serde_json::Value,
    redact_fields: &[String],
) -> serde_json::Value {
    let mut redacted = redact_value(value, redact_fields);
    let Some(arguments) = redacted.as_object_mut() else {
        return if matches!(
            tool_name,
            "redis_command" | "redis_bulk_load" | "redis_alias_set"
        ) {
            redacted_marker()
        } else {
            redacted
        };
    };

    match tool_name {
        "update_enterprise_cluster_certificates" => {
            // This tool's private-key argument is named generically `key`, so
            // it cannot be part of the global field-name denylist without
            // hiding ordinary Redis keys and Cloud tag keys.
            if let Some(key) = arguments.get_mut("key") {
                *key = redacted_marker();
            }
        }
        "redis_command" => {
            if let Some(command) = arguments.get_mut("command") {
                *command = safe_command_identity(command);
            }
            match arguments.get_mut("args") {
                Some(serde_json::Value::Array(values)) => redact_array_operands(values, false),
                Some(args) => *args = redacted_marker(),
                None => {}
            }
        }
        "redis_bulk_load" | "redis_alias_set" => match arguments.get_mut("commands") {
            Some(serde_json::Value::Array(commands)) => {
                for command in commands {
                    redact_packed_command(command);
                }
            }
            Some(commands) => *commands = redacted_marker(),
            None => {}
        },
        _ => {}
    }

    redacted
}

/// Recursively redact sensitive fields from a JSON value.
///
/// Replaces built-in credential fields and any object key matching
/// `redact_fields` with `"[REDACTED]"`. Redis URL credentials are sanitized
/// recursively even when their containing field has another name.
pub fn redact_value(value: &serde_json::Value, redact_fields: &[String]) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let redacted: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| {
                    if is_sensitive_field(k, redact_fields) {
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
        serde_json::Value::String(value) => serde_json::Value::String(sanitize_redis_url(value)),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::Mutex;

    use serde_json::json;
    use tower::Layer;
    use tower_mcp::router::Extensions;
    use tower_mcp::transport::InjectAnnotations;
    use tower_mcp::{
        CallToolParams, ClientCapabilities, Implementation, InitializeParams, McpNotification,
        McpRouter, RequestId, Tool, ToolBuilder,
    };
    use tracing::instrument::WithSubscriber;

    #[derive(Clone, Default)]
    struct LogCapture(Arc<Mutex<Vec<u8>>>);

    struct LogWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for LogWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("audit log capture mutex poisoned")
                .extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for LogCapture {
        type Writer = LogWriter;

        fn make_writer(&'writer self) -> Self::Writer {
            LogWriter(self.0.clone())
        }
    }

    impl LogCapture {
        fn contents(&self) -> String {
            String::from_utf8(
                self.0
                    .lock()
                    .expect("audit log capture mutex poisoned")
                    .clone(),
            )
            .expect("audit logs should be UTF-8")
        }
    }

    async fn initialize_router(router: &mut McpRouter) {
        let request = RouterRequest {
            id: RequestId::Number(0),
            inner: McpRequest::Initialize(InitializeParams {
                protocol_version: "2025-11-25".to_string(),
                capabilities: ClientCapabilities {
                    roots: None,
                    sampling: None,
                    elicitation: None,
                    tasks: None,
                    experimental: None,
                    extensions: None,
                },
                client_info: Implementation {
                    name: "audit-test".to_string(),
                    version: "1.0".to_string(),
                    ..Default::default()
                },
                meta: None,
            }),
            extensions: Extensions::new(),
        };
        Service::call(router, request).await.unwrap();
        router.handle_notification(McpNotification::Initialized);
    }

    async fn run_audited_tool(
        tool: Tool,
        level: AuditLevel,
        include_args: bool,
        calls: Vec<serde_json::Value>,
    ) -> (Vec<RouterResponse>, String) {
        let tool_name = tool.name.clone();
        let mut router = McpRouter::new().tool(tool);
        initialize_router(&mut router).await;
        let annotations = router.tool_annotations_map();
        let config = Arc::new(AuditConfig {
            enabled: true,
            level,
            include_args,
            ..Default::default()
        });
        let toolsets = Arc::new(HashMap::from([(tool_name.clone(), ToolsetKind::Database)]));
        let audited = AuditLayer::new(config, toolsets).layer(router);
        let mut service = InjectAnnotations::new(audited, annotations);

        let capture = LogCapture::default();
        let subscriber = tracing_subscriber::fmt()
            .json()
            .without_time()
            .with_ansi(false)
            .with_target(true)
            .with_writer(capture.clone())
            .finish();

        let responses = async move {
            let mut responses = Vec::with_capacity(calls.len());
            for (index, arguments) in calls.into_iter().enumerate() {
                let request = RouterRequest {
                    id: RequestId::Number(index as i64 + 1),
                    inner: McpRequest::CallTool(CallToolParams {
                        name: tool_name.clone(),
                        arguments,
                        input_responses: None,
                        request_state: None,
                        meta: None,
                        task: None,
                    }),
                    extensions: Extensions::new(),
                };
                responses.push(Service::call(&mut service, request).await.unwrap());
            }
            responses
        }
        .with_subscriber(subscriber)
        .await;

        (responses, capture.contents())
    }

    fn audit_records(logs: &str) -> Vec<serde_json::Value> {
        logs.lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .filter(|record: &serde_json::Value| record["target"] == "audit")
            .collect()
    }

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

    #[test]
    fn legacy_mutations_level_parses_as_writes() {
        let config: AuditConfig = toml::from_str("level = \"mutations\"").unwrap();
        assert_eq!(config.level, AuditLevel::Writes);
        assert!(
            toml::to_string(&config)
                .unwrap()
                .contains("level = \"writes\"")
        );
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

    #[test]
    fn mandatory_fields_are_case_and_separator_insensitive() {
        let value = json!({
            "dbPassword": "one",
            "access-secret-key": "two",
            "bind_pass": "three",
            "licenseKey": "four",
            "safe": "visible"
        });
        let serialized = redact_value(&value, &[]).to_string();
        for secret in ["one", "two", "three", "four"] {
            assert!(!serialized.contains(secret));
        }
        assert_eq!(redact_value(&value, &[])["safe"], "visible");
    }

    #[test]
    fn redis_url_credentials_are_sanitized() {
        for value in [
            "redis://default:marker-one@localhost:6379/2?keep=yes",
            "rediss://:marker-two@[::1]:6380/0",
        ] {
            let sanitized = sanitize_redis_url(value);
            assert!(!sanitized.contains("marker"));
            assert!(sanitized.contains("redacted"));
            assert!(sanitized.contains("localhost") || sanitized.contains("[::1]"));
        }

        assert_eq!(
            sanitize_redis_url("redis://localhost:6379/0"),
            "redis://localhost:6379/0"
        );
    }

    #[test]
    fn unix_url_query_credentials_are_sanitized() {
        let value = "redis+unix:///tmp/redis.sock?db=2&pass=marker-one&user=marker-user&token=marker-token#marker-fragment";
        let sanitized = sanitize_redis_url(value);
        assert!(!sanitized.contains("marker"));
        assert!(sanitized.contains("db=2"));
        let parsed = url::Url::parse(&sanitized).unwrap();
        let secret_values = parsed
            .query_pairs()
            .filter(|(key, _)| matches!(key.as_ref(), "pass" | "user" | "token"))
            .map(|(_, value)| value.into_owned())
            .collect::<Vec<_>>();
        assert_eq!(secret_values, vec!["redacted", "redacted", "redacted"]);
        assert_eq!(parsed.fragment(), Some("redacted"));
    }

    #[test]
    fn malformed_supported_url_fails_closed() {
        assert_eq!(sanitize_redis_url("redis://["), "[REDACTED]");
    }

    #[test]
    fn arbitrary_command_operands_are_always_redacted() {
        let raw = redact_arguments(
            "redis_command",
            &json!({
                "command": " auth ",
                "args": ["marker-user", "marker-password"],
                "url": "redis://default:marker-url@localhost"
            }),
            &[],
        );
        assert_eq!(raw["command"], "AUTH");
        assert_eq!(raw["args"], json!(["[REDACTED]", "[REDACTED]"]));
        assert!(!raw.to_string().contains("marker"));

        for (tool, command) in [
            (
                "redis_bulk_load",
                json!({"commands": [{"args": ["HELLO", "3", "AUTH", "marker-user", "marker-password"]}]}),
            ),
            (
                "redis_alias_set",
                json!({"commands": [{"args": ["MIGRATE", "host", "6379", "key", "0", "5000", "AUTH2", "marker-user", "marker-password"]}]}),
            ),
        ] {
            let redacted = redact_arguments(tool, &command, &[]);
            assert!(!redacted.to_string().contains("marker"));
            assert_ne!(redacted["commands"][0]["args"][0], "[REDACTED]");
            assert!(
                redacted["commands"][0]["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .skip(1)
                    .all(|value| value == "[REDACTED]")
            );
        }
    }

    #[test]
    fn malformed_command_operands_fail_closed() {
        let raw = redact_arguments(
            "redis_command",
            &json!({"command": "AUTH", "args": "marker-raw"}),
            &[],
        );
        assert_eq!(raw["args"], "[REDACTED]");
        assert!(!raw.to_string().contains("marker-raw"));

        for tool in ["redis_bulk_load", "redis_alias_set"] {
            let redacted = redact_arguments(
                tool,
                &json!({
                    "commands": [
                        {"args": "marker-one"},
                        "marker-two",
                        {"unexpected": "marker-three"}
                    ]
                }),
                &[],
            );
            let serialized = redacted.to_string();
            for marker in ["marker-one", "marker-two", "marker-three"] {
                assert!(!serialized.contains(marker), "{tool}: {serialized}");
            }
        }

        assert_eq!(
            redact_arguments("redis_command", &json!("marker-top-level"), &[]),
            "[REDACTED]"
        );
    }

    // -- should_log tests --

    #[test]
    fn all_level_logs_everything() {
        for safety in [
            ToolSafety::ReadOnly,
            ToolSafety::Write,
            ToolSafety::Destructive,
        ] {
            assert!(should_log(AuditLevel::All, "tool_invocation", safety));
            assert!(should_log(AuditLevel::All, "tool_error", safety));
        }
    }

    #[test]
    fn mutation_levels_follow_tool_safety() {
        for event in ["tool_invocation", "tool_error"] {
            assert!(!should_log(AuditLevel::Writes, event, ToolSafety::ReadOnly));
            assert!(should_log(AuditLevel::Writes, event, ToolSafety::Write));
            assert!(should_log(
                AuditLevel::Writes,
                event,
                ToolSafety::Destructive
            ));
            assert!(!should_log(
                AuditLevel::Destructive,
                event,
                ToolSafety::ReadOnly
            ));
            assert!(!should_log(
                AuditLevel::Destructive,
                event,
                ToolSafety::Write
            ));
            assert!(should_log(
                AuditLevel::Destructive,
                event,
                ToolSafety::Destructive
            ));
            assert!(!should_log(
                AuditLevel::Denied,
                event,
                ToolSafety::Destructive
            ));
        }
    }

    #[test]
    fn policy_denials_are_logged_at_every_level() {
        for level in [
            AuditLevel::All,
            AuditLevel::Writes,
            AuditLevel::Destructive,
            AuditLevel::Denied,
        ] {
            assert!(should_log(level, "tool_denied", ToolSafety::ReadOnly));
        }
    }

    #[tokio::test]
    async fn serialized_audit_records_never_contain_command_secrets() {
        let raw_tool = ToolBuilder::new("redis_command")
            .description("audit fixture")
            .destructive()
            .handler(|_: serde_json::Value| async {
                Err(tower_mcp::Error::tool("invalid raw arguments"))
            })
            .build();
        let (_, raw_logs) = run_audited_tool(
            raw_tool,
            AuditLevel::All,
            true,
            vec![json!({
                "command": "AUTH",
                "args": "marker-raw-password",
                "url": "redis://default:marker-url-password@localhost:6379"
            })],
        )
        .await;
        assert!(!raw_logs.contains("marker-raw-password"), "{raw_logs}");
        assert!(!raw_logs.contains("marker-url-password"), "{raw_logs}");
        let raw_records = audit_records(&raw_logs);
        assert_eq!(raw_records.len(), 1, "{raw_logs}");
        let raw_arguments: serde_json::Value = serde_json::from_str(
            raw_records[0]["fields"]["arguments"]
                .as_str()
                .expect("serialized audit arguments"),
        )
        .unwrap();
        assert_eq!(raw_arguments["command"], "AUTH");
        assert_eq!(raw_arguments["args"], "[REDACTED]");

        let bulk_tool = ToolBuilder::new("redis_bulk_load")
            .description("audit fixture")
            .non_destructive()
            .handler(|_: serde_json::Value| async {
                Err(tower_mcp::Error::tool("invalid bulk arguments"))
            })
            .build();
        let (_, bulk_logs) = run_audited_tool(
            bulk_tool,
            AuditLevel::All,
            true,
            vec![json!({
                "commands": [
                    {"args": "marker-bulk-one"},
                    "marker-bulk-two",
                    {"unexpected": "marker-bulk-three"}
                ]
            })],
        )
        .await;
        for marker in ["marker-bulk-one", "marker-bulk-two", "marker-bulk-three"] {
            assert!(!bulk_logs.contains(marker), "{bulk_logs}");
        }
        assert_eq!(audit_records(&bulk_logs).len(), 1, "{bulk_logs}");
    }

    #[tokio::test]
    async fn serialized_audit_records_redact_enterprise_certificate_private_keys() {
        let tool = ToolBuilder::new("update_enterprise_cluster_certificates")
            .description("audit fixture")
            .non_destructive()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build();
        let (_, logs) = run_audited_tool(
            tool,
            AuditLevel::All,
            true,
            vec![json!({
                "name": "api",
                "certificate": "marker-public-certificate",
                "key": "marker-private-key"
            })],
        )
        .await;

        assert!(!logs.contains("marker-private-key"), "{logs}");
        let records = audit_records(&logs);
        assert_eq!(records.len(), 1, "{logs}");
        let arguments: serde_json::Value = serde_json::from_str(
            records[0]["fields"]["arguments"]
                .as_str()
                .expect("serialized audit arguments"),
        )
        .unwrap();
        assert_eq!(arguments["key"], "[REDACTED]");
        assert_eq!(arguments["certificate"], "marker-public-certificate");
    }

    #[tokio::test]
    async fn denied_level_logs_handler_policy_denials() {
        let tool = ToolBuilder::new("redis_bulk_load")
            .description("audit fixture")
            .non_destructive()
            .handler(|_: serde_json::Value| async {
                Err(crate::policy::policy_denied_for_command("redis_bulk_load"))
            })
            .build();
        let (responses, logs) = run_audited_tool(
            tool,
            AuditLevel::Denied,
            false,
            vec![json!({"commands": [{"args": ["DEL", "key"]}]})],
        )
        .await;

        let Ok(McpResponse::CallTool(result)) = &responses[0].inner else {
            panic!("expected a tool result: {:?}", responses[0]);
        };
        assert!(result.is_error);
        let records = audit_records(&logs);
        assert_eq!(records.len(), 1, "{logs}");
        assert_eq!(records[0]["fields"]["event"], "tool_denied");
        assert_eq!(records[0]["fields"]["result"], "denied");
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn destructive_level_uses_dynamic_raw_command_safety() {
        let tool = ToolBuilder::new("redis_command")
            .description("audit fixture")
            .destructive()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build();
        let (_, logs) = run_audited_tool(
            tool,
            AuditLevel::Destructive,
            false,
            vec![
                json!({"command": "GET", "args": ["key"]}),
                json!({"command": "DEL", "args": ["key"]}),
            ],
        )
        .await;

        let records = audit_records(&logs);
        assert_eq!(records.len(), 1, "{logs}");
        assert_eq!(records[0]["fields"]["tool"], "redis_command");
    }

    #[cfg(feature = "database")]
    #[tokio::test]
    async fn destructive_level_uses_dynamic_batch_safety() {
        for tool_name in ["redis_bulk_load", "redis_alias_set"] {
            let builder = ToolBuilder::new(tool_name).description("audit fixture");
            let builder = if tool_name == "redis_bulk_load" {
                builder.destructive()
            } else {
                builder.non_destructive()
            };
            let tool = builder
                .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
                .build();
            let (_, logs) = run_audited_tool(
                tool,
                AuditLevel::Destructive,
                false,
                vec![
                    json!({"commands": [{"args": ["SET", "key", "value"]}]}),
                    json!({"commands": [{"args": ["DEL", "key"]}]}),
                ],
            )
            .await;

            let records = audit_records(&logs);
            assert_eq!(records.len(), 1, "{tool_name}: {logs}");
            assert_eq!(records[0]["fields"]["tool"], tool_name);
        }
    }

    #[tokio::test]
    async fn destructive_level_conservatively_logs_alias_runs() {
        let tool = ToolBuilder::new("redis_alias_run")
            .description("audit fixture")
            .non_destructive()
            .handler(|_: serde_json::Value| async { Ok(CallToolResult::text("ok")) })
            .build();
        let (_, logs) = run_audited_tool(
            tool,
            AuditLevel::Destructive,
            false,
            vec![json!({"name": "fixture"})],
        )
        .await;

        let records = audit_records(&logs);
        assert_eq!(records.len(), 1, "{logs}");
        assert_eq!(records[0]["fields"]["tool"], "redis_alias_run");
    }
}
