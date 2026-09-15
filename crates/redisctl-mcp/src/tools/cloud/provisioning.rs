//! Agent-native provisioning tools: cloud auth status + quick-database.
//!
//! Both reuse the shared engine in `redisctl_core::cloud::quick_database` — the same code the
//! CLI runs — so there is no logic duplication and no shelling out. The connection string /
//! password are written only to the credentials file; the tool response carries non-secret
//! metadata only (`QuickDatabaseReport`).

use redisctl_core::cloud::quick_database::{QuickDatabaseParams, provision};
use tower_mcp::CallToolResult;

use crate::tools::macros::{cloud_tool, mcp_module};

mcp_module! {
    cloud_auth_status => "cloud_auth_status",
    cloud_quick_database => "cloud_quick_database",
}

/// Input for `cloud_auth_status`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CloudAuthStatusInput {
    /// Profile name (uses the default / first configured profile if omitted).
    #[serde(default)]
    pub profile: Option<String>,
}

/// `cloud_auth_status` is written by hand (not via `cloud_tool!`) because "not authenticated"
/// is a valid *result*, not an error — the macro would turn a missing credential into a tool
/// failure. Read-only; never returns tokens.
pub fn cloud_auth_status(state: std::sync::Arc<crate::state::AppState>) -> tower_mcp::Tool {
    tower_mcp::ToolBuilder::new("cloud_auth_status")
        .description(
            "Report whether a Redis Cloud profile has credentials configured. Returns \
             {authenticated, profile}. This is an offline check (credentials resolve and a \
             client can be built) — it does not call the API to verify they still work. Never \
             returns tokens or secrets. If authenticated is false, run `redisctl cloud auth \
             login` in a shell to sign in.",
        )
        .read_only_safe()
        .extractor_handler(
            state,
            |tower_mcp::extract::State(state): tower_mcp::extract::State<
                std::sync::Arc<crate::state::AppState>,
            >,
             tower_mcp::extract::Json(input): tower_mcp::extract::Json<CloudAuthStatusInput>| async move {
                // Building the client resolves credentials (offline); success ⇒ authenticated.
                let authenticated = state
                    .cloud_client_for_profile(input.profile.as_deref())
                    .await
                    .is_ok();
                CallToolResult::from_serialize(&serde_json::json!({
                    "authenticated": authenticated,
                    "profile": input.profile,
                    "hint": if authenticated {
                        serde_json::Value::Null
                    } else {
                        serde_json::json!("run `redisctl cloud auth login`")
                    },
                }))
            },
        )
        .build()
}

/// Any directory is fine; the name has to be an env file, so credentials cannot be aimed at a
/// shell profile or an ssh config.
fn credentials_path_is_an_env_file(path: &str) -> Result<std::path::PathBuf, tower_mcp::Error> {
    let candidate = std::path::PathBuf::from(path);
    let name = candidate
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let looks_like_env =
        name == ".env" || name.starts_with(".env.") || name.ends_with(".env") && name.len() > 4;
    if looks_like_env {
        return Ok(candidate);
    }
    Err(tower_mcp::Error::invalid_params(format!(
        "output_credentials must name an env file (`.env`, `.env.local`, `something.env`), \
         got {path:?}"
    )))
}

cloud_tool!(write, cloud_quick_database, "cloud_quick_database",
    "Create or reuse a FREE Redis database and write its connection string to a file (default \
     ./.env). Idempotent by name: re-running returns the existing database. Returns database \
     metadata only — the connection string and password are written to the file, never in the \
     response. Requires an authenticated profile (see cloud_auth_status).",
    {
        /// Database name; also names the subscription (prefixed `redisctl-`). 3-40 chars,
        /// lowercase letters/digits/hyphens, starts with a letter, no `--`.
        pub name: String,
        /// File to write credentials into (default: ./.env).
        #[serde(default)]
        pub output_credentials: Option<String>,
        /// Primary URL variable name (default: REDIS_URL). Broken-out fields derive their prefix.
        #[serde(default)]
        pub variable: Option<String>,
        /// Max seconds to wait for each async operation (default: 600).
        #[serde(default)]
        pub wait_timeout: Option<u32>,
        /// Polling interval in seconds (default: 5).
        #[serde(default)]
        pub wait_interval: Option<u32>,
    } => |client, input| {
        let mut params = QuickDatabaseParams::new(input.name);
        if let Some(p) = input.output_credentials {
            params.output_credentials = credentials_path_is_an_env_file(&p)?;
        }
        if let Some(v) = input.variable {
            params.variable = v;
        }
        if let Some(t) = input.wait_timeout {
            params.wait_timeout = t.clamp(1, 3600);
        }
        if let Some(i) = input.wait_interval {
            params.wait_interval = i.clamp(1, 60);
        }
        let report = provision(&client, &params)
            .await
            .map_err(|e| tower_mcp::Error::tool(e.to_string()))?;
        CallToolResult::from_serialize(&report)
    }
);

#[cfg(test)]
mod tests {
    use super::credentials_path_is_an_env_file;

    #[test]
    fn credentials_target_must_be_an_env_file() {
        for ok in [
            ".env",
            "./.env",
            "config/.env",
            ".env.local",
            "prod.env",
            "/Users/someone/projects/app/.env",
            "/tmp/xyz/.env",
        ] {
            assert!(
                credentials_path_is_an_env_file(ok).is_ok(),
                "{ok} should be accepted"
            );
        }
        for bad in [
            "/Users/someone/.zshrc",
            "~/.bashrc",
            "/etc/passwd",
            "../.ssh/authorized_keys",
            "envfile",
            ".environment",
            "",
        ] {
            assert!(
                credentials_path_is_an_env_file(bad).is_err(),
                "{bad:?} should be refused"
            );
        }
    }
}
