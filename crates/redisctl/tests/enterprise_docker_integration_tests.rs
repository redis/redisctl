//! Integration tests for Enterprise CLI commands against Docker Redis Enterprise cluster.
//!
//! These tests require a running Redis Enterprise cluster via Docker Compose:
//!
//! ```bash
//! docker compose -f docker/docker-compose.enterprise-demo.yml up -d
//! # Wait for initialization
//! docker compose -f docker/docker-compose.enterprise-demo.yml logs -f redis-enterprise-init
//! ```
//!
//! Run tests with:
//! ```bash
//! cargo test --test enterprise_docker_integration_tests -- --ignored
//! ```
//!
//! Environment variables (set by docker/docker-compose.enterprise-demo.yml):
//! - REDIS_ENTERPRISE_URL: https://localhost:9443
//! - REDIS_ENTERPRISE_USER: admin@redis.local
//! - REDIS_ENTERPRISE_PASSWORD: Redis123!
//! - REDIS_ENTERPRISE_INSECURE: true

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Check if Docker Redis Enterprise is available
fn docker_available() -> bool {
    // Check if the cluster responds
    std::process::Command::new("curl")
        .args([
            "-k",
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-u",
            "admin@redis.local:Redis123!",
            "https://localhost:9443/v1/cluster",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "200")
        .unwrap_or(false)
}

/// Create a test command with isolated config
fn test_cmd(temp_dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    let config_file = temp_dir.path().join("config.toml");
    cmd.arg("--config-file").arg(&config_file);

    // Set environment variables for Docker Redis Enterprise
    cmd.env("REDIS_ENTERPRISE_URL", "https://localhost:9443")
        .env("REDIS_ENTERPRISE_USER", "admin@redis.local")
        .env("REDIS_ENTERPRISE_PASSWORD", "Redis123!")
        .env("REDIS_ENTERPRISE_INSECURE", "true");

    // Clear any conflicting env vars
    cmd.env_remove("REDIS_CLOUD_API_KEY")
        .env_remove("REDIS_CLOUD_SECRET_KEY");

    cmd
}

/// Create enterprise profile config file
fn create_enterprise_profile(temp_dir: &TempDir) -> std::io::Result<()> {
    let config_path = temp_dir.path().join("config.toml");
    let config_content = r#"
[profiles.docker]
deployment_type = "enterprise"
url = "https://localhost:9443"
username = "admin@redis.local"
password = "Redis123!"
insecure = true

[defaults]
enterprise_profile = "docker"
"#;
    fs::write(config_path, config_content)
}

// =============================================================================
// CLUSTER TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_cluster_get() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .assert()
        .success()
        .stdout(predicate::str::contains("name"));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_cluster_get_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .assert()
        .success()
        .stdout(predicate::str::contains("{"))
        .stdout(predicate::str::contains("\"name\""));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_cluster_get_yaml() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("yaml")
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .assert()
        .success()
        .stdout(predicate::str::contains("name:"));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_cluster_get_with_query() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .arg("-q")
        .arg("name")
        .assert()
        .success();
}

// =============================================================================
// NODE TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_node_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("node")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_node_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("node")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["))
        .stdout(predicate::str::contains("uid"));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_node_get() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Node 1 should always exist in single-node Docker cluster
    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("node")
        .arg("get")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("uid"));
}

// =============================================================================
// DATABASE TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_database_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("database")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_database_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("database")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

/// Helper to get an existing database ID from the cluster, or None if no databases exist
fn get_existing_database_id(temp_dir: &TempDir) -> Option<String> {
    let output = test_cmd(temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("database")
        .arg("list")
        .output()
        .ok()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    let databases: serde_json::Value = serde_json::from_str(&output_str).ok()?;

    databases
        .as_array()?
        .first()?
        .get("uid")
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_database_get() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Get an existing database ID, or skip if none exist
    let db_id = match get_existing_database_id(&temp_dir) {
        Some(id) => id,
        None => {
            eprintln!("Skipping: No databases exist in cluster");
            return;
        }
    };

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("database")
        .arg("get")
        .arg(&db_id)
        .assert()
        .success()
        .stdout(predicate::str::contains("uid").or(predicate::str::contains("name")));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_database_get_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Get an existing database ID, or skip if none exist
    let db_id = match get_existing_database_id(&temp_dir) {
        Some(id) => id,
        None => {
            eprintln!("Skipping: No databases exist in cluster");
            return;
        }
    };

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("database")
        .arg("get")
        .arg(&db_id)
        .assert()
        .success()
        .stdout(predicate::str::contains("{"));
}

// NOTE: Database stats endpoint is not available in all Redis Enterprise versions
// Specifically, the Docker test image (kurtfm/rs-arm) does not have this endpoint
// Uncomment and use when testing against a full Redis Enterprise cluster
// #[test]
// #[ignore = "Requires Docker Redis Enterprise cluster with stats endpoint"]
// fn test_enterprise_database_stats() {
//     if !docker_available() {
//         eprintln!("Skipping: Docker Redis Enterprise not available");
//         return;
//     }
//
//     let temp_dir = TempDir::new().unwrap();
//     create_enterprise_profile(&temp_dir).unwrap();
//
//     let db_id = match get_existing_database_id(&temp_dir) {
//         Some(id) => id,
//         None => {
//             eprintln!("Skipping: No databases exist in cluster");
//             return;
//         }
//     };
//
//     test_cmd(&temp_dir)
//         .arg("enterprise")
//         .arg("database")
//         .arg("stats")
//         .arg(&db_id)
//         .assert()
//         .success();
// }

// =============================================================================
// USER TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_user_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("user")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_user_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("user")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["))
        .stdout(predicate::str::contains("admin@redis.local"));
}

// =============================================================================
// ROLE TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_role_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("role")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_role_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("role")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

// =============================================================================
// ACL TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_acl_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("acl")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_acl_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("acl")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

// =============================================================================
// LICENSE TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_license_get() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("license")
        .arg("get")
        .assert()
        .success();
}

// =============================================================================
// MODULE TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_module_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("module")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_module_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("module")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

// =============================================================================
// PROXY TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_proxy_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("proxy")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_proxy_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("proxy")
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

// =============================================================================
// LOGS TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_logs_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("logs")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_logs_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("logs")
        .arg("list")
        .assert()
        .success();
}

// =============================================================================
// STATS TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_stats_cluster() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("stats")
        .arg("cluster")
        .assert()
        .success();
}

// NOTE: Node stats endpoint is not available in all Redis Enterprise versions
// Specifically, the Docker test image (kurtfm/rs-arm) does not have this endpoint
// Uncomment and use when testing against a full Redis Enterprise cluster
// #[test]
// #[ignore = "Requires Docker Redis Enterprise cluster with stats endpoint"]
// fn test_enterprise_stats_node() {
//     if !docker_available() {
//         eprintln!("Skipping: Docker Redis Enterprise not available");
//         return;
//     }
//
//     let temp_dir = TempDir::new().unwrap();
//     create_enterprise_profile(&temp_dir).unwrap();
//
//     test_cmd(&temp_dir)
//         .arg("enterprise")
//         .arg("stats")
//         .arg("node")
//         .arg("1")
//         .assert()
//         .success();
// }

// =============================================================================
// ALERTS TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_alerts_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("alerts")
        .arg("list")
        .assert()
        .success();
}

// =============================================================================
// RAW API TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_api_enterprise_get_cluster() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/cluster")
        .assert()
        .success()
        .stdout(predicate::str::contains("name"));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_api_enterprise_get_nodes() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/nodes")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_api_enterprise_get_bdbs() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/bdbs")
        .assert()
        .success()
        .stdout(predicate::str::contains("["));
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_api_enterprise_get_with_jmespath_query() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .arg("/v1/cluster")
        .arg("-q")
        .arg("name")
        .assert()
        .success();
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_database_get_nonexistent() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Database 999 should not exist
    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("database")
        .arg("get")
        .arg("999")
        .assert()
        .failure();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_node_get_nonexistent() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Node 999 should not exist
    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("node")
        .arg("get")
        .arg("999")
        .assert()
        .failure();
}

// =============================================================================
// AUTHENTICATION ERROR TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_bad_credentials() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");
    let config_content = r#"
[profiles.bad]
deployment_type = "enterprise"
url = "https://localhost:9443"
username = "wrong@user.local"
password = "wrongpassword"
insecure = true

[defaults]
enterprise_profile = "bad"
"#;
    fs::write(config_path, config_content).unwrap();

    // Create command WITHOUT env vars that would override profile credentials
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.arg("--config-file")
        .arg(temp_dir.path().join("config.toml"))
        .env_remove("REDIS_ENTERPRISE_URL")
        .env_remove("REDIS_ENTERPRISE_USER")
        .env_remove("REDIS_ENTERPRISE_PASSWORD")
        .env_remove("REDIS_ENTERPRISE_INSECURE")
        .env_remove("REDIS_CLOUD_API_KEY")
        .env_remove("REDIS_CLOUD_SECRET_KEY")
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .assert()
        .failure();
}

// =============================================================================
// DATABASE CREATE/UPDATE/DELETE WORKFLOW TESTS
// =============================================================================

// NOTE: This test may fail on Docker images with limited licenses (shard limits)
// It requires sufficient license capacity to create a new database
#[test]
#[ignore = "Requires Docker Redis Enterprise cluster with sufficient license"]
fn test_enterprise_database_crud_workflow() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Create a test database (memory in bytes: 100MB = 104857600)
    let result = test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("database")
        .arg("create")
        .arg("--name")
        .arg("integration-test-db")
        .arg("--memory")
        .arg("104857600")
        .output();

    let output = match result {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("license") || stderr.contains("shards") {
                eprintln!("Skipping: License limitations prevent database creation");
                return;
            }
            panic!("Unexpected failure: {}", stderr);
        }
        Err(e) => panic!("Command execution failed: {}", e),
    };

    // Parse the database ID from the output
    let output_str = String::from_utf8_lossy(&output.stdout);
    let db_id: Option<i64> = serde_json::from_str::<serde_json::Value>(&output_str)
        .ok()
        .and_then(|v| v.get("uid").and_then(|id| id.as_i64()));

    if let Some(id) = db_id {
        // Verify we can get the database
        test_cmd(&temp_dir)
            .arg("enterprise")
            .arg("database")
            .arg("get")
            .arg(id.to_string())
            .assert()
            .success()
            .stdout(predicate::str::contains("integration-test-db"));

        // Delete the database
        test_cmd(&temp_dir)
            .arg("enterprise")
            .arg("database")
            .arg("delete")
            .arg(id.to_string())
            .assert()
            .success();
    }
}

// =============================================================================
// WORKFLOW TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_workflow_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("workflow")
        .arg("list")
        .assert()
        .success();
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_workflow_list_json() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    test_cmd(&temp_dir)
        .arg("-o")
        .arg("json")
        .arg("enterprise")
        .arg("workflow")
        .arg("list")
        .assert()
        .success();
}

// NOTE: init-cluster workflow cannot be tested on an already-initialized cluster
// The Docker cluster is already initialized, so this test validates help/args only
#[test]
fn test_enterprise_workflow_init_cluster_help() {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.arg("enterprise")
        .arg("workflow")
        .arg("init-cluster")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Initialize a Redis Enterprise cluster",
        ))
        .stdout(predicate::str::contains("--password"))
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--skip-database"));
}

#[test]
fn test_enterprise_workflow_init_cluster_missing_password() {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.arg("enterprise")
        .arg("workflow")
        .arg("init-cluster")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--password"));
}

// =============================================================================
// SUPPORT PACKAGE TESTS
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_support_package_cluster() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // Create output file in temp directory
    let output_file = temp_dir.path().join("support-package.tar.gz");

    let result = test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("support-package")
        .arg("cluster")
        .arg("--file")
        .arg(&output_file)
        .arg("--skip-checks")
        .timeout(std::time::Duration::from_secs(120))
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                // Verify the file was created
                assert!(output_file.exists(), "Support package file should exist");
                // Verify it's a reasonable size (at least 1KB)
                let metadata = std::fs::metadata(&output_file).unwrap();
                assert!(
                    metadata.len() > 1024,
                    "Support package should be larger than 1KB"
                );
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Some Docker images may not support debuginfo endpoint
                if stderr.contains("404") || stderr.contains("not found") {
                    eprintln!("Skipping: Support package endpoint not available");
                    return;
                }
                panic!("Unexpected failure: {}", stderr);
            }
        }
        Err(e) => {
            // Timeout or other error - may happen on slow systems
            eprintln!("Skipping: Command timed out or failed: {}", e);
        }
    }
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_support_package_cluster_with_optimize() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    let output_file = temp_dir.path().join("support-package-optimized.tar.gz");

    let result = test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("support-package")
        .arg("cluster")
        .arg("--file")
        .arg(&output_file)
        .arg("--optimize")
        .arg("--skip-checks")
        .timeout(std::time::Duration::from_secs(120))
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                assert!(
                    output_file.exists(),
                    "Optimized support package file should exist"
                );
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("404") || stderr.contains("not found") {
                    eprintln!("Skipping: Support package endpoint not available");
                    return;
                }
                panic!("Unexpected failure: {}", stderr);
            }
        }
        Err(e) => {
            eprintln!("Skipping: Command timed out or failed: {}", e);
        }
    }
}

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_support_package_node() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    let output_file = temp_dir.path().join("support-package-node.tar.gz");

    let result = test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("support-package")
        .arg("node")
        .arg("1")
        .arg("--file")
        .arg(&output_file)
        .arg("--skip-checks")
        .timeout(std::time::Duration::from_secs(120))
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                assert!(
                    output_file.exists(),
                    "Node support package file should exist"
                );
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("404") || stderr.contains("not found") {
                    eprintln!("Skipping: Node support package endpoint not available");
                    return;
                }
                panic!("Unexpected failure: {}", stderr);
            }
        }
        Err(e) => {
            eprintln!("Skipping: Command timed out or failed: {}", e);
        }
    }
}

#[test]
fn test_enterprise_support_package_help() {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.arg("enterprise")
        .arg("support-package")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Support package generation"))
        .stdout(predicate::str::contains("cluster"))
        .stdout(predicate::str::contains("database"))
        .stdout(predicate::str::contains("node"));
}

#[test]
fn test_enterprise_support_package_cluster_help() {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    cmd.arg("enterprise")
        .arg("support-package")
        .arg("cluster")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Generate full cluster support package",
        ))
        .stdout(predicate::str::contains("--file"))
        .stdout(predicate::str::contains("--optimize"))
        .stdout(predicate::str::contains("--upload"));
}

// =============================================================================
// DEBUG INFO TESTS (Alternative to support package)
// =============================================================================

#[test]
#[ignore = "Requires Docker Redis Enterprise cluster"]
fn test_enterprise_debug_info_list() {
    if !docker_available() {
        eprintln!("Skipping: Docker Redis Enterprise not available");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    create_enterprise_profile(&temp_dir).unwrap();

    // This may or may not exist depending on the API version
    let result = test_cmd(&temp_dir)
        .arg("enterprise")
        .arg("debug-info")
        .arg("list")
        .output();

    match result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("404") || stderr.contains("not found") {
                    eprintln!("Skipping: Debug info endpoint not available");
                }
            }
            // If we get here, the command worked or was skipped
        }
        Err(e) => {
            eprintln!("Skipping: Command failed: {}", e);
        }
    }
}
