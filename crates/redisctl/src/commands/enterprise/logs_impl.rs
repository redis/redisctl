//! Implementation of enterprise logs commands

use crate::error::RedisCtlError;

use crate::cli::OutputFormat;
use crate::commands::enterprise::logs::LogsCommands;
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use futures::StreamExt;
use redis_enterprise::logs::LogsQuery;
use std::time::Duration;
use tokio::signal;

/// Parameters for log list operation
struct LogListParams {
    since: Option<String>,
    until: Option<String>,
    order: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
    follow: bool,
    poll_interval: u64,
}

pub async fn handle_logs_commands(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: &LogsCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match cmd {
        LogsCommands::List {
            since,
            until,
            order,
            limit,
            offset,
            follow,
            poll_interval,
        } => {
            let params = LogListParams {
                since: since.clone(),
                until: until.clone(),
                order: order.clone(),
                limit: *limit,
                offset: *offset,
                follow: *follow,
                poll_interval: *poll_interval,
            };
            handle_list_logs(conn_mgr, profile_name, params, output_format, query).await
        }
    }
}

async fn handle_list_logs(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    params: LogListParams,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = redis_enterprise::LogsHandler::new(client);

    // Handle streaming mode
    if params.follow {
        return handle_stream_logs(handler, params, output_format, query).await;
    }

    // Normal (non-streaming) mode
    let logs_query = if params.since.is_some()
        || params.until.is_some()
        || params.order.is_some()
        || params.limit.is_some()
        || params.offset.is_some()
    {
        Some(LogsQuery {
            stime: params.since,
            etime: params.until,
            order: params.order,
            limit: params.limit,
            offset: params.offset,
        })
    } else {
        None
    };

    let logs = handler
        .list(logs_query)
        .await
        .map_err(RedisCtlError::from)?;

    // Convert to JSON value for output
    let logs_json = serde_json::to_value(&logs)?;

    // Handle output with optional JMESPath query
    let output_data = if let Some(q) = query {
        crate::commands::enterprise::utils::apply_jmespath(&logs_json, q)?
    } else {
        logs_json
    };

    // Print the output
    crate::commands::enterprise::utils::print_formatted_output(output_data, output_format)?;

    Ok(())
}

async fn handle_stream_logs(
    handler: redis_enterprise::LogsHandler,
    params: LogListParams,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    // Only support table/auto format for streaming (JSON/YAML don't make sense for streams)
    if !matches!(output_format, OutputFormat::Auto | OutputFormat::Table) {
        return Err(RedisCtlError::InvalidInput {
            message: "Streaming logs (--follow) only supports table output format".to_string(),
        });
    }

    let poll_interval = Duration::from_secs(params.poll_interval);
    let mut stream = handler.stream_logs(poll_interval, params.limit);

    println!("Streaming logs (Ctrl+C to stop)...\n");

    loop {
        tokio::select! {
            // Handle Ctrl+C
            _ = signal::ctrl_c() => {
                println!("\nStopping log stream...");
                break;
            }
            // Handle next log entry
            entry_result = stream.next() => {
                match entry_result {
                    Some(Ok(entry)) => {
                        // Format and print the log entry
                        let entry_json = serde_json::to_value(&entry)?;

                        // Apply JMESPath query if provided
                        let output_data = if let Some(q) = query {
                            crate::commands::enterprise::utils::apply_jmespath(&entry_json, q)?
                        } else {
                            entry_json
                        };

                        // Print each entry as it arrives
                        // For table format, print a simple formatted line
                        if let Some(time) = output_data.get("time").and_then(|t| t.as_str()) {
                            let event_type = output_data.get("type")
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown");

                            // Print timestamp and event type
                            print!("[{}] {}", time, event_type);

                            // Print any extra fields
                            if let Some(obj) = output_data.as_object() {
                                for (key, value) in obj {
                                    if key != "time" && key != "type" {
                                        print!(" {}={}", key, value);
                                    }
                                }
                            }
                            println!();
                        }
                    }
                    Some(Err(e)) => {
                        eprintln!("Error fetching logs: {}", e);
                        break;
                    }
                    None => {
                        // Stream ended
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
