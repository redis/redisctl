use super::super::{Workflow, WorkflowArgs, WorkflowContext, WorkflowResult};
use anyhow::{Context, Result, bail};
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

/// Arguments for subscription setup workflow
#[derive(Args, Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionSetupArgs {
    /// Subscription name
    #[arg(long, default_value = "redisctl-test")]
    #[serde(default = "default_subscription_name")]
    pub name: String,

    /// Cloud provider (AWS, GCP, or Azure)
    #[arg(long, default_value = "AWS")]
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Cloud region
    #[arg(long, default_value = "us-east-1")]
    #[serde(default = "default_region")]
    pub region: String,

    /// Payment method ID (will look up credit card if not specified)
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_method_id: Option<String>,

    /// Database name
    #[arg(long, default_value = "default-db")]
    #[serde(default = "default_database_name")]
    pub database_name: String,

    /// Database memory in GB
    #[arg(long, default_value = "1")]
    #[serde(default = "default_database_memory")]
    pub database_memory_gb: f64,

    /// Database throughput (operations per second)
    #[arg(long, default_value = "1000")]
    #[serde(default = "default_database_throughput")]
    pub database_throughput: u32,

    /// Database modules (comma-separated, e.g., "RedisJSON,RediSearch")
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modules: Option<String>,

    /// Enable high availability
    #[arg(long)]
    #[serde(default)]
    pub high_availability: bool,

    /// Enable data persistence
    #[arg(long, default_value = "true")]
    #[serde(default = "default_true")]
    pub data_persistence: bool,

    /// Skip database creation (only create subscription)
    #[arg(long)]
    #[serde(default)]
    pub skip_database: bool,

    /// Wait for operations to complete
    #[arg(long, default_value = "true")]
    #[serde(default = "default_true")]
    pub wait: bool,

    /// Maximum time to wait in seconds
    #[arg(long, default_value = "600")]
    #[serde(default = "default_wait_timeout")]
    pub wait_timeout: u32,

    /// Polling interval in seconds
    #[arg(long, default_value = "10")]
    #[serde(default = "default_wait_interval")]
    pub wait_interval: u32,

    /// Dry run - show what would be created without actually creating
    #[arg(long)]
    #[serde(default)]
    pub dry_run: bool,
}

// Default value functions for serde
fn default_subscription_name() -> String {
    "redisctl-test".to_string()
}
fn default_provider() -> String {
    "AWS".to_string()
}
fn default_region() -> String {
    "us-east-1".to_string()
}
fn default_database_name() -> String {
    "default-db".to_string()
}
fn default_database_memory() -> f64 {
    1.0
}
fn default_database_throughput() -> u32 {
    1000
}
fn default_true() -> bool {
    true
}
fn default_wait_timeout() -> u32 {
    600
}
fn default_wait_interval() -> u32 {
    10
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubscriptionSetupOutputs {
    pub subscription_id: Option<u32>,
    pub subscription_name: String,
    pub database_id: Option<u32>,
    pub database_name: Option<String>,
    pub connection_string: Option<String>,
    pub provider: String,
    pub region: String,
    pub status: String,
}

pub struct SubscriptionSetupWorkflow;

impl Workflow for SubscriptionSetupWorkflow {
    fn name(&self) -> &str {
        "subscription-setup"
    }

    fn description(&self) -> &str {
        "Complete Redis Cloud subscription setup with optional database"
    }

    fn execute(
        &self,
        context: WorkflowContext,
        args: WorkflowArgs,
    ) -> Pin<Box<dyn Future<Output = Result<WorkflowResult>> + Send>> {
        Box::pin(async move {
            // Extract args from WorkflowArgs
            let setup_args: SubscriptionSetupArgs = args
                .get("args")
                .ok_or_else(|| anyhow::anyhow!("Missing workflow arguments"))?;

            let quiet = context.output_format.is_json() || context.output_format.is_yaml();

            if setup_args.dry_run {
                let mut outputs = HashMap::new();
                outputs.insert("dry_run".to_string(), json!(true));
                outputs.insert(
                    "would_create".to_string(),
                    json!({
                        "subscription": {
                            "name": setup_args.name,
                            "provider": setup_args.provider,
                            "region": setup_args.region,
                        },
                        "database": if !setup_args.skip_database {
                            json!({
                                "name": setup_args.database_name,
                                "memory_gb": setup_args.database_memory_gb,
                                "throughput": setup_args.database_throughput,
                                "modules": setup_args.modules,
                            })
                        } else {
                            json!(null)
                        }
                    }),
                );

                return Ok(WorkflowResult {
                    success: true,
                    message: "Dry run completed".to_string(),
                    outputs,
                });
            }

            // Create Cloud client
            let client = context
                .conn_mgr
                .create_cloud_client(context.profile_name.as_deref())
                .await
                .context("Failed to create Cloud client")?;

            let mut outputs = SubscriptionSetupOutputs {
                subscription_id: None,
                subscription_name: setup_args.name.clone(),
                database_id: None,
                database_name: None,
                connection_string: None,
                provider: setup_args.provider.clone(),
                region: setup_args.region.clone(),
                status: "pending".to_string(),
            };

            // Step 1: Get payment method if not provided
            let payment_method_id = if let Some(ref id) = setup_args.payment_method_id {
                id.clone()
            } else {
                if !quiet {
                    println!("Looking up payment method...");
                }

                let payment_methods = client
                    .get_raw("/payment-methods")
                    .await
                    .context("Failed to get payment methods")?;

                let payment_methods = payment_methods["paymentMethods"]
                    .as_array()
                    .context("Invalid payment methods response")?;

                if payment_methods.is_empty() {
                    bail!(
                        "No payment methods found. Please add a payment method to your account first."
                    );
                }

                // Use the first credit card found
                let credit_card = payment_methods
                    .iter()
                    .find(|pm| pm["type"].as_str() == Some("credit-card"))
                    .or_else(|| payment_methods.first())
                    .context("No suitable payment method found")?;

                credit_card["id"]
                    .as_u64()
                    .context("Invalid payment method ID")?
                    .to_string()
            };

            // Step 2: Create subscription
            if !quiet {
                println!("Creating subscription '{}'...", setup_args.name);
            }

            let subscription_payload = build_subscription_payload(
                &setup_args,
                payment_method_id
                    .parse::<u32>()
                    .context("Invalid payment method ID")?,
            );

            // Try to create subscription - handle error response properly
            let create_result = client
                .post_raw("/subscriptions", subscription_payload.clone())
                .await;

            let create_response = match create_result {
                Ok(resp) => resp,
                Err(e) => {
                    bail!("Failed to create subscription: {}", e);
                }
            };

            // Task ID is used to track the async operation
            let task_id = create_response["taskId"]
                .as_str()
                .context("No task ID in create response")?;

            // Step 3: Wait for subscription to be active
            if setup_args.wait {
                if !quiet {
                    let pb = ProgressBar::new_spinner();
                    pb.set_style(
                        ProgressStyle::with_template("{spinner:.green} {msg}")
                            .unwrap()
                            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
                    );
                    pb.set_message("Waiting for subscription to become active...");
                    pb.enable_steady_tick(Duration::from_millis(100));
                }

                // Wait for the task to complete and get the subscription ID
                let subscription_id = wait_for_task_completion(
                    &client,
                    task_id,
                    setup_args.wait_timeout,
                    setup_args.wait_interval,
                )
                .await?;

                outputs.subscription_id = Some(subscription_id);
                outputs.status = "active".to_string();

                if !quiet {
                    println!(
                        "Subscription created successfully (ID: {})",
                        subscription_id
                    );
                }
            }

            // Step 4: Get database details (database was created with subscription)
            if let Some(subscription_id) = outputs.subscription_id
                && !setup_args.skip_database
            {
                // Database was created with the subscription, so just get its details
                if setup_args.wait {
                    // Give it a moment for the database to be available
                    tokio::time::sleep(Duration::from_secs(5)).await;

                    // List databases in the subscription to get the database ID
                    let databases = client
                        .get_raw(&format!("/subscriptions/{}/databases", subscription_id))
                        .await
                        .context("Failed to get databases")?;

                    if let Some(db_array) = databases.as_array()
                        && let Some(first_db) = db_array.first()
                        && let Some(db_id) = first_db["databaseId"].as_u64()
                    {
                        outputs.database_id = Some(db_id as u32);
                        outputs.database_name = Some(setup_args.database_name.clone());

                        if let Some(public_endpoint) = first_db["publicEndpoint"].as_str() {
                            outputs.connection_string =
                                Some(format!("redis://{}", public_endpoint));
                        }

                        if !quiet {
                            println!("Database created successfully (ID: {})", db_id);
                        }
                    }
                }
            }

            // Generate final message
            let mut message = format!(
                "Subscription setup completed successfully\n\n\
                Subscription: {} (ID: {})\n\
                Provider: {} / {}\n",
                outputs.subscription_name,
                outputs
                    .subscription_id
                    .map_or("pending".to_string(), |id| id.to_string()),
                outputs.provider,
                outputs.region,
            );

            if let Some(db_name) = &outputs.database_name {
                message.push_str(&format!(
                    "Database: {} (ID: {})\n",
                    db_name,
                    outputs
                        .database_id
                        .map_or("pending".to_string(), |id| id.to_string()),
                ));
            }

            if let Some(conn_str) = &outputs.connection_string {
                message.push_str(&format!("\nConnection string: {}\n", conn_str));
            }

            let mut result_outputs = HashMap::new();
            result_outputs.insert("outputs".to_string(), serde_json::to_value(outputs)?);

            Ok(WorkflowResult {
                success: true,
                message,
                outputs: result_outputs,
            })
        })
    }
}

fn build_subscription_payload(args: &SubscriptionSetupArgs, payment_method_id: u32) -> Value {
    let cloud_providers = vec![json!({
        "provider": args.provider.to_uppercase(),  // API expects uppercase
        "regions": [{
            "region": args.region,
            "networking": {
                "deploymentCIDR": "10.0.0.0/24"
            }
        }]
    })];

    // Redis Cloud API requires at least one database in the initial subscription
    let databases = if !args.skip_database {
        vec![json!({
            "name": args.database_name,
            "memoryLimitInGb": args.database_memory_gb,
            "protocol": "redis"
        })]
    } else {
        // Even when skipping database, we need a minimal one for API requirements
        vec![json!({
            "name": "minimal-db",
            "memoryLimitInGb": 0.1,  // Smallest possible
            "protocol": "redis"
        })]
    };

    json!({
        "name": args.name,
        "paymentMethodId": payment_method_id,
        "cloudProviders": cloud_providers,
        "databases": databases
    })
}

async fn wait_for_task_completion(
    client: &redis_cloud::CloudClient,
    task_id: &str,
    timeout_seconds: u32,
    interval_seconds: u32,
) -> Result<u32> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(timeout_seconds as u64);
    let interval = Duration::from_secs(interval_seconds as u64);

    loop {
        if start.elapsed() > timeout {
            bail!("Operation timed out after {} seconds", timeout_seconds);
        }

        let task = client
            .get_raw(&format!("/tasks/{}", task_id))
            .await
            .context("Failed to get task status")?;

        let status = task["status"]
            .as_str()
            .or_else(|| task["state"].as_str())
            .unwrap_or("unknown");

        match status.to_lowercase().as_str() {
            "completed"
            | "complete"
            | "finished"
            | "succeeded"
            | "success"
            | "processing-completed" => {
                // Extract the resource ID from the completed task
                let resource_id = task["response"]["resourceId"]
                    .as_u64()
                    .or_else(|| task["response"]["resource"]["id"].as_u64())
                    .or_else(|| task["resourceId"].as_u64())
                    .context("No resource ID in completed task")?;
                return Ok(resource_id as u32);
            }
            "failed" | "error" => {
                let error = task["response"]["error"]
                    .as_str()
                    .or_else(|| task["error"].as_str())
                    .or_else(|| task["message"].as_str())
                    .unwrap_or("Unknown error");
                bail!("Task failed: {}", error);
            }
            _ => {
                tokio::time::sleep(interval).await;
            }
        }
    }
}
