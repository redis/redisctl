//! Cloud task command implementations

use crate::cli::{CloudTaskCommands, OutputFormat};
use crate::connection::ConnectionManager;
use crate::error::{RedisCtlError, Result as CliResult};
use crate::output::print_output;
use anyhow::Context;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use redis_cloud::CloudClient;
use serde_json::Value;
use std::time::Duration;
use tokio::time::{Instant, sleep};

/// Handle cloud task commands
pub async fn handle_task_command(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    command: &CloudTaskCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match command {
        CloudTaskCommands::List => list_tasks(conn_mgr, profile_name, output_format, query).await,
        CloudTaskCommands::Get { id } => {
            get_task(conn_mgr, profile_name, id, output_format, query).await
        }
        CloudTaskCommands::Wait {
            id,
            timeout,
            interval,
        } => {
            wait_for_task(
                conn_mgr,
                profile_name,
                id,
                *timeout,
                *interval,
                output_format,
            )
            .await
        }
        CloudTaskCommands::Poll {
            id,
            interval,
            max_polls,
        } => {
            poll_task(
                conn_mgr,
                profile_name,
                id,
                *interval,
                *max_polls,
                output_format,
            )
            .await
        }
    }
}

/// List all tasks for this account
async fn list_tasks(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let tasks = client
        .get_raw("/tasks")
        .await
        .with_context(|| "Failed to fetch tasks")
        .map_err(|e| RedisCtlError::ApiError {
            message: e.to_string(),
        })?;

    // Apply JMESPath query if provided
    let data = if let Some(q) = query {
        super::utils::apply_jmespath(&tasks, q)?
    } else {
        tasks
    };

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_tasks_table(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
}

/// Print tasks in table format
fn print_tasks_table(tasks: &Value) -> CliResult<()> {
    use super::utils::output_with_pager;
    use tabled::{Table, Tabled, settings::Style};

    #[derive(Tabled)]
    struct TaskRow {
        #[tabled(rename = "Task ID")]
        task_id: String,
        #[tabled(rename = "Status")]
        status: String,
        #[tabled(rename = "Command")]
        command: String,
        #[tabled(rename = "Progress")]
        progress: String,
        #[tabled(rename = "Description")]
        description: String,
    }

    let tasks_array = match tasks.as_array() {
        Some(arr) => arr,
        None => {
            println!("No tasks found");
            return Ok(());
        }
    };

    if tasks_array.is_empty() {
        println!("No tasks found");
        return Ok(());
    }

    let rows: Vec<TaskRow> = tasks_array
        .iter()
        .map(|task| {
            let status = task
                .get("status")
                .or_else(|| task.get("state"))
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");

            TaskRow {
                task_id: task
                    .get("taskId")
                    .or_else(|| task.get("id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                status: format_task_state(status),
                command: task
                    .get("commandType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .to_string(),
                progress: task
                    .get("progress")
                    .and_then(|p| p.as_u64())
                    .map(|p| format!("{}%", p))
                    .unwrap_or_else(|| "-".to_string()),
                description: task
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("-")
                    .chars()
                    .take(40)
                    .collect::<String>(),
            }
        })
        .collect();

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());

    Ok(())
}

/// Get task status and details
async fn get_task(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    task_id: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let task = fetch_task(&client, task_id).await?;

    // Apply JMESPath query if provided
    let data = if let Some(q) = query {
        super::utils::apply_jmespath(&task, q)?
    } else {
        task
    };

    match output_format {
        OutputFormat::Auto | OutputFormat::Table => {
            print_task_details(&data)?;
        }
        OutputFormat::Json => {
            print_output(data, crate::output::OutputFormat::Json, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
        OutputFormat::Yaml => {
            print_output(data, crate::output::OutputFormat::Yaml, None).map_err(|e| {
                RedisCtlError::OutputError {
                    message: e.to_string(),
                }
            })?;
        }
    }

    Ok(())
}

/// Wait for a task to complete
async fn wait_for_task(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    task_id: &str,
    timeout_secs: u64,
    interval_secs: u64,
    output_format: OutputFormat,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let interval = Duration::from_secs(interval_secs);

    // Create progress bar
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg} [{elapsed_precise}]")
            .unwrap(),
    );
    pb.set_message(format!("Waiting for task {}", task_id));

    loop {
        let task = fetch_task(&client, task_id).await?;
        let state = get_task_state(&task);

        pb.set_message(format!("Task {}: {}", task_id, format_task_state(&state)));

        if is_terminal_state(&state) {
            pb.finish_with_message(format!("Task {}: {}", task_id, format_task_state(&state)));

            match output_format {
                OutputFormat::Auto | OutputFormat::Table => {
                    print_task_details(&task)?;
                }
                OutputFormat::Json => {
                    print_output(task, crate::output::OutputFormat::Json, None)?;
                }
                OutputFormat::Yaml => {
                    print_output(task, crate::output::OutputFormat::Yaml, None)?;
                }
            }

            return Ok(());
        }

        if start.elapsed() > timeout {
            pb.finish_with_message(format!("Timeout waiting for task {}", task_id));
            return Err(RedisCtlError::Timeout {
                message: format!(
                    "Task {} did not complete within {} seconds",
                    task_id, timeout_secs
                ),
            });
        }

        sleep(interval).await;
    }
}

/// Poll task status with live updates
async fn poll_task(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    task_id: &str,
    interval_secs: u64,
    max_polls: u64,
    output_format: OutputFormat,
) -> CliResult<()> {
    let client = conn_mgr.create_cloud_client(profile_name).await?;
    let interval = Duration::from_secs(interval_secs);
    let mut poll_count = 0u64;

    println!(
        "Polling task {} every {} seconds...",
        task_id, interval_secs
    );
    println!("Press Ctrl+C to stop\n");

    loop {
        let task = fetch_task(&client, task_id).await?;
        let state = get_task_state(&task);
        let timestamp = chrono::Local::now().format("%H:%M:%S");

        // Clear screen for table output in auto mode
        if matches!(output_format, OutputFormat::Auto | OutputFormat::Table) {
            // Move cursor up to overwrite previous output
            if poll_count > 0 {
                print!("\x1B[2K"); // Clear line
                print!("\x1B[1A"); // Move up one line
                print!("\x1B[2K"); // Clear line
                print!("\r"); // Return to start of line
            }

            println!(
                "[{}] Task {}: {}",
                timestamp,
                task_id,
                format_task_state(&state)
            );

            if let Some(progress) = task.get("progress").and_then(|p| p.as_u64()) {
                print_progress_bar(progress);
            }
        } else {
            // For JSON/YAML, print the full output each time
            match output_format {
                OutputFormat::Json => {
                    print_output(task.clone(), crate::output::OutputFormat::Json, None)?;
                }
                OutputFormat::Yaml => {
                    print_output(task.clone(), crate::output::OutputFormat::Yaml, None)?;
                }
                _ => {} // Auto/Table already handled above
            }
        }

        if is_terminal_state(&state) {
            println!("\nTask completed with state: {}", format_task_state(&state));
            break;
        }

        poll_count += 1;
        if max_polls > 0 && poll_count >= max_polls {
            println!("\nReached maximum poll count ({})", max_polls);
            break;
        }

        sleep(interval).await;
    }

    Ok(())
}

/// Fetch task details from API
async fn fetch_task(client: &CloudClient, task_id: &str) -> CliResult<Value> {
    client
        .get_raw(&format!("/tasks/{}", task_id))
        .await
        .with_context(|| format!("Failed to fetch task {}", task_id))
        .map_err(|e| RedisCtlError::ApiError {
            message: e.to_string(),
        })
}

/// Extract task state from response
fn get_task_state(task: &Value) -> String {
    task.get("state")
        .or_else(|| task.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Check if task is in terminal state
fn is_terminal_state(state: &str) -> bool {
    matches!(
        state.to_lowercase().as_str(),
        "completed" | "failed" | "error" | "success" | "cancelled" | "aborted"
    )
}

/// Format task state with color
fn format_task_state(state: &str) -> String {
    match state.to_lowercase().as_str() {
        "processing" | "running" | "in_progress" => state.yellow().to_string(),
        "completed" | "success" => state.green().to_string(),
        "failed" | "error" => state.red().to_string(),
        "cancelled" | "aborted" => state.dimmed().to_string(),
        _ => state.to_string(),
    }
}

/// Print task details in table format
fn print_task_details(task: &Value) -> CliResult<()> {
    use super::utils::DetailRow;
    use tabled::{Table, settings::Style};

    let mut rows = Vec::new();

    // Task ID
    if let Some(id) = task.get("taskId").or_else(|| task.get("id")) {
        rows.push(DetailRow {
            field: "Task ID".to_string(),
            value: id.to_string().trim_matches('"').to_string(),
        });
    }

    // State/Status
    let state = get_task_state(task);
    rows.push(DetailRow {
        field: "State".to_string(),
        value: format_task_state(&state),
    });

    // Progress
    if let Some(progress) = task.get("progress").and_then(|p| p.as_u64()) {
        rows.push(DetailRow {
            field: "Progress".to_string(),
            value: format!("{}%", progress),
        });
    }

    // Description
    if let Some(desc) = task.get("description").and_then(|d| d.as_str()) {
        rows.push(DetailRow {
            field: "Description".to_string(),
            value: desc.to_string(),
        });
    }

    // Resource info
    if let Some(resource) = task.get("resourceId").and_then(|r| r.as_str()) {
        rows.push(DetailRow {
            field: "Resource ID".to_string(),
            value: resource.to_string(),
        });
    }

    if let Some(resource_type) = task.get("resourceType").and_then(|r| r.as_str()) {
        rows.push(DetailRow {
            field: "Resource Type".to_string(),
            value: resource_type.to_string(),
        });
    }

    // Timing
    if let Some(created) = task.get("createdTimestamp").and_then(|t| t.as_str()) {
        rows.push(DetailRow {
            field: "Created".to_string(),
            value: super::utils::format_date(created.to_string()),
        });
    }

    if let Some(updated) = task.get("lastUpdatedTimestamp").and_then(|t| t.as_str()) {
        rows.push(DetailRow {
            field: "Last Updated".to_string(),
            value: super::utils::format_date(updated.to_string()),
        });
    }

    // Error details if failed
    if matches!(state.to_lowercase().as_str(), "failed" | "error") {
        if let Some(error) = task.get("error").and_then(|e| e.as_str()) {
            rows.push(DetailRow {
                field: "Error".to_string(),
                value: error.red().to_string(),
            });
        }
        if let Some(error_msg) = task.get("errorMessage").and_then(|e| e.as_str()) {
            rows.push(DetailRow {
                field: "Error Message".to_string(),
                value: error_msg.red().to_string(),
            });
        }
    }

    if rows.is_empty() {
        println!("No task information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());

    println!("{}", table);
    Ok(())
}

/// Print a simple progress bar
fn print_progress_bar(progress: u64) {
    let width = 30;
    let filled = (progress as usize * width) / 100;
    let empty = width - filled;

    print!("Progress: [");
    print!("{}", "=".repeat(filled).green());
    print!("{}", "-".repeat(empty).dimmed());
    println!("] {}%", progress);
}
