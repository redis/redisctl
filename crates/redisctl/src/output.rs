use anyhow::{Context, Result};
use jpx_core::Runtime;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::io::IsTerminal;
use std::sync::OnceLock;
use tabled::builder::Builder;
use tabled::settings::Style;

use crate::error::{RedisCtlError, Result as CliResult};

/// Re-export the single OutputFormat enum from cli.
pub use crate::cli::OutputFormat;

/// Global JMESPath runtime with extended functions
static JMESPATH_RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Get or initialize the JMESPath runtime with extended functions
pub fn get_jmespath_runtime() -> &'static Runtime {
    JMESPATH_RUNTIME.get_or_init(|| Runtime::builder().with_all_extensions().build())
}

/// Normalize backtick literals in JMESPath expressions.
///
/// The JMESPath specification allows "elided quotes" in backtick literals,
/// meaning `` `foo` `` is equivalent to `` `"foo"` ``. However, the Rust
/// jmespath crate requires valid JSON inside backticks.
///
/// This function converts unquoted string literals like `` `foo` `` to
/// properly quoted JSON strings like `` `"foo"` ``.
///
/// Examples:
/// - `` `foo` `` -> `` `"foo"` ``
/// - `` `true` `` -> `` `true` `` (unchanged, valid JSON boolean)
/// - `` `123` `` -> `` `123` `` (unchanged, valid JSON number)
/// - `` `"already quoted"` `` -> `` `"already quoted"` `` (unchanged)
fn normalize_backtick_literals(query: &str) -> String {
    static BACKTICK_RE: OnceLock<Regex> = OnceLock::new();
    let re = BACKTICK_RE.get_or_init(|| {
        // Match backtick-delimited content, handling escaped backticks
        Regex::new(r"`([^`\\]*(?:\\.[^`\\]*)*)`").unwrap()
    });

    re.replace_all(query, |caps: &regex::Captures| {
        let content = &caps[1];
        let trimmed = content.trim();

        // Check if it's already valid JSON
        if serde_json::from_str::<Value>(trimmed).is_ok() {
            // Already valid JSON (number, boolean, null, quoted string, array, object)
            format!("`{}`", content)
        } else {
            // Not valid JSON - treat as unquoted string literal and add quotes
            // Escape any double quotes in the content
            let escaped = trimmed.replace('\\', "\\\\").replace('"', "\\\"");
            format!("`\"{}\"`", escaped)
        }
    })
    .into_owned()
}

/// Compile a JMESPath expression using the extended runtime.
///
/// This function normalizes backtick literals to handle the JMESPath
/// specification's "elided quotes" feature before compilation.
pub fn compile_jmespath(
    query: &str,
) -> Result<jpx_core::Expression<'static>, jpx_core::JmespathError> {
    let normalized = normalize_backtick_literals(query);
    get_jmespath_runtime().compile(&normalized)
}

/// Resolve `Auto` format to a concrete format.
///
/// `Auto` resolves to `Table` when stdout is a TTY, `Json` when piped.
pub fn resolve_auto(format: OutputFormat) -> OutputFormat {
    match format {
        OutputFormat::Auto => {
            if std::io::stdout().is_terminal() {
                OutputFormat::Table
            } else {
                OutputFormat::Json
            }
        }
        other => other,
    }
}

pub fn print_output<T: Serialize>(
    data: T,
    format: OutputFormat,
    query: Option<&str>,
) -> Result<()> {
    let mut json_value = serde_json::to_value(data)?;

    // Apply JMESPath query if provided (using extended runtime with 400+ functions)
    if let Some(query_str) = query {
        let expr = compile_jmespath(query_str)
            .with_context(|| format!("Invalid JMESPath expression: {}", query_str))?;
        json_value = expr.search(&json_value).context("JMESPath query failed")?;
    }

    let resolved = resolve_auto(format);
    match resolved {
        OutputFormat::Json | OutputFormat::Auto => {
            println!("{}", serde_json::to_string_pretty(&json_value)?);
        }
        OutputFormat::Yaml => {
            println!("{}", serde_yaml::to_string(&json_value)?);
        }
        OutputFormat::Table => {
            print_as_table(&json_value)?;
        }
    }

    Ok(())
}

/// Apply JMESPath query to JSON data (using extended runtime with 400+ functions)
pub fn apply_jmespath(data: &Value, query: &str) -> CliResult<Value> {
    let expr = compile_jmespath(query)
        .with_context(|| format!("Invalid JMESPath expression: {}", query))?;
    expr.search(data)
        .with_context(|| format!("Failed to apply JMESPath query: {}", query))
        .map_err(Into::into)
}

/// Handle output with optional JMESPath query
pub fn handle_output(
    data: Value,
    _output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<Value> {
    if let Some(q) = query {
        apply_jmespath(&data, q)
    } else {
        Ok(data)
    }
}

/// Print data in the requested output format, mapping errors to `RedisCtlError::OutputError`.
pub fn print_formatted_output(data: Value, output_format: OutputFormat) -> CliResult<()> {
    let resolved = resolve_auto(output_format);
    print_output(data, resolved, None).map_err(|e| RedisCtlError::OutputError {
        message: e.to_string(),
    })?;
    Ok(())
}

fn print_as_table(value: &Value) -> Result<()> {
    match value {
        Value::Array(arr) if !arr.is_empty() => {
            let mut builder = Builder::default();

            // Get headers from first object
            if let Value::Object(first) = &arr[0] {
                let headers: Vec<String> = first.keys().cloned().collect();
                builder.push_record(&headers);

                // Add rows
                for item in arr {
                    if let Value::Object(obj) = item {
                        let row: Vec<String> = headers
                            .iter()
                            .map(|h| format_value(obj.get(h).unwrap_or(&Value::Null)))
                            .collect();
                        builder.push_record(row);
                    }
                }
            } else {
                // Simple array of values
                builder.push_record(["Value"]);
                for item in arr {
                    builder.push_record([format_value(item)]);
                }
            }

            println!("{}", builder.build().with(Style::blank()));
        }
        Value::Object(obj) => {
            let mut builder = Builder::default();
            builder.push_record(["Key", "Value"]);

            for (key, val) in obj {
                builder.push_record([key.clone(), format_value(val)]);
            }

            println!("{}", builder.build().with(Style::blank()));
        }
        _ => {
            println!("{}", format_value(value));
        }
    }

    Ok(())
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => format!("[{} items]", arr.len()),
        Value::Object(obj) => format!("{{{} fields}}", obj.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_backtick_unquoted_string() {
        // Standard JMESPath backtick literal without quotes
        assert_eq!(
            normalize_backtick_literals(r#"[?name==`foo`]"#),
            r#"[?name==`"foo"`]"#
        );
    }

    #[test]
    fn test_normalize_backtick_already_quoted() {
        // Already properly quoted - should not double-quote
        assert_eq!(
            normalize_backtick_literals(r#"[?name==`"foo"`]"#),
            r#"[?name==`"foo"`]"#
        );
    }

    #[test]
    fn test_normalize_backtick_number() {
        // Numbers are valid JSON - should not be quoted
        assert_eq!(
            normalize_backtick_literals(r#"[?count==`123`]"#),
            r#"[?count==`123`]"#
        );
    }

    #[test]
    fn test_normalize_backtick_boolean() {
        // Booleans are valid JSON - should not be quoted
        assert_eq!(
            normalize_backtick_literals(r#"[?enabled==`true`]"#),
            r#"[?enabled==`true`]"#
        );
        assert_eq!(
            normalize_backtick_literals(r#"[?enabled==`false`]"#),
            r#"[?enabled==`false`]"#
        );
    }

    #[test]
    fn test_normalize_backtick_null() {
        // null is valid JSON - should not be quoted
        assert_eq!(
            normalize_backtick_literals(r#"[?value==`null`]"#),
            r#"[?value==`null`]"#
        );
    }

    #[test]
    fn test_normalize_backtick_array() {
        // Arrays are valid JSON - should not be modified
        assert_eq!(
            normalize_backtick_literals(r#"`[1, 2, 3]`"#),
            r#"`[1, 2, 3]`"#
        );
    }

    #[test]
    fn test_normalize_backtick_object() {
        // Objects are valid JSON - should not be modified
        assert_eq!(
            normalize_backtick_literals(r#"`{"key": "value"}`"#),
            r#"`{"key": "value"}`"#
        );
    }

    #[test]
    fn test_normalize_multiple_backticks() {
        // Multiple backtick literals in one expression
        assert_eq!(
            normalize_backtick_literals(r#"[?name==`foo` && type==`bar`]"#),
            r#"[?name==`"foo"` && type==`"bar"`]"#
        );
    }

    #[test]
    fn test_jmespath_backtick_literal_compiles() {
        // The original failing case should now work
        let query = r#"[?module_name==`jmespath`]"#;
        let result = compile_jmespath(query);
        assert!(
            result.is_ok(),
            "Backtick literals should be supported: {:?}",
            result
        );
    }

    #[test]
    fn test_jmespath_complex_filter() {
        // Complex filter expression from the bug report
        let query = r#"[?module_name==`jmespath`].uid | [0]"#;
        let result = compile_jmespath(query);
        assert!(
            result.is_ok(),
            "Complex filter with backtick should work: {:?}",
            result
        );
    }

    #[test]
    fn test_jmespath_double_quote_literal() {
        // Double quotes work as field references, not literals
        let query = r#"[?module_name=="jmespath"]"#;
        let result = compile_jmespath(query);
        // This compiles but semantically compares field to field, not field to literal
        assert!(result.is_ok());
    }

    #[test]
    fn test_jmespath_single_quote_literal() {
        // Single quotes are raw string literals in JMESPath
        let query = "[?module_name=='jmespath']";
        let result = compile_jmespath(query);
        assert!(result.is_ok());
    }
}
