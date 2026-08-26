//! Initialize Redis Enterprise cluster workflow
//!
//! This workflow automates the process of setting up a new Redis Enterprise cluster,
//! including bootstrap, waiting for initialization, creating admin user, and
//! optionally creating a default database.

use crate::workflows::{Workflow, WorkflowArgs, WorkflowContext, WorkflowResult};
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use redis_enterprise::EnterpriseClient;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::sleep;

pub struct InitClusterWorkflow;

impl InitClusterWorkflow {
    pub fn new() -> Self {
        Self
    }
}

impl Workflow for InitClusterWorkflow {
    fn name(&self) -> &str {
        "init-cluster"
    }

    fn description(&self) -> &str {
        "Initialize a Redis Enterprise cluster with bootstrap and optional database creation"
    }

    fn execute(
        &self,
        context: WorkflowContext,
        args: WorkflowArgs,
    ) -> Pin<Box<dyn Future<Output = Result<WorkflowResult>> + Send>> {
        Box::pin(async move {
            use crate::output::OutputFormat;

            // Only print human-readable output for Table/Auto format
            let is_human_output = matches!(
                context.output_format,
                OutputFormat::Table | OutputFormat::Auto
            );

            if is_human_output {
                println!("Initializing Redis Enterprise cluster...");
            }

            // Get parameters
            let cluster_name = args
                .get_string("name")
                .unwrap_or_else(|| "redis-cluster".to_string());
            let username = args
                .get_string("username")
                .unwrap_or_else(|| "admin@redis.local".to_string());
            let password = args
                .get_string("password")
                .context("Password is required for cluster initialization")?;
            let create_db = args.get_bool("create_database").unwrap_or(true);
            let db_name = args
                .get_string("database_name")
                .unwrap_or_else(|| "default-db".to_string());
            let db_memory_gb = args.get_i64("database_memory_gb").unwrap_or(1);
            let requested_db_redis_version = args.get_string("database_redis_version");

            // Bootstrap does not require existing cluster credentials. The 0.10 client
            // builder still requires non-empty values, so use the credentials this
            // workflow is about to install; uninitialized bootstrap routes accept them.
            let base_url = std::env::var("REDIS_ENTERPRISE_URL")
                .unwrap_or_else(|_| "https://localhost:9443".to_string());
            let insecure = std::env::var("REDIS_ENTERPRISE_INSECURE")
                .unwrap_or_else(|_| "false".to_string())
                .parse::<bool>()
                .unwrap_or(false);

            let client = redis_enterprise::EnterpriseClient::builder()
                .base_url(base_url)
                .username(username.clone())
                .password(password.clone())
                .insecure(insecure)
                .build()
                .context("Failed to create Enterprise client for bootstrap")?;

            // Step 1: Check if cluster is already initialized
            let needs_bootstrap = check_if_needs_bootstrap(&client).await?;

            if !needs_bootstrap {
                if is_human_output {
                    println!("Cluster is already initialized");
                }
                return Ok(WorkflowResult::success("Cluster already initialized")
                    .with_output("cluster_name", &cluster_name)
                    .with_output("already_initialized", true));
            }

            // Step 2: Bootstrap the cluster
            let bootstrap_data = json!({
                "action": "create_cluster",
                "cluster": {
                    "name": cluster_name
                },
                "credentials": {
                    "username": username,
                    "password": password
                },
                "flash_enabled": false
            });

            let bootstrap_result = client
                .post_bootstrap("/v1/bootstrap/create_cluster", &bootstrap_data)
                .await
                .context("Failed to bootstrap cluster")?;

            // Check if bootstrap returned an action ID (async operation)
            if let Some(action_id) = bootstrap_result.get("action_uid").and_then(|v| v.as_str()) {
                // Wait for bootstrap to complete
                wait_for_action(&client, action_id, "cluster bootstrap").await?;
            } else {
                // Bootstrap was synchronous, just wait a bit for cluster to stabilize
                sleep(Duration::from_secs(5)).await;
            }

            if is_human_output {
                println!("Bootstrap completed successfully");
            }

            // Step 3: Cluster should be ready after bootstrap
            // Wait longer for cluster to fully stabilize
            sleep(Duration::from_secs(10)).await;
            if is_human_output {
                println!("Cluster is ready");
            }

            // After bootstrap, we need to create a new client with the credentials we just set
            // Get the base URL from environment or use default
            let base_url = std::env::var("REDIS_ENTERPRISE_URL")
                .unwrap_or_else(|_| "https://localhost:9443".to_string());
            let insecure = std::env::var("REDIS_ENTERPRISE_INSECURE")
                .unwrap_or_else(|_| "true".to_string())
                .parse::<bool>()
                .unwrap_or(true);

            let authenticated_client = redis_enterprise::EnterpriseClient::builder()
                .base_url(base_url)
                .username(username.clone())
                .password(password.clone())
                .insecure(insecure)
                .build()
                .context("Failed to create authenticated client after bootstrap")?;

            // Step 4: Optionally create a default database
            let mut database_created = false;
            let mut database_uid = None;
            let mut database_redis_version = None;
            if create_db {
                if is_human_output {
                    println!("Creating default database '{}'...", db_name);
                }

                let selected_version = select_database_redis_version(
                    &authenticated_client,
                    requested_db_redis_version.as_deref(),
                )
                .await?;
                database_redis_version = Some(selected_version.clone());

                let db_data = json!({
                    "name": db_name,
                    "memory_size": db_memory_gb * 1024 * 1024 * 1024,  // Convert GB to bytes
                    "type": "redis",
                    "replication": false,
                    "redis_version": selected_version,
                });

                let db_result = create_database_strict(&authenticated_client, db_data)
                    .await
                    .context("Cluster initialized, but requested database creation failed")?;

                if let Some(action_id) = db_result.get("action_uid").and_then(|v| v.as_str()) {
                    wait_for_action(&authenticated_client, action_id, "database creation").await?;
                }

                let db_uid = db_result
                    .get("uid")
                    .or_else(|| db_result.get("resource_id"))
                    .and_then(|v| v.as_u64())
                    .and_then(|uid| u32::try_from(uid).ok());
                database_uid = db_uid;
                database_created = true;

                if is_human_output {
                    if let Some(db_uid) = db_uid {
                        println!("Database created successfully (ID: {db_uid})");
                    } else {
                        println!("Database created successfully");
                    }
                }

                // Connectivity verification is informative after successful creation.
                if let Some(db_uid) = db_uid {
                    sleep(Duration::from_secs(2)).await;
                    match authenticated_client.execute_command(db_uid, "PING").await {
                        Ok(response) => {
                            if let Some(result) = response.get("response")
                                && (result.as_bool() == Some(true)
                                    || result.as_str() == Some("PONG"))
                                && is_human_output
                            {
                                println!("Database connectivity verified (PING successful)");
                            }
                        }
                        Err(error) if is_human_output => {
                            eprintln!("Note: Could not verify database connectivity: {error}");
                        }
                        Err(_) => {}
                    }
                }
            } else if is_human_output {
                println!("Skipping database creation (--skip-database flag set)");
            }

            // Final summary (only for human output)
            if is_human_output {
                println!();
                println!("Cluster initialization completed successfully");
                println!();
                println!("Cluster name: {}", cluster_name);
                println!("Admin user: {}", username);
                if database_created {
                    println!("Database: {} ({}GB)", db_name, db_memory_gb);
                    if let Some(version) = &database_redis_version {
                        println!("Redis version: {version}");
                    }
                }
                println!();
                println!("Access endpoints:");
                println!("  Web UI: https://localhost:8443");
                println!("  API: https://localhost:9443");
            }

            let mut result = WorkflowResult::success("Cluster initialized successfully")
                .with_output("cluster_name", &cluster_name)
                .with_output("username", &username)
                .with_output("database_created", database_created);
            if database_created {
                result = result
                    .with_output("database_name", &db_name)
                    .with_output("database_uid", database_uid)
                    .with_output("database_redis_version", database_redis_version);
            }
            Ok(result)
        })
    }
}

async fn create_database_strict(client: &EnterpriseClient, body: Value) -> Result<Value> {
    create_database_with_retry_delay(client, body, Duration::from_secs(5)).await
}

async fn create_database_with_retry_delay(
    client: &EnterpriseClient,
    body: Value,
    retry_delay: Duration,
) -> Result<Value> {
    match client.post_raw("/v1/bdbs", body.clone()).await {
        Ok(database) => Ok(database),
        Err(redis_enterprise::RestError::ApiError { code: 406, .. }) => {
            // A freshly bootstrapped cluster can briefly reject an otherwise valid
            // version constraint while its capabilities finish initializing.
            sleep(retry_delay).await;
            client
                .post_raw("/v1/bdbs", body)
                .await
                .context("Database creation still failed after the cluster-settling retry")
        }
        Err(error) => Err(error.into()),
    }
}

async fn select_database_redis_version(
    client: &EnterpriseClient,
    requested: Option<&str>,
) -> Result<String> {
    if let Some(requested) = requested {
        return normalize_major_minor(requested).with_context(|| {
            format!("Invalid database Redis version '{requested}'; expected MAJOR.MINOR")
        });
    }

    if let Ok(cluster) = client.get_raw("/v1/cluster").await
        && let Some(version) = cluster
            .get("default_provisioned_redis_version")
            .and_then(Value::as_str)
            .and_then(normalize_major_minor)
    {
        return Ok(version);
    }

    let nodes = client
        .get_raw("/v1/nodes")
        .await
        .context("Failed to discover supported database Redis versions")?;
    let mut versions = nodes
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|node| node.get("supported_database_versions"))
        .flat_map(extract_supported_versions)
        .collect::<Vec<_>>();
    versions.sort_by_key(|version| version_tuple(version));
    versions.dedup();
    versions.pop().context(
        "Cluster did not advertise a supported database Redis version; pass --database-redis-version MAJOR.MINOR",
    )
}

fn extract_supported_versions(value: &Value) -> Vec<String> {
    match value {
        Value::Array(values) => values.iter().flat_map(extract_supported_versions).collect(),
        Value::String(version) => normalize_major_minor(version).into_iter().collect(),
        Value::Object(object) => ["version", "redis_version", "semantic_version", "name"]
            .into_iter()
            .filter_map(|key| object.get(key).and_then(Value::as_str))
            .filter_map(normalize_major_minor)
            .collect(),
        _ => Vec::new(),
    }
}

fn normalize_major_minor(version: &str) -> Option<String> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    Some(format!("{major}.{minor}"))
}

fn version_tuple(version: &str) -> (u32, u32) {
    let mut parts = version
        .split('.')
        .filter_map(|part| part.parse::<u32>().ok());
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0))
}

/// Check if the cluster needs bootstrap
async fn check_if_needs_bootstrap(client: &EnterpriseClient) -> Result<bool> {
    let status = client
        .bootstrap()
        .status()
        .await
        .context("Failed to read cluster bootstrap status")?;
    Ok(bootstrap_state_needs_initialization(
        &status.bootstrap_status.state,
    ))
}

fn bootstrap_state_needs_initialization(state: &str) -> bool {
    matches!(state, "idle" | "unconfigured" | "new")
}

/// Wait for an async action to complete
async fn wait_for_action(
    client: &EnterpriseClient,
    action_id: &str,
    operation_name: &str,
) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("Waiting for {} to complete...", operation_name));

    let max_attempts = 120; // 10 minutes with 5 second intervals
    for attempt in 1..=max_attempts {
        pb.set_message(format!(
            "Waiting for {} to complete... (attempt {}/{})",
            operation_name, attempt, max_attempts
        ));

        match client.get_raw(&format!("/v1/actions/{}", action_id)).await {
            Ok(action) => {
                if let Some(status) = action.get("status").and_then(|v| v.as_str()) {
                    match status {
                        "completed" | "done" => {
                            pb.finish_and_clear();
                            return Ok(());
                        }
                        "failed" | "error" => {
                            pb.finish_and_clear();
                            let error_msg = action
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown error");
                            anyhow::bail!("{} failed: {}", operation_name, error_msg);
                        }
                        _ => {
                            // Still in progress
                        }
                    }
                }
            }
            Err(_) => {
                // Action might not be available yet
            }
        }

        sleep(Duration::from_secs(5)).await;
    }

    pb.finish_and_clear();
    anyhow::bail!("{} did not complete within 10 minutes", operation_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> EnterpriseClient {
        EnterpriseClient::builder()
            .base_url(server.uri())
            .username("admin")
            .password("secret")
            .build()
            .unwrap()
    }

    #[test]
    fn normalizes_patch_versions_to_server_accepted_major_minor() {
        assert_eq!(normalize_major_minor("7.2.4").as_deref(), Some("7.2"));
        assert_eq!(normalize_major_minor("v8.0").as_deref(), Some("8.0"));
        assert_eq!(normalize_major_minor("latest"), None);
    }

    #[test]
    fn extracts_supported_versions_from_known_node_shapes() {
        let value = serde_json::json!([
            "6.2.14",
            {"version": "7.2.4"},
            {"redis_version": "7.4"}
        ]);
        assert_eq!(extract_supported_versions(&value), ["6.2", "7.2", "7.4"]);
    }

    #[test]
    fn bootstrap_status_distinguishes_idle_from_completed() {
        assert!(bootstrap_state_needs_initialization("idle"));
        assert!(bootstrap_state_needs_initialization("unconfigured"));
        assert!(!bootstrap_state_needs_initialization("initializing"));
        assert!(!bootstrap_state_needs_initialization("completed"));
    }

    #[tokio::test]
    async fn selected_version_is_sent_on_successful_database_creation() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/cluster"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "default_provisioned_redis_version": "7.2.4"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/bdbs"))
            .and(body_json(serde_json::json!({
                "name": "default-db",
                "memory_size": 1_073_741_824_u64,
                "type": "redis",
                "replication": false,
                "redis_version": "7.2"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"uid": 1})))
            .expect(1)
            .mount(&server)
            .await;

        let client = client(&server);
        let version = select_database_redis_version(&client, None).await.unwrap();
        let result = create_database_strict(
            &client,
            serde_json::json!({
                "name": "default-db",
                "memory_size": 1_073_741_824_u64,
                "type": "redis",
                "replication": false,
                "redis_version": version,
            }),
        )
        .await
        .unwrap();

        assert_eq!(result["uid"], 1);
    }

    #[tokio::test]
    async fn database_creation_failure_is_strict() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/bdbs"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "description": "creation failed"
            })))
            .expect(1)
            .mount(&server)
            .await;

        let error = create_database_strict(
            &client(&server),
            serde_json::json!({"name": "default-db", "redis_version": "7.2"}),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("creation failed"));
    }

    #[tokio::test]
    async fn database_creation_retries_one_transient_406() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/bdbs"))
            .respond_with(ResponseTemplate::new(406).set_body_json(serde_json::json!({
                "description": "version constraints still initializing",
                "error_code": "invalid_version"
            })))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/bdbs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"uid": 1})))
            .expect(1)
            .mount(&server)
            .await;

        let result = create_database_with_retry_delay(
            &client(&server),
            serde_json::json!({"name": "default-db", "redis_version": "7.2"}),
            Duration::ZERO,
        )
        .await
        .unwrap();

        assert_eq!(result["uid"], 1);
    }
}
