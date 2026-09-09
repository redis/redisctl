//! RediSearch module tools (FT.CREATE, FT.SEARCH, FT.AGGREGATE, FT.INFO, etc.)

use tower_mcp::{CallToolResult, Error as McpError, ResultExt};

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
    "Execute an aggregation query against a search index. Use raw_args for complex pipelines. Requires the RediSearch module.\n\n\
     Architectural note: FT.AGGREGATE can replace client-side fan-out queries across many keys \
     with a single server-side aggregation. If you're doing SCAN + filter + reduce, an \
     aggregation pipeline is likely simpler and faster.\n\nCommon raw_args patterns:\n- Group by field: `[\"GROUPBY\", \"1\", \"@category\", \"REDUCE\", \"COUNT\", \"0\", \"AS\", \"count\"]`\n- Average: `[\"GROUPBY\", \"1\", \"@category\", \"REDUCE\", \"AVG\", \"1\", \"@price\", \"AS\", \"avg_price\"]`\n- Sort results: `[\"SORTBY\", \"2\", \"@count\", \"DESC\"]`\n- Computed field: `[\"APPLY\", \"@price * 1.1\", \"AS\", \"price_with_tax\"]`\n\nChain multiple pipeline steps in a single raw_args array.",
    {
        /// Index name
        pub index: String,
        /// Search query (use "*" for all documents)
        pub query: String,
        /// Fields to load from the document
        #[serde(default)]
        pub load_fields: Option<Vec<String>>,
        /// Result offset for pagination
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub limit_offset: Option<u64>,
        /// Number of results to return
        #[serde(default, deserialize_with = "serde_helpers::string_or_opt_u64::deserialize")]
        pub limit_num: Option<u64>,
        /// Additional raw arguments for complex pipelines (for example,
        /// `["GROUPBY", "1", "@city", "REDUCE", "COUNT", "0", "AS", "count"]`)
        #[serde(default)]
        pub raw_args: Option<Vec<String>>,
    } => |conn, input| {
        let mut cmd = redis::cmd("FT.AGGREGATE");
        cmd.arg(&input.index).arg(&input.query);

        if let Some(ref fields) = input.load_fields {
            cmd.arg("LOAD").arg(fields.len());
            for f in fields {
                cmd.arg(f);
            }
        }
        if let Some(ref args) = input.raw_args {
            for arg in args {
                cmd.arg(arg);
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
            .tool_context("FT.AGGREGATE failed")?;

        if result.is_empty() {
            return Ok(CallToolResult::text("No results"));
        }

        let total = format_value(&result[0]);
        let mut output = format!("Total results: {}\n\n", total);

        for (idx, row) in result.iter().skip(1).enumerate() {
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
