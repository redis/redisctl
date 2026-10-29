//! Tool parameters the MCP skills tell an agent to pass must exist in that tool's
//! input schema. With strict tool inputs an undeclared parameter is an
//! `Invalid input: unknown field` error as soon as an agent follows the skill.
//!
//! Prose is ambiguous, so only forms that clearly name an argument are read, and
//! only on a line that names exactly one tool:
//! - the `arguments` keys of a JSON call envelope in a `json` code block, which
//!   must also name a real tool;
//! - `name=value` written without spaces, as in `count=100`;
//! - a backticked name right after "with", as in "with `limit` 100";
//! - the leading backticked name of each list item after a line ending in "with:".
//!
//! Other backticked names are usually output fields (`status`, `num_docs`) and are
//! not checked. Tool schemas come from the all-features catalog fixture, which the
//! catalog contract test keeps equal to the live catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

const CATALOG: &str = include_str!("fixtures/mcp-catalog-v1.json");

/// Tool name to every property name its input schema declares.
type Tools = BTreeMap<String, BTreeSet<String>>;

/// Property names at any depth, so a nested argument field such as a search
/// schema entry's `field_type` counts as declared.
fn declared_names(schema: &Value, names: &mut BTreeSet<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            names.insert(name.clone());
            declared_names(property, names);
        }
    }
    for key in ["items", "additionalProperties"] {
        if let Some(sub) = object.get(key) {
            declared_names(sub, names);
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        for sub in object
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            declared_names(sub, names);
        }
    }
    for sub in object
        .get("$defs")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|defs| defs.values())
    {
        declared_names(sub, names);
    }
}

fn catalog_tools() -> Tools {
    let catalog: Value = serde_json::from_str(CATALOG).expect("catalog fixture is JSON");
    catalog["tools"]
        .as_array()
        .expect("catalog fixture lists tools")
        .iter()
        .map(|tool| {
            let mut names = BTreeSet::new();
            declared_names(&tool["inputSchema"], &mut names);
            let name = tool["name"].as_str().expect("catalog tool has a name");
            (name.to_string(), names)
        })
        .collect()
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'
}

fn is_ident(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(is_ident_char)
}

/// The contents of each backticked span on a line.
fn code_spans(line: &str) -> impl Iterator<Item = &str> {
    line.split('`').skip(1).step_by(2)
}

/// `name=value` written without spaces. Spaced `a = b` and comparisons such as
/// `==` are prose, and a `?a=b` query string is part of a URL.
fn assignments(line: &str) -> Vec<&str> {
    line.match_indices('=')
        .filter_map(|(eq, _)| {
            let next = line[eq + 1..].chars().next()?;
            if next == '=' || next.is_whitespace() {
                return None;
            }
            let start = line[..eq]
                .char_indices()
                .rev()
                .find(|(_, c)| !is_ident_char(*c))
                .map_or(0, |(i, c)| i + c.len_utf8());
            let name = &line[start..eq];
            let before = line[..start].chars().next_back();
            (is_ident(name) && before.is_none_or(|c| c.is_whitespace() || c == '(' || c == '`'))
                .then_some(name)
        })
        .collect()
}

/// Backticked names right after the word "with".
fn with_names(line: &str) -> Vec<&str> {
    line.match_indices("with `")
        .filter(|(at, _)| {
            line[..*at]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric())
        })
        .filter_map(|(at, pattern)| {
            let rest = &line[at + pattern.len()..];
            let name = &rest[..rest.find('`')?];
            is_ident(name).then_some(name)
        })
        .collect()
}

/// The leading backticked name of a `- `name`: ...` list item.
fn list_item_name(line: &str) -> Option<&str> {
    let rest = line
        .trim_start()
        .strip_prefix(|c: char| c == '-' || c == '*')?
        .trim_start()
        .strip_prefix('`')?;
    let name = &rest[..rest.find('`')?];
    is_ident(name).then_some(name)
}

/// Problems in one skill's markdown, as `line: message`.
fn check_skill(markdown: &str, tools: &Tools) -> Vec<String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut problems = Vec::new();
    let mut fence: Option<(usize, bool)> = None; // (opening line, is a json block)
    let mut block = String::new();

    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        if line.trim_start().starts_with("```") {
            match fence.take() {
                None => {
                    fence = Some((number, line.trim() == "```json"));
                    block.clear();
                }
                Some((opened, true)) => problems.extend(check_envelope(opened, &block, tools)),
                Some((_, false)) => {}
            }
            continue;
        }
        if fence.is_some() {
            block.push_str(line);
            block.push('\n');
            continue;
        }

        let mut mentioned: Vec<&str> = code_spans(line)
            .filter(|span| tools.contains_key(*span))
            .collect();
        mentioned.sort_unstable();
        mentioned.dedup();
        let [tool] = mentioned[..] else {
            continue;
        };
        let mut params: Vec<(usize, &str)> = assignments(line)
            .into_iter()
            .chain(with_names(line))
            .map(|param| (number, param))
            .collect();
        if line.trim_end().ends_with("with:") {
            params.extend(
                lines[index + 1..]
                    .iter()
                    .map_while(|item| list_item_name(item))
                    .enumerate()
                    .map(|(offset, param)| (number + 1 + offset, param)),
            );
        }
        for (at, param) in params {
            if !tools[tool].contains(param) && !tools.contains_key(param) {
                problems.push(format!("{at}: `{tool}` has no `{param}` input"));
            }
        }
    }
    problems
}

/// A JSON block shaped like `{"name": ..., "arguments": {...}}` is a call envelope.
fn check_envelope(opened: usize, block: &str, tools: &Tools) -> Vec<String> {
    let Ok(Value::Object(call)) = serde_json::from_str::<Value>(block) else {
        return Vec::new();
    };
    let (Some(Value::String(name)), Some(Value::Object(arguments))) =
        (call.get("name"), call.get("arguments"))
    else {
        return Vec::new();
    };
    let Some(declared) = tools.get(name) else {
        return vec![format!(
            "{opened}: the JSON example calls `{name}`, which is not a tool"
        )];
    };
    arguments
        .keys()
        .filter(|arg| !declared.contains(*arg))
        .map(|arg| {
            format!("{opened}: the JSON example passes `{arg}`, which `{name}` has no input for")
        })
        .collect()
}

#[test]
fn skills_name_only_declared_tool_inputs() {
    let tools = catalog_tools();
    let skills = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let mut paths: Vec<_> = std::fs::read_dir(&skills)
        .expect("skills directory is readable")
        .map(|entry| {
            entry
                .expect("skills entry is readable")
                .path()
                .join("SKILL.md")
        })
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no SKILL.md files under {}",
        skills.display()
    );

    let problems: Vec<String> = paths
        .iter()
        .flat_map(|path| {
            let skill = path
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let markdown = std::fs::read_to_string(path).expect("SKILL.md is readable");
            check_skill(&markdown, &tools)
                .into_iter()
                .map(move |problem| format!("skills/{skill}/SKILL.md:{problem}"))
        })
        .collect();
    assert!(
        problems.is_empty(),
        "these skills name tool inputs that do not exist; an agent following them gets \
         `Invalid input: unknown field`. Fix the skill text:\n  {}",
        problems.join("\n  ")
    );
}

fn sample_tools() -> Tools {
    let tools: [(&str, &[&str]); 5] = [
        ("redis_scan", &["pattern", "limit", "key_type"]),
        ("create_fixed_subscription", &["name", "plan_id"]),
        (
            "create_fixed_database",
            &["subscription_id", "name", "redis_version"],
        ),
        ("get_fixed_redis_versions", &[]),
        ("list_alerts", &[]),
    ];
    tools
        .into_iter()
        .map(|(tool, inputs)| {
            (
                tool.to_string(),
                inputs.iter().map(|s| s.to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn catches_the_shapes_that_shipped_broken() {
    let tools = sample_tools();
    assert_eq!(
        check_skill("1. Use `redis_scan` with count=100 to sample keys", &tools),
        ["1: `redis_scan` has no `count` input"]
    );
    let subscription = "Call `create_fixed_subscription` with:\n\
                        - `name`: descriptive name\n\
                        - `plan_id`: from the chosen plan\n\
                        - `redis_version` (optional): call `get_fixed_redis_versions` first\n";
    assert_eq!(
        check_skill(subscription, &tools),
        ["4: `create_fixed_subscription` has no `redis_version` input"]
    );
    assert_eq!(
        check_skill("Use `redis_scan` with `cursor` 0", &tools),
        ["1: `redis_scan` has no `cursor` input"]
    );
    let envelope = "```json\n{\"name\":\"redis_scan\",\"arguments\":{\"count\":5}}\n```\n";
    assert_eq!(
        check_skill(envelope, &tools),
        ["1: the JSON example passes `count`, which `redis_scan` has no input for"]
    );
}

#[test]
fn leaves_output_fields_values_and_prose_alone() {
    let tools = sample_tools();
    for prose in [
        "Use `redis_scan` with `limit` 100 to sample keys",
        "Call `create_fixed_database` with:\n- `name`: database name\n- `redis_version` (optional)",
        "Call `list_alerts` to get alert states. Entries with `state: true` are firing.",
        "Call `list_alerts` to get:\n- `status` field — must be `active`",
        "Call `list_alerts` and check that `status == \"active\"` (higher = better)",
        "Connect `redis_scan` to `redis://host:6379/0?protocol=3`",
        "Compare `redis_scan` output with `num_docs` and `create_fixed_database`",
        "```\nredis_scan count=5\n```",
    ] {
        assert_eq!(check_skill(prose, &tools), Vec::<String>::new(), "{prose}");
    }
}
