//! Shared safety classification for caller-supplied Redis commands.

use crate::policy::ToolSafety;
use crate::state::AppState;
use tower_mcp::Error as McpError;

const BLOCKED_COMMANDS: &[&str] = &[
    "ASKING",
    "AUTH",
    "BLMPOP",
    "BLMOVE",
    "BLPOP",
    "BGSAVE",
    "BGREWRITEAOF",
    "BRPOP",
    "BRPOPLPUSH",
    "BZMPOP",
    "BZPOPMAX",
    "BZPOPMIN",
    "DEBUG",
    "DISCARD",
    "EXEC",
    "FAILOVER",
    "FLUSHALL",
    "FLUSHDB",
    "HELLO",
    "MONITOR",
    "MULTI",
    "PSUBSCRIBE",
    "PSYNC",
    "PUNSUBSCRIBE",
    "QUIT",
    "READONLY",
    "READWRITE",
    "REPLCONF",
    "REPLICAOF",
    "RESET",
    "SAVE",
    "SELECT",
    "SHUTDOWN",
    "SLAVEOF",
    "SSUBSCRIBE",
    "SUBSCRIBE",
    "SUNSUBSCRIBE",
    "SYNC",
    "UNSUBSCRIBE",
    "UNWATCH",
    "WAIT",
    "WAITAOF",
    "WATCH",
];

const READ_ONLY_COMMANDS: &[&str] = &[
    "BITCOUNT",
    "BITPOS",
    "COMMAND",
    "DBSIZE",
    "DUMP",
    "ECHO",
    "EXISTS",
    "EXPIRETIME",
    "FT._LIST",
    "FT.AGGREGATE",
    "FT.DICTDUMP",
    "FT.EXPLAIN",
    "FT.EXPLAINCLI",
    "FT.INFO",
    "FT.PROFILE",
    "FT.SEARCH",
    "FT.SYNDUMP",
    "FT.TAGVALS",
    "GET",
    "GETBIT",
    "GETRANGE",
    "HGET",
    "HGETALL",
    "HEXISTS",
    "HKEYS",
    "HLEN",
    "HMGET",
    "HRANDFIELD",
    "HSCAN",
    "HSTRLEN",
    "HVALS",
    "INFO",
    "JSON.ARRLEN",
    "JSON.GET",
    "JSON.MGET",
    "JSON.OBJKEYS",
    "JSON.OBJLEN",
    "JSON.STRLEN",
    "JSON.TYPE",
    "KEYS",
    "LASTSAVE",
    "LINDEX",
    "LLEN",
    "LPOS",
    "LRANGE",
    "MEMORY",
    "MGET",
    "PEXPIRETIME",
    "PFCOUNT",
    "PING",
    "PTTL",
    "RANDOMKEY",
    "ROLE",
    "SCARD",
    "SDIFF",
    "SCAN",
    "SINTER",
    "SINTERCARD",
    "SISMEMBER",
    "SMEMBERS",
    "SMISMEMBER",
    "SRANDMEMBER",
    "SSCAN",
    "STRLEN",
    "SUNION",
    "TIME",
    "TOUCH",
    "TTL",
    "TYPE",
    "XINFO",
    "XLEN",
    "XPENDING",
    "XRANGE",
    "XREVRANGE",
    "ZCARD",
    "ZCOUNT",
    "ZDIFF",
    "ZINTER",
    "ZLEXCOUNT",
    "ZMSCORE",
    "ZRANDMEMBER",
    "ZRANGE",
    "ZRANGEBYLEX",
    "ZRANGEBYSCORE",
    "ZRANK",
    "ZREVRANGE",
    "ZREVRANGEBYLEX",
    "ZREVRANGEBYSCORE",
    "ZREVRANK",
    "ZSCAN",
    "ZSCORE",
    "ZUNION",
];

const WRITE_COMMANDS: &[&str] = &[
    "APPEND",
    "BITFIELD",
    "BITOP",
    "COPY",
    "DECR",
    "DECRBY",
    "EXPIRE",
    "EXPIREAT",
    "FT.ALIASADD",
    "FT.ALIASDEL",
    "FT.ALIASUPDATE",
    "FT.ALTER",
    "FT.CREATE",
    "FT.DICTADD",
    "FT.DICTDEL",
    "FT.SYNUPDATE",
    "GETSET",
    "HDEL",
    "HEXPIRE",
    "HINCRBY",
    "HINCRBYFLOAT",
    "HMSET",
    "HSET",
    "HSETNX",
    "INCR",
    "INCRBY",
    "INCRBYFLOAT",
    "JSON.ARRAPPEND",
    "JSON.ARRINSERT",
    "JSON.NUMINCRBY",
    "JSON.SET",
    "JSON.TOGGLE",
    "LINSERT",
    "LMOVE",
    "LPOP",
    "LPUSH",
    "LPUSHX",
    "LSET",
    "MSET",
    "MSETNX",
    "PERSIST",
    "PEXPIRE",
    "PEXPIREAT",
    "PSETEX",
    "RENAME",
    "RENAMENX",
    "RESTORE",
    "RPOP",
    "RPOPLPUSH",
    "RPUSH",
    "RPUSHX",
    "SADD",
    "SET",
    "SETBIT",
    "SETEX",
    "SETNX",
    "SETRANGE",
    "SMOVE",
    "SREM",
    "XACK",
    "XADD",
    "XCLAIM",
    "XTRIM",
    "ZADD",
    "ZINCRBY",
    "ZREM",
    "ZREMRANGEBYLEX",
    "ZREMRANGEBYRANK",
    "ZREMRANGEBYSCORE",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClassifiedCommand {
    canonical_name: String,
    safety: ToolSafety,
}

fn normalize_token(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

fn subcommand(args: &[String]) -> Option<String> {
    args.first().and_then(|value| normalize_token(value))
}

fn classify_privileged_family(command: &str, args: &[String]) -> Result<ToolSafety, String> {
    let subcommand = subcommand(args);
    let allowed = match command {
        "CONFIG" => matches!(subcommand.as_deref(), Some("GET" | "HELP")),
        "ACL" => match subcommand.as_deref() {
            Some(
                "CAT" | "DRYRUN" | "GENPASS" | "GETUSER" | "HELP" | "LIST" | "USERS" | "WHOAMI",
            ) => true,
            Some("LOG") => !matches!(
                args.get(1)
                    .and_then(|value| normalize_token(value))
                    .as_deref(),
                Some("RESET")
            ),
            _ => false,
        },
        "CLUSTER" => matches!(
            subcommand.as_deref(),
            Some(
                "COUNTKEYSINSLOT"
                    | "GETKEYSINSLOT"
                    | "HELP"
                    | "INFO"
                    | "KEYSLOT"
                    | "LINKS"
                    | "MYID"
                    | "MYSHARDID"
                    | "NODES"
                    | "SHARDS"
                    | "SLOTS"
            )
        ),
        "MODULE" => matches!(subcommand.as_deref(), Some("HELP" | "LIST")),
        "CLIENT" => matches!(
            subcommand.as_deref(),
            Some("GETNAME" | "HELP" | "ID" | "INFO" | "LIST" | "TRACKINGINFO")
        ),
        _ => false,
    };

    if allowed {
        Ok(ToolSafety::ReadOnly)
    } else {
        Err(format!("command '{command}' is blocked for safety"))
    }
}

fn classify_family(command: &str, args: &[String]) -> Option<ToolSafety> {
    let subcommand = subcommand(args);
    match command {
        "LATENCY" => Some(
            if matches!(
                subcommand.as_deref(),
                Some("DOCTOR" | "GRAPH" | "HELP" | "HISTOGRAM" | "HISTORY" | "LATEST")
            ) {
                ToolSafety::ReadOnly
            } else {
                ToolSafety::Destructive
            },
        ),
        "MEMORY" => Some(
            if matches!(
                subcommand.as_deref(),
                Some("DOCTOR" | "HELP" | "MALLOC-STATS" | "STATS" | "USAGE")
            ) {
                ToolSafety::ReadOnly
            } else {
                ToolSafety::Destructive
            },
        ),
        "OBJECT" => Some(
            if matches!(
                subcommand.as_deref(),
                Some("ENCODING" | "FREQ" | "HELP" | "IDLETIME" | "REFCOUNT")
            ) {
                ToolSafety::ReadOnly
            } else {
                ToolSafety::Destructive
            },
        ),
        "SCRIPT" => Some(
            if matches!(subcommand.as_deref(), Some("EXISTS" | "HELP")) {
                ToolSafety::ReadOnly
            } else {
                ToolSafety::Destructive
            },
        ),
        "XREAD" => Some(ToolSafety::ReadOnly),
        "XREADGROUP" => Some(ToolSafety::Write),
        "SLOWLOG" => Some(
            if matches!(subcommand.as_deref(), Some("GET" | "HELP" | "LEN")) {
                ToolSafety::ReadOnly
            } else {
                ToolSafety::Destructive
            },
        ),
        "FUNCTION" => Some(
            if matches!(
                subcommand.as_deref(),
                Some("DUMP" | "HELP" | "LIST" | "STATS")
            ) {
                ToolSafety::ReadOnly
            } else {
                ToolSafety::Destructive
            },
        ),
        "PUBSUB" | "COMMAND" | "XINFO" => Some(ToolSafety::ReadOnly),
        _ => None,
    }
}

fn option_before_streams(args: &[String], option: &str) -> bool {
    args.iter()
        .filter_map(|argument| normalize_token(argument))
        .take_while(|argument| argument != "STREAMS")
        .any(|argument| argument == option)
}

fn reject_unsafe_variant(command: &str, args: &[String]) -> Result<(), String> {
    if matches!(command, "XREAD" | "XREADGROUP") && option_before_streams(args, "BLOCK") {
        return Err(format!(
            "command '{command}' with BLOCK is blocked because it can pin the shared connection"
        ));
    }

    if command == "SCRIPT" && subcommand(args).as_deref() == Some("DEBUG") {
        return Err(
            "command 'SCRIPT DEBUG' is blocked because it changes connection/server execution state"
                .to_string(),
        );
    }

    Ok(())
}

fn classify(command: &str, args: &[String]) -> Result<ClassifiedCommand, String> {
    let canonical_name = normalize_token(command)
        .ok_or_else(|| "command name must be one non-empty token".to_string())?;

    if BLOCKED_COMMANDS.contains(&canonical_name.as_str()) {
        return Err(format!("command '{canonical_name}' is blocked for safety"));
    }
    reject_unsafe_variant(&canonical_name, args)?;

    let safety = if matches!(
        canonical_name.as_str(),
        "ACL" | "CLIENT" | "CLUSTER" | "CONFIG" | "MODULE"
    ) {
        classify_privileged_family(&canonical_name, args)?
    } else if let Some(safety) = classify_family(&canonical_name, args) {
        safety
    } else if READ_ONLY_COMMANDS.contains(&canonical_name.as_str()) {
        ToolSafety::ReadOnly
    } else if WRITE_COMMANDS.contains(&canonical_name.as_str()) {
        ToolSafety::Write
    } else {
        // Destructive commands (for example DEL, UNLINK, JSON.DEL, and
        // FT.DROPINDEX) and unknown/module commands both require Full.
        // Unknown commands remain available as an escape hatch, but never
        // inherit the read-write tier accidentally.
        ToolSafety::Destructive
    };

    Ok(ClassifiedCommand {
        canonical_name,
        safety,
    })
}

fn authorize(
    state: &AppState,
    surface_tool: &str,
    floor: ToolSafety,
    commands: &[ClassifiedCommand],
) -> Result<(), McpError> {
    let required = commands
        .iter()
        .map(|command| command.safety)
        .max()
        .unwrap_or(floor)
        .max(floor);

    // The normal handler guard already authorizes the tool's advertised/base
    // behavior, including explicit per-tool allows. Only behavior above that
    // floor needs a second check, and explicit allows must not raise the
    // policy's dynamic safety ceiling.
    if required <= floor || state.is_dynamic_safety_allowed(surface_tool, required) {
        Ok(())
    } else {
        Err(crate::policy::policy_denied_for_command(surface_tool))
    }
}

fn authorize_surface(
    state: &AppState,
    surface_tool: &str,
    advertised_safety: ToolSafety,
) -> Result<(), McpError> {
    if state.is_tool_allowed(surface_tool, advertised_safety) {
        Ok(())
    } else {
        Err(crate::policy::policy_denied(surface_tool))
    }
}

/// Classify one caller-supplied command for audit and policy enforcement.
pub(crate) fn classify_command_safety(
    command: &str,
    args: &[String],
) -> Result<ToolSafety, String> {
    classify(command, args).map(|classified| classified.safety)
}

/// Return the highest safety tier required by a command batch.
pub(crate) fn classify_batch_safety<'a>(
    commands: impl IntoIterator<Item = &'a [String]>,
) -> Result<ToolSafety, String> {
    commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let (name, args) = command
                .split_first()
                .ok_or_else(|| format!("command at index {index} is empty"))?;
            classify_command_safety(name, args)
                .map_err(|error| format!("command at index {index}: {error}"))
        })
        .try_fold(ToolSafety::ReadOnly, |highest, safety| {
            safety.map(|safety| highest.max(safety))
        })
}

pub(super) fn preflight_command(
    state: &AppState,
    surface_tool: &str,
    advertised_safety: ToolSafety,
    floor: ToolSafety,
    command: &str,
    args: &[String],
) -> Result<String, McpError> {
    authorize_surface(state, surface_tool, advertised_safety)?;
    let classified = classify(command, args).map_err(McpError::tool)?;
    authorize(
        state,
        surface_tool,
        floor,
        std::slice::from_ref(&classified),
    )?;
    Ok(classified.canonical_name)
}

pub(super) fn preflight_packed_commands<'a>(
    state: &AppState,
    surface_tool: &str,
    advertised_safety: ToolSafety,
    floor: ToolSafety,
    commands: impl IntoIterator<Item = &'a [String]>,
) -> Result<Vec<String>, McpError> {
    authorize_surface(state, surface_tool, advertised_safety)?;
    let classified = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let (name, args) = command
                .split_first()
                .ok_or_else(|| McpError::tool(format!("command at index {index} is empty")))?;
            classify(name, args)
                .map_err(|error| McpError::tool(format!("command at index {index}: {error}")))
        })
        .collect::<Result<Vec<_>, _>>()?;

    authorize(state, surface_tool, floor, &classified)?;
    Ok(classified
        .into_iter()
        .map(|command| command.canonical_name)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{Policy, PolicyConfig, SafetyTier, ToolsetKind, ToolsetPolicy};
    use crate::state::CredentialSource;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn test_state(config: PolicyConfig) -> AppState {
        AppState::new(
            CredentialSource::Profiles(Vec::new()),
            Arc::new(Policy::new(
                config,
                HashMap::from([
                    ("redis_bulk_load".to_string(), ToolsetKind::Database),
                    ("redis_command".to_string(), ToolsetKind::Database),
                ]),
                "test".to_string(),
            )),
            None,
            false,
            None,
        )
        .unwrap()
    }

    #[test]
    fn normalizes_known_commands_and_classifies_them() {
        let read = classify("  get  ", &args(&["key"])).unwrap();
        assert_eq!(read.canonical_name, "GET");
        assert_eq!(read.safety, ToolSafety::ReadOnly);

        assert_eq!(
            classify("json.set", &args(&["key", "$", "{}"]))
                .unwrap()
                .safety,
            ToolSafety::Write
        );
        assert_eq!(
            classify("del", &args(&["key"])).unwrap().safety,
            ToolSafety::Destructive
        );
    }

    #[test]
    fn unknown_commands_require_full_tier() {
        let command = classify("my.module", &args(&["secret"])).unwrap();
        assert_eq!(command.safety, ToolSafety::Destructive);
    }

    #[test]
    fn blocks_connection_state_and_privileged_mutations() {
        for (command, command_args) in [
            ("AUTH", args(&["user", "password-never-echoed"])),
            ("BLPOP", args(&["queue", "0"])),
            ("CONFIG", args(&["SET", "requirepass", "secret"])),
            ("ACL", args(&["LOG", "RESET"])),
            ("CLIENT", args(&["KILL", "TYPE", "normal"])),
            ("BGSAVE", Vec::new()),
            ("XREAD", args(&["BLOCK", "0", "STREAMS", "events", "$"])),
            (
                "XREADGROUP",
                args(&[
                    "GROUP", "workers", "one", "BLOCK", "5000", "STREAMS", "events", ">",
                ]),
            ),
            ("SCRIPT", args(&["DEBUG", "SYNC"])),
        ] {
            let error = classify(command, &command_args).unwrap_err();
            assert!(error.contains(&command.to_ascii_uppercase()));
            assert!(!error.contains("password-never-echoed"));
            assert!(!error.contains("requirepass"));
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn permits_read_only_privileged_subcommands() {
        for (command, command_args) in [
            ("CONFIG", args(&["GET", "maxmemory"])),
            ("ACL", args(&["WHOAMI"])),
            ("CLUSTER", args(&["NODES"])),
            ("MODULE", args(&["LIST"])),
            ("CLIENT", args(&["INFO"])),
        ] {
            assert_eq!(
                classify(command, &command_args).unwrap().safety,
                ToolSafety::ReadOnly
            );
        }

        assert_eq!(
            classify("XREAD", &args(&["COUNT", "1", "STREAMS", "events", "$"]))
                .unwrap()
                .safety,
            ToolSafety::ReadOnly
        );
        assert_eq!(
            classify(
                "XREADGROUP",
                &args(&["GROUP", "workers", "one", "STREAMS", "events", ">"]),
            )
            .unwrap()
            .safety,
            ToolSafety::Write
        );
    }

    #[test]
    fn rejects_empty_or_multi_token_command_names() {
        assert!(classify(" ", &[]).is_err());
        assert!(classify("CONFIG SET", &[]).is_err());
    }

    #[test]
    fn batch_preflight_uses_the_highest_command_risk() {
        let state = test_state(PolicyConfig {
            tier: SafetyTier::ReadWrite,
            allow: vec!["redis_bulk_load".to_string()],
            ..Default::default()
        });
        let writes = [args(&["SET", "key", "value"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                writes.iter().map(Vec::as_slice),
            )
            .is_ok()
        );

        let mixed = [args(&["SET", "new-key", "value"]), args(&["DEL", "canary"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                mixed.iter().map(Vec::as_slice),
            )
            .is_err()
        );
    }

    #[test]
    fn full_allows_destructive_but_never_blocked_commands() {
        let state = test_state(PolicyConfig {
            tier: SafetyTier::Full,
            ..Default::default()
        });
        let destructive = [args(&["DEL", "key"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                destructive.iter().map(Vec::as_slice),
            )
            .is_ok()
        );

        let blocked = [args(&["BGSAVE"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                blocked.iter().map(Vec::as_slice),
            )
            .is_err()
        );
    }

    #[test]
    fn batch_preflight_honors_toolset_and_category_policy() {
        let state = test_state(PolicyConfig {
            tier: SafetyTier::ReadOnly,
            database: Some(ToolsetPolicy {
                tier: Some(SafetyTier::ReadWrite),
                allow: vec!["redis_bulk_load".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        });
        let writes = [args(&["SET", "key", "value"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                writes.iter().map(Vec::as_slice),
            )
            .is_ok()
        );

        let state = test_state(PolicyConfig {
            tier: SafetyTier::Full,
            deny_categories: vec!["destructive".to_string()],
            ..Default::default()
        });
        let destructive = [args(&["DEL", "key"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                destructive.iter().map(Vec::as_slice),
            )
            .is_err()
        );
    }

    #[test]
    fn explicit_tool_allow_does_not_raise_dynamic_safety_ceiling() {
        let state = test_state(PolicyConfig {
            tier: SafetyTier::ReadOnly,
            allow: vec!["redis_bulk_load".to_string(), "redis_command".to_string()],
            ..Default::default()
        });

        // The explicit grant admits each surface's base operation.
        let writes = [args(&["SET", "key", "value"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                writes.iter().map(Vec::as_slice),
            )
            .is_ok()
        );
        assert!(
            preflight_command(
                &state,
                "redis_command",
                ToolSafety::Destructive,
                ToolSafety::ReadOnly,
                "GET",
                &args(&["key"]),
            )
            .is_ok()
        );

        // It does not grant more dangerous caller-supplied behavior.
        let destructive = [args(&["DEL", "key"])];
        assert!(
            preflight_packed_commands(
                &state,
                "redis_bulk_load",
                ToolSafety::Destructive,
                ToolSafety::Write,
                destructive.iter().map(Vec::as_slice),
            )
            .is_err()
        );
        assert!(
            preflight_command(
                &state,
                "redis_command",
                ToolSafety::Destructive,
                ToolSafety::ReadOnly,
                "SET",
                &args(&["key", "value"]),
            )
            .is_err()
        );
    }
}
