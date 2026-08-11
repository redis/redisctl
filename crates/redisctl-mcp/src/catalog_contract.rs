//! Compatibility contract for the all-features MCP tool catalog.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tower_mcp::TestClient;

use super::*;

const CATALOG_FORMAT_VERSION: u32 = 1;
#[cfg(all(feature = "cloud", feature = "enterprise", feature = "database"))]
const CATALOG_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/mcp-catalog-v1.json"
);
const CATALOG_JSON: &str = include_str!("../tests/fixtures/mcp-catalog-v1.json");
const SYSTEM_TOOLS: &[&str] = &["list_available_tools", "show_policy"];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    format_version: u32,
    tools: Vec<CatalogTool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogTool {
    name: String,
    toolset: String,
    submodule: Option<String>,
    required_tier: String,
    input_schema: Value,
    output_schema: Option<Value>,
    annotations: CatalogAnnotations,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogAnnotations {
    title: Option<String>,
    read_only_hint: bool,
    destructive_hint: bool,
    idempotent_hint: bool,
    open_world_hint: bool,
}

#[derive(Default)]
struct Changes {
    breaking: Vec<String>,
    additive: Vec<String>,
}

impl Changes {
    fn is_empty(&self) -> bool {
        self.breaking.is_empty() && self.additive.is_empty()
    }

    fn render(&self) -> String {
        let mut output = String::new();
        if !self.breaking.is_empty() {
            output.push_str("\nBreaking or incompatible changes:\n");
            for change in &self.breaking {
                output.push_str(&format!("- {change}\n"));
            }
        }
        if !self.additive.is_empty() {
            output.push_str("\nAdditive changes requiring catalog review:\n");
            for change in &self.additive {
                output.push_str(&format!("- {change}\n"));
            }
        }
        output
    }
}

fn all_toolsets() -> EnabledToolsets {
    #[allow(unused_mut)]
    let mut toolsets = vec![Toolset::App];
    #[cfg(feature = "cloud")]
    toolsets.push(Toolset::Cloud);
    #[cfg(feature = "enterprise")]
    toolsets.push(Toolset::Enterprise);
    #[cfg(feature = "database")]
    toolsets.push(Toolset::Database);
    EnabledToolsets::all_of(toolsets)
}

async fn current_catalog() -> Catalog {
    current_catalog_for_tier(SafetyTier::Full).await
}

async fn current_catalog_for_tier(tier: SafetyTier) -> Catalog {
    let enabled = all_toolsets();
    let tool_toolset = build_tool_toolset_mapping(&enabled);
    let policy_config = PolicyConfig {
        tier,
        ..Default::default()
    };
    let policy = Arc::new(Policy::new(
        policy_config,
        tool_toolset.clone(),
        "catalog-contract".to_string(),
    ));
    let state = Arc::new(
        AppState::new(
            CredentialSource::Profiles(vec![]),
            policy.clone(),
            None,
            false,
            Some("redisctl-mcp-catalog-test".to_string()),
        )
        .expect("catalog state should build"),
    );
    let router = build_router(
        state,
        policy,
        &enabled,
        ToolsConfig::default(),
        &tool_toolset,
        None,
    )
    .expect("catalog router should build");

    let mut client = TestClient::from_router(router);
    client.initialize().await;
    let definitions = client.list_tools().await;

    let mut tools = definitions
        .into_iter()
        .map(|definition| catalog_tool(definition, &tool_toolset))
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| left.name.cmp(&right.name));

    Catalog {
        format_version: CATALOG_FORMAT_VERSION,
        tools,
    }
}

fn catalog_tool(definition: Value, toolsets: &HashMap<String, ToolsetKind>) -> CatalogTool {
    let name = definition
        .get("name")
        .and_then(Value::as_str)
        .expect("tools/list entry should contain a string name")
        .to_string();
    let toolset_kind = toolsets.get(&name).copied();
    let toolset = toolset_kind
        .map(|kind| kind.to_string())
        .or_else(|| {
            SYSTEM_TOOLS
                .contains(&name.as_str())
                .then(|| "system".to_string())
        })
        .unwrap_or_else(|| panic!("tool {name} is missing a toolset assignment"));
    let annotations = normalized_annotations(definition.get("annotations"));
    let required_tier = required_tier(&annotations).to_string();
    let submodule = toolset_kind
        .and_then(|kind| submodule_for_tool(kind, &name))
        .map(str::to_string);

    CatalogTool {
        name,
        toolset,
        submodule,
        required_tier,
        input_schema: contract_schema(
            definition
                .get("inputSchema")
                .cloned()
                .expect("tools/list entry should contain inputSchema"),
        ),
        output_schema: definition.get("outputSchema").cloned().map(contract_schema),
        annotations,
    }
}

fn submodule_for_tool(kind: ToolsetKind, _name: &str) -> Option<&'static str> {
    match kind {
        ToolsetKind::App => None,
        ToolsetKind::Cloud => {
            #[cfg(feature = "cloud")]
            {
                tools::cloud::SUB_MODULES
                    .iter()
                    .find(|submodule| submodule.tool_names.contains(&_name))
                    .map(|submodule| submodule.name)
            }
            #[cfg(not(feature = "cloud"))]
            {
                None
            }
        }
        ToolsetKind::Enterprise => {
            #[cfg(feature = "enterprise")]
            {
                tools::enterprise::SUB_MODULES
                    .iter()
                    .find(|submodule| submodule.tool_names.contains(&_name))
                    .map(|submodule| submodule.name)
            }
            #[cfg(not(feature = "enterprise"))]
            {
                None
            }
        }
        ToolsetKind::Database => {
            #[cfg(feature = "database")]
            {
                tools::redis::SUB_MODULES
                    .iter()
                    .find(|submodule| submodule.tool_names.contains(&_name))
                    .map(|submodule| submodule.name)
            }
            #[cfg(not(feature = "database"))]
            {
                None
            }
        }
    }
}

fn contract_schema(value: Value) -> Value {
    normalize_schema_node(value)
}

fn normalize_schema_node(mut value: Value) -> Value {
    let Value::Object(object) = &mut value else {
        return value;
    };

    for field in ["$schema", "description", "examples", "title"] {
        object.remove(field);
    }

    for field in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "patternProperties",
        "properties",
    ] {
        if let Some(Value::Object(schemas)) = object.get_mut(field) {
            for schema in schemas.values_mut() {
                *schema = normalize_schema_node(schema.take());
            }
        }
    }
    for field in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(schemas)) = object.get_mut(field) {
            for schema in schemas {
                *schema = normalize_schema_node(schema.take());
            }
        }
    }
    for field in [
        "additionalProperties",
        "contains",
        "else",
        "if",
        "items",
        "not",
        "propertyNames",
        "then",
        "unevaluatedProperties",
    ] {
        if let Some(schema) = object.get_mut(field) {
            *schema = normalize_schema_node(schema.take());
        }
    }

    value
}

fn normalized_annotations(value: Option<&Value>) -> CatalogAnnotations {
    let annotations = value.and_then(Value::as_object);
    CatalogAnnotations {
        title: annotations
            .and_then(|object| object.get("title"))
            .and_then(Value::as_str)
            .map(str::to_string),
        read_only_hint: annotation_bool(annotations, "readOnlyHint", false),
        destructive_hint: annotation_bool(annotations, "destructiveHint", true),
        idempotent_hint: annotation_bool(annotations, "idempotentHint", false),
        open_world_hint: annotation_bool(annotations, "openWorldHint", true),
    }
}

fn annotation_bool(annotations: Option<&Map<String, Value>>, name: &str, default: bool) -> bool {
    annotations
        .and_then(|object| object.get(name))
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn required_tier(annotations: &CatalogAnnotations) -> &'static str {
    if annotations.read_only_hint {
        "read-only"
    } else if annotations.destructive_hint {
        "full"
    } else {
        "read-write"
    }
}

fn tier_rank(tier: &str) -> u8 {
    match tier {
        "read-only" => 0,
        "read-write" => 1,
        "full" => 2,
        _ => panic!("unknown catalog safety tier: {tier}"),
    }
}

fn compare_catalog(baseline: &Catalog, current: &Catalog) -> Changes {
    let mut changes = Changes::default();
    if baseline.format_version != CATALOG_FORMAT_VERSION {
        changes.breaking.push(format!(
            "catalog format {} is unsupported; expected {}",
            baseline.format_version, CATALOG_FORMAT_VERSION
        ));
        return changes;
    }

    let baseline_by_name = index_tools(&baseline.tools, "baseline", &mut changes);
    let current_by_name = index_tools(&current.tools, "current catalog", &mut changes);

    for (name, before) in &baseline_by_name {
        let Some(after) = current_by_name.get(name) else {
            changes.breaking.push(format!("tool removed: {name}"));
            continue;
        };

        if before.toolset != after.toolset {
            changes.breaking.push(format!(
                "{name}: toolset changed from {} to {}",
                before.toolset, after.toolset
            ));
        }
        if before.submodule != after.submodule {
            changes.breaking.push(format!(
                "{name}: submodule changed from {:?} to {:?}",
                before.submodule, after.submodule
            ));
        }
        if before.required_tier != after.required_tier {
            changes.breaking.push(format!(
                "{name}: required safety tier changed from {} to {}",
                before.required_tier, after.required_tier
            ));
        }
        compare_annotations(name, &before.annotations, &after.annotations, &mut changes);
        compare_schema(
            &format!("{name}.inputSchema"),
            &before.input_schema,
            &after.input_schema,
            &mut changes,
        );
        compare_output_schema(
            name,
            &before.output_schema,
            &after.output_schema,
            &mut changes,
        );
    }

    for name in current_by_name.keys() {
        if !baseline_by_name.contains_key(name) {
            changes.additive.push(format!("tool added: {name}"));
        }
    }

    changes
}

fn index_tools<'a>(
    tools: &'a [CatalogTool],
    label: &str,
    changes: &mut Changes,
) -> BTreeMap<&'a str, &'a CatalogTool> {
    let mut indexed = BTreeMap::new();
    for tool in tools {
        if indexed.insert(tool.name.as_str(), tool).is_some() {
            changes
                .breaking
                .push(format!("duplicate tool name in {label}: {}", tool.name));
        }
    }
    indexed
}

fn compare_annotations(
    name: &str,
    before: &CatalogAnnotations,
    after: &CatalogAnnotations,
    changes: &mut Changes,
) {
    if before.title != after.title {
        changes.additive.push(format!(
            "{name}: annotation title changed from {:?} to {:?}",
            before.title, after.title
        ));
    }
    for (field, old, new) in [
        ("readOnlyHint", before.read_only_hint, after.read_only_hint),
        (
            "destructiveHint",
            before.destructive_hint,
            after.destructive_hint,
        ),
        (
            "idempotentHint",
            before.idempotent_hint,
            after.idempotent_hint,
        ),
        (
            "openWorldHint",
            before.open_world_hint,
            after.open_world_hint,
        ),
    ] {
        if old != new {
            let message = format!("{name}: {field} changed from {old} to {new}");
            let weakens_safety = matches!(field, "readOnlyHint" | "destructiveHint")
                || (field == "idempotentHint" && old)
                || (field == "openWorldHint" && !old);
            if weakens_safety {
                changes.breaking.push(message);
            } else {
                changes.additive.push(message);
            }
        }
    }
}

fn compare_output_schema(
    name: &str,
    before: &Option<Value>,
    after: &Option<Value>,
    changes: &mut Changes,
) {
    match (before, after) {
        (None, None) => {}
        (None, Some(_)) => changes
            .additive
            .push(format!("{name}: structured output schema added")),
        (Some(_), None) => changes
            .breaking
            .push(format!("{name}: structured output schema removed")),
        (Some(before), Some(after)) if before != after => changes
            .breaking
            .push(format!("{name}: structured output schema changed")),
        (Some(_), Some(_)) => {}
    }
}

fn compare_schema(path: &str, before: &Value, after: &Value, changes: &mut Changes) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            compare_schema_object(path, before, after, changes)
        }
        (Value::Array(before), Value::Array(after)) => {
            if before != after {
                changes
                    .breaking
                    .push(format!("{path}: schema array changed"));
            }
        }
        _ if before != after => changes.breaking.push(format!(
            "{path}: schema value changed from {before} to {after}"
        )),
        _ => {}
    }
}

fn compare_schema_object(
    path: &str,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    changes: &mut Changes,
) {
    compare_named_schemas(path, "properties", before, after, changes);
    compare_named_schemas(path, "$defs", before, after, changes);
    compare_required(path, before, after, changes);
    compare_variants(path, "enum", before, after, changes);
    compare_variants(path, "anyOf", before, after, changes);
    compare_variants(path, "oneOf", before, after, changes);

    const HANDLED: &[&str] = &["properties", "$defs", "required", "enum", "anyOf", "oneOf"];
    const DOCUMENTATION: &[&str] = &["title", "description", "examples"];

    for (key, value) in before {
        if HANDLED.contains(&key.as_str()) || DOCUMENTATION.contains(&key.as_str()) {
            continue;
        }
        match after.get(key) {
            Some(current) => compare_schema(&format!("{path}.{key}"), value, current, changes),
            None if key == "default" => changes.breaking.push(format!("{path}: default removed")),
            None => changes
                .additive
                .push(format!("{path}: constraint removed: {key}")),
        }
    }
    for key in after.keys() {
        if before.contains_key(key)
            || HANDLED.contains(&key.as_str())
            || DOCUMENTATION.contains(&key.as_str())
        {
            continue;
        }
        if key == "default" {
            changes.breaking.push(format!("{path}: default added"));
        } else {
            changes
                .breaking
                .push(format!("{path}: new constraint added: {key}"));
        }
    }
}

fn compare_named_schemas(
    path: &str,
    field: &str,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    changes: &mut Changes,
) {
    let before = before
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let after = after
        .get(field)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (name, schema) in &before {
        match after.get(name) {
            Some(current) => {
                compare_schema(&format!("{path}.{field}.{name}"), schema, current, changes)
            }
            None => changes
                .breaking
                .push(format!("{path}: {field} entry removed: {name}")),
        }
    }
    for name in after.keys() {
        if !before.contains_key(name) {
            let message = format!("{path}: {field} entry added: {name}");
            changes.additive.push(message);
        }
    }
}

fn required_fields(object: &Map<String, Value>) -> BTreeSet<String> {
    object
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn compare_required(
    path: &str,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    changes: &mut Changes,
) {
    let before = required_fields(before);
    let after = required_fields(after);
    for field in before.difference(&after) {
        changes
            .additive
            .push(format!("{path}: input is no longer required: {field}"));
    }
    for field in after.difference(&before) {
        changes
            .breaking
            .push(format!("{path}: new required input: {field}"));
    }
}

fn compare_variants(
    path: &str,
    field: &str,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    changes: &mut Changes,
) {
    let before = before
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let after = after
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for variant in &before {
        if !after.contains(variant) {
            changes.breaking.push(format!(
                "{path}.{field}: accepted schema variant removed: {variant}"
            ));
        }
    }
    for variant in &after {
        if !before.contains(variant) {
            changes.additive.push(format!(
                "{path}.{field}: accepted schema variant added: {variant}"
            ));
        }
    }
}

fn sample_catalog() -> Catalog {
    Catalog {
        format_version: CATALOG_FORMAT_VERSION,
        tools: vec![CatalogTool {
            name: "sample_tool".to_string(),
            toolset: "database".to_string(),
            submodule: Some("keys".to_string()),
            required_tier: "read-only".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }),
            output_schema: None,
            annotations: CatalogAnnotations {
                title: None,
                read_only_hint: true,
                destructive_hint: false,
                idempotent_hint: true,
                open_world_hint: true,
            },
        }],
    }
}

#[test]
fn catalog_comparison_classifies_tool_additions_and_removals() {
    let baseline = sample_catalog();
    let mut current = sample_catalog();
    current.tools.clear();

    let changes = compare_catalog(&baseline, &current);
    assert!(
        changes
            .breaking
            .iter()
            .any(|change| change == "tool removed: sample_tool"),
        "tool removal should be breaking"
    );

    let mut current = sample_catalog();
    let mut added = current.tools[0].clone();
    added.name = "new_tool".to_string();
    current.tools.push(added);
    let changes = compare_catalog(&baseline, &current);
    assert!(
        changes
            .additive
            .iter()
            .any(|change| change == "tool added: new_tool"),
        "tool addition should be reviewable and additive"
    );
}

#[test]
fn catalog_comparison_distinguishes_optional_and_required_inputs() {
    let baseline = sample_catalog();
    let mut current = sample_catalog();
    current.tools[0].input_schema["properties"]["limit"] = serde_json::json!({ "type": "integer" });

    let changes = compare_catalog(&baseline, &current);
    assert!(changes.breaking.is_empty());
    assert!(
        changes
            .additive
            .iter()
            .any(|change| change.contains("properties entry added: limit"))
    );

    current.tools[0].input_schema["required"] = serde_json::json!(["key", "limit"]);
    let changes = compare_catalog(&baseline, &current);
    assert!(
        changes
            .breaking
            .iter()
            .any(|change| change.contains("new required input: limit"))
    );
}

#[test]
fn catalog_comparison_detects_safety_tier_regressions() {
    let mut baseline = sample_catalog();
    baseline.tools[0].annotations.read_only_hint = false;
    baseline.tools[0].required_tier = "read-write".to_string();
    let mut current = baseline.clone();
    current.tools[0].annotations.read_only_hint = true;
    current.tools[0].required_tier = "read-only".to_string();

    let changes = compare_catalog(&baseline, &current);
    assert!(changes.breaking.iter().any(|change| {
        change.contains("required safety tier changed from read-write to read-only")
    }));
    assert!(
        changes
            .breaking
            .iter()
            .any(|change| change.contains("readOnlyHint changed from false to true"))
    );
}

#[test]
fn schema_normalization_preserves_fields_named_like_documentation_keywords() {
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "ExampleInput",
        "properties": {
            "description": {
                "description": "User-provided description",
                "type": "string"
            },
            "title": {
                "description": "User-provided title",
                "type": "string"
            }
        }
    });

    let normalized = contract_schema(schema);
    assert!(normalized.get("$schema").is_none());
    assert!(normalized.get("title").is_none());
    assert_eq!(
        normalized["properties"]["description"],
        serde_json::json!({ "type": "string" })
    );
    assert_eq!(
        normalized["properties"]["title"],
        serde_json::json!({ "type": "string" })
    );
}

#[test]
fn documented_tool_counts_match_catalog() {
    let catalog: Catalog = serde_json::from_str(CATALOG_JSON).expect("catalog fixture is valid");
    let reference_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/docs/mcp/tools-reference.md");
    let reference = std::fs::read_to_string(&reference_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", reference_path.display()));
    let configuration_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/docs/mcp/configuration.md");
    let configuration = std::fs::read_to_string(&configuration_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", configuration_path.display()));

    assert!(
        reference.contains(&format!("**{} tools**", catalog.tools.len())),
        "tools reference total must match the catalog"
    );
    for (toolset, heading) in [
        ("app", "App Toolset"),
        ("cloud", "Cloud Toolset"),
        ("database", "Database Toolset"),
        ("enterprise", "Enterprise Toolset"),
        ("system", "System Tools"),
    ] {
        let count = catalog
            .tools
            .iter()
            .filter(|tool| tool.toolset == toolset)
            .count();
        assert!(
            reference.contains(&format!("## {heading} ({count} tools)")),
            "{toolset} heading count must match the catalog"
        );
        if toolset != "system" {
            let row_prefix = format!("| `{toolset}` |");
            let row = configuration
                .lines()
                .find(|line| line.starts_with(&row_prefix))
                .unwrap_or_else(|| panic!("missing {toolset} row in MCP configuration"));
            assert!(
                row.ends_with(&format!("| {count} |")),
                "{toolset} configuration count must match the catalog"
            );
        }
    }

    let mut submodule_counts = BTreeMap::<(&str, &str), usize>::new();
    for tool in &catalog.tools {
        if let Some(submodule) = tool.submodule.as_deref() {
            *submodule_counts
                .entry((tool.toolset.as_str(), submodule))
                .or_default() += 1;
        }
    }
    for ((toolset, submodule), count) in submodule_counts {
        let noun = if count == 1 { "tool" } else { "tools" };
        assert!(
            reference.contains(&format!("### `{toolset}:{submodule}` ({count} {noun})")),
            "{toolset}:{submodule} heading count must match the catalog"
        );
    }
}

#[tokio::test]
async fn policy_tiers_filter_every_cataloged_tool() {
    let mut baseline: Catalog =
        serde_json::from_str(CATALOG_JSON).expect("catalog fixture is valid");
    baseline
        .tools
        .retain(|tool| compiled_toolset(&tool.toolset));

    for (tier, maximum_rank) in [
        (SafetyTier::ReadOnly, 0),
        (SafetyTier::ReadWrite, 1),
        (SafetyTier::Full, 2),
    ] {
        let actual = current_catalog_for_tier(tier)
            .await
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect::<BTreeSet<_>>();
        let expected = baseline
            .tools
            .iter()
            .filter(|tool| tier_rank(&tool.required_tier) <= maximum_rank)
            .map(|tool| tool.name.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "{tier} policy must expose exactly the cataloged tiers it permits"
        );
    }
}

#[tokio::test]
async fn mcp_catalog_matches_1_0_baseline() {
    let current = current_catalog().await;

    if std::env::var_os("UPDATE_MCP_CATALOG").is_some() {
        #[cfg(not(all(feature = "cloud", feature = "enterprise", feature = "database")))]
        panic!("UPDATE_MCP_CATALOG requires --all-features");
        #[cfg(all(feature = "cloud", feature = "enterprise", feature = "database"))]
        {
            let json = serde_json::to_string_pretty(&current).expect("catalog should serialize");
            std::fs::write(CATALOG_PATH, format!("{json}\n"))
                .expect("catalog fixture should update");
            return;
        }
    }

    let mut baseline: Catalog =
        serde_json::from_str(CATALOG_JSON).expect("catalog fixture is valid");
    baseline
        .tools
        .retain(|tool| compiled_toolset(&tool.toolset));
    let changes = compare_catalog(&baseline, &current);
    assert!(
        changes.is_empty(),
        "MCP catalog changed. Review the classification below, then run \
         `UPDATE_MCP_CATALOG=1 cargo test -p redisctl-mcp --all-features \
         mcp_catalog_matches_1_0_baseline` and inspect the fixture diff.{}",
        changes.render()
    );
}

#[test]
fn bundled_skills_have_valid_metadata_and_tool_references() {
    let known_tools = known_tool_names();
    let skills_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("skills");
    let mut entries = std::fs::read_dir(&skills_dir)
        .expect("bundled skills directory should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("bundled skill entries should be readable");
    entries.sort_by_key(std::fs::DirEntry::file_name);

    let mut skill_names = HashSet::new();
    assert!(
        !entries.is_empty(),
        "at least one bundled skill is expected"
    );
    for entry in entries {
        if !entry
            .file_type()
            .expect("skill entry type should be readable")
            .is_dir()
        {
            continue;
        }
        let directory_name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path().join("SKILL.md");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let skill = parse_skill(&content)
            .unwrap_or_else(|| panic!("{} has invalid frontmatter", path.display()));

        assert_eq!(
            skill.name,
            directory_name,
            "{}: frontmatter name must match its directory",
            path.display()
        );
        assert!(
            skill_names.insert(skill.name.clone()),
            "duplicate bundled skill name: {}",
            skill.name
        );
        assert!(
            !skill.description.trim().is_empty(),
            "{}: description must not be empty",
            path.display()
        );
        assert!(
            !skill.body.trim().is_empty(),
            "{}: body must not be empty",
            path.display()
        );

        let references = skill_tool_references(&skill.body, &known_tools, &path);
        assert!(
            !references.is_empty(),
            "{}: skill must reference at least one registered tool",
            path.display()
        );
    }
}

fn compiled_toolset(toolset: &str) -> bool {
    matches!(toolset, "app" | "system")
        || (cfg!(feature = "cloud") && toolset == "cloud")
        || (cfg!(feature = "enterprise") && toolset == "enterprise")
        || (cfg!(feature = "database") && toolset == "database")
}

fn known_tool_names() -> HashSet<String> {
    let catalog: Catalog = serde_json::from_str(CATALOG_JSON).expect("catalog fixture is valid");
    catalog.tools.into_iter().map(|tool| tool.name).collect()
}

fn skill_tool_references(body: &str, known: &HashSet<String>, path: &Path) -> HashSet<String> {
    let mut references = HashSet::new();
    for line in body.lines() {
        let mut remainder = line;
        while let Some(open) = remainder.find('`') {
            let before = &remainder[..open];
            let after_open = &remainder[open + 1..];
            let Some(close) = after_open.find('`') else {
                break;
            };
            let token = after_open[..close].trim();
            if is_identifier(token) {
                if known.contains(token) {
                    references.insert(token.to_string());
                } else if looks_like_tool_reference(token, before) {
                    panic!(
                        "{}: `{token}` looks like a tool reference but is not registered",
                        path.display()
                    );
                }
            }
            remainder = &after_open[close + 1..];
        }
    }
    references
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn looks_like_tool_reference(token: &str, before: &str) -> bool {
    let tool_verb = [
        "create_",
        "delete_",
        "enterprise_",
        "get_",
        "list_",
        "profile_",
        "redis_",
        "update_",
        "wait_",
    ]
    .iter()
    .any(|prefix| token.starts_with(prefix));
    let preceding_word = before
        .trim_end_matches(|character: char| character.is_whitespace() || character == '*')
        .split(|character: char| !character.is_ascii_alphabetic())
        .next_back()
        .unwrap_or_default()
        .to_ascii_lowercase();
    tool_verb && matches!(preceding_word.as_str(), "call" | "use" | "using" | "with")
}
