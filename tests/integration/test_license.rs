//! Integration tests for Redis Enterprise license commands
//!
//! This module demonstrates license management in Redis Enterprise.
//! Licenses control cluster features, resource limits, and expiration.
//!
//! # Supported Operations
//!
//! - **Get license** - Retrieve current license information and status
//! - **Get usage** - Check resource usage against license limits
//! - **Update license** - Install new license key from string or file
//! - **Validate license** - Test license key validity before installation
//! - **Get cluster license** - Retrieve license from cluster configuration
//!
//! # License Information
//!
//! ## License Types
//! - `trial` - Limited time trial license
//! - `subscription` - Commercial subscription license
//! - `developer` - Development/testing license
//! - `enterprise` - Full enterprise license
//!
//! ## Resource Limits
//! - **Nodes** - Maximum cluster nodes allowed
//! - **Shards** - Maximum database shards allowed
//! - **RAM** - Maximum memory usage allowed
//! - **Features** - Enabled enterprise features
//!
//! # Usage Examples
//!
//! ## Get current license information
//! ```bash
//! redis-enterprise license get
//! redis-enterprise license get --query 'expiration_date'
//! ```
//!
//! ## Check license usage
//! ```bash
//! redis-enterprise license usage
//! redis-enterprise license usage --query 'shards_used'
//! ```
//!
//! ## Update license from key
//! ```bash
//! redis-enterprise license update --key "LICENSE_KEY_STRING"
//! ```
//!
//! ## Update license from file
//! ```bash
//! redis-enterprise license update --file /path/to/license.key
//! ```
//!
//! ## Validate license before installing
//! ```bash
//! redis-enterprise license validate --key "NEW_LICENSE_KEY"
//! redis-enterprise license validate --file /path/to/new_license.key
//! ```

use redis_enterprise_cli::handlers::handle_license_command;
use redis_enterprise_cli::commands::LicenseCommands;
use serde_json::json;
use tempfile::NamedTempFile;
use std::io::Write;
use wiremock::{
    matchers::{method, path, body_json},
    Mock, ResponseTemplate,
};

mod common;
use common::setup_mock_server;

/// Test getting current license information
/// 
/// This test demonstrates:
/// - Retrieving comprehensive license details
/// - Checking license status and expiration
/// - Understanding resource limits and features
#[tokio::test]
async fn test_license_get() {
    let (mock_server, client) = setup_mock_server().await;

    let mock_response = json!({
        "license_key": "ENTERPRISE-2024-ABC123...",
        "type_": "enterprise",
        "expired": false,
        "expiration_date": "2025-12-31T23:59:59Z",
        "shards_limit": 1000,
        "node_limit": 50,
        "features": [
            "RediSearch",
            "RedisJSON",
            "RedisTimeSeries",
            "RedisGraph",
            "RedisBloom",
            "Active-Active",
            "Multi-Zone"
        ],
        "owner": "Acme Corporation"
    });

    Mock::given(method("GET"))
        .and(path("/v1/license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = LicenseCommands::Get;
    let result = handle_license_command(&client, command).await.unwrap();

    assert_eq!(result["type_"], "enterprise");
    assert_eq!(result["expired"], false);
    assert_eq!(result["shards_limit"], 1000);
    assert_eq!(result["node_limit"], 50);
    assert_eq!(result["features"].as_array().unwrap().len(), 7);
    assert_eq!(result["owner"], "Acme Corporation");
}

/// Test updating license with key string
/// 
/// This test demonstrates:
/// - Installing new license from command line argument
/// - Handling license key validation
/// - Receiving updated license information
#[tokio::test]
async fn test_license_update_with_key() {
    let (mock_server, client) = setup_mock_server().await;

    let expected_request = json!({
        "license": "NEW-ENTERPRISE-2024-XYZ789..."
    });

    let mock_response = json!({
        "license_key": "NEW-ENTERPRISE-2024-XYZ789...",
        "type_": "enterprise",
        "expired": false,
        "expiration_date": "2026-12-31T23:59:59Z",
        "shards_limit": 2000,
        "node_limit": 100,
        "features": [
            "RediSearch",
            "RedisJSON",
            "RedisTimeSeries",
            "RedisGraph",
            "RedisBloom",
            "Active-Active",
            "Multi-Zone",
            "Redis-on-Flash"
        ],
        "owner": "Acme Corporation"
    });

    Mock::given(method("PUT"))
        .and(path("/v1/license"))
        .and(body_json(&expected_request))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = LicenseCommands::Update {
        key: Some("NEW-ENTERPRISE-2024-XYZ789...".to_string()),
        file: None,
    };
    let result = handle_license_command(&client, command).await.unwrap();

    assert_eq!(result["type_"], "enterprise");
    assert_eq!(result["expired"], false);
    assert_eq!(result["shards_limit"], 2000);
    assert_eq!(result["node_limit"], 100);
    assert_eq!(result["features"].as_array().unwrap().len(), 8);
}

/// Test updating license from file
/// 
/// This test demonstrates:
/// - Reading license key from file
/// - Handling whitespace and newlines in license files
/// - File-based license management workflows
#[tokio::test]
async fn test_license_update_from_file() {
    let (mock_server, client) = setup_mock_server().await;

    // Create a temporary license file with whitespace to test trimming
    let mut temp_file = NamedTempFile::new().unwrap();
    let license_key = "FILE-ENTERPRISE-2024-DEF456...";
    temp_file.write_all(format!("  {}  \n", license_key).as_bytes()).unwrap();
    temp_file.flush().unwrap();

    let expected_request = json!({
        "license": license_key
    });

    let mock_response = json!({
        "license_key": license_key,
        "type_": "enterprise",
        "expired": false,
        "expiration_date": "2026-06-30T23:59:59Z",
        "shards_limit": 500,
        "node_limit": 25,
        "features": ["RediSearch", "RedisJSON", "Active-Active"],
        "owner": "Test Company"
    });

    Mock::given(method("PUT"))
        .and(path("/v1/license"))
        .and(body_json(&expected_request))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = LicenseCommands::Update {
        key: None,
        file: Some(temp_file.path().to_path_buf()),
    };
    let result = handle_license_command(&client, command).await.unwrap();

    assert_eq!(result["license_key"], license_key);
    assert_eq!(result["shards_limit"], 500);
    assert_eq!(result["node_limit"], 25);
    assert_eq!(result["features"].as_array().unwrap().len(), 3);
}

/// Test getting cluster license configuration
/// 
/// This test demonstrates:
/// - Retrieving license from cluster settings
/// - Alternative license information source
/// - Cluster-level license management
#[tokio::test]
async fn test_license_cluster() {
    let (mock_server, client) = setup_mock_server().await;

    let mock_response = json!({
        "license_key": "CLUSTER-CONFIG-2024-JKL012...",
        "type_": "subscription",
        "expired": false,
        "expiration_date": "2025-08-15T23:59:59Z",
        "shards_limit": 750,
        "node_limit": 30,
        "features": [
            "RediSearch",
            "RedisJSON",
            "RedisTimeSeries",
            "Active-Active"
        ],
        "owner": "Enterprise Customer"
    });

    Mock::given(method("GET"))
        .and(path("/v1/cluster/license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = LicenseCommands::Cluster;
    let result = handle_license_command(&client, command).await.unwrap();

    assert_eq!(result["type_"], "subscription");
    assert_eq!(result["expired"], false);
    assert_eq!(result["shards_limit"], 750);
    assert_eq!(result["node_limit"], 30);
    assert_eq!(result["features"].as_array().unwrap().len(), 4);
}

/// Test expired license handling
/// 
/// This test demonstrates:
/// - Handling expired license scenarios
/// - Understanding license expiration status
/// - Expired license information display
#[tokio::test]
async fn test_license_expired() {
    let (mock_server, client) = setup_mock_server().await;

    let mock_response = json!({
        "license_key": "EXPIRED-LICENSE-2023-OLD123...",
        "type_": "trial",
        "expired": true,
        "expiration_date": "2023-12-31T23:59:59Z",
        "shards_limit": 50,
        "node_limit": 5,
        "features": ["RediSearch"],
        "owner": "Trial User"
    });

    Mock::given(method("GET"))
        .and(path("/v1/license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&mock_response))
        .mount(&mock_server)
        .await;

    let command = LicenseCommands::Get;
    let result = handle_license_command(&client, command).await.unwrap();

    assert_eq!(result["expired"], true);
    assert_eq!(result["type_"], "trial");
    // Verify expiration date is in the past relative to test context
    assert_eq!(result["expiration_date"], "2023-12-31T23:59:59Z");
}

/// Test license update error scenarios
/// 
/// This test demonstrates:
/// - Handling invalid license keys
/// - Error response processing
/// - License validation failures
#[tokio::test]
async fn test_license_update_invalid_key() {
    let (mock_server, client) = setup_mock_server().await;

    Mock::given(method("PUT"))
        .and(path("/v1/license"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "Invalid license key format",
            "details": "License key does not match expected format"
        })))
        .mount(&mock_server)
        .await;

    let command = LicenseCommands::Update {
        key: Some("INVALID-LICENSE-KEY".to_string()),
        file: None,
    };
    let result = handle_license_command(&client, command).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("400"));
}

/// Test license command argument validation
/// 
/// This test demonstrates:
/// - Command line argument validation
/// - Error handling for missing arguments
/// - Mutual exclusion of key and file arguments
#[tokio::test]
async fn test_license_update_missing_args() {
    let (_, client) = setup_mock_server().await;

    let command = LicenseCommands::Update {
        key: None,
        file: None,
    };
    let result = handle_license_command(&client, command).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Must specify either"));
}

#[tokio::test]
async fn test_license_update_both_args() {
    let (_, client) = setup_mock_server().await;

    let command = LicenseCommands::Update {
        key: Some("key".to_string()),
        file: Some("/some/file".into()),
    };
    let result = handle_license_command(&client, command).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Cannot specify both"));
}

#[tokio::test]
async fn test_license_update_file_not_found() {
    let (_, client) = setup_mock_server().await;

    let command = LicenseCommands::Update {
        key: None,
        file: Some("/nonexistent/license.key".into()),
    };
    let result = handle_license_command(&client, command).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Failed to read license file"));
}
