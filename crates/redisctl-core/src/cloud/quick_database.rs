//! Idempotent free-tier provisioning — the engine behind `redisctl cloud workflow
//! quick-database` and the `cloud_quick_database` MCP tool.
//!
//! [`provision`] is pure: given a [`redis_cloud::CloudClient`] and [`QuickDatabaseParams`] it
//! creates (or reuses) a free Essentials subscription + database and writes the connection
//! string to a dotenv file (via [`super::env_delivery`]). It has no front-end concerns — no
//! clap, no exit codes, no spinner — so both the CLI and the MCP server call it directly.
//!
//! The connection string is written only to the credentials file, never returned in a form
//! that a caller is expected to print. Failures are typed ([`QuickDatabaseError`]) so front
//! ends can map them to their own contracts (CLI exit codes / MCP tool errors).
//!
//! Recognizability / safety: the subscription is always named `redisctl-<name>`. That prefix
//! is how a re-run finds and reuses its own resource, and it guarantees we never touch a
//! subscription a human created under a bare name (D6).

use std::path::PathBuf;
use std::time::Duration;

use redis_cloud::fixed::databases::{FixedDatabase, FixedDatabaseCreateRequest};
use redis_cloud::fixed::subscriptions::FixedSubscriptionCreateRequest;
use redis_cloud::{CloudClient, CloudError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::CoreError;
use crate::progress::poll_task;

const SUBSCRIPTION_PREFIX: &str = "redisctl-";
const DEFAULT_USER: &str = "default";

/// Branchable failures for the quick-database flow. Front-ends map these to their own
/// contracts (see the CLI's `StructuredError` / the MCP tool error).
#[derive(Debug, Error)]
pub enum QuickDatabaseError {
    #[error("{0}")]
    InvalidName(String),
    #[error("{0}")]
    NameConflict(String),
    #[error("{0}")]
    FreeDbExists(String),
    #[error("{0}")]
    QuotaExceeded(String),
    #[error("{0}")]
    NotAuthenticated(String),
    #[error("{0}")]
    Transient(String),
    #[error("{0}")]
    RateLimited(String),
    #[error("{0}")]
    Other(String),
}

type QResult<T> = std::result::Result<T, QuickDatabaseError>;

/// Inputs to [`provision`]. Plain data — front-ends build it from their own arg parsing.
#[derive(Debug, Clone)]
pub struct QuickDatabaseParams {
    /// Database name (also used, prefixed with `redisctl-`, for the subscription).
    pub name: String,
    /// File to write the credentials into.
    pub output_credentials: PathBuf,
    /// Primary URL variable name (e.g. `REDIS_URL`); broken-out fields derive their prefix.
    pub variable: String,
    /// Max seconds to wait for each async operation.
    pub wait_timeout: u32,
    /// Polling interval in seconds.
    pub wait_interval: u32,
}

impl QuickDatabaseParams {
    /// Params for `name` with the conventional defaults (`./.env`, `REDIS_URL`, 600s/5s).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            output_credentials: PathBuf::from("./.env"),
            variable: "REDIS_URL".to_string(),
            wait_timeout: 600,
            wait_interval: 5,
        }
    }
}

/// PRD §5.2 report — the agent contract. Serialized verbatim by both front-ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickDatabaseReport {
    /// `ok` for fresh/resumed provisioning, `reused` when an existing DB was returned.
    pub status: String,
    pub database: DatabaseSummary,
    pub credentials_written_to: String,
    pub credentials_variable: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSummary {
    pub id: String,
    pub name: String,
    pub region: Option<String>,
    pub plan: String,
    pub tls: bool,
}

/// Create or reuse a free database for `params.name` and deliver its credentials to a file.
///
/// The `client` must already be authenticated (the caller builds it from its own credential
/// source); `provision` never touches login state.
pub async fn provision(
    client: &CloudClient,
    params: &QuickDatabaseParams,
) -> QResult<QuickDatabaseReport> {
    validate_name(&params.name)?;
    let sub_name = format!("{SUBSCRIPTION_PREFIX}{}", params.name);

    // Find our subscription (idempotent re-run / crash resume).
    let (subscription_id, database_id, status) = match find_subscription(client, &sub_name).await? {
        Some(sub_id) => match first_database_id(client, sub_id).await? {
            Some(db_id) => (sub_id, db_id, "reused"),
            // Half-provisioned: subscription created but DB create never finished.
            None => (sub_id, create_database(client, sub_id, params).await?, "ok"),
        },
        None => {
            let sub_id = create_subscription(client, &sub_name, params).await?;
            (sub_id, create_database(client, sub_id, params).await?, "ok")
        }
    };

    // Fetch full details, polling until the public endpoint is populated (it can lag a few
    // seconds behind task completion on a fresh create).
    let db = fetch_ready_database(client, subscription_id, database_id, params).await?;
    deliver_and_report(&db, params, database_id, status, "free")
}

/// Write an EXISTING Essentials database's credentials to the file, without provisioning.
/// Identified by subscription + database id; `params` supplies the output path / variable /
/// wait settings (its `name` is only a report fallback, overridden by the database's name).
pub async fn existing_database_report(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    params: &QuickDatabaseParams,
) -> QResult<QuickDatabaseReport> {
    let db = fetch_ready_database(client, subscription_id, database_id, params).await?;
    deliver_and_report(&db, params, database_id, "existing", "essentials")
}

/// Shared tail: build the connection parts, write the URL + broken-out fields to the
/// credentials file, and assemble the report. Used by both `provision` and
/// `existing_database_report`.
fn deliver_and_report(
    db: &FixedDatabase,
    params: &QuickDatabaseParams,
    database_id: i32,
    status: &str,
    plan: &str,
) -> QResult<QuickDatabaseReport> {
    let parts = connection_parts(db)?;

    // Deliver the primary URL plus broken-out fields so apps can read whichever form they
    // expect. The discrete fields share a prefix derived from the URL var name.
    let prefix = params
        .variable
        .strip_suffix("_URL")
        .unwrap_or(&params.variable);
    let host_key = format!("{prefix}_HOST");
    let port_key = format!("{prefix}_PORT");
    let password_key = format!("{prefix}_PASSWORD");
    let username_key = format!("{prefix}_USERNAME");
    let tls_key = format!("{prefix}_TLS");
    let tls_val = parts.tls.to_string();
    let vars: Vec<(&str, &str)> = vec![
        (params.variable.as_str(), parts.url.as_str()),
        (host_key.as_str(), parts.host.as_str()),
        (port_key.as_str(), parts.port.as_str()),
        (password_key.as_str(), parts.password.as_str()),
        (username_key.as_str(), parts.username.as_str()),
        (tls_key.as_str(), tls_val.as_str()),
    ];
    let outcome = super::env_delivery::deliver_vars(&params.output_credentials, &vars)
        .map_err(|e| QuickDatabaseError::Other(format!("failed to write credentials file: {e}")))?;
    let _ = super::env_delivery::ensure_gitignored(&params.output_credentials);

    Ok(QuickDatabaseReport {
        status: status.to_string(),
        database: DatabaseSummary {
            id: database_id.to_string(),
            name: db.name.clone().unwrap_or_else(|| params.name.clone()),
            region: db.region.clone(),
            plan: plan.to_string(),
            tls: parts.tls,
        },
        credentials_written_to: outcome.path.display().to_string(),
        credentials_variable: outcome.variable,
    })
}

/// PRD §5.1.1 name rules: lowercase alnum + hyphens, 3–40 chars, no leading/trailing hyphen,
/// no `--`.
fn validate_name(name: &str) -> QResult<()> {
    let ok = (3..=40).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !name.contains("--");
    if !ok {
        return Err(QuickDatabaseError::InvalidName(format!(
            "invalid database name '{name}': must be 3-40 chars, lowercase letters/digits/hyphens, \
             start with a letter, end with a letter or digit, and contain no '--'"
        )));
    }
    Ok(())
}

async fn find_subscription(client: &CloudClient, sub_name: &str) -> QResult<Option<i32>> {
    let subs = client
        .fixed_subscriptions()
        .list()
        .await
        .map_err(|e| classify_cloud_error("list subscriptions", e))?;
    Ok(subs
        .subscriptions
        .unwrap_or_default()
        .into_iter()
        .find(|s| {
            s.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(sub_name))
        })
        .and_then(|s| s.id))
}

async fn first_database_id(client: &CloudClient, subscription_id: i32) -> QResult<Option<i32>> {
    let list = client
        .fixed_databases()
        .list(subscription_id, None, None)
        .await
        .map_err(|e| classify_cloud_error("list databases", e))?;
    Ok(list
        .subscription
        .map(|info| info.databases)
        .unwrap_or_default()
        .into_iter()
        .find_map(|d| d.database_id))
}

async fn create_subscription(
    client: &CloudClient,
    sub_name: &str,
    params: &QuickDatabaseParams,
) -> QResult<i32> {
    let plan_id = pick_free_plan(client).await?;
    // Free plan: no paymentMethod / paymentMethodId sent (a non-null value is rejected).
    let request = FixedSubscriptionCreateRequest::builder()
        .name(sub_name.to_string())
        .plan_id(plan_id)
        .build();
    let task = client
        .fixed_subscriptions()
        .create(&request)
        .await
        .map_err(classify_create_error)?;
    run_task(client, task.task_id, params).await
}

async fn create_database(
    client: &CloudClient,
    subscription_id: i32,
    params: &QuickDatabaseParams,
) -> QResult<i32> {
    let request = FixedDatabaseCreateRequest::builder()
        .name(params.name.clone())
        .build();
    let task = client
        .fixed_databases()
        .create(subscription_id, &request)
        .await
        .map_err(|e| classify_cloud_error("create database", e))?;
    run_task(client, task.task_id, params).await
}

/// Poll a create task to completion (via core's [`poll_task`]) and return the resource id.
/// Task-level failures are re-classified so the one-free-sub limit maps to `free_db_exists`.
async fn run_task(
    client: &CloudClient,
    task_id: Option<String>,
    params: &QuickDatabaseParams,
) -> QResult<i32> {
    let task_id = task_id.ok_or_else(|| {
        QuickDatabaseError::Other("create response did not include a task id".to_string())
    })?;
    let timeout = Duration::from_secs(params.wait_timeout as u64);
    let interval = Duration::from_secs(params.wait_interval.max(1) as u64);

    match poll_task(client, &task_id, timeout, interval, None).await {
        Ok(completed) => completed
            .response
            .and_then(|r| r.resource_id)
            .ok_or_else(|| {
                QuickDatabaseError::Other("completed task did not return a resource id".to_string())
            }),
        Err(CoreError::TaskFailed(msg)) => Err(classify_task_error(&msg)),
        Err(CoreError::TaskTimeout(_)) => Err(QuickDatabaseError::Transient(format!(
            "operation timed out after {}s; retry in a moment",
            params.wait_timeout
        ))),
        Err(CoreError::Cloud(e)) => Err(classify_cloud_error("poll task", e)),
        Err(other) => Err(QuickDatabaseError::Other(format!("task failed: {other}"))),
    }
}

/// Choose a free Essentials plan (`price == 0`). The region is server-chosen for the free
/// tier, so the first free plan is fine.
async fn pick_free_plan(client: &CloudClient) -> QResult<i32> {
    let plans = client
        .fixed_subscriptions()
        .list_plans(None, None)
        .await
        .map_err(|e| classify_cloud_error("list plans", e))?;
    plans
        .plans
        .unwrap_or_default()
        .into_iter()
        .find(|p| p.price == Some(0))
        .and_then(|p| p.id)
        .ok_or_else(|| {
            QuickDatabaseError::Other(
                "no free Essentials plan is available on this account".to_string(),
            )
        })
}

/// Read the database, polling until its `public_endpoint` is populated. A persistent absence
/// past `wait_timeout` is reported as transient (retryable) rather than an unknown error.
async fn fetch_ready_database(
    client: &CloudClient,
    subscription_id: i32,
    database_id: i32,
    params: &QuickDatabaseParams,
) -> QResult<FixedDatabase> {
    let start = std::time::Instant::now();
    let timeout = Duration::from_secs(params.wait_timeout as u64);
    let interval = Duration::from_secs(params.wait_interval.max(1) as u64);

    loop {
        let db = client
            .fixed_databases()
            .get_by_id(subscription_id, database_id)
            .await
            .map_err(|e| classify_cloud_error("read database details", e))?;
        if db.public_endpoint.as_deref().is_some_and(|s| !s.is_empty()) {
            return Ok(db);
        }
        if start.elapsed() > timeout {
            return Err(QuickDatabaseError::Transient(format!(
                "database {database_id} has no public endpoint after {}s; it may still be \
                 provisioning — retry in a moment",
                params.wait_timeout
            )));
        }
        tokio::time::sleep(interval).await;
    }
}

/// Connection details pulled from a database read, for credential-file delivery.
struct ConnParts {
    url: String,
    host: String,
    port: String,
    password: String,
    username: String,
    tls: bool,
}

/// Extract connection parts (URL + broken-out fields) from a database read. The password is
/// only present on the GET response, not the list.
fn connection_parts(db: &FixedDatabase) -> QResult<ConnParts> {
    let endpoint = db
        .public_endpoint
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            QuickDatabaseError::Other("database has no public endpoint yet".to_string())
        })?;
    let security = db.security.as_ref();
    let tls = security.and_then(|s| s.enable_tls).unwrap_or(true);
    let password = security
        .and_then(|s| s.password.as_deref())
        .ok_or_else(|| {
            QuickDatabaseError::Other(
                "database password was not returned by the API (the account may have \
             'enable-capi-return-empty-bdb-passwords' set); cannot build a connection string"
                    .to_string(),
            )
        })?;
    let (host, port) = endpoint.rsplit_once(':').unwrap_or((endpoint, ""));
    let scheme = if tls { "rediss" } else { "redis" };
    // Percent-encode the password for the URL userinfo: a raw `@`, `:`, `/`, `#`, `%`, … would
    // otherwise break authority parsing or be mis-decoded by the client. The broken-out
    // `password` field below stays raw (apps use it directly, not inside a URL).
    let encoded_password = urlencoding::encode(password);
    Ok(ConnParts {
        url: format!("{scheme}://{DEFAULT_USER}:{encoded_password}@{endpoint}"),
        host: host.to_string(),
        port: port.to_string(),
        password: password.to_string(),
        username: DEFAULT_USER.to_string(),
        tls,
    })
}

/// Classify a subscription-create failure. The public CAPI rejects free-plan creation when
/// the account has no payment method and the caller is not on the trusted User-Agent
/// allowlist — treated as "a free database already exists / is not permitted".
fn classify_create_error(err: CloudError) -> QuickDatabaseError {
    if err.to_string().to_uppercase().contains("PAYMENT") {
        QuickDatabaseError::FreeDbExists(
            "free database creation was rejected: the account either already has a free \
             database, or is not eligible for the free tier (a payment method may be \
             required). Check the Redis Cloud console."
                .to_string(),
        )
    } else {
        classify_cloud_error("create subscription", err)
    }
}

/// Classify an async task failure. The one-free-subscription limit and payment/free-plan
/// rejections come back here (as a task `ProcessingError`), not as a synchronous 4xx, so they
/// must map to the same `free_db_exists` / `quota_exceeded` codes rather than `unknown`.
fn classify_task_error(msg: &str) -> QuickDatabaseError {
    let up = msg.to_uppercase();
    if up.contains("FREE PLAN") || up.contains("FREE-PLAN") || up.contains("PAYMENT") {
        QuickDatabaseError::FreeDbExists(format!("task failed: {msg}"))
    } else if is_quota_message(msg) {
        QuickDatabaseError::QuotaExceeded(format!("task failed: {msg}"))
    } else {
        QuickDatabaseError::Other(format!("task failed: {msg}"))
    }
}

/// Map a generic CAPI error to a branchable class. 5xx / network / connection are transient;
/// 429 is rate-limited; quota messages map to quota_exceeded; everything else is `Other`.
fn classify_cloud_error(action: &str, err: CloudError) -> QuickDatabaseError {
    match err {
        CloudError::RateLimited { message } => {
            QuickDatabaseError::RateLimited(format!("{action}: {message}"))
        }
        CloudError::ServiceUnavailable { message }
        | CloudError::InternalServerError { message } => {
            QuickDatabaseError::Transient(format!("{action}: {message}"))
        }
        CloudError::Request(m) | CloudError::ConnectionError(m) => {
            QuickDatabaseError::Transient(format!("{action}: {m}"))
        }
        CloudError::ApiError { code, message } if (500..=599).contains(&code) => {
            QuickDatabaseError::Transient(format!("{action}: {message}"))
        }
        CloudError::ApiError { code: 429, message } => {
            QuickDatabaseError::RateLimited(format!("{action}: {message}"))
        }
        CloudError::BadRequest { message } if is_quota_message(&message) => {
            QuickDatabaseError::QuotaExceeded(format!("{action}: {message}"))
        }
        other => QuickDatabaseError::Other(format!("{action}: {other}")),
    }
}

fn is_quota_message(message: &str) -> bool {
    let m = message.to_uppercase();
    m.contains("QUOTA") || m.contains("LIMIT") || m.contains("EXCEED")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for n in ["abc", "my-db", "test-123", "a1b2c3"] {
            assert!(validate_name(n).is_ok(), "{n} should be valid");
        }
    }

    #[test]
    fn rejects_invalid_names() {
        for n in [
            "ab",
            "-abc",
            "abc-",
            "ab--cd",
            "Abc",
            "my_db",
            "1abc",
            &"a".repeat(41),
        ] {
            assert!(validate_name(n).is_err(), "{n} should be rejected");
        }
    }

    #[test]
    fn connection_parts_splits_host_and_port() {
        let db: FixedDatabase = serde_json::from_value(serde_json::json!({
            "publicEndpoint": "host.example.com:12000",
            "security": { "enableTls": true, "password": "s3cr3t" }
        }))
        .unwrap();
        let p = connection_parts(&db).unwrap();
        assert_eq!(p.host, "host.example.com");
        assert_eq!(p.port, "12000");
        assert_eq!(p.username, "default");
        assert!(p.tls);
        assert_eq!(p.url, "rediss://default:s3cr3t@host.example.com:12000");
    }

    #[test]
    fn password_is_percent_encoded_in_url_but_raw_in_field() {
        // A password with URL-reserved characters must not corrupt the connection string.
        let db: FixedDatabase = serde_json::from_value(serde_json::json!({
            "publicEndpoint": "h:1",
            "security": { "enableTls": true, "password": "p@ss/w#rd%20x" }
        }))
        .unwrap();
        let p = connection_parts(&db).unwrap();
        // URL userinfo is percent-encoded (@ / # % all escaped), so the authority isn't
        // corrupted and the host/port stay intact after the userinfo.
        assert_eq!(p.url, "rediss://default:p%40ss%2Fw%23rd%2520x@h:1");
        // The encoded userinfo round-trips back to the original password (what a client does).
        assert_eq!(
            urlencoding::decode("p%40ss%2Fw%23rd%2520x").unwrap(),
            "p@ss/w#rd%20x"
        );
        // The broken-out field stays raw for apps that read it directly.
        assert_eq!(p.password, "p@ss/w#rd%20x");
    }

    #[test]
    fn plain_url_when_tls_off() {
        let db: FixedDatabase = serde_json::from_value(serde_json::json!({
            "publicEndpoint": "h:1",
            "security": { "enableTls": false, "password": "p" }
        }))
        .unwrap();
        assert_eq!(connection_parts(&db).unwrap().url, "redis://default:p@h:1");
    }

    #[test]
    fn errors_when_password_missing() {
        let db: FixedDatabase = serde_json::from_value(serde_json::json!({
            "publicEndpoint": "h:1",
            "security": { "enableTls": true }
        }))
        .unwrap();
        assert!(connection_parts(&db).is_err());
    }

    #[test]
    fn free_gate_error_classifies_as_free_db_exists() {
        let err = CloudError::BadRequest {
            message: "FREE_PLAN_IS_ALLOWED_ONLY_FOR_ACCOUNTS_WITH_VALID_PAYMENT_INFO".to_string(),
        };
        assert!(matches!(
            classify_create_error(err),
            QuickDatabaseError::FreeDbExists(_)
        ));
    }

    #[test]
    fn task_free_plan_error_classifies_as_free_db_exists() {
        let e = classify_task_error("The account already has a free plan Essentials subscription.");
        assert!(matches!(e, QuickDatabaseError::FreeDbExists(_)));
    }

    #[test]
    fn report_serialization_carries_no_secrets() {
        // The report is the entire tool/CLI response; it must never structurally carry the
        // password or a connection URL (those go only to the credentials file).
        let report = QuickDatabaseReport {
            status: "ok".to_string(),
            database: DatabaseSummary {
                id: "9001".to_string(),
                name: "my-app".to_string(),
                region: Some("us-east-1".to_string()),
                plan: "free".to_string(),
                tls: true,
            },
            credentials_written_to: "./.env".to_string(),
            credentials_variable: "REDIS_URL".to_string(),
        };
        let s = serde_json::to_string(&report).unwrap();
        assert!(!s.contains("password"));
        assert!(!s.contains("rediss://"));
        assert!(!s.contains('@'));
    }

    #[test]
    fn transient_5xx_classifies_as_transient() {
        let err = CloudError::ServiceUnavailable {
            message: "try later".to_string(),
        };
        assert!(matches!(
            classify_cloud_error("x", err),
            QuickDatabaseError::Transient(_)
        ));
    }
}
