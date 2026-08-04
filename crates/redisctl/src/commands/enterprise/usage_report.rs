use anyhow::Context;
use clap::Subcommand;
use futures::StreamExt;
use redis_enterprise::usage_report::{UsageReport, UsageReportRecord};
use serde::Serialize;
use std::collections::BTreeSet;

use crate::error::RedisCtlError;
use crate::{cli::OutputFormat, connection::ConnectionManager, error::Result as CliResult};

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
    /// Get the current usage report
    Get,

    /// Export the usage report to a file
    Export {
        /// Output file path
        #[arg(long)]
        file: String,

        /// Export format (json or csv)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
}

#[derive(Debug, Serialize)]
struct UsageReportOutput {
    /// Database usage records. This is always an array, including for zero or one record.
    reports: Vec<UsageReport>,
    /// Final MD5 checksum supplied by Redis Software, or null for an empty response.
    checksum: Option<String>,
}

impl UsageReportCommands {
    pub async fn execute(
        &self,
        conn_mgr: &ConnectionManager,
        profile_name: Option<&str>,
        output_format: OutputFormat,
        query: Option<&str>,
    ) -> CliResult<()> {
        let client = conn_mgr.create_enterprise_client(profile_name).await?;
        let report = collect_usage_report(&client).await?;
        let structured =
            serde_json::to_value(&report).context("Failed to serialize usage report")?;
        let output_data = if let Some(q) = query {
            super::utils::apply_jmespath(&structured, q)?
        } else {
            structured
        };

        match self {
            UsageReportCommands::Get => {
                if query.is_none()
                    && matches!(
                        crate::output::resolve_auto(output_format),
                        OutputFormat::Table
                    )
                {
                    print_usage_report_table(&report)?;
                } else {
                    super::utils::print_formatted_output(output_data, output_format)?;
                }
            }
            UsageReportCommands::Export { file, format } => match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&output_data)
                        .context("Failed to serialize usage report to JSON")?;
                    std::fs::write(file, json)
                        .with_context(|| format!("Failed to write usage report to {file}"))?;
                    print_export_summary(
                        file,
                        "JSON",
                        report.reports.len(),
                        report.checksum.as_deref(),
                    );
                }
                "csv" => {
                    let csv = json_to_csv(&output_data)?;
                    std::fs::write(file, csv)
                        .with_context(|| format!("Failed to write usage report to {file}"))?;
                    print_export_summary(
                        file,
                        "CSV",
                        report.reports.len(),
                        report.checksum.as_deref(),
                    );
                }
                _ => {
                    return Err(RedisCtlError::InvalidInput {
                        message: format!("Unsupported format: {format}. Use 'json' or 'csv'"),
                    });
                }
            },
        }

        Ok(())
    }
}

async fn collect_usage_report(
    client: &redis_enterprise::EnterpriseClient,
) -> CliResult<UsageReportOutput> {
    let mut stream = client
        .usage_reports()
        .stream()
        .await
        .map_err(RedisCtlError::from)?;
    let mut reports = Vec::new();
    let mut checksum = None;

    while let Some(record) = stream.next().await {
        match record.map_err(RedisCtlError::from)? {
            UsageReportRecord::Report(report) => reports.push(*report),
            UsageReportRecord::Checksum(value) => checksum = Some(value),
        }
    }

    Ok(UsageReportOutput { reports, checksum })
}

fn print_usage_report_table(report: &UsageReportOutput) -> CliResult<()> {
    if report.reports.is_empty() {
        println!("No usage records available.");
    } else {
        let rows = serde_json::to_value(&report.reports)
            .context("Failed to serialize usage report rows")?;
        super::utils::print_formatted_output(rows, OutputFormat::Table)?;
    }

    match &report.checksum {
        Some(checksum) => println!("Checksum: {checksum}"),
        None => println!("Checksum: unavailable (empty response)"),
    }
    Ok(())
}

fn print_export_summary(output: &str, format: &str, records: usize, checksum: Option<&str>) {
    println!("Usage report exported to {output} as {format} ({records} records)");
    if let Some(checksum) = checksum {
        println!("Checksum: {checksum}");
    }
}

fn json_to_csv(data: &serde_json::Value) -> CliResult<String> {
    let rows = match data {
        serde_json::Value::Object(object) => object
            .get("reports")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![data.clone()]),
        serde_json::Value::Array(rows) => rows.clone(),
        serde_json::Value::Null => Vec::new(),
        _ => {
            return Err(RedisCtlError::InvalidInput {
                message: "CSV export requires usage-report object or array data".to_string(),
            });
        }
    };

    let headers: Vec<String> = rows
        .iter()
        .filter_map(serde_json::Value::as_object)
        .flat_map(|object| object.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    if rows.is_empty() || headers.is_empty() {
        return Ok(String::new());
    }
    if rows.iter().any(|row| !row.is_object()) {
        return Err(RedisCtlError::InvalidInput {
            message: "CSV export requires an array of usage-report objects".to_string(),
        });
    }

    let mut csv = String::new();
    csv.push_str(
        &headers
            .iter()
            .map(|h| csv_field(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    csv.push('\n');

    for row in rows {
        let Some(object) = row.as_object() else {
            return Err(RedisCtlError::InvalidInput {
                message: "usage report rows must be JSON objects".to_string(),
            });
        };
        let values = headers
            .iter()
            .map(|header| object.get(header).map(csv_value).unwrap_or_default())
            .collect::<Vec<_>>();
        csv.push_str(&values.join(","));
        csv.push('\n');
    }

    Ok(csv)
}

fn csv_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => csv_field(value),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => value.to_string(),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => csv_field(&value.to_string()),
    }
}

fn csv_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
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

        let cli = TestCli::parse_from(["test", "get"]);
        assert!(matches!(cli.cmd, UsageReportCommands::Get));

        let cli = TestCli::parse_from(["test", "export", "--file", "report.json"]);
        assert!(matches!(
            cli.cmd,
            UsageReportCommands::Export { file, format }
                if file == "report.json" && format == "json"
        ));
    }

    #[test]
    fn csv_has_one_row_per_report_and_unions_headers() {
        let data = serde_json::json!({
            "reports": [
                {"bdb_uid": "1", "cluster_name": "one"},
                {"bdb_uid": "2", "used_memory": 2048}
            ],
            "checksum": "d41d8cd98f00b204e9800998ecf8427e"
        });
        let csv = json_to_csv(&data).unwrap();
        let lines = csv.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "\"bdb_uid\",\"cluster_name\",\"used_memory\"");
        assert!(lines[1].contains("\"one\""));
        assert!(lines[2].contains("2048"));
    }

    #[test]
    fn csv_is_empty_for_checksum_only_or_empty_responses() {
        assert_eq!(
            json_to_csv(&serde_json::json!({"reports": [], "checksum": "abc"})).unwrap(),
            ""
        );
        assert_eq!(
            json_to_csv(&serde_json::json!({"reports": [], "checksum": null})).unwrap(),
            ""
        );
    }
}
