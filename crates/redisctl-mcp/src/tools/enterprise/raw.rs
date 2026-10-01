//! Raw API passthrough tool for Redis Enterprise

use std::sync::Arc;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use tower_mcp::extract::{Json, State};
use tower_mcp::{CallToolResult, Error as McpError, McpRouter, ResultExt, Tool, ToolBuilder};

use crate::policy::ToolSafety;
use crate::state::AppState;

/// HTTP method for the raw API call.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Input for the enterprise_raw_api tool.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EnterpriseRawApiInput {
    /// HTTP method (GET, POST, PUT, PATCH, DELETE)
    pub method: HttpMethod,
    /// API path (e.g., "/v1/bdbs/1"). Must start with "/".
    pub path: String,
    /// Optional JSON request body (required for POST, PUT, PATCH)
    #[serde(default)]
    pub body: Option<Value>,
    /// Profile name for multi-cluster support
    #[serde(default)]
    pub profile: Option<String>,
    /// If true, return what would be sent without executing the request
    #[serde(default)]
    pub dry_run: bool,
}

/// Build the enterprise_raw_api tool.
pub fn enterprise_raw_api(state: Arc<AppState>) -> Tool {
    ToolBuilder::new("enterprise_raw_api")
        .description(
            "DANGEROUS: Execute a raw REST API request against the Enterprise API. \
             Escape hatch for endpoints not covered by dedicated tools.",
        )
        .destructive()
        .extractor_handler(
            state,
            |State(state): State<Arc<AppState>>,
             Json(input): Json<EnterpriseRawApiInput>| async move {
                if !state.is_tool_allowed("enterprise_raw_api", ToolSafety::Destructive) {
                    return Err(crate::policy::policy_denied("enterprise_raw_api"));
                }

                // Validate path starts with /
                if !input.path.starts_with('/') {
                    return Err(McpError::tool("path must start with '/'"));
                }

                // Dry run: return preview
                if input.dry_run {
                    let preview = serde_json::json!({
                        "dry_run": true,
                        "method": format!("{:?}", input.method).to_uppercase(),
                        "path": input.path,
                        "body": input.body,
                        "profile": input.profile,
                    });
                    return CallToolResult::from_serialize(&preview);
                }

                let client = state
                    .enterprise_client_for_profile(input.profile.as_deref())
                    .await
                    .map_err(|e| crate::tools::credential_error("enterprise", e))?;

                let result: Value = match input.method {
                    HttpMethod::Get => client
                        .get_raw(&input.path)
                        .await
                        .tool_context("enterprise_raw_api GET failed")?,
                    HttpMethod::Post => client
                        .post_raw(
                            &input.path,
                            input.body.unwrap_or(Value::Object(Default::default())),
                        )
                        .await
                        .tool_context("enterprise_raw_api POST failed")?,
                    HttpMethod::Put => client
                        .put_raw(
                            &input.path,
                            input.body.unwrap_or(Value::Object(Default::default())),
                        )
                        .await
                        .tool_context("enterprise_raw_api PUT failed")?,
                    HttpMethod::Patch => client
                        .patch_raw(
                            &input.path,
                            input.body.unwrap_or(Value::Object(Default::default())),
                        )
                        .await
                        .tool_context("enterprise_raw_api PATCH failed")?,
                    HttpMethod::Delete => client
                        .delete_raw(&input.path)
                        .await
                        .tool_context("enterprise_raw_api DELETE failed")?,
                };

                CallToolResult::from_serialize(&result)
            },
        )
        .build()
}

/// All tool names registered by this sub-module.
pub(super) const TOOL_NAMES: &[&str] = &["enterprise_raw_api"];

/// Build a sub-router containing the raw Enterprise API tool.
pub fn router(state: Arc<AppState>) -> McpRouter {
    McpRouter::new().tool(enterprise_raw_api(state))
}
