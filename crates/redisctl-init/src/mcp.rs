//! MCP registration per agent: the official Redis data-plane server in each agent's
//! project config. Every config stays credential-free and safe to commit: a shell
//! launcher sources `.env` when the MCP client starts the server, so the URL (and
//! its password) never lands in a committed file.

use std::path::Path;

use crate::InitError;
use crate::change::{Change, Status};
use crate::docker::docker_ok;
use crate::env::{FileAction, read_for_planning};
use crate::project::Agent;
use crate::util::{has_bin, mask_url};

/// How the launcher runs the server: uvx when available, a Docker bridge otherwise.
/// With neither, the config is still written for uvx and the caller shows a note.
enum Runner {
    Uvx,
    Docker,
    UvxMissing,
}

fn server_entry(runner: &Runner) -> serde_json::Value {
    let inner = match runner {
        // Inside the container, localhost is the container itself.
        Runner::Docker => {
            r#"exec docker run --rm -i mcp/redis --url "$(printf %s "$REDIS_URL" | sed -e 's/localhost/host.docker.internal/' -e 's/127\.0\.0\.1/host.docker.internal/')""#
        }
        _ => r#"exec uvx --from redis-mcp-server@latest redis-mcp-server --url "$REDIS_URL""#,
    };
    serde_json::json!({
        "command": "sh",
        "args": ["-c", format!("set -a; . ./.env 2>/dev/null; set +a; {inner}")]
    })
}

/// One agent's registration, decided at plan time.
#[derive(Debug)]
pub(crate) enum McpAction {
    File(FileAction),
    Report(Change),
}

impl McpAction {
    pub(crate) fn preview(&self) -> Change {
        match self {
            McpAction::File(action) => action.preview(),
            McpAction::Report(change) => change.clone(),
        }
    }

    pub(crate) fn perform(&self, cwd: &Path) -> Result<Change, InitError> {
        match self {
            McpAction::File(action) => action.perform(cwd),
            McpAction::Report(change) => Ok(change.clone()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct McpPlan {
    pub(crate) actions: Vec<McpAction>,
    pub(crate) uvx_missing: bool,
}

pub(crate) fn plan_mcp(cwd: &Path, agents: &[Agent]) -> Result<McpPlan, InitError> {
    let runner = if has_bin("uvx") {
        Runner::Uvx
    } else if docker_ok() {
        Runner::Docker
    } else {
        Runner::UvxMissing
    };
    let entry = server_entry(&runner);
    let mut actions = Vec::new();
    for agent in agents {
        actions.push(match agent {
            Agent::Claude => upsert(cwd, ".mcp.json", "mcpServers", &entry, false)?,
            Agent::Cursor => upsert(cwd, ".cursor/mcp.json", "mcpServers", &entry, false)?,
            Agent::Vscode => upsert(cwd, ".vscode/mcp.json", "servers", &entry, true)?,
            Agent::Codex => McpAction::Report(Change::new(
                "mcp (codex)",
                Status::Skipped,
                "codex MCP config is user-scoped (~/.codex/config.toml); the skills cover it",
            )),
        });
    }
    Ok(McpPlan {
        actions,
        uvx_missing: matches!(runner, Runner::UvxMissing),
    })
}

fn kept_invalid(rel: &str) -> McpAction {
    McpAction::Report(Change::new(
        rel,
        Status::Kept,
        "existing file is not valid JSON; left untouched",
    ))
}

fn upsert(
    cwd: &Path,
    rel: &str,
    top_key: &str,
    entry: &serde_json::Value,
    stdio: bool,
) -> Result<McpAction, InitError> {
    let mut server = entry.clone();
    if stdio {
        server["type"] = "stdio".into();
    }
    let existing = read_for_planning(cwd, rel)?;
    let mut cfg = match &existing {
        None => serde_json::json!({}),
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => value,
            Err(_) => return Ok(kept_invalid(rel)),
        },
    };
    let Some(root) = cfg.as_object_mut() else {
        return Ok(kept_invalid(rel));
    };
    let servers = root.entry(top_key).or_insert_with(|| serde_json::json!({}));
    let Some(map) = servers.as_object_mut() else {
        return Ok(kept_invalid(rel));
    };
    let note = match map.get("redis") {
        Some(previous) if *previous == server => {
            return Ok(McpAction::Report(Change::new(rel, Status::Unchanged, "")));
        }
        Some(previous) => {
            let command = previous["command"].as_str().unwrap_or_default();
            let args = previous["args"]
                .as_array()
                .map(|args| {
                    args.iter()
                        .filter_map(|a| a.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            format!(
                "replaced existing redis server (was: {})",
                mask_url(format!("{command} {args}").trim())
            )
        }
        None => "redis: reads REDIS_URL from .env at launch".to_string(),
    };
    map.insert("redis".to_string(), server);
    let status = if existing.is_some() {
        Status::Updated
    } else {
        Status::Created
    };
    let content = format!(
        "{}\n",
        serde_json::to_string_pretty(&cfg).map_err(|e| InitError::WriteFailed {
            rel: rel.to_string(),
            message: e.to_string(),
        })?
    );
    Ok(McpAction::File(FileAction::Write {
        rel: rel.to_string(),
        content,
        status,
        note,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_for(dir: &Path, agents: &[Agent]) -> McpPlan {
        plan_mcp(dir, agents).unwrap()
    }

    fn apply_all(dir: &Path, plan: &McpPlan) -> Vec<Change> {
        plan.actions
            .iter()
            .map(|a| a.perform(dir).unwrap())
            .collect()
    }

    #[test]
    fn registers_per_agent_with_the_right_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_for(
            dir.path(),
            &[Agent::Claude, Agent::Cursor, Agent::Vscode, Agent::Codex],
        );
        let changes = apply_all(dir.path(), &plan);
        assert_eq!(changes[0].subject, ".mcp.json");
        assert_eq!(changes[1].subject, ".cursor/mcp.json");
        assert_eq!(changes[2].subject, ".vscode/mcp.json");
        assert_eq!(changes[3].status, Status::Skipped);

        let claude: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap())
                .unwrap();
        let launcher = claude["mcpServers"]["redis"]["args"][1].as_str().unwrap();
        assert!(launcher.starts_with("set -a; . ./.env 2>/dev/null; set +a; exec "));
        assert!(launcher.contains("$REDIS_URL"));
        // Credential-free: the config carries the env reference, never a URL value.
        assert!(!launcher.contains("redis://"));

        let vscode: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(".vscode/mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(vscode["servers"]["redis"]["type"], "stdio");
    }

    #[test]
    fn rerun_is_unchanged_and_foreign_servers_survive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".mcp.json"),
            r#"{"mcpServers":{"mine":{"command":"custom"}}}"#,
        )
        .unwrap();
        let plan = plan_for(dir.path(), &[Agent::Claude]);
        apply_all(dir.path(), &plan);
        let cfg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap())
                .unwrap();
        assert_eq!(cfg["mcpServers"]["mine"]["command"], "custom");
        assert!(cfg["mcpServers"]["redis"].is_object());

        let rerun = plan_for(dir.path(), &[Agent::Claude]);
        assert_eq!(rerun.actions[0].preview().status, Status::Unchanged);
    }

    #[test]
    fn a_different_existing_redis_server_is_replaced_with_a_masked_note() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".mcp.json"),
            r#"{"mcpServers":{"redis":{"command":"redis-mcp","args":["--url","redis://default:s3cret@h:1"]}}}"#,
        )
        .unwrap();
        let plan = plan_for(dir.path(), &[Agent::Claude]);
        let change = plan.actions[0].preview();
        assert_eq!(change.status, Status::Updated);
        assert!(change.note.contains("replaced existing redis server"));
        assert!(
            change.note.contains("redis://default:****@h:1"),
            "{}",
            change.note
        );
        assert!(!change.note.contains("s3cret"));
    }

    #[test]
    fn invalid_json_is_kept_untouched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".mcp.json"), "{not json").unwrap();
        let plan = plan_for(dir.path(), &[Agent::Claude]);
        let change = plan.actions[0].preview();
        assert_eq!(change.status, Status::Kept);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap(),
            "{not json"
        );
    }
}
