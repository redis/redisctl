//! Higher-level Enterprise workflows that compose Layer 1 operations
//!
//! These workflows handle common patterns like:
//! - Start operation, poll for completion, return result
//! - Validate inputs before making API calls
//! - Progress reporting for long-running operations

use crate::enterprise::progress::{EnterpriseProgressCallback, poll_action};
use crate::error::Result;
use redis_enterprise::bdb::{DatabaseActionResponse, DatabaseUpgradeRequest, ModuleUpgrade};
use redis_enterprise::{Database, EnterpriseClient};
use std::time::Duration;

/// Default timeout for Enterprise async operations (10 minutes)
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

/// Default polling interval for Enterprise operations (5 seconds)
pub const DEFAULT_INTERVAL: Duration = Duration::from_secs(5);

/// Upgrade a database's Redis version and wait for completion
///
/// This workflow:
/// 1. Submits the upgrade request
/// 2. Polls the returned action until completion
/// 3. Returns the updated database
///
/// # Arguments
///
/// * `client` - The Enterprise API client
/// * `bdb_uid` - The database UID to upgrade
/// * `request` - The upgrade request parameters
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Example
///
/// ```rust,ignore
/// use redisctl_core::enterprise::upgrade_database_and_wait;
/// use redis_enterprise::DatabaseUpgradeRequest;
/// use std::time::Duration;
///
/// let request = DatabaseUpgradeRequest::builder()
///     .redis_version("7.2")
///     .build();
///
/// let db = upgrade_database_and_wait(
///     &client,
///     1,
///     &request,
///     Duration::from_secs(600),
///     None,
/// ).await?;
/// ```
pub async fn upgrade_database_and_wait(
    client: &EnterpriseClient,
    bdb_uid: u32,
    request: &DatabaseUpgradeRequest,
    timeout: Duration,
    on_progress: Option<EnterpriseProgressCallback>,
) -> Result<Database> {
    // Submit the upgrade request - returns the action with action_uid
    let action = client
        .databases()
        .upgrade_redis_version(bdb_uid, request.clone())
        .await?;

    // Poll until completion
    poll_action(
        client,
        &action.action_uid,
        timeout,
        DEFAULT_INTERVAL,
        on_progress,
    )
    .await?;

    // Fetch and return the updated database
    let db = client.databases().get(bdb_uid).await?;
    Ok(db)
}

/// Upgrade a database module and wait for completion
///
/// # Arguments
///
/// * `client` - The Enterprise API client
/// * `bdb_uid` - The database UID
/// * `module_name` - The module to upgrade (e.g., "search", "json")
/// * `new_version` - The target module version
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn upgrade_module_and_wait(
    client: &EnterpriseClient,
    bdb_uid: u32,
    module_name: &str,
    new_version: &str,
    timeout: Duration,
    on_progress: Option<EnterpriseProgressCallback>,
) -> Result<Database> {
    // Submit the module upgrade as part of a database upgrade request. The
    // dedicated module-upgrade method was folded into the upgrade endpoint
    // upstream; modules are now carried in the `modules` field.
    let request = DatabaseUpgradeRequest::builder()
        .modules(vec![ModuleUpgrade {
            module_name: module_name.to_string(),
            new_version: Some(new_version.to_string()),
            module_args: None,
        }])
        .build();
    let action = client
        .databases()
        .upgrade_redis_version(bdb_uid, request)
        .await?;

    // Poll until completion
    poll_action(
        client,
        &action.action_uid,
        timeout,
        DEFAULT_INTERVAL,
        on_progress,
    )
    .await?;

    // Fetch and return the updated database
    let db = client.databases().get(bdb_uid).await?;
    Ok(db)
}

/// Backup an Enterprise database and wait for completion
///
/// # Arguments
///
/// * `client` - The Enterprise API client
/// * `bdb_uid` - The database UID to backup
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
///
/// # Returns
///
/// Returns `Ok(())` on success. The backup can be retrieved using the
/// database's backup endpoints.
pub async fn backup_database_and_wait(
    client: &EnterpriseClient,
    bdb_uid: u32,
    timeout: Duration,
    on_progress: Option<EnterpriseProgressCallback>,
) -> Result<()> {
    // Trigger backup. The dedicated `backup` handler method was removed
    // upstream; POST the backup action directly (BDB.BACKUP).
    let response: DatabaseActionResponse = client
        .post(
            &format!("/v1/bdbs/{}/actions/backup", bdb_uid),
            &serde_json::json!({}),
        )
        .await?;

    // Poll until completion
    poll_action(
        client,
        &response.action_uid,
        timeout,
        DEFAULT_INTERVAL,
        on_progress,
    )
    .await?;

    Ok(())
}

/// Import data into an Enterprise database and wait for completion
///
/// WARNING: If `flush` is true, this will delete existing data before import!
///
/// # Arguments
///
/// * `client` - The Enterprise API client
/// * `bdb_uid` - The database UID to import into
/// * `import_location` - The location to import from (file path or URL)
/// * `flush` - Whether to flush the database before import
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn import_database_and_wait(
    client: &EnterpriseClient,
    bdb_uid: u32,
    import_location: &str,
    flush: bool,
    timeout: Duration,
    on_progress: Option<EnterpriseProgressCallback>,
) -> Result<()> {
    // Start import
    let response = client
        .databases()
        .import(bdb_uid, import_location, flush)
        .await?;

    // Poll until completion if we got an action_uid
    if let Some(action_uid) = response.action_uid {
        poll_action(client, &action_uid, timeout, DEFAULT_INTERVAL, on_progress).await?;
    }

    Ok(())
}

/// Flush all data from an Enterprise database and wait for completion
///
/// WARNING: This permanently deletes all data in the database!
///
/// # Arguments
///
/// * `client` - The Enterprise API client
/// * `bdb_uid` - The database UID to flush
/// * `timeout` - Maximum time to wait for completion
/// * `on_progress` - Optional callback for progress updates
pub async fn flush_database_and_wait(
    client: &EnterpriseClient,
    bdb_uid: u32,
    timeout: Duration,
    on_progress: Option<EnterpriseProgressCallback>,
) -> Result<()> {
    // Trigger flush
    let response = client.databases().flush(bdb_uid).await?;

    // Poll until completion
    poll_action(
        client,
        &response.action_uid,
        timeout,
        DEFAULT_INTERVAL,
        on_progress,
    )
    .await?;

    Ok(())
}
