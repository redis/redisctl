//! Raw Redis command passthrough tool

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use tower_mcp::extract::{Json, State};
use tower_mcp::{CallToolResult, Error as McpError, McpRouter, Tool, ToolBuilder};

use crate::policy::ToolSafety;
use crate::state::AppState;

/// Input for the redis_command tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct RedisCommandInput {
    /// Redis command name (e.g., "GET", "HGETALL", "CLIENT")
    pub command: String,
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional Redis URL (overrides profile)
    #[serde(default)]
    pub url: Option<String>,
    /// Optional profile name to resolve connection from
    #[serde(default)]
    pub profile: Option<String>,
    /// If true, return what would be sent without executing the command
    #[serde(default)]
    pub dry_run: bool,
}

/// Build the redis_command tool.
pub fn redis_command(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("redis_command")
        .description(
            "DANGEROUS: Execute an arbitrary Redis command. \
             Escape hatch for commands not covered by dedicated tools. \
             Certain dangerous commands and subcommands are blocked.",
        )
        .destructive()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>, Json(input): Json<RedisCommandInput>| async move {
                let command = super::command_safety::preflight_command(
                    &state,
                    "redis_command",
                    ToolSafety::Destructive,
                    ToolSafety::ReadOnly,
                    &input.command,
                    &input.args,
                )?;

                // Dry run: return preview
                if input.dry_run {
                    let preview = serde_json::json!({
                        "dry_run": true,
                        "command": command,
                        "args": input.args,
                        "url": input.url,
                        "profile": input.profile,
                    });
                    return CallToolResult::from_serialize(&preview);
                }

                let mut conn =
                    super::get_connection(input.url, input.profile.as_deref(), &state).await?;

                let mut cmd = redis::cmd(&command);
                for arg in &input.args {
                    cmd.arg(arg);
                }

                let result: redis::Value = cmd
                    .query_async(&mut conn)
                    .await
                    .map_err(|e| McpError::tool(format!("command failed: {e}")))?;

                Ok(CallToolResult::text(super::format_value(&result)))
            },
        )
        .build()
}

/// All tool names registered by this sub-module.
pub(super) const TOOL_NAMES: &[&str] = &["redis_command"];

/// Build a sub-router containing the raw Redis command tool.
pub fn router(state: Arc<AppState>) -> McpRouter {
    McpRouter::new().tool(redis_command(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    use crate::policy::{Policy, PolicyConfig, SafetyTier, ToolsetKind};
    use crate::state::CredentialSource;
    use serde_json::json;

    fn state(config: PolicyConfig) -> Arc<AppState> {
        Arc::new(
            AppState::new(
                CredentialSource::Profiles(Vec::new()),
                Arc::new(Policy::new(
                    config,
                    HashMap::from([("redis_command".to_string(), ToolsetKind::Database)]),
                    "raw-command-test".to_string(),
                )),
                None,
                false,
                None,
            )
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn explicit_allow_exposes_raw_reads_without_raising_the_dynamic_ceiling() {
        let config = PolicyConfig {
            tier: SafetyTier::ReadOnly,
            allow: vec!["redis_command".to_string()],
            ..PolicyConfig::default()
        };
        let tool = redis_command(state(config));

        let read = tool
            .call(json!({"command": " get ", "args": ["key"], "dry_run": true}))
            .await;
        assert!(!read.is_error, "{:?}", read.content);

        for command in ["SET", "DEL"] {
            let denied = tool
                .call(json!({
                    "command": command,
                    "args": ["key", "value"],
                    "dry_run": true
                }))
                .await;
            assert!(denied.is_error, "{command} must remain above read-only");
            assert!(denied.first_text().is_some_and(|text| {
                text.contains("not allowed for this command by the active policy")
            }));
        }
    }
}
