//! `redisctl cloud workflow quick-database` — thin CLI wrapper over the shared engine in
//! [`redisctl_core::cloud::quick_database`].
//!
//! This layer owns only CLI concerns: clap arg parsing, the terminal spinner, and wrapping
//! the [`QuickDatabaseReport`] into a [`WorkflowResult`]. The provisioning logic itself lives
//! in `redisctl-core` so the MCP server reuses it directly. Error → exit-code mapping lives
//! in [`crate::structured_error`].

use super::super::{Workflow, WorkflowArgs, WorkflowContext, WorkflowResult};
use crate::error::RedisCtlError;
use anyhow::Result;
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use redisctl_core::cloud::quick_database::{QuickDatabaseError, QuickDatabaseParams, provision};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

/// Key under which the typed report is stored in `WorkflowResult.outputs`.
pub const REPORT_KEY: &str = "report";

#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct QuickDatabaseArgs {
    /// Database name (also used, prefixed with `redisctl-`, for the subscription).
    /// Must match `^[a-z][a-z0-9-]{1,38}[a-z0-9]$` (no leading/trailing/double hyphen).
    #[arg(long)]
    pub name: String,

    /// File to write the connection string into (created if missing; managed keys updated
    /// in place, other lines preserved).
    #[arg(long, default_value = "./.env")]
    #[serde(default = "default_output")]
    pub output_credentials: PathBuf,

    /// Environment variable name to write.
    #[arg(long, default_value = "REDIS_URL")]
    #[serde(default = "default_variable")]
    pub variable: String,

    /// Maximum time to wait for each async operation, in seconds.
    #[arg(long, default_value = "600")]
    #[serde(default = "default_wait_timeout")]
    pub wait_timeout: u32,

    /// Polling interval for async operations, in seconds.
    #[arg(long, default_value = "5")]
    #[serde(default = "default_wait_interval")]
    pub wait_interval: u32,
}

impl QuickDatabaseArgs {
    fn to_params(&self) -> QuickDatabaseParams {
        QuickDatabaseParams {
            name: self.name.clone(),
            output_credentials: self.output_credentials.clone(),
            variable: self.variable.clone(),
            wait_timeout: self.wait_timeout,
            wait_interval: self.wait_interval,
        }
    }
}

/// Args for `cloud workflow database-credentials` — write an EXISTING database's connection
/// string to a file (no provisioning). The heavy lifting is `core::…::existing_database_report`.
#[derive(Args, Debug, Clone)]
pub struct DatabaseCredentialsArgs {
    /// Subscription id of the existing Essentials database.
    #[arg(long)]
    pub subscription_id: i32,
    /// Database id within that subscription.
    #[arg(long)]
    pub database_id: i32,
    /// File to write the connection string into.
    #[arg(long, default_value = "./.env")]
    pub output_credentials: PathBuf,
    /// Environment variable name to write.
    #[arg(long, default_value = "REDIS_URL")]
    pub variable: String,
    /// Maximum time to wait for the endpoint to be readable, in seconds.
    #[arg(long, default_value = "600")]
    pub wait_timeout: u32,
    /// Polling interval in seconds.
    #[arg(long, default_value = "5")]
    pub wait_interval: u32,
}

impl DatabaseCredentialsArgs {
    pub(crate) fn to_params(&self) -> QuickDatabaseParams {
        QuickDatabaseParams {
            // `name` is only a report fallback here; the database's own name overrides it.
            name: format!("database-{}", self.database_id),
            output_credentials: self.output_credentials.clone(),
            variable: self.variable.clone(),
            wait_timeout: self.wait_timeout,
            wait_interval: self.wait_interval,
        }
    }
}

fn default_output() -> PathBuf {
    PathBuf::from("./.env")
}
fn default_variable() -> String {
    "REDIS_URL".to_string()
}
fn default_wait_timeout() -> u32 {
    600
}
fn default_wait_interval() -> u32 {
    5
}

pub struct QuickDatabaseWorkflow;

impl Workflow for QuickDatabaseWorkflow {
    fn name(&self) -> &str {
        "quick-database"
    }

    fn description(&self) -> &str {
        "Create or reuse a free Redis database and write its connection string to a file"
    }

    fn execute(
        &self,
        context: WorkflowContext,
        args: WorkflowArgs,
    ) -> Pin<Box<dyn Future<Output = Result<WorkflowResult>> + Send>> {
        Box::pin(async move {
            let args: QuickDatabaseArgs = args
                .get("args")
                .ok_or_else(|| anyhow::anyhow!("Missing workflow arguments"))?;
            // Box the typed error into anyhow; main.rs downcasts it for the exit contract.
            run(context, args).await.map_err(anyhow::Error::new)
        })
    }
}

async fn run(
    context: WorkflowContext,
    args: QuickDatabaseArgs,
) -> std::result::Result<WorkflowResult, QuickDatabaseError> {
    let quiet = context.output_format.is_json() || context.output_format.is_yaml();

    let client = context
        .conn_mgr
        .create_cloud_client(context.profile_name.as_deref())
        .await
        .map_err(client_setup_error)?;

    let pb = spinner(quiet, "Provisioning free database…");
    let result = provision(&client, &args.to_params()).await;
    finish(pb);
    let report = result?;

    let human = format!(
        "{} database '{}' (id {}). Connection string written to {} as {}.",
        if report.status == "reused" {
            "Reused"
        } else {
            "Provisioned"
        },
        report.database.name,
        report.database.id,
        report.credentials_written_to,
        report.credentials_variable,
    );

    let mut outputs = HashMap::new();
    outputs.insert(
        REPORT_KEY.to_string(),
        serde_json::to_value(&report)
            .map_err(|e| QuickDatabaseError::Other(format!("failed to serialize report: {e}")))?,
    );
    Ok(WorkflowResult {
        success: true,
        message: human,
        outputs,
    })
}

/// Map a client-construction failure (before any API call) to a branchable error. A missing
/// or bad credential means the caller needs to authenticate.
fn client_setup_error(err: RedisCtlError) -> QuickDatabaseError {
    match err {
        RedisCtlError::MissingCredentials { .. }
        | RedisCtlError::NoProfileConfigured { .. }
        | RedisCtlError::ProfileNotFound { .. }
        | RedisCtlError::AuthenticationFailed { .. } => QuickDatabaseError::NotAuthenticated(
            format!("{err}. Run `redisctl cloud auth login` first."),
        ),
        other => QuickDatabaseError::Other(other.to_string()),
    }
}

fn spinner(quiet: bool, message: &str) -> Option<ProgressBar> {
    if quiet {
        return None;
    }
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    Some(pb)
}

fn finish(pb: Option<ProgressBar>) {
    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
}
