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

/// Extra directories credentials may be written into, beyond the working directory. Colon-
/// separated, absolute. For a server whose working directory is not the caller's project.
const OUTPUT_ROOTS_ENV: &str = "REDISCTL_MCP_OUTPUT_DIRS";

/// Where credentials may be written: the working directory, plus anything in
/// [`OUTPUT_ROOTS_ENV`].
fn permitted_output_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(extra) = std::env::var(OUTPUT_ROOTS_ENV) {
        roots.extend(
            extra
                .split(':')
                .filter(|p| !p.is_empty())
                .map(std::path::PathBuf::from),
        );
    }
    roots.iter().filter_map(|r| r.canonicalize().ok()).collect()
}

/// The deepest ancestor of `path` that exists, with symlinks resolved.
///
/// Canonicalising only the full path fails for a file that is not there yet, and canonicalising
/// nothing at all would let `proj/.env` through while `proj` is a symlink to somewhere else.
fn resolved_parent(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut current = path.parent()?.to_path_buf();
    loop {
        // A relative name like `.env` has an empty parent, which means the working directory.
        let probe = if current.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            current.as_path()
        };
        if let Ok(resolved) = probe.canonicalize() {
            return Some(resolved);
        }
        current = current.parent()?.to_path_buf();
    }
}

/// Credentials may only be written to an env file inside a permitted root.
///
/// The name alone is not a boundary: it stops a shell profile being the target but leaves every
/// directory on the machine available. Both halves are checked here, and the destination is
/// resolved through symlinks first, so a link in any component cannot carry the write outside.
fn credentials_path_is_permitted(path: &str) -> Result<std::path::PathBuf, tower_mcp::Error> {
    let candidate = std::path::PathBuf::from(path);
    let name = candidate
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let looks_like_env =
        name == ".env" || name.starts_with(".env.") || name.ends_with(".env") && name.len() > 4;
    if !looks_like_env {
        return Err(tower_mcp::Error::invalid_params(format!(
            "output_credentials must name an env file (`.env`, `.env.local`, `something.env`), \
             got {path:?}"
        )));
    }

    let roots = permitted_output_roots();
    let Some(parent) = resolved_parent(&candidate) else {
        return Err(tower_mcp::Error::invalid_params(format!(
            "no existing directory to write {path:?} into"
        )));
    };
    if roots.iter().any(|root| parent.starts_with(root)) {
        return Ok(candidate);
    }
    Err(tower_mcp::Error::invalid_params(format!(
        "output_credentials must be inside the working directory{}, and {path:?} resolves \
         outside it. Set {OUTPUT_ROOTS_ENV} to permit another directory",
        if std::env::var_os(OUTPUT_ROOTS_ENV).is_some() {
            format!(" or a directory in {OUTPUT_ROOTS_ENV}")
        } else {
            String::new()
        }
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
            params.output_credentials = credentials_path_is_permitted(&p)?;
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
    use super::{OUTPUT_ROOTS_ENV, credentials_path_is_permitted};

    /// One test rather than several: it moves a process-wide environment variable, so it must
    /// not run alongside another that reads the same one.
    #[test]
    fn credentials_target_must_be_an_env_file_inside_a_permitted_root() {
        // Relative names land in the working directory, which is always permitted.
        for ok in [".env", "./.env", ".env.local", "prod.env"] {
            assert!(
                credentials_path_is_permitted(ok).is_ok(),
                "{ok} should be accepted"
            );
        }
        // The name still has to be an env file, whatever the directory.
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
                credentials_path_is_permitted(bad).is_err(),
                "{bad:?} should be refused"
            );
        }

        // An env file outside the working directory is refused until its directory is permitted.
        let project = tempfile::tempdir().unwrap();
        let inside = project.path().join(".env");
        let inside = inside.to_str().unwrap();
        assert!(
            credentials_path_is_permitted(inside).is_err(),
            "an absolute path outside every root should be refused"
        );

        // SAFETY: single-threaded test, and this is the only reader of the variable.
        unsafe { std::env::set_var(OUTPUT_ROOTS_ENV, project.path()) };
        let permitted = credentials_path_is_permitted(inside).is_ok();

        // A symlink in the path cannot carry the write out of the permitted root, which a
        // filename check on its own cannot prevent.
        let escape = project.path().join("out");
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", &escape).unwrap();
        #[cfg(unix)]
        let escaped = credentials_path_is_permitted(escape.join(".env").to_str().unwrap()).is_err();
        #[cfg(not(unix))]
        let escaped = true;

        // Absolute paths keep working: the server's directory is often not the caller's project.
        let nested = project.path().join("sub").join(".env");
        let nested_ok = credentials_path_is_permitted(nested.to_str().unwrap()).is_ok();

        unsafe { std::env::remove_var(OUTPUT_ROOTS_ENV) };
        assert!(
            permitted,
            "a directory named in {OUTPUT_ROOTS_ENV} should be accepted"
        );
        assert!(escaped, "a symlink out of the root should be refused");
        assert!(
            nested_ok,
            "a not-yet-created subdirectory of a root should be accepted"
        );
    }
}
