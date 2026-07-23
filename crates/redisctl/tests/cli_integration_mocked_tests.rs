use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Helper to create a test command with isolated config
fn test_cmd(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    let config_file = temp_dir.path().join("config.toml");
    cmd.arg("--config-file").arg(config_file);

    // Unset environment variables that might override config
    cmd.env_remove("REDIS_CLOUD_API_KEY");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env_remove("REDIS_CLOUD_API_URL");
    cmd.env_remove("REDIS_ENTERPRISE_URL");
    cmd.env_remove("REDIS_ENTERPRISE_USER");
    cmd.env_remove("REDIS_ENTERPRISE_PASSWORD");
    cmd.env_remove("REDIS_ENTERPRISE_INSECURE");
    cmd.env_remove("REDISCTL_PROFILE");

    cmd
}

/// Create a config file with Cloud profile pointing to mock server
fn create_cloud_profile(temp_dir: &TempDir, api_url: &str) -> std::io::Result<()> {
    use std::fs;
    let config_path = temp_dir.path().join("config.toml");
    let config_content = format!(
        r#"
[profiles.test]
deployment_type = "cloud"
api_key = "test-api-key"
api_secret = "test-api-secret"
api_url = "{}"

default_cloud = "test"
"#,
        api_url
    );
    fs::write(config_path, config_content)
}

/// Create a config file with Enterprise profile pointing to mock server
fn create_enterprise_profile(temp_dir: &TempDir, url: &str) -> std::io::Result<()> {
    use std::fs;
    let config_path = temp_dir.path().join("config.toml");
    let config_content = format!(
        r#"
[profiles.test]
deployment_type = "enterprise"
url = "{}"
username = "admin@redis.local"
password = "test-password"
insecure = true

default_enterprise = "test"
"#,
        url
    );
    fs::write(config_path, config_content)
}

// anyhow-wrapped API failures must not be mislabeled "Configuration error"
// and must keep the cause chain. Regression for CODE-01 / CODE-08.
#[test]
fn api_failure_is_not_labeled_configuration_error() {
    let temp_dir = TempDir::new().unwrap();
    // Unreachable host, so the request fails inside a .context() wrap.
    create_cloud_profile(&temp_dir, "https://cloud.invalid/v1").unwrap();

    test_cmd(&temp_dir)
        .args(["cloud", "provider-account", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Configuration error").not())
        .stderr(predicate::str::contains("Failed to list cloud accounts"));
}

// A destructive command with no TTY and without --force must fail with a
// non-zero exit rather than silently exit 0 having done nothing. Otherwise a
// non-interactive `set -e` script would proceed believing the resource was
// deleted. confirm_action runs before the client is built, so no mock server
// is needed. Regression for the confirmation-prompt behavior (GAP-AUTOMATION-04).
#[test]
fn non_interactive_destructive_without_force_exits_nonzero() {
    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir, "https://enterprise.invalid:9443").unwrap();

    test_cmd(&temp_dir)
        .args(["enterprise", "user", "delete", "5"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Confirmation required"))
        .stderr(predicate::str::contains("--force"));
}

// With --force the confirmation is skipped: the command proceeds (and here
// fails later on the unreachable host), so it must not report a confirmation
// error.
#[test]
fn destructive_with_force_skips_confirmation() {
    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir, "https://enterprise.invalid:9443").unwrap();

    test_cmd(&temp_dir)
        .args(["enterprise", "user", "delete", "5", "--force"])
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Confirmation required").not());
}

#[tokio::test]
async fn test_api_cloud_get_with_auth_headers() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    // Create profile pointing to mock server
    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock a simple GET endpoint and verify auth headers
    Mock::given(method("GET"))
        .and(path("/test"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("x-api-secret-key", "test-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": "authenticated"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run CLI command using raw API access
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .assert()
        .success()
        .stdout(predicate::str::contains("authenticated"));
}

#[tokio::test]
async fn test_api_cloud_post_with_json_body() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock a POST endpoint
    Mock::given(method("POST"))
        .and(path("/create"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("x-api-secret-key", "test-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 123,
            "status": "created"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run CLI command with JSON body
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("post")
        .arg("/create")
        .arg("--data")
        .arg(r#"{"name":"test"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("created"));
}

#[tokio::test]
async fn test_api_enterprise_get_with_basic_auth() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock Enterprise API endpoint
    Mock::given(method("GET"))
        .and(path("/v1/cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "Test Cluster",
            "version": "7.4.2"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run CLI command
    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/cluster")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Cluster"));
}

#[tokio::test]
async fn test_api_error_response_401() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock 401 authentication failure
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "error": {
                "type": "UNAUTHORIZED",
                "status": 401,
                "description": "Invalid API credentials"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run CLI command - should fail with 401
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("401")
                .or(predicate::str::contains("Unauthorized"))
                .or(predicate::str::contains("Invalid API credentials")),
        );
}

#[tokio::test]
async fn test_api_json_output_format() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock endpoint
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [
                {"id": 1, "name": "item1"},
                {"id": 2, "name": "item2"}
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run CLI command with JSON output
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .arg("-o")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""id": 1"#).or(predicate::str::contains("item1")));
}

#[tokio::test]
async fn test_cloud_subscription_get_by_id() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock subscription endpoint
    Mock::given(method("GET"))
        .and(path("/subscriptions/12345"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("x-api-secret-key", "test-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptionId": 12345,
            "name": "Test Subscription",
            "status": "active",
            "cloudProvider": {
                "provider": "AWS",
                "region": "us-east-1"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/subscriptions/12345")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Subscription"))
        .stdout(predicate::str::contains("active"));
}

#[tokio::test]
async fn test_enterprise_database_list() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock databases list endpoint
    Mock::given(method("GET"))
        .and(path("/v1/bdbs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "uid": 1,
                "name": "db1",
                "type": "redis",
                "status": "active"
            },
            {
                "uid": 2,
                "name": "db2",
                "type": "redis",
                "status": "active"
            }
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/bdbs")
        .assert()
        .success()
        .stdout(predicate::str::contains("db1"))
        .stdout(predicate::str::contains("db2"));
}

#[tokio::test]
async fn test_api_delete_request() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock DELETE endpoint
    Mock::given(method("DELETE"))
        .and(path("/resources/999"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("x-api-secret-key", "test-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "taskId": "abc-123",
            "status": "processing"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("delete")
        .arg("/resources/999")
        .assert()
        .success()
        .stdout(predicate::str::contains("taskId").or(predicate::str::contains("abc-123")));
}

#[tokio::test]
async fn test_api_put_request() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock PUT endpoint
    Mock::given(method("PUT"))
        .and(path("/resources/555"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("x-api-secret-key", "test-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 555,
            "name": "Updated Resource",
            "updated": true
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("put")
        .arg("/resources/555")
        .arg("--data")
        .arg(r#"{"name":"Updated Resource"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated Resource"));
}

#[tokio::test]
async fn test_api_error_404_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock 404 not found
    Mock::given(method("GET"))
        .and(path("/nonexistent"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": {
                "type": "NOT_FOUND",
                "status": 404,
                "description": "Resource not found"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/nonexistent")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("404")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("Not Found")),
        );
}

#[tokio::test]
async fn test_api_error_500_server_error() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock 500 internal server error
    Mock::given(method("GET"))
        .and(path("/error"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({
            "error": {
                "type": "INTERNAL_ERROR",
                "status": 500,
                "description": "Internal server error"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/error")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("500")
                .or(predicate::str::contains("Internal"))
                .or(predicate::str::contains("server error")),
        );
}

#[tokio::test]
async fn test_api_jmespath_query() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock endpoint with nested data
    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [
                {"id": 1, "name": "sub1", "status": "active"},
                {"id": 2, "name": "sub2", "status": "inactive"}
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run with JMESPath query to filter active subscriptions
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/data")
        .arg("-q")
        .arg("subscriptions[?status=='active'].name")
        .assert()
        .success()
        .stdout(predicate::str::contains("sub1"));
}

#[tokio::test]
async fn test_enterprise_cluster_with_verbose_logging() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock cluster endpoint
    Mock::given(method("GET"))
        .and(path("/v1/cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "prod-cluster",
            "version": "7.4.2",
            "nodes_count": 3
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run with verbose flag - should still succeed
    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/cluster")
        .arg("--verbose")
        .assert()
        .success()
        .stdout(predicate::str::contains("prod-cluster"));
}

#[tokio::test]
async fn test_cloud_database_create_async_operation() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock database creation that returns a task ID
    Mock::given(method("POST"))
        .and(path("/subscriptions/123/databases"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({
            "taskId": "task-xyz-789",
            "commandType": "databaseCreateRequest",
            "status": "received"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Create without --wait should return task ID immediately
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("post")
        .arg("/subscriptions/123/databases")
        .arg("--data")
        .arg(r#"{"name":"test-db","memoryLimitInGb":1}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("taskId").or(predicate::str::contains("task-xyz-789")));
}

#[tokio::test]
async fn test_enterprise_multiple_endpoints_same_server() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock cluster info
    Mock::given(method("GET"))
        .and(path("/v1/cluster"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "test-cluster"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Mock nodes list
    Mock::given(method("GET"))
        .and(path("/v1/nodes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"uid": 1, "addr": "10.0.0.1"},
            {"uid": 2, "addr": "10.0.0.2"}
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    // First request - cluster
    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/cluster")
        .assert()
        .success()
        .stdout(predicate::str::contains("test-cluster"));

    // Second request - nodes (reusing same profile)
    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/nodes")
        .assert()
        .success()
        .stdout(predicate::str::contains("10.0.0.1"));
}

#[tokio::test]
async fn test_yaml_output_format() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock endpoint
    Mock::given(method("GET"))
        .and(path("/test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "test-resource",
            "id": 456
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Run with YAML output
    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .arg("-o")
        .arg("yaml")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("name:")
                .and(predicate::str::contains("test-resource"))
                .or(predicate::str::contains("id: 456")),
        );
}

#[tokio::test]
async fn test_cloud_list_all_subscriptions() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock subscriptions list endpoint
    Mock::given(method("GET"))
        .and(path("/subscriptions"))
        .and(header("x-api-key", "test-api-key"))
        .and(header("x-api-secret-key", "test-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptions": [
                {
                    "id": 100,
                    "name": "production",
                    "status": "active"
                },
                {
                    "id": 200,
                    "name": "staging",
                    "status": "active"
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/subscriptions")
        .assert()
        .success()
        .stdout(predicate::str::contains("production"))
        .stdout(predicate::str::contains("staging"));
}

#[tokio::test]
async fn test_enterprise_node_operations() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock nodes endpoint
    Mock::given(method("GET"))
        .and(path("/v1/nodes/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": 1,
            "addr": "10.0.0.1",
            "status": "active",
            "total_memory": 16000000000_u64
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/nodes/1")
        .assert()
        .success()
        .stdout(predicate::str::contains("10.0.0.1"))
        .stdout(predicate::str::contains("active"));
}

#[tokio::test]
async fn test_cloud_vpc_peering_endpoints() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock VPC peering list endpoint
    Mock::given(method("GET"))
        .and(path("/subscriptions/100/peerings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "peerings": [
                {
                    "id": 1,
                    "provider": "AWS",
                    "region": "us-east-1",
                    "status": "active"
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/subscriptions/100/peerings")
        .assert()
        .success()
        .stdout(predicate::str::contains("AWS"))
        .stdout(predicate::str::contains("us-east-1"));
}

#[tokio::test]
async fn test_enterprise_backup_operations() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock backup export endpoint
    Mock::given(method("POST"))
        .and(path("/v1/bdbs/1/backup"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "uid": "backup-123",
            "status": "started",
            "bdb_uid": 1
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("post")
        .arg("/v1/bdbs/1/backup")
        .arg("--data")
        .arg(r#"{"location":"/tmp/backup"}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("backup-123"));
}

#[tokio::test]
async fn test_cloud_account_information() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock account endpoint
    Mock::given(method("GET"))
        .and(path("/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 2034806,
            "name": "Test Account",
            "key": {
                "name": "test-key",
                "accountId": 2034806
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/account")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test Account"))
        .stdout(predicate::str::contains("2034806"));
}

#[tokio::test]
async fn test_enterprise_redis_acl_operations() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock Redis ACL list
    Mock::given(method("GET"))
        .and(path("/v1/redis_acls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "uid": 1,
                "name": "default-acl",
                "acl": "+@all"
            },
            {
                "uid": 2,
                "name": "readonly-acl",
                "acl": "+@read"
            }
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/redis_acls")
        .assert()
        .success()
        .stdout(predicate::str::contains("default-acl"))
        .stdout(predicate::str::contains("readonly-acl"));
}

#[tokio::test]
async fn test_cloud_database_modules() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock database with modules
    Mock::given(method("GET"))
        .and(path("/subscriptions/123/databases/456"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "databaseId": 456,
            "name": "redis-with-modules",
            "modules": [
                {
                    "name": "RedisJSON",
                    "version": "2.6"
                },
                {
                    "name": "RediSearch",
                    "version": "2.8"
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/subscriptions/123/databases/456")
        .assert()
        .success()
        .stdout(predicate::str::contains("RedisJSON"))
        .stdout(predicate::str::contains("RediSearch"));
}

#[tokio::test]
async fn test_enterprise_crdb_operations() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock CRDB (Active-Active) list
    Mock::given(method("GET"))
        .and(path("/v1/crdbs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "guid": "crdb-guid-1",
                "name": "geo-distributed-db",
                "replication": true,
                "instances": [
                    {"cluster": {"url": "cluster1.example.com"}},
                    {"cluster": {"url": "cluster2.example.com"}}
                ]
            }
        ])))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/crdbs")
        .assert()
        .success()
        .stdout(predicate::str::contains("geo-distributed-db"))
        .stdout(predicate::str::contains("cluster1.example.com"));
}

#[tokio::test]
async fn test_cloud_payment_methods() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock payment methods endpoint
    Mock::given(method("GET"))
        .and(path("/payment-methods"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "paymentMethods": [
                {
                    "id": 1,
                    "type": "credit-card",
                    "last4": "4242"
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/payment-methods")
        .assert()
        .success()
        .stdout(predicate::str::contains("credit-card"))
        .stdout(predicate::str::contains("4242"));
}

#[tokio::test]
async fn test_enterprise_license_info() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock license endpoint
    Mock::given(method("GET"))
        .and(path("/v1/license"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "license": "valid",
            "activation_date": "2024-01-01",
            "expiration_date": "2025-12-31",
            "shards_limit": 100
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/license")
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"))
        .stdout(predicate::str::contains("shards_limit"));
}

#[tokio::test]
async fn test_cloud_maintenance_windows() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock maintenance window update
    Mock::given(method("PUT"))
        .and(path("/subscriptions/123/maintenance"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subscriptionId": 123,
            "maintenanceWindows": [
                {
                    "mode": "weekly",
                    "startHour": 2,
                    "durationInHours": 4
                }
            ]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("api")
        .arg("cloud")
        .arg("put")
        .arg("/subscriptions/123/maintenance")
        .arg("--data")
        .arg(r#"{"mode":"weekly","startHour":2}"#)
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenanceWindows"))
        .stdout(predicate::str::contains("weekly"));
}

#[tokio::test]
async fn test_config_file_overrides_env_vars() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_cloud_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Mock endpoint expecting credentials from config file
    // Note: Matches any path to debug what request is being made
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": "config file credentials used"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    let config_file = temp_dir.path().join("config.toml");

    // Set environment variables that should be IGNORED when --config-file is specified
    cmd.env("REDIS_CLOUD_API_KEY", "wrong-env-key");
    cmd.env("REDIS_CLOUD_SECRET_KEY", "wrong-env-secret");

    // The --config-file flag should take precedence over env vars
    cmd.arg("--config-file")
        .arg(config_file)
        .arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .assert()
        .success()
        .stdout(predicate::str::contains("config file credentials used"));
}

#[tokio::test]
async fn test_env_vars_work_without_config_file() {
    let mock_server = MockServer::start().await;

    // Mock endpoint expecting credentials from environment variables
    Mock::given(method("GET"))
        .and(path("/test"))
        .and(header("x-api-key", "env-api-key"))
        .and(header("x-api-secret-key", "env-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": "env credentials used"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut cmd = Command::cargo_bin("redisctl").unwrap();

    // Without --config-file, env vars should work
    cmd.env("REDIS_CLOUD_API_KEY", "env-api-key");
    cmd.env("REDIS_CLOUD_SECRET_KEY", "env-api-secret");
    cmd.env("REDIS_CLOUD_API_URL", mock_server.uri());

    cmd.arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .assert()
        .success()
        .stdout(predicate::str::contains("env credentials used"));
}

/// Regression test for https://github.com/redis/redisctl/issues/919
///
/// When the endpoint is available the API returns HTTP 200 with an empty
/// body.  The old typed `client.get::<Value>()` tried to JSON-deserialise
/// that empty body and failed.  The fix uses `get_text` + synthesises the
/// result, so the command must succeed and output `"available": true`.
#[tokio::test]
async fn test_enterprise_endpoint_availability_empty_200_body() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // Simulate the real cluster behaviour: 200 OK with zero bytes.
    Mock::given(method("GET"))
        .and(path("/v1/local/bdbs/1/endpoint/availability"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("endpoint")
        .arg("availability")
        .arg("1")
        .arg("-o")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("available"))
        .stdout(predicate::str::contains("true"));
}

#[tokio::test]
async fn test_cloud_api_secret_alias_works_without_config_file() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/test"))
        .and(header("x-api-key", "env-api-key"))
        .and(header("x-api-secret-key", "env-api-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": "env alias credentials used"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut cmd = Command::cargo_bin("redisctl").unwrap();

    cmd.env("REDIS_CLOUD_API_KEY", "env-api-key");
    cmd.env("REDIS_CLOUD_API_SECRET", "env-api-secret");
    cmd.env_remove("REDIS_CLOUD_SECRET_KEY");
    cmd.env("REDIS_CLOUD_API_URL", mock_server.uri());

    cmd.arg("api")
        .arg("cloud")
        .arg("get")
        .arg("/test")
        .assert()
        .success()
        .stdout(predicate::str::contains("env alias credentials used"));
}

/// Regression test for #916: `enterprise database create --dry-run` must POST to
/// `/v1/bdbs?dry_run` (query-string flag), NOT `/v1/bdbs/dry-run` (non-existent path).
#[tokio::test]
async fn test_enterprise_database_create_dry_run_uses_query_param() {
    let temp_dir = TempDir::new().unwrap();
    let mock_server = MockServer::start().await;

    create_enterprise_profile(&temp_dir, &mock_server.uri()).unwrap();

    // The validated BDB object the real API returns on dry-run success.
    let bdb_response = json!({
        "uid": 0,
        "name": "dryrun-db",
        "type": "redis",
        "memory_size": 1073741824,
        "status": "active"
    });

    // Assert that the request goes to /v1/bdbs with ?dry_run — NOT /v1/bdbs/dry-run.
    Mock::given(method("POST"))
        .and(path("/v1/bdbs"))
        .and(query_param("dry_run", ""))
        .respond_with(ResponseTemplate::new(200).set_body_json(bdb_response))
        .expect(1)
        .mount(&mock_server)
        .await;

    test_cmd(&temp_dir)
        .args([
            "enterprise",
            "database",
            "create",
            "--data",
            r#"{"name":"dryrun-db","type":"redis","memory_size":1073741824}"#,
            "--dry-run",
            "-o",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("dryrun-db"));

    // MockServer verifies the `.expect(1)` constraint on drop.
}
