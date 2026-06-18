//! Cloud workflows - multi-step operations
//!
//! These workflows compose Layer 1 operations with progress tracking
//! and additional logic.

use crate::error::{CoreError, Result};
use crate::progress::{ProgressCallback, poll_task};
use redis_cloud::databases::{
    Database, DatabaseBackupRequest, DatabaseCreateRequest, DatabaseImportRequest,
    DatabaseUpdateRequest,
};
use redis_cloud::subscriptions::{
    Subscription, SubscriptionCreateRequest, SubscriptionUpdateRequest,
};
use redis_cloud::{CloudClient, DatabaseHandler, SubscriptionHandler};
use std::time::Duration;

/// Create a database and wait for completion
///
/// This is a convenience workflow that:
/// 1. Creates a database (returns task)
/// 2. Polls the task until completion
/// 3. Fetches and returns the created database
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription to create the database in
/// * `request` - The database creation request
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Example
///
/// ```rust,ignore
/// use redis_cloud::databases::DatabaseCreateRequest;
/// use redisctl_core::cloud::create_database_and_wait;
/// use std::time::Duration;
///
/// let request = DatabaseCreateRequest::builder()
///     .name("my-database")
///     .memory_limit_in_gb(1.0)
///     .build();
///
/// let database = create_database_and_wait(
///     &client,
///     subscription_id,
///     &request,
///     Duration::from_secs(600),
///     None,  // No progress callback
/// ).await?;
///
/// println!("Created database: {}", database.name.unwrap_or_default());
/// ```
pub async fn create_database_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    request: &DatabaseCreateRequest,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<Database> {
    let handler = DatabaseHandler::new(client.clone());

    // Step 1: Create (returns task)
    let task = handler.create(subscription_id, request).await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    let completed = poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    // Step 3: Fetch the created resource
    let resource_id = completed
        .response
        .and_then(|r| r.resource_id)
        .ok_or_else(|| CoreError::TaskFailed("No resource ID in completed task".to_string()))?;

    let db = handler.get(subscription_id, resource_id as i32).await?;
    Ok(db)
}

/// Delete a database and wait for completion
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription containing the database
/// * `database_id` - The database to delete
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn delete_database_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let handler = DatabaseHandler::new(client.clone());

    // Step 1: Delete (returns task)
    let task = handler.delete(subscription_id, database_id).await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    Ok(())
}

/// Update a database and wait for completion
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription containing the database
/// * `database_id` - The database to update
/// * `request` - The database update request
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Example
///
/// ```rust,ignore
/// use redis_cloud::databases::DatabaseUpdateRequest;
/// use redisctl_core::cloud::update_database_and_wait;
/// use std::time::Duration;
///
/// let request = DatabaseUpdateRequest::builder()
///     .name("new-name")
///     .memory_limit_in_gb(2.0)
///     .build();
///
/// let database = update_database_and_wait(
///     &client,
///     subscription_id,
///     database_id,
///     &request,
///     Duration::from_secs(600),
///     None,
/// ).await?;
/// ```
pub async fn update_database_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    request: &DatabaseUpdateRequest,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<Database> {
    let handler = DatabaseHandler::new(client.clone());

    // Step 1: Update (returns task)
    let task = handler
        .update(subscription_id, database_id, request)
        .await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    // Step 3: Fetch the updated database
    let db = handler.get(subscription_id, database_id).await?;
    Ok(db)
}

/// Backup a database and wait for completion
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription containing the database
/// * `database_id` - The database to backup
/// * `region_name` - Optional region name (required for Active-Active databases)
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn backup_database_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    region_name: Option<&str>,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let handler = DatabaseHandler::new(client.clone());

    // Build backup request
    let request = if let Some(region) = region_name {
        DatabaseBackupRequest::builder().region_name(region).build()
    } else {
        DatabaseBackupRequest::builder().build()
    };

    // Step 1: Trigger backup (returns task)
    let task = handler
        .backup_database(subscription_id, database_id, &request)
        .await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    Ok(())
}

/// Import data into a database and wait for completion
///
/// WARNING: This will overwrite existing data in the database!
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription containing the database
/// * `database_id` - The database to import into
/// * `request` - The import request specifying source type and URIs
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Example
///
/// ```rust,ignore
/// use redis_cloud::databases::DatabaseImportRequest;
/// use redisctl_core::cloud::import_database_and_wait;
/// use std::time::Duration;
///
/// let request = DatabaseImportRequest::builder()
///     .source_type("aws-s3")
///     .import_from_uri(vec!["s3://bucket/file.rdb".to_string()])
///     .build();
///
/// import_database_and_wait(
///     &client,
///     subscription_id,
///     database_id,
///     &request,
///     Duration::from_secs(1800), // Imports can take longer
///     None,
/// ).await?;
/// ```
pub async fn import_database_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    request: &DatabaseImportRequest,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let handler = DatabaseHandler::new(client.clone());

    // Step 1: Start import (returns task)
    let task = handler
        .import_database(subscription_id, database_id, request)
        .await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    Ok(())
}

/// Flush all data from a database and wait for completion
///
/// WARNING: This permanently deletes ALL data in the database!
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription containing the database
/// * `database_id` - The database to flush
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn flush_database_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let handler = DatabaseHandler::new(client.clone());

    // Step 1: Flush (returns task)
    let task = handler.flush_database(subscription_id, database_id).await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(5),
        on_progress,
    )
    .await?;

    Ok(())
}

// =============================================================================
// Subscription Workflows
// =============================================================================

/// Create a subscription and wait for completion
///
/// This workflow:
/// 1. Creates a subscription (returns task)
/// 2. Polls the task until completion
/// 3. Fetches and returns the created subscription
///
/// Note: Subscription creation requires complex nested structures (cloudProviders,
/// databases arrays). Use `SubscriptionCreateRequest::builder()` from redis-cloud
/// to construct the request.
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `request` - The subscription creation request
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Example
///
/// ```rust,ignore
/// use redis_cloud::subscriptions::{
///     SubscriptionCreateRequest, SubscriptionSpec, SubscriptionRegionSpec,
///     SubscriptionDatabaseSpec,
/// };
/// use redisctl_core::cloud::create_subscription_and_wait;
/// use std::time::Duration;
///
/// let request = SubscriptionCreateRequest::builder()
///     .name("my-subscription")
///     .cloud_providers(vec![SubscriptionSpec {
///         provider: Some("AWS".to_string()),
///         cloud_account_id: Some(1),
///         regions: vec![SubscriptionRegionSpec {
///             region: "us-east-1".to_string(),
///             ..Default::default()
///         }],
///     }])
///     .databases(vec![SubscriptionDatabaseSpec { /* ... */ }])
///     .build();
///
/// let subscription = create_subscription_and_wait(
///     &client,
///     &request,
///     Duration::from_secs(1800), // Subscriptions can take longer
///     None,
/// ).await?;
/// ```
pub async fn create_subscription_and_wait(
    client: &CloudClient,
    request: &SubscriptionCreateRequest,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<Subscription> {
    let handler = SubscriptionHandler::new(client.clone());

    // Step 1: Create (returns task)
    let task = handler.create_subscription(request).await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    let completed = poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(15), // Subscriptions take longer, poll less frequently
        on_progress,
    )
    .await?;

    // Step 3: Fetch the created resource
    let resource_id = completed
        .response
        .and_then(|r| r.resource_id)
        .ok_or_else(|| CoreError::TaskFailed("No resource ID in completed task".to_string()))?;

    let subscription = handler.get_subscription_by_id(resource_id).await?;
    Ok(subscription)
}

/// Update a subscription and wait for completion
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription to update
/// * `request` - The update request
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Example
///
/// ```rust,ignore
/// use redis_cloud::subscriptions::SubscriptionUpdateRequest;
/// use redisctl_core::cloud::update_subscription_and_wait;
/// use std::time::Duration;
///
/// let request = SubscriptionUpdateRequest::builder()
///     .name("renamed-subscription")
///     .build();
///
/// let subscription = update_subscription_and_wait(
///     &client,
///     123,
///     &request,
///     Duration::from_secs(600),
///     None,
/// ).await?;
/// ```
pub async fn update_subscription_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    request: &SubscriptionUpdateRequest,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<Subscription> {
    let handler = SubscriptionHandler::new(client.clone());

    // Step 1: Update (returns task)
    let task = handler
        .update_subscription(subscription_id, request)
        .await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    // Step 3: Fetch the updated subscription
    let subscription = handler.get_subscription_by_id(subscription_id).await?;
    Ok(subscription)
}

/// Delete a subscription and wait for completion
///
/// WARNING: This will delete the subscription and all its databases!
/// Ensure all databases are deleted first or this operation may fail.
///
/// # Arguments
///
/// * `client` - The Cloud API client
/// * `subscription_id` - The subscription to delete
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn delete_subscription_and_wait(
    client: &CloudClient,
    subscription_id: i32,
    timeout: Duration,
    on_progress: Option<ProgressCallback>,
) -> Result<()> {
    let handler = SubscriptionHandler::new(client.clone());

    // Step 1: Delete (returns task)
    let task = handler.delete_subscription_by_id(subscription_id).await?;
    let task_id = task
        .task_id
        .ok_or_else(|| CoreError::TaskFailed("No task ID returned".to_string()))?;

    // Step 2: Poll until complete
    poll_task(
        client,
        &task_id,
        timeout,
        Duration::from_secs(10),
        on_progress,
    )
    .await?;

    Ok(())
}
