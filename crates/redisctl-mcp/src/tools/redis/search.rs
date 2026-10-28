//! RediSearch module tools (FT.CREATE, FT.SEARCH, FT.AGGREGATE, FT.INFO, etc.)

use tower_mcp::{CallToolResult, Error as McpError};

use super::RedisResultExt;

use super::format_value;
use crate::serde_helpers;
use crate::tools::macros::{database_tool, mcp_module};

/// Format alternating key-value pairs from a Redis value slice into `"key: value"` strings.
fn format_kv_pairs(values: &[redis::Value]) -> Vec<String> {
    values
        .chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some(format!(
                    "{}: {}",
                    format_value(&chunk[0]),
                    format_value(&chunk[1])
                ))
            } else {
                None
            }
        })
        .collect()
}

/// Field definition for FT.CREATE and FT.ALTER schema definitions.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// Field name (or JSONPath for JSON indexes)
    pub name: String,
    /// Field type: TEXT, NUMERIC, TAG, GEO, VECTOR, GEOSHAPE
    pub field_type: String,
    /// Make field sortable (enables SORTBY in queries)
    #[serde(default)]
    pub sortable: bool,
    /// Exclude field from indexing (store only)
    #[serde(default)]
    pub noindex: bool,
    /// Disable stemming for TEXT fields
    #[serde(default)]
    pub nostem: bool,
    /// Weight for TEXT fields (default 1.0)
    #[serde(default)]
    pub weight: Option<f64>,
    /// Separator character for TAG fields (default ",")
    #[serde(default)]
    pub separator: Option<String>,
    /// Alias for this field (AS alias)
    #[serde(default)]
    pub alias: Option<String>,
}

const VALID_FIELD_TYPES: &[&str] = &["TEXT", "NUMERIC", "TAG", "GEO", "VECTOR", "GEOSHAPE"];

impl FieldDefinition {
    fn validate(&self) -> Result<(), McpError> {
        let ft = self.field_type.to_uppercase();
        if !VALID_FIELD_TYPES.contains(&ft.as_str()) {
            return Err(McpError::tool(format!(
                "Invalid field_type '{}' for field '{}'. Valid types: {}",
                self.field_type,
                self.name,
                VALID_FIELD_TYPES.join(", "),
            )));
        }
        if let Some(ref sep) = self.separator
            && sep.chars().count() != 1
        {
            return Err(McpError::tool(format!(
                "Invalid separator '{}' for field '{}'. Must be a single character",
                sep, self.name,
            )));
        }
        Ok(())
    }

    fn to_args(&self) -> Vec<String> {
        let mut args = vec![self.name.clone()];
        if let Some(ref alias) = self.alias {
            args.push("AS".to_string());
            args.push(alias.clone());
        }
        args.push(self.field_type.to_uppercase());
        if self.nostem {
            args.push("NOSTEM".to_string());
        }
        if let Some(weight) = self.weight {
            args.push("WEIGHT".to_string());
            args.push(weight.to_string());
        }
        if let Some(ref sep) = self.separator {
            args.push("SEPARATOR".to_string());
            args.push(sep.clone());
        }
        if self.sortable {
            args.push("SORTABLE".to_string());
        }
        if self.noindex {
            args.push("NOINDEX".to_string());
        }
        args
    }
}

mcp_module! {
    ft_list => "redis_ft_list",
    ft_info => "redis_ft_info",
    ft_search => "redis_ft_search",
    ft_aggregate => "redis_ft_aggregate",
    ft_explain => "redis_ft_explain",
    ft_profile => "redis_ft_profile",
    ft_tagvals => "redis_ft_tagvals",
    ft_syndump => "redis_ft_syndump",
    ft_dictdump => "redis_ft_dictdump",
    ft_create => "redis_ft_create",
    ft_alter => "redis_ft_alter",
    ft_synupdate => "redis_ft_synupdate",
    ft_dictadd => "redis_ft_dictadd",
    ft_aliasadd => "redis_ft_aliasadd",
    ft_aliasupdate => "redis_ft_aliasupdate",
    ft_dropindex => "redis_ft_dropindex",
    ft_aliasdel => "redis_ft_aliasdel",
    ft_dictdel => "redis_ft_dictdel"
}

// ---------------------------------------------------------------------------
// Read-only tools
// ---------------------------------------------------------------------------

database_tool!(read_only, ft_list, "redis_ft_list",
    "List all search indexes. Requires the RediSearch module.",
    {} => |conn, _input| {
        let indexes: Vec<String> = redis::cmd("FT._LIST")
            .query_async(&mut conn)
            .await
            .tool_context("FT._LIST failed")?;

        if indexes.is_empty() {
            Ok(CallToolResult::text("No search indexes found"))
        } else {
            Ok(CallToolResult::text(format!(
                "Search indexes ({}):\n{}",
                indexes.len(),
                indexes.join("\n")
            )))
        }
    }
);

database_tool!(read_only, ft_info, "redis_ft_info",
    "Get search index metadata including fields, document count, and indexing status. Requires the RediSearch module.",
    {
        /// Index name
        pub index: String,
    } => |conn, input| {
        let result: Vec<redis::Value> = redis::cmd("FT.INFO")
            .arg(&input.index)
            .query_async(&mut conn)
            .await
            .tool_context("FT.INFO failed")?;

        // FT.INFO returns alternating key-value pairs
        let mut output = format!("Index: {}\n\n", input.index);
        for pair in format_kv_pairs(&result) {
            output.push_str(&pair);
            output.push('\n');
        }

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, ft_search, "redis_ft_search",
    "Execute a full-text search query against an index. Requires the RediSearch module.\n\n\
     Query syntax examples:\n\
     - Full-text: `wireless headphones`\n\
     - Field-specific text: `@name:wireless`\n\
     - TAG exact match: `@category:{electronics}`\n\
     - Multiple tags: `@category:{electronics|sports}`\n\
     - Numeric range: `@price:[50 200]`\n\
     - Numeric comparison: `@rating:[(4 +inf]` (greater than 4)\n\
     - Negation: `-@category:{kitchen}`\n\
     - Wildcard: `wire*`\n\
     - All documents: `*`\n\
     - Combined: `@category:{electronics} @price:[0 100]`\n\n\
     For JSON indexes, use field aliases (set via AS in FT.CREATE) in queries, not the raw JSONPath.\n\n\
     Field-specific queries (e.g. `@name:value`) only work on fields that are included in the index schema. \
     Querying a field that is not indexed returns zero results without an error.\n\n\
     Architectural note: search indexes decouple query patterns from key structure. If you're \
     doing SCAN + client-side filter, a search index may be simpler and significantly faster.",
    {
        /// Index name
        pub index: String,
        /// Search query (e.g. "@title:hello", "*" for all)
        pub query: String,
        /// Result offset for pagination
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub limit_offset: Option<u64>,
        /// Number of results to return
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub limit_num: Option<u64>,
        /// Sort by field name
        #[serde(default)]
        pub sortby: Option<String>,
        /// Sort order: ASC or DESC
        #[serde(default)]
        pub sortby_order: Option<String>,
        /// Fields to return (empty = all fields)
        #[serde(default)]
        pub return_fields: Option<Vec<String>>,
        /// Return document IDs only (no content)
        #[serde(default)]
        pub nocontent: bool,
        /// Do not stem query terms
        #[serde(default)]
        pub verbatim: bool,
        /// Include match scores in results
        #[serde(default)]
        pub withscores: bool,
    } => |conn, input| {
        if let Some(ref order) = input.sortby_order {
            let upper = order.to_uppercase();
            if upper != "ASC" && upper != "DESC" {
                return Err(McpError::tool(format!(
                    "Invalid sortby_order '{}'. Valid values: ASC, DESC",
                    order,
                )));
            }
        }

        let mut cmd = redis::cmd("FT.SEARCH");
        cmd.arg(&input.index).arg(&input.query);

        if input.nocontent {
            cmd.arg("NOCONTENT");
        }
        if input.verbatim {
            cmd.arg("VERBATIM");
        }
        if input.withscores {
            cmd.arg("WITHSCORES");
        }
        if let Some(ref fields) = input.return_fields {
            cmd.arg("RETURN").arg(fields.len());
            for f in fields {
                cmd.arg(f);
            }
        }
        if let Some(ref field) = input.sortby {
            cmd.arg("SORTBY").arg(field);
            if let Some(ref order) = input.sortby_order {
                cmd.arg(order);
            }
        }
        if input.limit_offset.is_some() || input.limit_num.is_some() {
            cmd.arg("LIMIT")
                .arg(input.limit_offset.unwrap_or(0))
                .arg(input.limit_num.unwrap_or(10));
        }

        let result: Vec<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.SEARCH failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text("No results"));
        }

        // First element is total count
        let total = format_value(&result[0]);
        let mut output = format!("Total results: {}\n\n", total);

        // Remaining elements: doc_id, [field, value, ...] pairs
        let mut i = 1;
        let mut doc_num = 1;
        while i < result.len() {
            let doc_id = format_value(&result[i]);
            output.push_str(&format!("{}. {}", doc_num, doc_id));
            i += 1;

            // If withscores, next element is the score
            if input.withscores && i < result.len() {
                let score = format_value(&result[i]);
                output.push_str(&format!(" (score: {})", score));
                i += 1;
            }
            output.push('\n');

            // If not nocontent, next element is the field array
            if !input.nocontent && i < result.len() {
                if let redis::Value::Array(ref fields) = result[i] {
                    for pair in format_kv_pairs(fields) {
                        output.push_str(&format!("  {}\n", pair));
                    }
                }
                i += 1;
            }
            doc_num += 1;
        }

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, ft_aggregate, "redis_ft_aggregate",
    "Execute an aggregation query against a search index. Requires the RediSearch module.\n\n\
     An aggregation needs a pipeline: set group_by/reduce, load_fields, sort_by, or raw_args. \
     Calling with only index and query is rejected.\n\n\
     Structured pipeline (preferred):\n\
     - group_by: fields to group on, e.g. [\"@category\"] ([] groups everything into one row)\n\
     - reduce: one reducer per entry, e.g. [\"COUNT 0 AS count\", \"AVG 1 @price AS avg_price\"]\n\
     - sort_by: properties with optional ASC/DESC, optionally followed by MAX and a count, \
       e.g. [\"@count\", \"DESC\", \"MAX\", \"10\"]. Use @ for keyword-named properties.\n\
     - load_fields: document fields to load when not grouping\n\n\
     raw_args appends any other FT.AGGREGATE steps after GROUPBY/REDUCE and before SORTBY, e.g. \
     [\"APPLY\", \"@price * 1.1\", \"AS\", \"price_with_tax\"] or [\"FILTER\", \"@count > 5\"].\n\n\
     Architectural note: FT.AGGREGATE can replace client-side fan-out queries across many keys \
     with a single server-side aggregation. If you're doing SCAN + filter + reduce, an \
     aggregation pipeline is likely simpler and faster.",
    {
        /// Index name
        pub index: String,
        /// Search query (use "*" for all documents)
        pub query: String,
        /// Fields to load from the document (LOAD)
        #[serde(default)]
        pub load_fields: Option<Vec<String>>,
        /// Fields to group by (GROUPBY). Use an empty list to aggregate all results into one row.
        #[serde(default)]
        pub group_by: Option<Vec<String>>,
        /// Reducers, one per entry, as "FUNC nargs args... AS alias"
        /// (e.g. "COUNT 0 AS count", "AVG 1 @price AS avg_price"). Requires group_by.
        #[serde(default)]
        pub reduce: Option<Vec<String>>,
        /// SORTBY properties with optional ASC/DESC and trailing MAX count
        /// (e.g. `["@count", "DESC", "MAX", "10"]`). Use @ for keyword-named properties.
        /// Applied after raw_args.
        #[serde(default)]
        pub sort_by: Option<Vec<String>>,
        /// Result offset for pagination
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub limit_offset: Option<u64>,
        /// Number of results to return
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub limit_num: Option<u64>,
        /// Additional raw FT.AGGREGATE arguments (APPLY, FILTER, ...). Appended after
        /// GROUPBY/REDUCE and before SORTBY.
        #[serde(default)]
        pub raw_args: Option<Vec<String>>,
    } => |conn, input| {
        let args = build_aggregate_args(&input).map_err(McpError::tool)?;

        let mut cmd = redis::cmd("FT.AGGREGATE");
        cmd.arg(&input.index).arg(&input.query);
        for arg in &args {
            cmd.arg(arg);
        }

        let result: Vec<redis::Value> = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.AGGREGATE failed")?;

        // The first element of a non-cursor FT.AGGREGATE reply is not a reliable
        // total (it is 1 for pipelines without GROUPBY), so report the rows we got.
        let rows = result.iter().skip(1);
        let mut output = format!("Results: {}\n\n", rows.len());

        for (idx, row) in rows.enumerate() {
            output.push_str(&format!("{}. ", idx + 1));
            if let redis::Value::Array(fields) = row {
                output.push_str(&format_kv_pairs(fields).join(", "));
            } else {
                output.push_str(&format_value(row));
            }
            output.push('\n');
        }

        Ok(CallToolResult::text(output))
    }
);

/// Prefix a property reference with `@` unless it already has one.
fn field_ref(field: &str) -> String {
    let trimmed = field.trim();
    if trimmed.starts_with('@') {
        trimmed.to_string()
    } else {
        format!("@{trimmed}")
    }
}

/// SORTBY counts only properties/directions; the optional MAX clause follows them.
fn aggregate_sort_args(sort: &[String]) -> Result<Vec<String>, String> {
    if sort.is_empty() {
        return Err("sort_by must name at least one field, e.g. [\"@count\", \"DESC\"]".into());
    }

    let mut fields = Vec::new();
    let mut max = None;
    let mut tokens = sort.iter().map(|s| s.trim()).peekable();
    while let Some(property) = tokens.next() {
        if !fields.is_empty() && property.eq_ignore_ascii_case("MAX") {
            let count = tokens
                .next()
                .ok_or("sort_by MAX requires a non-negative integer count")?;
            if count.parse::<u64>().is_err() || tokens.next().is_some() {
                return Err(
                    "sort_by MAX must be followed by one non-negative integer at the end".into(),
                );
            }
            max = Some(count);
            break;
        }
        if property.is_empty() {
            return Err("sort_by properties must not be empty".into());
        }
        fields.push(field_ref(property));
        if let Some(direction) =
            tokens.next_if(|s| s.eq_ignore_ascii_case("ASC") || s.eq_ignore_ascii_case("DESC"))
        {
            fields.push(direction.to_ascii_uppercase());
        }
    }

    let mut args = vec!["SORTBY".into(), fields.len().to_string()];
    args.extend(fields);
    if let Some(count) = max {
        args.extend(["MAX".into(), count.into()]);
    }
    Ok(args)
}

/// Build the FT.AGGREGATE arguments that follow `index query`.
///
/// Order: LOAD, GROUPBY + REDUCE, raw_args, SORTBY, LIMIT. Returns an error message
/// when the input describes no pipeline at all, or when `reduce` is given without
/// `group_by`.
fn build_aggregate_args(input: &FtAggregateInput) -> Result<Vec<String>, String> {
    if input.reduce.is_some() && input.group_by.is_none() {
        return Err(
            "reduce requires group_by. Use group_by=[] to reduce over all results.".to_string(),
        );
    }

    let has_pipeline = input.load_fields.is_some()
        || input.group_by.is_some()
        || input.sort_by.is_some()
        || input.raw_args.as_ref().is_some_and(|args| !args.is_empty());
    if !has_pipeline {
        return Err(
            "FT.AGGREGATE needs a pipeline. Set group_by (with reduce) to aggregate, \
             load_fields to project document fields, sort_by to order, or raw_args for \
             other steps. Example: group_by=[\"@category\"], reduce=[\"COUNT 0 AS count\"]. \
             For a plain document search use redis_ft_search."
                .to_string(),
        );
    }

    let mut args: Vec<String> = Vec::new();

    if let Some(fields) = &input.load_fields {
        args.push("LOAD".into());
        args.push(fields.len().to_string());
        args.extend(fields.iter().cloned());
    }

    if let Some(fields) = &input.group_by {
        args.push("GROUPBY".into());
        args.push(fields.len().to_string());
        args.extend(fields.iter().map(|f| field_ref(f)));
        for (i, reducer) in input.reduce.iter().flatten().enumerate() {
            let mut tokens = reducer.split_whitespace().peekable();
            if tokens
                .peek()
                .is_some_and(|t| t.eq_ignore_ascii_case("REDUCE"))
            {
                tokens.next();
            }
            let tokens: Vec<&str> = tokens.collect();
            if tokens.len() < 2 {
                return Err(format!(
                    "reduce[{i}] '{reducer}' must be 'FUNC nargs args... AS alias', \
                     e.g. 'COUNT 0 AS count' or 'AVG 1 @price AS avg_price'"
                ));
            }
            args.push("REDUCE".into());
            args.extend(tokens.iter().map(|t| t.to_string()));
        }
    }

    if let Some(raw) = &input.raw_args {
        args.extend(raw.iter().cloned());
    }

    if let Some(sort) = &input.sort_by {
        args.extend(aggregate_sort_args(sort)?);
    }

    if input.limit_offset.is_some() || input.limit_num.is_some() {
        args.push("LIMIT".into());
        args.push(input.limit_offset.unwrap_or(0).to_string());
        args.push(input.limit_num.unwrap_or(10).to_string());
    }

    Ok(args)
}

database_tool!(read_only, ft_explain, "redis_ft_explain",
    "Get the execution plan for a search query. Useful for understanding and optimizing queries. Requires the RediSearch module.",
    {
        /// Index name
        pub index: String,
        /// Search query to explain
        pub query: String,
    } => |conn, input| {
        let plan: String = redis::cmd("FT.EXPLAIN")
            .arg(&input.index)
            .arg(&input.query)
            .query_async(&mut conn)
            .await
            .tool_context("FT.EXPLAIN failed")?;

        Ok(CallToolResult::text(format!(
            "Query plan for '{}' on index '{}':\n\n{}",
            input.query, input.index, plan
        )))
    }
);

database_tool!(read_only, ft_profile, "redis_ft_profile",
    "Profile a search or aggregate query to analyze performance. Shows timing and index intersection details. Requires the RediSearch module.\n\n\
     The output includes:\n\
     - **Iterators profile**: Shows which index iterators were used (TEXT, TAG, NUMERIC, INTERSECT, UNION), \
     how many documents each scanned (Counter), and the index size (Size). \
     Intersect iterators combine multiple conditions; the smallest child drives performance.\n\
     - **Result processors**: Shows time spent in scoring, sorting, and loading document content.\n\
     - **Total profile time**: Wall-clock microseconds for the entire query.\n\n\
     Use this to identify which query clause is the bottleneck and whether adding/removing index fields would help.",
    {
        /// Index name
        pub index: String,
        /// Command type: SEARCH or AGGREGATE
        pub command: String,
        /// Query to profile
        pub query: String,
    } => |conn, input| {
        let command_upper = input.command.to_uppercase();
        if command_upper != "SEARCH" && command_upper != "AGGREGATE" {
            return Err(McpError::tool(format!(
                "Invalid command '{}'. Valid values: SEARCH, AGGREGATE",
                input.command,
            )));
        }

        let result: Vec<redis::Value> = redis::cmd("FT.PROFILE")
            .arg(&input.index)
            .arg(&command_upper)
            .arg("QUERY")
            .arg(&input.query)
            .query_async(&mut conn)
            .await
            .tool_context("FT.PROFILE failed")?;

        // FT.PROFILE returns [results, profile_data]
        let mut output = format!(
            "Profile for {} '{}' on '{}':\n\n",
            command_upper, input.query, input.index
        );
        for (i, val) in result.iter().enumerate() {
            output.push_str(&format!("[{}]: {}\n", i, format_value(val)));
        }

        Ok(CallToolResult::text(output))
    }
);

database_tool!(read_only, ft_tagvals, "redis_ft_tagvals",
    "Get all distinct values of a TAG field in an index. Requires the RediSearch module.",
    {
        /// Index name
        pub index: String,
        /// TAG field name
        pub field: String,
    } => |conn, input| {
        let values: Vec<String> = redis::cmd("FT.TAGVALS")
            .arg(&input.index)
            .arg(&input.field)
            .query_async(&mut conn)
            .await
            .tool_context("FT.TAGVALS failed")?;

        if values.is_empty() {
            Ok(CallToolResult::text(format!(
                "No values for tag field '{}' in index '{}'", input.field, input.index
            )))
        } else {
            Ok(CallToolResult::text(format!(
                "Tag values for '{}' ({}):\n{}",
                input.field, values.len(), values.join("\n")
            )))
        }
    }
);

database_tool!(read_only, ft_syndump, "redis_ft_syndump",
    "Dump synonym groups for an index. Requires the RediSearch module.",
    {
        /// Index name
        pub index: String,
    } => |conn, input| {
        let result: Vec<redis::Value> = redis::cmd("FT.SYNDUMP")
            .arg(&input.index)
            .query_async(&mut conn)
            .await
            .tool_context("FT.SYNDUMP failed")?;

        if result.is_empty() {
            Ok(CallToolResult::text(format!(
                "No synonym groups for index '{}'", input.index
            )))
        } else {
            let mut output = format!("Synonym groups for '{}':\n\n", input.index);
            for pair in format_kv_pairs(&result) {
                output.push_str(&pair);
                output.push('\n');
            }
            Ok(CallToolResult::text(output))
        }
    }
);

database_tool!(read_only, ft_dictdump, "redis_ft_dictdump",
    "Dump all terms in a dictionary. Requires the RediSearch module.",
    {
        /// Dictionary name
        pub dict: String,
    } => |conn, input| {
        let terms: Vec<String> = redis::cmd("FT.DICTDUMP")
            .arg(&input.dict)
            .query_async(&mut conn)
            .await
            .tool_context("FT.DICTDUMP failed")?;

        if terms.is_empty() {
            Ok(CallToolResult::text(format!(
                "Dictionary '{}' is empty", input.dict
            )))
        } else {
            Ok(CallToolResult::text(format!(
                "Dictionary '{}' ({} terms):\n{}",
                input.dict, terms.len(), terms.join("\n")
            )))
        }
    }
);

// ---------------------------------------------------------------------------
// Write tools (non-destructive)
// ---------------------------------------------------------------------------

database_tool!(write, ft_create, "redis_ft_create",
    "Create a search index with the specified schema. Requires the RediSearch module.\n\n\
     For JSON indexes, set `on` to `JSON` and use JSONPath expressions as field names (e.g. `$.name`). \
     Always set `alias` on JSON fields to provide clean query names (e.g. alias `name` for `$.name`), \
     since raw JSONPath cannot be used in search queries.\n\n\
     For HASH indexes (default), use plain field names (e.g. `name`, `price`) — no JSONPath or alias needed.\n\n\
     Field types: TEXT (full-text searchable, stemmed by default), NUMERIC (range queries), \
     TAG (exact match/filtering, case-insensitive), GEO (geo queries), VECTOR (similarity search).\n\n\
     Example schema for JSON:\n\
     ```\n\
     {\"name\": \"$.name\", \"alias\": \"name\", \"field_type\": \"TEXT\", \"sortable\": true}\n\
     {\"name\": \"$.price\", \"alias\": \"price\", \"field_type\": \"NUMERIC\", \"sortable\": true}\n\
     {\"name\": \"$.category\", \"alias\": \"category\", \"field_type\": \"TAG\"}\n\
     ```\n\n\
     Note: Changing a schema requires dropping and recreating the index — indexes do not auto-update. \
     Use FT.ALIASUPDATE for zero-downtime index swaps.\n\n\
     if_exists controls behavior when the index already exists:\n\
     - \"error\" (default): return an error\n\
     - \"skip\": succeed without recreating — use this for idempotent startup/bootstrap code\n\
     - \"drop\": drop and recreate with the new schema — use this when iterating on index design. \
     Only the index definition is dropped; documents are not deleted.",
    {
        /// Index name
        pub index: String,
        /// Data type to index: HASH or JSON (default: HASH)
        #[serde(default)]
        pub on: Option<String>,
        /// Key prefixes to index (e.g. ["user:", "product:"])
        #[serde(default)]
        pub prefixes: Option<Vec<String>>,
        /// Schema field definitions
        pub schema: Vec<FieldDefinition>,
        /// What to do if the index already exists:
        /// - "error" (default): return an error
        /// - "skip": return success without recreating (idempotent startup pattern)
        /// - "drop": drop the existing index then create (schema migration / iterative prototyping).
        ///   Only the index definition is dropped — documents are not deleted.
        #[serde(default)]
        pub if_exists: Option<String>,
    } => |conn, input| {
        if input.schema.is_empty() {
            return Err(McpError::tool("schema must contain at least one field definition"));
        }
        if let Some(ref on) = input.on {
            let upper = on.to_uppercase();
            if upper != "HASH" && upper != "JSON" {
                return Err(McpError::tool(format!(
                    "Invalid 'on' value '{}'. Valid values: HASH, JSON",
                    on,
                )));
            }
        }
        for field in &input.schema {
            field.validate()?;
        }

        let if_exists = input.if_exists.as_deref().unwrap_or("error").to_lowercase();
        match if_exists.as_str() {
            "error" | "skip" | "drop" => {}
            other => return Err(McpError::tool(format!(
                "Invalid if_exists value '{}'. Valid values: error, skip, drop", other
            ))),
        }

        if if_exists == "skip" || if_exists == "drop" {
            let exists: bool = redis::cmd("FT.INFO")
                .arg(&input.index)
                .query_async::<redis::Value>(&mut conn)
                .await
                .map(|_| true)
                .unwrap_or(false);

            if exists {
                if if_exists == "skip" {
                    return Ok(CallToolResult::text(format!(
                        "Index '{}' already exists — skipped (if_exists=skip)",
                        input.index
                    )));
                }
                // drop
                let _: () = redis::cmd("FT.DROPINDEX")
                    .arg(&input.index)
                    .query_async(&mut conn)
                    .await
                    .tool_context("FT.DROPINDEX failed during if_exists=drop")?;
            }
        }

        let mut cmd = redis::cmd("FT.CREATE");
        cmd.arg(&input.index);

        if let Some(ref on) = input.on {
            cmd.arg("ON").arg(on.to_uppercase());
        }
        if let Some(ref prefixes) = input.prefixes {
            cmd.arg("PREFIX").arg(prefixes.len());
            for p in prefixes {
                cmd.arg(p);
            }
        }

        cmd.arg("SCHEMA");
        for field in &input.schema {
            for arg in field.to_args() {
                cmd.arg(arg);
            }
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.CREATE failed")?;

        let field_summary = input.schema.iter()
            .map(|f| format!("{} ({})", f.name, f.field_type))
            .collect::<Vec<_>>()
            .join(", ");

        let note = if if_exists == "drop" { " (replaced existing index)" } else { "" };
        Ok(CallToolResult::text(format!(
            "Created index '{}' with {} field(s): {}{}",
            input.index, input.schema.len(), field_summary, note
        )))
    }
);

database_tool!(write, ft_alter, "redis_ft_alter",
    "Add a field to an existing search index. Requires the RediSearch module.",
    {
        /// Index name
        pub index: String,
        /// Field definition to add
        pub field: FieldDefinition,
    } => |conn, input| {
        input.field.validate()?;

        let mut cmd = redis::cmd("FT.ALTER");
        cmd.arg(&input.index).arg("SCHEMA").arg("ADD");
        for arg in input.field.to_args() {
            cmd.arg(arg);
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.ALTER failed")?;

        Ok(CallToolResult::text(format!(
            "Added field '{}' ({}) to index '{}'",
            input.field.name, input.field.field_type, input.index
        )))
    }
);

database_tool!(write, ft_synupdate, "redis_ft_synupdate",
    "Update a synonym group for an index. Requires the RediSearch module.",
    {
        /// Index name
        pub index: String,
        /// Synonym group ID
        pub group_id: String,
        /// Terms in the synonym group
        pub terms: Vec<String>,
    } => |conn, input| {
        if input.terms.is_empty() {
            return Err(McpError::tool("terms must contain at least one synonym term"));
        }

        let mut cmd = redis::cmd("FT.SYNUPDATE");
        cmd.arg(&input.index).arg(&input.group_id);
        for term in &input.terms {
            cmd.arg(term);
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.SYNUPDATE failed")?;

        Ok(CallToolResult::text(format!(
            "Updated synonym group '{}' in index '{}' with {} term(s)",
            input.group_id, input.index, input.terms.len()
        )))
    }
);

database_tool!(write, ft_dictadd, "redis_ft_dictadd",
    "Add terms to a dictionary for spell checking or auto-complete. Requires the RediSearch module.",
    {
        /// Dictionary name
        pub dict: String,
        /// Terms to add
        pub terms: Vec<String>,
    } => |conn, input| {
        if input.terms.is_empty() {
            return Err(McpError::tool("terms must contain at least one term to add"));
        }

        let mut cmd = redis::cmd("FT.DICTADD");
        cmd.arg(&input.dict);
        for term in &input.terms {
            cmd.arg(term);
        }

        let added: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.DICTADD failed")?;

        Ok(CallToolResult::text(format!(
            "Added {} term(s) to dictionary '{}'", added, input.dict
        )))
    }
);

database_tool!(write, ft_aliasadd, "redis_ft_aliasadd",
    "Add an alias for a search index. Requires the RediSearch module.",
    {
        /// Alias name
        pub alias: String,
        /// Index name
        pub index: String,
    } => |conn, input| {
        let _: () = redis::cmd("FT.ALIASADD")
            .arg(&input.alias)
            .arg(&input.index)
            .query_async(&mut conn)
            .await
            .tool_context("FT.ALIASADD failed")?;

        Ok(CallToolResult::text(format!(
            "Added alias '{}' for index '{}'", input.alias, input.index
        )))
    }
);

database_tool!(write, ft_aliasupdate, "redis_ft_aliasupdate",
    "Update an alias to point to a different index. Useful for zero-downtime index migrations. Requires the RediSearch module.",
    {
        /// Alias name
        pub alias: String,
        /// New index name
        pub index: String,
    } => |conn, input| {
        let _: () = redis::cmd("FT.ALIASUPDATE")
            .arg(&input.alias)
            .arg(&input.index)
            .query_async(&mut conn)
            .await
            .tool_context("FT.ALIASUPDATE failed")?;

        Ok(CallToolResult::text(format!(
            "Updated alias '{}' to point to index '{}'", input.alias, input.index
        )))
    }
);

// ---------------------------------------------------------------------------
// Destructive tools
// ---------------------------------------------------------------------------

database_tool!(destructive, ft_dropindex, "redis_ft_dropindex",
    "DANGEROUS: Drop a search index. Use delete_docs=true to also delete the indexed documents.",
    {
        /// Index name to drop
        pub index: String,
        /// Also delete the indexed documents (DD flag)
        #[serde(default)]
        pub delete_docs: bool,
    } => |conn, input| {
        let mut cmd = redis::cmd("FT.DROPINDEX");
        cmd.arg(&input.index);
        if input.delete_docs {
            cmd.arg("DD");
        }

        let _: () = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.DROPINDEX failed")?;

        let dd_note = if input.delete_docs { " (documents also deleted)" } else { "" };
        Ok(CallToolResult::text(format!(
            "Dropped index '{}'{}",  input.index, dd_note
        )))
    }
);

database_tool!(write, ft_aliasdel, "redis_ft_aliasdel",
    "Delete a search index alias.",
    {
        /// Alias name to delete
        pub alias: String,
    } => |conn, input| {
        let _: () = redis::cmd("FT.ALIASDEL")
            .arg(&input.alias)
            .query_async(&mut conn)
            .await
            .tool_context("FT.ALIASDEL failed")?;

        Ok(CallToolResult::text(format!(
            "Deleted alias '{}'", input.alias
        )))
    }
);

database_tool!(write, ft_dictdel, "redis_ft_dictdel",
    "Remove terms from a dictionary.",
    {
        /// Dictionary name
        pub dict: String,
        /// Terms to remove
        pub terms: Vec<String>,
    } => |conn, input| {
        if input.terms.is_empty() {
            return Err(McpError::tool("terms must contain at least one term to remove"));
        }

        let mut cmd = redis::cmd("FT.DICTDEL");
        cmd.arg(&input.dict);
        for term in &input.terms {
            cmd.arg(term);
        }

        let removed: i64 = cmd
            .query_async(&mut conn)
            .await
            .tool_context("FT.DICTDEL failed")?;

        Ok(CallToolResult::text(format!(
            "Removed {} term(s) from dictionary '{}'", removed, input.dict
        )))
    }
);

#[cfg(test)]
mod tests {
    use super::{FtAggregateInput, build_aggregate_args};

    fn input(json: serde_json::Value) -> Result<FtAggregateInput, serde_json::Error> {
        serde_json::from_value(json)
    }

    #[test]
    fn group_by_and_reduce_build_a_pipeline() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*",
            "group_by": ["category"],
            "reduce": ["COUNT 0 AS count", "REDUCE AVG 1 @price AS avg_price"],
            "sort_by": ["count", "DESC"],
            "limit_num": 5
        }))
        .unwrap();
        let args = build_aggregate_args(&input).unwrap();
        assert_eq!(
            args,
            [
                "GROUPBY",
                "1",
                "@category",
                "REDUCE",
                "COUNT",
                "0",
                "AS",
                "count",
                "REDUCE",
                "AVG",
                "1",
                "@price",
                "AS",
                "avg_price",
                "SORTBY",
                "2",
                "@count",
                "DESC",
                "LIMIT",
                "0",
                "5",
            ]
        );
    }

    #[test]
    fn group_by_keyword_names_are_properties() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*",
            "group_by": ["max", "asc", "DeSc", "category", "@max"]
        }))
        .unwrap();
        assert_eq!(
            build_aggregate_args(&input).unwrap(),
            ["GROUPBY", "5", "@max", "@asc", "@DeSc", "@category", "@max"]
        );
    }

    #[test]
    fn sort_max_is_outside_the_sort_argument_count() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*",
            "sort_by": ["count", "desc", "@category", "ASC", "max", "10"],
            "limit_offset": 2, "limit_num": 3
        }))
        .unwrap();
        assert_eq!(
            build_aggregate_args(&input).unwrap(),
            [
                "SORTBY",
                "4",
                "@count",
                "DESC",
                "@category",
                "ASC",
                "MAX",
                "10",
                "LIMIT",
                "2",
                "3"
            ]
        );
    }

    #[test]
    fn sort_properties_and_optional_directions_are_positional() {
        for (sort, expected) in [
            (vec!["max"], vec!["SORTBY", "1", "@max"]),
            (vec!["asc", "desc"], vec!["SORTBY", "2", "@asc", "DESC"]),
            (
                vec!["count", "@max", "DESC"],
                vec!["SORTBY", "3", "@count", "@max", "DESC"],
            ),
            (
                vec!["count", "MAX", "0"],
                vec!["SORTBY", "1", "@count", "MAX", "0"],
            ),
        ] {
            let input = input(serde_json::json!({
                "index": "idx", "query": "*", "sort_by": sort
            }))
            .unwrap();
            assert_eq!(build_aggregate_args(&input).unwrap(), expected);
        }
    }

    #[test]
    fn malformed_sort_max_is_rejected() {
        for sort in [
            vec!["count", "DESC", "MAX"],
            vec!["count", "MAX", "-1"],
            vec!["count", "MAX", "@10"],
            vec!["count", "MAX", "1.5"],
            vec!["count", "MAX", "18446744073709551616"],
            vec!["count", "MAX", "10", "DESC"],
        ] {
            let input = input(serde_json::json!({
                "index": "idx", "query": "*", "sort_by": sort
            }))
            .unwrap();
            assert!(build_aggregate_args(&input).unwrap_err().contains("MAX"));
        }
    }

    #[test]
    fn empty_group_by_reduces_over_everything() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*", "group_by": [], "reduce": ["COUNT 0 AS total"]
        }))
        .unwrap();
        let args = build_aggregate_args(&input).unwrap();
        assert_eq!(
            args,
            ["GROUPBY", "0", "REDUCE", "COUNT", "0", "AS", "total"]
        );
    }

    #[test]
    fn raw_args_sit_between_group_and_sort() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*",
            "load_fields": ["name", "price"],
            "raw_args": ["APPLY", "@price * 1.1", "AS", "with_tax"],
            "sort_by": ["@with_tax", "DESC"]
        }))
        .unwrap();
        let args = build_aggregate_args(&input).unwrap();
        assert_eq!(
            args,
            [
                "LOAD",
                "2",
                "name",
                "price",
                "APPLY",
                "@price * 1.1",
                "AS",
                "with_tax",
                "SORTBY",
                "2",
                "@with_tax",
                "DESC",
            ]
        );
    }

    #[test]
    fn no_pipeline_is_rejected_with_guidance() {
        let input = input(serde_json::json!({"index": "idx", "query": "*"})).unwrap();
        let err = build_aggregate_args(&input).unwrap_err();
        assert!(err.contains("needs a pipeline"), "{err}");
        assert!(err.contains("group_by"), "{err}");
    }

    #[test]
    fn reduce_without_group_by_is_rejected() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*", "reduce": ["COUNT 0 AS count"]
        }))
        .unwrap();
        let err = build_aggregate_args(&input).unwrap_err();
        assert!(err.contains("reduce requires group_by"), "{err}");
    }

    #[test]
    fn malformed_reducer_is_rejected() {
        let input = input(serde_json::json!({
            "index": "idx", "query": "*", "group_by": [], "reduce": ["COUNT"]
        }))
        .unwrap();
        let err = build_aggregate_args(&input).unwrap_err();
        assert!(err.contains("reduce[0]"), "{err}");
    }

    #[test]
    fn unknown_input_fields_are_rejected_not_ignored() {
        // The agent-facing regression behind #1212: a misspelled or invented
        // parameter must fail loudly instead of silently running a bare command.
        let err = input(serde_json::json!({
            "index": "idx", "query": "*", "groupby": ["@category"]
        }))
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown field `groupby`"), "{err}");
        assert!(
            err.contains("group_by"),
            "should list accepted fields: {err}"
        );
    }
}
