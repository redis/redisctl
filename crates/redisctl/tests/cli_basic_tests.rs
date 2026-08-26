use assert_cmd::Command;
use predicates::prelude::*;

/// Helper to create a test command
fn redisctl() -> Command {
    Command::cargo_bin("redisctl").unwrap()
}

#[test]
fn test_help_flag() {
    redisctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Redis management CLI"))
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_help_short_flag() {
    redisctl()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn test_version_flag() {
    redisctl()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("redisctl"))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_version_short_flag() {
    redisctl()
        .arg("-V")
        .assert()
        .success()
        .stdout(predicate::str::contains("redisctl"));
}

#[test]
fn test_no_args_shows_help() {
    redisctl()
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn test_invalid_subcommand() {
    redisctl()
        .arg("invalid-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn test_profile_help() {
    redisctl()
        .arg("profile")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile management"));
}

#[test]
fn test_cloud_help() {
    redisctl()
        .arg("cloud")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cloud-specific"));
}

#[test]
fn test_enterprise_help() {
    redisctl()
        .arg("enterprise")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Enterprise-specific"));
}

#[test]
fn test_api_help() {
    redisctl()
        .arg("api")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Raw API access"));
}

#[test]
fn test_output_format_json() {
    // Test that -o json flag is accepted (doesn't test actual output)
    redisctl()
        .arg("profile")
        .arg("list")
        .arg("-o")
        .arg("json")
        .assert()
        .success();
}

#[test]
fn test_output_format_yaml() {
    redisctl()
        .arg("profile")
        .arg("list")
        .arg("-o")
        .arg("yaml")
        .assert()
        .success();
}

#[test]
fn test_output_format_table() {
    redisctl()
        .arg("profile")
        .arg("list")
        .arg("-o")
        .arg("table")
        .assert()
        .success();
}

#[test]
fn test_invalid_output_format() {
    redisctl()
        .arg("profile")
        .arg("list")
        .arg("-o")
        .arg("invalid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn test_verbose_flag() {
    redisctl()
        .arg("-v")
        .arg("profile")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_multiple_verbose_flags() {
    redisctl()
        .arg("-vvv")
        .arg("profile")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_config_file_flag() {
    redisctl()
        .arg("--config-file")
        .arg("/tmp/test-config.toml")
        .arg("profile")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_profile_flag() {
    // Just test that the flag is accepted, actual profile doesn't need to exist for this test
    redisctl()
        .arg("--profile")
        .arg("nonexistent")
        .arg("profile")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_query_flag() {
    redisctl()
        .arg("profile")
        .arg("list")
        .arg("--query")
        .arg("profiles")
        .assert()
        .success();
}

#[test]
fn test_global_flags_before_subcommand() {
    redisctl()
        .arg("-v")
        .arg("-o")
        .arg("json")
        .arg("profile")
        .arg("list")
        .assert()
        .success();
}

#[test]
fn test_profile_set_missing_required_args() {
    redisctl()
        .arg("profile")
        .arg("set")
        .arg("test-profile")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_profile_set_missing_deployment_type() {
    redisctl()
        .arg("profile")
        .arg("set")
        .arg("test-profile")
        .arg("--api-key")
        .arg("key")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--type"));
}

#[test]
fn test_profile_show_missing_name() {
    redisctl()
        .arg("profile")
        .arg("show")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_profile_remove_missing_name() {
    redisctl()
        .arg("profile")
        .arg("remove")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_database_upgrade_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("upgrade")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Upgrade database Redis version"))
        .stdout(predicate::str::contains("--version"))
        .stdout(predicate::str::contains("--preserve-roles"));
}

#[test]
fn test_payment_method_help() {
    redisctl()
        .arg("cloud")
        .arg("payment-method")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Payment method operations"))
        .stdout(predicate::str::contains("list"));
}

#[test]
fn test_payment_method_list_help() {
    redisctl()
        .arg("cloud")
        .arg("payment-method")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("List payment methods"));
}

// === SLOW-LOG COMMAND TESTS ===

#[test]
fn test_cloud_database_slow_log_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Get slow query log"))
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--offset"));
}

#[test]
fn test_cloud_database_slow_log_has_default_limit() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("default: 100"));
}

#[test]
fn test_cloud_database_slow_log_has_default_offset() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("default: 0"));
}

#[test]
fn test_cloud_fixed_database_slow_log_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Get slow query log"))
        .stdout(predicate::str::contains("--limit"))
        .stdout(predicate::str::contains("--offset"));
}

#[test]
fn test_cloud_fixed_database_slow_log_has_defaults() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("default: 100"))
        .stdout(predicate::str::contains("default: 0"));
}

#[test]
fn test_cloud_database_slow_log_offset_description() {
    // Both should use "Offset for pagination" consistently
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Offset for pagination"));
}

#[test]
fn test_cloud_fixed_database_slow_log_offset_description() {
    // Both should use "Offset for pagination" consistently
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Offset for pagination"));
}

#[test]
fn test_slow_log_descriptions_match() {
    // Ensure both commands have the same description
    let database_output = redisctl()
        .arg("cloud")
        .arg("database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let fixed_database_output = redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("slow-log")
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let database_desc = String::from_utf8_lossy(&database_output);
    let fixed_database_desc = String::from_utf8_lossy(&fixed_database_output);

    // Both should say "Get slow query log"
    assert!(database_desc.contains("Get slow query log"));
    assert!(fixed_database_desc.contains("Get slow query log"));
}

// === FILES-KEY COMMAND TESTS ===

#[test]
fn test_files_key_help() {
    redisctl()
        .arg("files-key")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Files.com API key management"))
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("remove"));
}

#[test]
fn test_files_key_set_help() {
    redisctl()
        .arg("files-key")
        .arg("set")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Store Files.com API key"));
}

#[test]
fn test_files_key_get_help() {
    redisctl()
        .arg("files-key")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Get the currently configured Files.com API key",
        ));
}

#[test]
fn test_files_key_remove_help() {
    redisctl()
        .arg("files-key")
        .arg("remove")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Remove Files.com API key"));
}

// === API COMMAND ADDITIONAL TESTS ===

#[test]
fn test_api_help_shows_examples() {
    redisctl()
        .arg("api")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"))
        .stdout(predicate::str::contains("api cloud get /subscriptions"))
        .stdout(predicate::str::contains("api enterprise get /v1/cluster"));
}

#[test]
fn test_completions_help() {
    redisctl()
        .arg("completions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate shell completions"))
        .stdout(predicate::str::contains("bash"))
        .stdout(predicate::str::contains("zsh"));
}

// Generating completions walks the entire command tree, which is what trips
// clap's duplicate-short debug assert. `completions --help` does not build the
// subtree, so it cannot catch a duplicate short flag. Generate for every shell
// so a future collision fails here instead of only in a debug build.
#[test]
fn test_completions_generate_all_shells() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        redisctl().arg("completions").arg(shell).assert().success();
    }
}

// === CLOUD SUBCOMMAND HELP TESTS ===

#[test]
fn test_cloud_account_help() {
    redisctl()
        .arg("cloud")
        .arg("account")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Account operations"));
}

#[test]
fn test_cloud_account_get_help() {
    redisctl()
        .arg("cloud")
        .arg("account")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Get account information"));
}

#[test]
fn test_cloud_subscription_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Subscription operations"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("delete"));
}

#[test]
fn test_cloud_subscription_list_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("List all subscriptions"));
}

#[test]
fn test_cloud_database_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Database operations"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("create"));
}

#[test]
fn test_cloud_database_list_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("List all databases"));
}

#[test]
fn test_cloud_user_help() {
    redisctl()
        .arg("cloud")
        .arg("user")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("User operations"));
}

#[test]
fn test_cloud_acl_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ACL"));
}

#[test]
fn test_cloud_task_help() {
    redisctl()
        .arg("cloud")
        .arg("task")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Task operations"));
}

#[test]
fn test_cloud_task_get_help() {
    redisctl()
        .arg("cloud")
        .arg("task")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Get task status"));
}

#[test]
fn test_cloud_connectivity_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Network connectivity"))
        .stdout(predicate::str::contains("vpc-peering"))
        .stdout(predicate::str::contains("psc"))
        .stdout(predicate::str::contains("tgw"));
}

#[test]
fn test_cloud_fixed_database_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fixed database operations"));
}

#[test]
fn test_cloud_fixed_subscription_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-subscription")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Fixed subscription operations"));
}

#[test]
fn test_cloud_workflow_help() {
    redisctl()
        .arg("cloud")
        .arg("workflow")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Workflow operations"));
}

#[test]
fn test_cloud_cost_report_help() {
    redisctl()
        .arg("cloud")
        .arg("cost-report")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cost report operations"))
        .stdout(predicate::str::contains("generate"))
        .stdout(predicate::str::contains("download"));
}

#[test]
fn test_cloud_cost_report_generate_help() {
    redisctl()
        .arg("cloud")
        .arg("cost-report")
        .arg("generate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate a cost report"))
        .stdout(predicate::str::contains("--start-date"))
        .stdout(predicate::str::contains("--end-date"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--subscription"))
        .stdout(predicate::str::contains("--region"))
        .stdout(predicate::str::contains("--tag"));
}

#[test]
fn test_cloud_cost_report_download_help() {
    redisctl()
        .arg("cloud")
        .arg("cost-report")
        .arg("download")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Download a generated cost report"))
        .stdout(predicate::str::contains("--output"));
}

// === ENTERPRISE SUBCOMMAND HELP TESTS ===

#[test]
fn test_enterprise_cluster_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cluster operations"));
}

#[test]
fn test_enterprise_cluster_get_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Get cluster configuration"));
}

#[test]
fn test_enterprise_database_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Database operations"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("get"))
        .stdout(predicate::str::contains("create"));
}

#[test]
fn test_enterprise_database_list_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("List all databases"));
}

#[test]
fn test_enterprise_node_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Node operations"));
}

#[test]
fn test_enterprise_user_help() {
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("User operations"));
}

#[test]
fn test_enterprise_role_help() {
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Role operations"));
}

#[test]
fn test_enterprise_acl_help() {
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ACL operations"));
}

#[test]
fn test_enterprise_license_help() {
    redisctl()
        .arg("enterprise")
        .arg("license")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("License management"));
}

#[test]
fn test_enterprise_support_package_help() {
    redisctl()
        .arg("enterprise")
        .arg("support-package")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Support package"));
}

#[test]
fn test_enterprise_support_package_cluster_help() {
    redisctl()
        .arg("enterprise")
        .arg("support-package")
        .arg("cluster")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Generate full cluster support package",
        ));
}

#[test]
fn test_enterprise_workflow_help() {
    redisctl()
        .arg("enterprise")
        .arg("workflow")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Workflow operations"));
}

#[test]
fn test_enterprise_workflow_init_cluster_help() {
    redisctl()
        .arg("enterprise")
        .arg("workflow")
        .arg("init-cluster")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Initialize a Redis Enterprise cluster",
        ));
}

#[test]
fn test_enterprise_crdb_help() {
    redisctl()
        .arg("enterprise")
        .arg("crdb")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Active-Active database"));
}

#[test]
fn test_enterprise_proxy_help() {
    redisctl()
        .arg("enterprise")
        .arg("proxy")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Proxy management"));
}

#[test]
fn test_enterprise_module_help() {
    redisctl()
        .arg("enterprise")
        .arg("module")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Module management"));
}

// Additional Enterprise command help tests

#[test]
fn test_enterprise_action_help() {
    redisctl()
        .arg("enterprise")
        .arg("action")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Action"));
}

#[test]
fn test_enterprise_alerts_help() {
    redisctl()
        .arg("enterprise")
        .arg("alerts")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Alert"));
}

#[test]
fn test_enterprise_auth_help() {
    redisctl()
        .arg("enterprise")
        .arg("auth")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Authentication"));
}

#[test]
fn test_enterprise_bdb_group_help() {
    redisctl()
        .arg("enterprise")
        .arg("bdb-group")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Database group"));
}

#[test]
fn test_enterprise_bootstrap_help() {
    redisctl()
        .arg("enterprise")
        .arg("bootstrap")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Bootstrap"));
}

#[test]
fn test_enterprise_cm_settings_help() {
    redisctl()
        .arg("enterprise")
        .arg("cm-settings")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cluster manager settings"));
}

#[test]
fn test_enterprise_crdb_task_help() {
    redisctl()
        .arg("enterprise")
        .arg("crdb-task")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CRDB task"));
}

#[test]
fn test_enterprise_debug_info_help() {
    redisctl()
        .arg("enterprise")
        .arg("debug-info")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Debug info"));
}

#[test]
fn test_enterprise_diagnostics_help() {
    redisctl()
        .arg("enterprise")
        .arg("diagnostics")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Diagnostics"));
}

#[test]
fn test_enterprise_endpoint_help() {
    redisctl()
        .arg("enterprise")
        .arg("endpoint")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Endpoint"));
}

#[test]
fn test_enterprise_job_scheduler_help() {
    redisctl()
        .arg("enterprise")
        .arg("job-scheduler")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Job scheduler"));
}

#[test]
fn test_enterprise_jsonschema_help() {
    redisctl()
        .arg("enterprise")
        .arg("jsonschema")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("JSON schema"));
}

#[test]
fn test_enterprise_ldap_help() {
    redisctl()
        .arg("enterprise")
        .arg("ldap")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("LDAP"));
}

#[test]
fn test_enterprise_ldap_mappings_help() {
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("LDAP mappings"));
}

#[test]
fn test_enterprise_local_help() {
    redisctl()
        .arg("enterprise")
        .arg("local")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Local"));
}

#[test]
fn test_enterprise_logs_help() {
    redisctl()
        .arg("enterprise")
        .arg("logs")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Log"));
}

#[test]
fn test_enterprise_migration_help() {
    redisctl()
        .arg("enterprise")
        .arg("migration")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Migration"));
}

#[test]
fn test_enterprise_ocsp_help() {
    redisctl()
        .arg("enterprise")
        .arg("ocsp")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OCSP"));
}

#[test]
fn test_enterprise_services_help() {
    redisctl()
        .arg("enterprise")
        .arg("services")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Service"));
}

#[test]
fn test_enterprise_shard_help() {
    redisctl()
        .arg("enterprise")
        .arg("shard")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Shard"));
}

#[test]
fn test_enterprise_stats_help() {
    redisctl()
        .arg("enterprise")
        .arg("stats")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Statistics"));
}

#[test]
fn test_enterprise_status_help() {
    redisctl()
        .arg("enterprise")
        .arg("status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("status"));
}

#[test]
fn test_enterprise_suffix_help() {
    redisctl()
        .arg("enterprise")
        .arg("suffix")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("DNS suffix"));
}

#[test]
fn test_enterprise_usage_report_help() {
    redisctl()
        .arg("enterprise")
        .arg("usage-report")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage report"));
}

// Cloud command help tests

#[test]
fn test_cloud_provider_account_help() {
    redisctl()
        .arg("cloud")
        .arg("provider-account")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cloud provider account"));
}

// Error case tests - Cloud database commands

#[test]
fn test_cloud_database_create_missing_subscription() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("create")
        .arg("--data")
        .arg("{}")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_cloud_database_create_missing_data() {
    // With first-class parameters, --name is required (not --data)
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("create")
        .arg("--subscription")
        .arg("123")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--name")
                .or(predicate::str::contains("required"))
                .or(predicate::str::contains("No cloud profiles configured")),
        );
}

#[test]
fn test_cloud_database_get_missing_id() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("get")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_cloud_database_delete_missing_args() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("delete")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Error case tests - Cloud subscription commands

#[test]
fn test_cloud_subscription_create_missing_data() {
    // With first-class parameters, --name is required (not --data)
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("create")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--name")
                .or(predicate::str::contains("required"))
                .or(predicate::str::contains("No cloud profiles configured")),
        );
}

#[test]
fn test_cloud_subscription_get_missing_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("get")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_cloud_subscription_delete_missing_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("delete")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Error case tests - Enterprise database commands

#[test]
fn test_enterprise_database_create_missing_data() {
    // With first-class parameters, --name is required (not --data)
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("create")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--name")
                .or(predicate::str::contains("required"))
                .or(predicate::str::contains(
                    "No enterprise profiles configured",
                )),
        );
}

#[test]
fn test_enterprise_database_get_missing_id() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("get")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_database_delete_missing_id() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("delete")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_database_update_missing_id() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .arg("--data")
        .arg("{}")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_database_update_missing_data() {
    // With first-class parameters, --data is no longer required
    // The command now requires at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise ACL create first-class params tests

#[test]
fn test_enterprise_acl_create_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--acl"))
        .stdout(predicate::str::contains("--description"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_acl_create_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_acl_create_requires_name() {
    // Without --name, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("create")
        .arg("--acl")
        .arg("+@all ~*")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--name is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

#[test]
fn test_enterprise_acl_create_requires_acl() {
    // Without --acl, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("create")
        .arg("--name")
        .arg("test-acl")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--acl is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

// Enterprise ACL update first-class params tests

#[test]
fn test_enterprise_acl_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--acl"))
        .stdout(predicate::str::contains("--description"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_acl_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_acl_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_acl_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("acl")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise node update first-class params tests

#[test]
fn test_enterprise_node_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--accept-servers"))
        .stdout(predicate::str::contains("--external-addr"))
        .stdout(predicate::str::contains("--rack-id"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_node_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_node_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_node_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise cluster update first-class params tests

#[test]
fn test_enterprise_cluster_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--email-alerts"))
        .stdout(predicate::str::contains("--rack-aware"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_cluster_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_cluster_update_requires_at_least_one_field() {
    // With no fields provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise CRDB update first-class params tests

#[test]
fn test_enterprise_crdb_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("crdb")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--memory-size"))
        .stdout(predicate::str::contains("--encryption"))
        .stdout(predicate::str::contains("--data-persistence"))
        .stdout(predicate::str::contains("--replication"))
        .stdout(predicate::str::contains("--eviction-policy"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_crdb_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("crdb")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_crdb_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("crdb")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_crdb_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("crdb")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Error case tests - API commands

#[test]
fn test_api_cloud_missing_method() {
    redisctl()
        .arg("api")
        .arg("cloud")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_api_cloud_invalid_method() {
    redisctl()
        .arg("api")
        .arg("cloud")
        .arg("invalid")
        .arg("/subscriptions")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid HTTP method"));
}

#[test]
fn test_api_enterprise_missing_path() {
    redisctl()
        .arg("api")
        .arg("enterprise")
        .arg("get")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Argument validation tests

#[test]
fn test_cloud_database_list_accepts_subscription() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("list")
        .arg("--subscription")
        .arg("123")
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn test_enterprise_node_list_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("List all nodes"));
}

#[test]
fn test_enterprise_cluster_get_accepts_query() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("get")
        .arg("-q")
        .arg("name")
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn test_wait_flags_accepted() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--wait"))
        .stdout(predicate::str::contains("--wait-timeout"))
        .stdout(predicate::str::contains("--wait-interval"));
}

// Comprehensive Cloud Database subcommand tests

#[test]
fn test_cloud_database_backup_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("backup")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("backup"));
}

#[test]
fn test_cloud_database_backup_status_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("backup-status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("backup"));
}

#[test]
fn test_cloud_database_import_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("import")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("import"));
}

#[test]
fn test_cloud_database_import_status_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("import-status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("import"));
}

#[test]
fn test_cloud_database_update_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update"));
}

#[test]
fn test_cloud_database_delete_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("delete")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Delete"));
}

#[test]
fn test_cloud_database_get_certificate_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("get-certificate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("certificate"));
}

#[test]
fn test_cloud_database_add_tag_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("add-tag")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"));
}

#[test]
fn test_cloud_database_delete_tag_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("delete-tag")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"));
}

#[test]
fn test_cloud_database_list_tags_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("list-tags")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"));
}

#[test]
fn test_cloud_database_update_tags_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-tags")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tag"));
}

#[test]
fn test_cloud_database_flush_crdb_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("flush-crdb")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("flush"));
}

#[test]
fn test_cloud_database_upgrade_redis_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("upgrade-redis")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("upgrade"));
}

#[test]
fn test_cloud_database_upgrade_status_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("upgrade-status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("upgrade"));
}

// Comprehensive Cloud Subscription subcommand tests

#[test]
fn test_cloud_subscription_create_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Create"));
}

#[test]
fn test_cloud_subscription_get_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("subscription"));
}

#[test]
fn test_cloud_subscription_update_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update"));
}

#[test]
fn test_cloud_subscription_delete_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("delete")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Delete"));
}

#[test]
fn test_cloud_subscription_get_cidr_allowlist_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("get-cidr-allowlist")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CIDR"));
}

#[test]
fn test_cloud_subscription_update_cidr_allowlist_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update-cidr-allowlist")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CIDR"));
}

#[test]
fn test_cloud_subscription_get_maintenance_windows_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("get-maintenance-windows")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenance"));
}

#[test]
fn test_cloud_subscription_update_maintenance_windows_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update-maintenance-windows")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenance"));
}

#[test]
fn test_cloud_subscription_get_pricing_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("get-pricing")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("pricing"));
}

#[test]
fn test_cloud_subscription_redis_versions_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("redis-versions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Redis"));
}

#[test]
fn test_cloud_subscription_add_aa_region_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("add-aa-region")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Active-Active"));
}

#[test]
fn test_cloud_subscription_list_aa_regions_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("list-aa-regions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Active-Active"));
}

#[test]
fn test_cloud_subscription_delete_aa_regions_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("delete-aa-regions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Active-Active"));
}

// Comprehensive Enterprise Database subcommand tests

#[test]
fn test_enterprise_database_create_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Create"));
}

#[test]
fn test_enterprise_database_get_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("database"));
}

#[test]
fn test_enterprise_database_update_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update"));
}

#[test]
fn test_enterprise_database_delete_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("delete")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Delete"));
}

#[test]
fn test_enterprise_database_backup_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("backup")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("backup"));
}

#[test]
fn test_enterprise_database_import_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("import")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("import"));
}

#[test]
fn test_enterprise_database_export_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("export")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("export"));
}

#[test]
fn test_enterprise_database_restore_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("restore")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("restore"));
}

#[test]
fn test_enterprise_database_flush_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("flush")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("flush"));
}

#[test]
fn test_enterprise_database_get_shards_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("get-shards")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("shard"));
}

#[test]
fn test_enterprise_database_update_shards_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update-shards")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("shard"));
}

#[test]
fn test_enterprise_database_get_modules_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("get-modules")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("module"));
}

#[test]
fn test_enterprise_database_update_modules_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update-modules")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("module"));
}

#[test]
fn test_enterprise_database_get_acl_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("get-acl")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ACL"));
}

#[test]
fn test_enterprise_database_update_acl_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update-acl")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ACL"));
}

#[test]
fn test_enterprise_database_client_list_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("client-list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("client"));
}

#[test]
fn test_enterprise_database_slowlog_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("slowlog")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("slow"));
}

#[test]
fn test_enterprise_database_stats_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("stats")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("stat"));
}

#[test]
fn test_enterprise_database_metrics_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("metrics")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("metric"));
}

// Comprehensive Cloud ACL subcommand tests

#[test]
fn test_cloud_acl_create_acl_user_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("create-acl-user")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_acl_list_acl_users_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("list-acl-users")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_acl_get_acl_user_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("get-acl-user")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_acl_update_acl_user_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("update-acl-user")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_acl_delete_acl_user_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("delete-acl-user")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_acl_create_role_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("create-role")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("role"));
}

#[test]
fn test_cloud_acl_list_roles_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("list-roles")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("role"));
}

#[test]
fn test_cloud_acl_update_role_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("update-role")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("role"));
}

#[test]
fn test_cloud_acl_delete_role_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("delete-role")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("role"));
}

#[test]
fn test_cloud_acl_create_redis_rule_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("create-redis-rule")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rule"));
}

#[test]
fn test_cloud_acl_list_redis_rules_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("list-redis-rules")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rule"));
}

#[test]
fn test_cloud_acl_update_redis_rule_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("update-redis-rule")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rule"));
}

#[test]
fn test_cloud_acl_delete_redis_rule_help() {
    redisctl()
        .arg("cloud")
        .arg("acl")
        .arg("delete-redis-rule")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rule"));
}

// Comprehensive Cloud User subcommand tests

#[test]
fn test_cloud_user_list_help() {
    redisctl()
        .arg("cloud")
        .arg("user")
        .arg("list")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_user_get_help() {
    redisctl()
        .arg("cloud")
        .arg("user")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_user_update_help() {
    redisctl()
        .arg("cloud")
        .arg("user")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

#[test]
fn test_cloud_user_delete_help() {
    redisctl()
        .arg("cloud")
        .arg("user")
        .arg("delete")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("user"));
}

// Comprehensive Enterprise Node subcommand tests

#[test]
fn test_enterprise_node_get_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("get")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("node"));
}

#[test]
fn test_enterprise_node_add_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("add")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("node"));
}

#[test]
fn test_enterprise_node_remove_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("remove")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("node"));
}

#[test]
fn test_enterprise_node_update_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("node"));
}

#[test]
fn test_enterprise_node_stats_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("stats")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("stat"));
}

#[test]
fn test_enterprise_node_metrics_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("metrics")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("metric"));
}

#[test]
fn test_enterprise_node_maintenance_enable_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("maintenance-enable")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenance"));
}

#[test]
fn test_enterprise_node_maintenance_disable_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("maintenance-disable")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenance"));
}

#[test]
fn test_enterprise_node_alerts_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("alerts")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("alert"));
}

#[test]
fn test_enterprise_node_get_config_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("get-config")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("config"));
}

#[test]
fn test_enterprise_node_update_config_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("update-config")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("config"));
}

#[test]
fn test_enterprise_node_drain_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("drain")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("drain"));
}

#[test]
fn test_enterprise_node_rebalance_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("rebalance")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rebalance"));
}

#[test]
fn test_enterprise_node_status_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("status"));
}

#[test]
fn test_enterprise_node_cpu_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("cpu")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("CPU"));
}

#[test]
fn test_enterprise_node_memory_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("memory")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("memory"));
}

#[test]
fn test_enterprise_node_storage_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("storage")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("storage"));
}

#[test]
fn test_enterprise_node_network_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("network")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("network"));
}

#[test]
fn test_enterprise_node_resources_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("resources")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("resource"));
}

#[test]
fn test_enterprise_node_check_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("check")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("check"));
}

#[test]
fn test_enterprise_node_restart_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("restart")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("restart"));
}

#[test]
fn test_enterprise_node_get_rack_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("get-rack")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rack"));
}

#[test]
fn test_enterprise_node_set_rack_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("set-rack")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("rack"));
}

#[test]
fn test_enterprise_node_get_role_help() {
    redisctl()
        .arg("enterprise")
        .arg("node")
        .arg("get-role")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("role"));
}

// Comprehensive Enterprise Cluster subcommand tests

#[test]
fn test_enterprise_cluster_update_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update"));
}

#[test]
fn test_enterprise_cluster_join_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("join")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("join"));
}

#[test]
fn test_enterprise_cluster_reset_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("reset")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("reset"));
}

#[test]
fn test_enterprise_cluster_recover_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("recover")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("recover"));
}

#[test]
fn test_enterprise_cluster_bootstrap_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("bootstrap")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("bootstrap"));
}

#[test]
fn test_enterprise_cluster_check_status_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("check-status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("status"));
}

#[test]
fn test_enterprise_cluster_stats_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("stats")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("stat"));
}

#[test]
fn test_enterprise_cluster_metrics_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("metrics")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("metric"));
}

#[test]
fn test_enterprise_cluster_alerts_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("alerts")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("alert"));
}

#[test]
fn test_enterprise_cluster_events_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("events")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("event"));
}

#[test]
fn test_enterprise_cluster_audit_log_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("audit-log")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("audit"));
}

#[test]
fn test_enterprise_cluster_get_license_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("get-license")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("license"));
}

#[test]
fn test_enterprise_cluster_update_license_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update-license")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("license"));
}

#[test]
fn test_enterprise_cluster_get_certificates_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("get-certificates")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("certificate"));
}

#[test]
fn test_enterprise_cluster_update_certificates_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update-certificates")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("certificate"));
}

#[test]
fn test_enterprise_cluster_rotate_certificates_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("rotate-certificates")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("certificate"));
}

#[test]
fn test_enterprise_cluster_get_policy_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("get-policy")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("policy"));
}

#[test]
fn test_enterprise_cluster_update_policy_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update-policy")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("policy"));
}

#[test]
fn test_enterprise_cluster_get_ocsp_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("get-ocsp")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OCSP"));
}

#[test]
fn test_enterprise_cluster_update_ocsp_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("update-ocsp")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("OCSP"));
}

#[test]
fn test_enterprise_cluster_maintenance_mode_enable_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("maintenance-mode-enable")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenance"));
}

#[test]
fn test_enterprise_cluster_maintenance_mode_disable_help() {
    redisctl()
        .arg("enterprise")
        .arg("cluster")
        .arg("maintenance-mode-disable")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("maintenance"));
}

#[test]
fn test_cloud_database_update_tag_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-tag")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Update a single tag value"))
        .stdout(predicate::str::contains("--key"))
        .stdout(predicate::str::contains("--value"));
}

#[test]
fn test_cloud_fixed_database_upgrade_status_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("upgrade-status")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("upgrade status"))
        .stdout(predicate::str::contains("subscription_id:database_id"));
}

#[test]
fn test_cloud_fixed_database_upgrade_redis_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("upgrade-redis")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Upgrade Redis version"))
        .stdout(predicate::str::contains("--version"));
}

#[test]
fn test_cloud_connectivity_privatelink_delete_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("privatelink")
        .arg("delete")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Delete PrivateLink"))
        .stdout(predicate::str::contains("--subscription"))
        .stdout(predicate::str::contains("--force"));
}

// === CONNECTIVITY FIRST-CLASS PARAMETERS TESTS ===

// VPC Peering first-class params tests

#[test]
fn test_vpc_peering_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("vpc-peering")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--region"))
        .stdout(predicate::str::contains("--aws-account-id"))
        .stdout(predicate::str::contains("--vpc-id"))
        .stdout(predicate::str::contains("--gcp-project-id"))
        .stdout(predicate::str::contains("--gcp-network-name"))
        .stdout(predicate::str::contains("--vpc-cidr"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_vpc_peering_create_shows_examples() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("vpc-peering")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"))
        .stdout(predicate::str::contains("--region us-east-1"))
        .stdout(predicate::str::contains("--aws-account-id"));
}

#[test]
fn test_vpc_peering_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("vpc-peering")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--vpc-cidr"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_vpc_peering_create_aa_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("vpc-peering")
        .arg("create-aa")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--source-region"))
        .stdout(predicate::str::contains("--destination-region"))
        .stdout(predicate::str::contains("--aws-account-id"))
        .stdout(predicate::str::contains("--vpc-id"))
        .stdout(predicate::str::contains("--gcp-project-id"))
        .stdout(predicate::str::contains("--gcp-network-name"))
        .stdout(predicate::str::contains("--vpc-cidr"));
}

// PrivateLink first-class params tests

#[test]
fn test_privatelink_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("privatelink")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--share-name"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_privatelink_add_principal_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("privatelink")
        .arg("add-principal")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--principal"))
        .stdout(predicate::str::contains("--type"))
        .stdout(predicate::str::contains("--alias"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_privatelink_add_principal_shows_examples() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("privatelink")
        .arg("add-principal")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"))
        .stdout(predicate::str::contains("--principal"));
}

#[test]
fn test_privatelink_remove_principal_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("privatelink")
        .arg("remove-principal")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--principal"))
        .stdout(predicate::str::contains("--type"))
        .stdout(predicate::str::contains("--data"));
}

// PSC first-class params tests

#[test]
fn test_psc_endpoint_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("psc")
        .arg("endpoint-create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--gcp-project-id"))
        .stdout(predicate::str::contains("--gcp-vpc-name"))
        .stdout(predicate::str::contains("--gcp-vpc-subnet-name"))
        .stdout(predicate::str::contains("--endpoint-connection-name"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_psc_endpoint_create_shows_examples() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("psc")
        .arg("endpoint-create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"))
        .stdout(predicate::str::contains("--gcp-project-id"));
}

#[test]
fn test_psc_endpoint_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("psc")
        .arg("endpoint-update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--psc-service-id"))
        .stdout(predicate::str::contains("--gcp-project-id"))
        .stdout(predicate::str::contains("--gcp-vpc-name"))
        .stdout(predicate::str::contains("--gcp-vpc-subnet-name"))
        .stdout(predicate::str::contains("--endpoint-connection-name"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_psc_aa_endpoint_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("psc")
        .arg("aa-endpoint-create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--gcp-project-id"))
        .stdout(predicate::str::contains("--gcp-vpc-name"))
        .stdout(predicate::str::contains("--gcp-vpc-subnet-name"))
        .stdout(predicate::str::contains("--endpoint-connection-name"))
        .stdout(predicate::str::contains("--data"));
}

// TGW first-class params tests

#[test]
fn test_tgw_attachment_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("tgw")
        .arg("attachment-create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--aws-account-id"))
        .stdout(predicate::str::contains("--tgw-id"))
        .stdout(predicate::str::contains("--cidr"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_tgw_attachment_create_shows_examples() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("tgw")
        .arg("attachment-create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"))
        .stdout(predicate::str::contains("--aws-account-id"))
        .stdout(predicate::str::contains("--tgw-id"));
}

#[test]
fn test_tgw_attachment_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("tgw")
        .arg("attachment-update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--attachment-id"))
        .stdout(predicate::str::contains("--cidr"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_tgw_aa_attachment_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("tgw")
        .arg("aa-attachment-create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--region-id"))
        .stdout(predicate::str::contains("--aws-account-id"))
        .stdout(predicate::str::contains("--tgw-id"))
        .stdout(predicate::str::contains("--cidr"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_tgw_aa_attachment_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("connectivity")
        .arg("tgw")
        .arg("aa-attachment-update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--region-id"))
        .stdout(predicate::str::contains("--attachment-id"))
        .stdout(predicate::str::contains("--cidr"))
        .stdout(predicate::str::contains("--data"));
}

// === SUBSCRIPTION FIRST-CLASS PARAMETERS TESTS ===

// Subscription create first-class params tests

#[test]
fn test_subscription_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--deployment-type"))
        .stdout(predicate::str::contains("--payment-method"))
        .stdout(predicate::str::contains("--payment-method-id"))
        .stdout(predicate::str::contains("--memory-storage"))
        .stdout(predicate::str::contains("--persistent-storage-encryption"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_subscription_create_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Examples:").or(predicate::str::contains("EXAMPLES:")));
}

// Subscription update first-class params tests

#[test]
fn test_subscription_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--payment-method"))
        .stdout(predicate::str::contains("--payment-method-id"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_subscription_update_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Subscription update-cidr-allowlist first-class params tests

#[test]
fn test_subscription_update_cidr_allowlist_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update-cidr-allowlist")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--cidr"))
        .stdout(predicate::str::contains("--security-group"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_subscription_update_cidr_allowlist_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update-cidr-allowlist")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Subscription update-maintenance-windows first-class params tests

#[test]
fn test_subscription_update_maintenance_windows_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update-maintenance-windows")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--mode"))
        .stdout(predicate::str::contains("--window"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_subscription_update_maintenance_windows_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("update-maintenance-windows")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Subscription add-aa-region first-class params tests

#[test]
fn test_subscription_add_aa_region_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("add-aa-region")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--region"))
        .stdout(predicate::str::contains("--deployment-cidr"))
        .stdout(predicate::str::contains("--vpc-id"))
        .stdout(predicate::str::contains("--resp-version"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_subscription_add_aa_region_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("add-aa-region")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Subscription delete-aa-regions first-class params tests

#[test]
fn test_subscription_delete_aa_regions_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("delete-aa-regions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--region"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--data"))
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn test_subscription_delete_aa_regions_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("subscription")
        .arg("delete-aa-regions")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// === DATABASE FIRST-CLASS PARAMETERS TESTS ===

// Database update first-class params tests

#[test]
fn test_database_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--memory"))
        .stdout(predicate::str::contains("--replication"))
        .stdout(predicate::str::contains("--data-persistence"))
        .stdout(predicate::str::contains("--eviction-policy"))
        .stdout(predicate::str::contains("--oss-cluster"))
        .stdout(predicate::str::contains("--regex-rules"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_database_update_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_database_update_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Database import first-class params tests

#[test]
fn test_database_import_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("import")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--source-type"))
        .stdout(predicate::str::contains("--import-from-uri"))
        .stdout(predicate::str::contains("--aws-access-key"))
        .stdout(predicate::str::contains("--aws-secret-key"))
        .stdout(predicate::str::contains("--gcs-client-email"))
        .stdout(predicate::str::contains("--gcs-private-key"))
        .stdout(predicate::str::contains("--azure-account-name"))
        .stdout(predicate::str::contains("--azure-account-key"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_database_import_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("import")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_database_import_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("import")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Database update-tags first-class params tests

#[test]
fn test_database_update_tags_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-tags")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--tag"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_database_update_tags_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-tags")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_database_update_tags_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-tags")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Fixed database create first-class params tests

#[test]
fn test_fixed_database_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--password"))
        .stdout(predicate::str::contains("--enable-tls"))
        .stdout(predicate::str::contains("--eviction-policy"))
        .stdout(predicate::str::contains("--replication"))
        .stdout(predicate::str::contains("--data-persistence"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_fixed_database_create_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_fixed_database_create_requires_subscription_id() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("create")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Fixed database update first-class params tests

#[test]
fn test_fixed_database_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--password"))
        .stdout(predicate::str::contains("--enable-tls"))
        .stdout(predicate::str::contains("--eviction-policy"))
        .stdout(predicate::str::contains("--replication"))
        .stdout(predicate::str::contains("--data-persistence"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_fixed_database_update_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_fixed_database_update_requires_ids() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Fixed database import first-class params tests

#[test]
fn test_fixed_database_import_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("import")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--source-type"))
        .stdout(predicate::str::contains("--import-from-uri"))
        .stdout(predicate::str::contains("--aws-access-key"))
        .stdout(predicate::str::contains("--aws-secret-key"))
        .stdout(predicate::str::contains("--gcs-client-email"))
        .stdout(predicate::str::contains("--gcs-private-key"))
        .stdout(predicate::str::contains("--azure-account-name"))
        .stdout(predicate::str::contains("--azure-account-key"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_fixed_database_import_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("import")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_fixed_database_import_requires_ids() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("import")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Fixed database update-tags first-class params tests

#[test]
fn test_fixed_database_update_tags_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("update-tags")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--tag"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_fixed_database_update_tags_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("update-tags")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_fixed_database_update_tags_requires_ids() {
    redisctl()
        .arg("cloud")
        .arg("fixed-database")
        .arg("update-tags")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Fixed subscription create first-class params tests

#[test]
fn test_fixed_subscription_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-subscription")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--plan-id"))
        .stdout(predicate::str::contains("--payment-method"))
        .stdout(predicate::str::contains("--payment-method-id"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_fixed_subscription_create_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("fixed-subscription")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

// Fixed subscription update first-class params tests

#[test]
fn test_fixed_subscription_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("fixed-subscription")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--plan-id"))
        .stdout(predicate::str::contains("--payment-method"))
        .stdout(predicate::str::contains("--payment-method-id"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_fixed_subscription_update_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("fixed-subscription")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_fixed_subscription_update_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("fixed-subscription")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Provider account create first-class params tests

#[test]
fn test_provider_account_create_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("provider-account")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--provider"))
        .stdout(predicate::str::contains("--access-key-id"))
        .stdout(predicate::str::contains("--access-secret-key"))
        .stdout(predicate::str::contains("--console-username"))
        .stdout(predicate::str::contains("--console-password"))
        .stdout(predicate::str::contains("--sign-in-login-url"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_provider_account_create_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("provider-account")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

// Provider account update first-class params tests

#[test]
fn test_provider_account_update_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("provider-account")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--access-key-id"))
        .stdout(predicate::str::contains("--access-secret-key"))
        .stdout(predicate::str::contains("--console-username"))
        .stdout(predicate::str::contains("--console-password"))
        .stdout(predicate::str::contains("--sign-in-login-url"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_provider_account_update_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("provider-account")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_provider_account_update_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("provider-account")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// Database update-aa-regions first-class params tests

#[test]
fn test_database_update_aa_regions_first_class_params_help() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-aa-regions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--memory"))
        .stdout(predicate::str::contains("--dataset-size"))
        .stdout(predicate::str::contains("--global-data-persistence"))
        .stdout(predicate::str::contains("--global-password"))
        .stdout(predicate::str::contains("--eviction-policy"))
        .stdout(predicate::str::contains("--enable-tls"))
        .stdout(predicate::str::contains("--oss-cluster"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_database_update_aa_regions_has_examples() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-aa-regions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_database_update_aa_regions_requires_id() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-aa-regions")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_database_update_aa_regions_async_flags() {
    redisctl()
        .arg("cloud")
        .arg("database")
        .arg("update-aa-regions")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--wait"))
        .stdout(predicate::str::contains("--wait-timeout"))
        .stdout(predicate::str::contains("--wait-interval"));
}

// Enterprise database update first-class params tests

#[test]
fn test_enterprise_database_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--memory"))
        .stdout(predicate::str::contains("--replication"))
        .stdout(predicate::str::contains("--persistence"))
        .stdout(predicate::str::contains("--eviction-policy"))
        .stdout(predicate::str::contains("--shards-count"))
        .stdout(predicate::str::contains("--proxy-policy"))
        .stdout(predicate::str::contains("--redis-password"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_database_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_database_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_database_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("database")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise user create first-class params tests

#[test]
fn test_enterprise_user_create_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--email"))
        .stdout(predicate::str::contains("--password"))
        .stdout(predicate::str::contains("--role"))
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--email-alerts"))
        .stdout(predicate::str::contains("--role-uid"))
        .stdout(predicate::str::contains("--auth-method"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_user_create_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_user_create_requires_email() {
    // Without --email, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("create")
        .arg("--password")
        .arg("secret")
        .arg("--role")
        .arg("admin")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--email is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

// Enterprise user update first-class params tests

#[test]
fn test_enterprise_user_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--email"))
        .stdout(predicate::str::contains("--password"))
        .stdout(predicate::str::contains("--role"))
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--email-alerts"))
        .stdout(predicate::str::contains("--role-uid"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_user_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_user_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_user_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("user")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise role create first-class params tests

#[test]
fn test_enterprise_role_create_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--management"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_role_create_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_role_create_requires_name() {
    // Without --name, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("create")
        .arg("--management")
        .arg("admin")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--name is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

// Enterprise role update first-class params tests

#[test]
fn test_enterprise_role_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--management"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_role_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_role_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_role_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("role")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// Enterprise LDAP mappings create first-class params tests

#[test]
fn test_enterprise_ldap_mappings_create_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--dn"))
        .stdout(predicate::str::contains("--role"))
        .stdout(predicate::str::contains("--email"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_ldap_mappings_create_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("create")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_ldap_mappings_create_requires_name() {
    // Without --name, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("create")
        .arg("--dn")
        .arg("CN=Engineers,OU=Groups,DC=example,DC=com")
        .arg("--role")
        .arg("db_viewer")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--name is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

#[test]
fn test_enterprise_ldap_mappings_create_requires_dn() {
    // Without --dn, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("create")
        .arg("--name")
        .arg("engineers")
        .arg("--role")
        .arg("db_viewer")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--dn is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

#[test]
fn test_enterprise_ldap_mappings_create_requires_role() {
    // Without --role, should fail requiring it
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("create")
        .arg("--name")
        .arg("engineers")
        .arg("--dn")
        .arg("CN=Engineers,OU=Groups,DC=example,DC=com")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("--role is required").or(predicate::str::contains(
                "No enterprise profiles configured",
            )),
        );
}

// Enterprise LDAP mappings update first-class params tests

#[test]
fn test_enterprise_ldap_mappings_update_first_class_params_help() {
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--dn"))
        .stdout(predicate::str::contains("--role"))
        .stdout(predicate::str::contains("--email"))
        .stdout(predicate::str::contains("--data"));
}

#[test]
fn test_enterprise_ldap_mappings_update_has_examples() {
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("update")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("EXAMPLES:"));
}

#[test]
fn test_enterprise_ldap_mappings_update_requires_id() {
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_enterprise_ldap_mappings_update_requires_at_least_one_field() {
    // With only ID provided, should fail at runtime requiring at least one update field
    // Note: In CI without profiles, may fail with profile configuration error instead
    redisctl()
        .arg("enterprise")
        .arg("ldap-mappings")
        .arg("update")
        .arg("1")
        .assert()
        .failure()
        .stderr(predicate::str::contains("At least one update field").or(
            predicate::str::contains("No enterprise profiles configured"),
        ));
}

// =====================================================================
// Platform prefix inference tests
// =====================================================================

/// Cloud-only commands should work without the `cloud` prefix and no config needed.
#[test]
fn test_prefix_inference_subscription_help() {
    // `subscription` is cloud-only — should route to `cloud subscription --help`
    redisctl()
        .args(["subscription", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Subscription"));
}

#[test]
fn test_prefix_inference_account_help() {
    redisctl()
        .args(["account", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Account"));
}

#[test]
fn test_prefix_inference_cost_report_help() {
    redisctl()
        .args(["cost-report", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cost"));
}

/// Enterprise-only commands should work without the `enterprise` prefix and no config needed.
#[test]
fn test_prefix_inference_cluster_help() {
    redisctl()
        .args(["cluster", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cluster"));
}

#[test]
fn test_prefix_inference_node_help() {
    redisctl()
        .args(["node", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Node"));
}

#[test]
fn test_prefix_inference_shard_help() {
    redisctl()
        .args(["shard", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Shard"));
}

#[test]
fn test_prefix_inference_module_help() {
    redisctl()
        .args(["module", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Module"));
}

/// Backwards compat: explicit prefixes still work.
#[test]
fn test_prefix_inference_explicit_cloud_still_works() {
    redisctl()
        .args(["cloud", "subscription", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Subscription"));
}

#[test]
fn test_prefix_inference_explicit_enterprise_still_works() {
    redisctl()
        .args(["enterprise", "cluster", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cluster"));
}

/// Cloud-only command with global flags.
#[test]
fn test_prefix_inference_cloud_with_output_flag() {
    redisctl()
        .args(["-o", "json", "subscription", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Subscription"));
}

/// Enterprise-only command with verbose flag.
#[test]
fn test_prefix_inference_enterprise_with_verbose() {
    redisctl()
        .args(["-vvv", "cluster", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cluster"));
}

/// Shared command `database` with cloud-only config → routes to cloud.
#[test]
fn test_prefix_inference_shared_database_cloud_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[profiles.mycloud]
deployment_type = "cloud"
api_key = "k"
api_secret = "s"
"#,
    )
    .unwrap();

    redisctl()
        .args([
            "--config-file",
            config_path.to_str().unwrap(),
            "database",
            "--help",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Database"));
}

/// Shared command `database` with enterprise-only config → routes to enterprise.
#[test]
fn test_prefix_inference_shared_database_enterprise_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[profiles.myent]
deployment_type = "enterprise"
url = "https://localhost:9443"
username = "admin"
password = "pw"
"#,
    )
    .unwrap();

    redisctl()
        .args([
            "--config-file",
            config_path.to_str().unwrap(),
            "database",
            "--help",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Database"));
}

/// Shared command with both cloud and enterprise profiles (no --profile) → ambiguity error.
#[test]
fn test_prefix_inference_shared_database_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[profiles.mycloud]
deployment_type = "cloud"
api_key = "k"
api_secret = "s"

[profiles.myent]
deployment_type = "enterprise"
url = "https://localhost:9443"
username = "admin"
password = "pw"
"#,
    )
    .unwrap();

    redisctl()
        .args([
            "--config-file",
            config_path.to_str().unwrap(),
            "database",
            "list",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot determine platform"));
}

/// Shared command with --profile selecting a cloud profile → routes to cloud.
#[test]
fn test_prefix_inference_shared_database_with_profile_flag() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
[profiles.mycloud]
deployment_type = "cloud"
api_key = "k"
api_secret = "s"

[profiles.myent]
deployment_type = "enterprise"
url = "https://localhost:9443"
username = "admin"
password = "pw"
"#,
    )
    .unwrap();

    redisctl()
        .args([
            "--config-file",
            config_path.to_str().unwrap(),
            "-p",
            "mycloud",
            "database",
            "--help",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Database"));
}

/// Updated help text shows the new shorthand examples.
#[test]
fn test_help_shows_prefix_inference_examples() {
    redisctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Commands infer platform from your profile",
        ));
}
