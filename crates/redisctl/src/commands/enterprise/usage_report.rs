use crate::error::RedisCtlError;
use anyhow::Context;
use clap::Subcommand;

use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

#[allow(dead_code)]
pub async fn handle_usage_report_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    usage_report_cmd: UsageReportCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    usage_report_cmd
        .execute(conn_mgr, profile_name, output_format, query)
        .await
}

#[derive(Debug, Clone, Subcommand)]
pub enum UsageReportCommands {
    /// Get current usage report
    Get,

    /// Export usage report to file
    Export {
        /// Output file path
        #[arg(short, long)]
        output: String,

        /// Export format (json or csv)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

impl UsageReportCommands {
    #[allow(dead_code)]
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        handle_usage_report_command_impl(conn_mgr, profile_name, self, output_format, query).await
    }
}

#[allow(dead_code)]
async fn handle_usage_report_command_impl(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &UsageReportCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;

    match command {
        UsageReportCommands::Get => {
            let body = client
                .get_text("/v1/usage_report")
                .await
                .map_err(RedisCtlError::from)?;

            // Try JSON first; if the body is non-JSON (e.g. a bare MD5 checksum
            // returned by demo clusters with no usage data), emit a clear message.
            let body_trimmed = body.trim();
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(body_trimmed) {
                let output_data = if let Some(q) = query {
                    super::utils::apply_jmespath(&json_val, q)?
                } else {
                    json_val
                };
                super::utils::print_formatted_output(output_data, output_format)?;
            } else if is_md5_hex(body_trimmed) {
                // Bare MD5 checksum = cluster has no usage data yet
                println!(
                    "usage-report: cluster returned a checksum ({body_trimmed}) with no report \
                     data — this cluster has not yet accrued any usage. \
                     No report to display."
                );
            } else if body_trimmed.is_empty() {
                println!(
                    "usage-report: endpoint returned an empty body — no usage data available."
                );
            } else {
                // Unknown non-JSON shape; surface the raw text so the user can act on it
                println!("usage-report: endpoint returned non-JSON content:\n{body_trimmed}");
            }
        }
        UsageReportCommands::Export { output, format } => {
            let response: serde_json::Value = client
                .get("/v1/usage_report")
                .await
                .map_err(RedisCtlError::from)?;

            let output_data = if let Some(q) = query {
                super::utils::apply_jmespath(&response, q)?
            } else {
                response
            };

            match format.as_str() {
                "json" => {
                    let json_str = serde_json::to_string_pretty(&output_data)
                        .context("Failed to serialize to JSON")?;
                    std::fs::write(output, json_str)
                        .context(format!("Failed to write to {}", output))?;
                    println!("Usage report exported to {}", output);
                }
                "csv" => {
                    // Convert JSON to CSV format
                    let csv_data = json_to_csv(&output_data)?;
                    std::fs::write(output, csv_data)
                        .context(format!("Failed to write to {}", output))?;
                    println!("Usage report exported to {} as CSV", output);
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unsupported format: {}. Use 'json' or 'csv'",
                        format
                    )
                    .into());
                }
            }
        }
    }

    Ok(())
}

fn json_to_csv(data: &serde_json::Value) -> CliResult<String> {
    // Simple CSV conversion for usage report data
    let mut csv = String::new();

    if let Some(obj) = data.as_object() {
        // Create header row from keys
        let headers: Vec<String> = obj.keys().map(|k| k.to_string()).collect();
        csv.push_str(&headers.join(","));
        csv.push('\n');

        // Create data row from values
        let values: Vec<String> = obj
            .values()
            .map(|v| match v {
                serde_json::Value::String(s) => format!("\"{}\"", s.replace('"', "\"\"")),
                _ => v.to_string(),
            })
            .collect();
        csv.push_str(&values.join(","));
        csv.push('\n');
    } else if let Some(arr) = data.as_array() {
        // Handle array of objects
        if let Some(first) = arr.first()
            && let Some(obj) = first.as_object()
        {
            // Create header row from first object's keys
            let headers: Vec<String> = obj.keys().map(|k| k.to_string()).collect();
            csv.push_str(&headers.join(","));
            csv.push('\n');

            // Create data rows
            for item in arr {
                if let Some(obj) = item.as_object() {
                    let values: Vec<String> = headers
                        .iter()
                        .map(|h| {
                            obj.get(h)
                                .map(|v| match v {
                                    serde_json::Value::String(s) => {
                                        format!("\"{}\"", s.replace('"', "\"\""))
                                    }
                                    _ => v.to_string(),
                                })
                                .unwrap_or_else(|| String::from(""))
                        })
                        .collect();
                    csv.push_str(&values.join(","));
                    csv.push('\n');
                }
            }
        }
    }

    Ok(csv)
}

/// Returns true if `s` looks like a bare MD5 hex digest (32 hex chars).
fn is_md5_hex(s: &str) -> bool {
    s.len() == 32 && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_report_command_parsing() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: UsageReportCommands,
        }

        // Test get command
        let cli = TestCli::parse_from(["test", "get"]);
        assert!(matches!(cli.cmd, UsageReportCommands::Get));

        // Test export command
        let cli = TestCli::parse_from(["test", "export", "--output", "report.json"]);
        if let UsageReportCommands::Export { output, format } = cli.cmd {
            assert_eq!(output, "report.json");
            assert_eq!(format, "json");
        } else {
            panic!("Expected Export command");
        }

        // Test export with CSV format
        let cli = TestCli::parse_from(["test", "export", "-o", "report.csv", "-f", "csv"]);
        if let UsageReportCommands::Export { output, format } = cli.cmd {
            assert_eq!(output, "report.csv");
            assert_eq!(format, "csv");
        } else {
            panic!("Expected Export command");
        }
    }

    #[test]
    fn test_json_to_csv() {
        // Test single object
        let json = serde_json::json!({
            "cluster": "test-cluster",
            "databases": 5,
            "memory_gb": 128
        });
        let csv = json_to_csv(&json).unwrap();
        assert!(csv.contains("cluster,databases,memory_gb"));
        assert!(csv.contains("\"test-cluster\",5,128"));

        // Test array of objects
        let json = serde_json::json!([
            {"name": "db1", "memory": 1024},
            {"name": "db2", "memory": 2048}
        ]);
        let csv = json_to_csv(&json).unwrap();
        // Check header (order may vary)
        assert!(csv.contains("memory,name") || csv.contains("name,memory"));
        // Check data rows
        assert!(csv.contains("\"db1\""));
        assert!(csv.contains("\"db2\""));
        assert!(csv.contains("1024"));
        assert!(csv.contains("2048"));
    }

    #[test]
    fn test_is_md5_hex() {
        assert!(is_md5_hex("d41d8cd98f00b204e9800998ecf8427e"));
        assert!(is_md5_hex("D41D8CD98F00B204E9800998ECF8427E"));
        assert!(!is_md5_hex("not-a-checksum"));
        assert!(!is_md5_hex(""));
        assert!(!is_md5_hex("d41d8cd98f00b204e9800998ecf8427")); // 31 chars, too short
    }
}
