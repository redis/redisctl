//! Command alias tools — save and replay named Redis command sequences

use tower_mcp::CallToolResult;

use super::RedisResultExt;

use crate::policy::ToolSafety;
use crate::tools::macros::{database_tool, mcp_module};

/// A command entry for defining an alias.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AliasCommand {
    /// Redis command arguments (e.g. ["SET", "key", "value"] or ["JSON.SET", "doc:1", "$", "{}"])
    pub args: Vec<String>,
}

mcp_module! {
    alias_set => "redis_alias_set",
    alias_run => "redis_alias_run",
    alias_list => "redis_alias_list",
    alias_delete => "redis_alias_delete"
}

database_tool!(write_stateful, alias_set, "redis_alias_set",
    "Save a named command alias for this session. The alias stores a sequence of Redis commands \
     that can be replayed with redis_alias_run.\n\n\
     Use this to capture a repeatable workflow (e.g. seed + query, write + verify round-trip) \
     and replay it without reconstructing the commands each time.\n\n\
     Aliases are session-scoped (in-memory only) and are lost when the MCP server restarts.",
    {
        /// Alias name (e.g. "seed-users", "health-check")
        pub name: String,
        /// Commands to store. Each command is an args array (e.g. [\"SET\", \"k\", \"v\"]).
        pub commands: Vec<AliasCommand>,
    } => |state, _conn, input| {
        if input.commands.is_empty() {
            return Err(tower_mcp::Error::tool("commands must not be empty"));
        }

        let canonical_commands = super::command_safety::preflight_packed_commands(
            state,
            "redis_alias_set",
            ToolSafety::Write,
            ToolSafety::Write,
            input.commands.iter().map(|command| command.args.as_slice()),
        )?;

        let command_count = input.commands.len();
        let commands: Vec<Vec<String>> = input.commands
            .into_iter()
            .zip(canonical_commands)
            .map(|(mut command, canonical_name)| {
                command.args[0] = canonical_name;
                command.args
            })
            .collect();
        state.set_alias(input.name.clone(), commands).await;

        Ok(CallToolResult::text(format!(
            "Alias '{}' saved with {} command(s)", input.name, command_count
        )))
    }
);

database_tool!(destructive_stateful, alias_run, "redis_alias_run",
    "Run a previously saved command alias. Executes the stored commands in order via a \
     Redis pipeline and returns per-command results. Commands are reclassified before every \
     run; destructive or unknown commands require Full policy.\n\n\
     Use redis_alias_list to see available aliases.",
    {
        /// Alias name to run
        pub name: String,
    } => |state, conn, input| {
        let commands = state.get_alias(&input.name).await
            .ok_or_else(|| tower_mcp::Error::tool(format!(
                "Alias '{}' not found. Use redis_alias_list to see available aliases.", input.name
            )))?;

        // Revalidate at execution time as defense in depth: aliases can be
        // inserted through test/support APIs or survive future policy changes.
        let canonical_commands = super::command_safety::preflight_packed_commands(
            state,
            "redis_alias_run",
            ToolSafety::Destructive,
            ToolSafety::Write,
            commands.iter().map(Vec::as_slice),
        )?;

        let mut pipe = redis::pipe();
        for (cmd_args, canonical_name) in commands.iter().zip(&canonical_commands) {
            let mut cmd = redis::cmd(canonical_name);
            for arg in &cmd_args[1..] {
                cmd.arg(arg);
            }
            pipe.add_command(cmd);
        }

        let results: Vec<redis::Value> = pipe
            .query_async(&mut conn)
            .await
            .tool_context(format!("Alias '{}' pipeline failed", input.name))?;

        let mut lines = Vec::with_capacity(results.len() + 2);
        for (i, ((cmd_args, canonical_name), result)) in commands
            .iter()
            .zip(&canonical_commands)
            .zip(results.iter())
            .enumerate()
        {
            let label = canonical_name;
            let key = cmd_args.get(1).map(|s| s.as_str()).unwrap_or("");
            let result_str = super::format_value(result);
            lines.push(format!("[{:>4}] {:<12} {}  →  {}", i, label, key, result_str));
        }
        lines.push(String::new());
        lines.push(format!("Alias '{}': {} command(s) executed", input.name, results.len()));

        Ok(CallToolResult::text(lines.join("\n")))
    }
);

database_tool!(read_only_stateful, alias_list, "redis_alias_list",
    "List all saved command aliases for this session.",
    {
    } => |state, _conn, _input| {
        let entries = state.list_aliases().await;

        if entries.is_empty() {
            return Ok(CallToolResult::text(
                "No aliases saved. Use redis_alias_set to create one."
            ));
        }

        let lines: Vec<String> = entries.iter()
            .map(|(name, count)| format!("  {:<24} {} command(s)", name, count))
            .collect();

        Ok(CallToolResult::text(format!(
            "Aliases ({}):\n{}",
            entries.len(),
            lines.join("\n")
        )))
    }
);

database_tool!(write_stateful, alias_delete, "redis_alias_delete",
    "Delete a saved command alias.",
    {
        /// Alias name to delete
        pub name: String,
    } => |state, _conn, input| {
        if state.delete_alias(&input.name).await {
            Ok(CallToolResult::text(format!("Deleted alias '{}'", input.name)))
        } else {
            Ok(CallToolResult::text(format!(
                "Alias '{}' not found", input.name
            )))
        }
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::CredentialSource;
    use std::sync::Arc;

    #[test]
    fn alias_run_is_advertised_as_potentially_destructive() {
        let state = Arc::new(
            crate::state::AppState::new(
                CredentialSource::Profiles(Vec::new()),
                crate::state::AppState::test_write_policy(),
                None,
                false,
                None,
            )
            .unwrap(),
        );
        let tool = alias_run(state);
        let annotations = tool.annotations.expect("alias_run annotations");

        assert!(!annotations.read_only_hint);
        assert!(annotations.destructive_hint);
        assert!(!annotations.idempotent_hint);
    }
}
