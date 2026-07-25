//! Enterprise CLI command definitions

use clap::Subcommand;

#[derive(Subcommand, Debug)]
#[command(after_long_help = "\
COMMAND GROUPS:
  Core:          database (db), cluster, node, shard, endpoint
  Access:        user, role, acl, ldap, ldap-mappings, auth
  Monitoring:    stats, status, alerts (alert), logs (log), diagnostics (diag),
                 debug-info
  Admin:         license (lic), module, proxy, services (svc), cm-settings, suffix
  Advanced:      crdb, crdb-task, bdb-group, migration, bootstrap, job-scheduler
  Troubleshoot:  support-package, ocsp, usage-report, local
  Other:         action, jsonschema, workflow

Aliases shown in parentheses (e.g. 'enterprise db list' instead of 'enterprise database list').")]
pub enum EnterpriseCommands {
    // -- Core Operations --
    /// Database operations
    #[command(subcommand, display_order = 1, visible_alias = "db")]
    Database(EnterpriseDatabaseCommands),

    /// Cluster operations
    #[command(subcommand, display_order = 2)]
    Cluster(EnterpriseClusterCommands),

    /// Node operations
    #[command(subcommand, display_order = 3)]
    Node(EnterpriseNodeCommands),

    /// Shard management operations
    #[command(subcommand, display_order = 4)]
    Shard(crate::commands::enterprise::shard::ShardCommands),

    /// Endpoint operations
    #[command(subcommand, display_order = 5)]
    Endpoint(crate::commands::enterprise::endpoint::EndpointCommands),

    // -- Access Control --
    /// User operations
    #[command(subcommand, display_order = 10)]
    User(EnterpriseUserCommands),

    /// Role operations
    #[command(subcommand, display_order = 11)]
    Role(EnterpriseRoleCommands),

    /// ACL operations
    #[command(subcommand, display_order = 12)]
    Acl(EnterpriseAclCommands),

    /// LDAP integration
    #[command(subcommand, display_order = 13)]
    Ldap(crate::commands::enterprise::ldap::LdapCommands),

    /// LDAP mappings management
    #[command(subcommand, name = "ldap-mappings", display_order = 14)]
    LdapMappings(crate::commands::enterprise::ldap::LdapMappingsCommands),

    /// Authentication and sessions
    #[command(subcommand, display_order = 15)]
    Auth(EnterpriseAuthCommands),

    // -- Monitoring --
    /// Statistics and metrics operations
    #[command(subcommand, display_order = 20)]
    Stats(EnterpriseStatsCommands),

    /// Comprehensive cluster status (cluster, nodes, databases, shards)
    #[command(display_order = 21)]
    Status {
        /// Show only cluster information
        #[arg(long)]
        cluster: bool,

        /// Show only nodes information
        #[arg(long)]
        nodes: bool,

        /// Show only databases information
        #[arg(long)]
        databases: bool,

        /// Show only shards information
        #[arg(long)]
        shards: bool,

        /// Show compact pass/fail health summary
        #[arg(long)]
        brief: bool,
    },

    /// Alert management operations
    #[command(subcommand, display_order = 22, visible_alias = "alert")]
    Alerts(EnterpriseAlertsCommands),

    /// Log operations
    #[command(subcommand, display_order = 23, visible_alias = "log")]
    Logs(crate::commands::enterprise::logs::LogsCommands),

    /// Diagnostics operations
    #[command(subcommand, display_order = 24, visible_alias = "diag")]
    Diagnostics(crate::commands::enterprise::diagnostics::DiagnosticsCommands),

    /// Debug info collection
    #[command(subcommand, display_order = 25)]
    DebugInfo(crate::commands::enterprise::debuginfo::DebugInfoCommands),

    // -- Administration --
    /// License management
    #[command(subcommand, display_order = 30, visible_alias = "lic")]
    License(EnterpriseLicenseCommands),

    /// Module management operations
    #[command(subcommand, display_order = 31)]
    Module(crate::commands::enterprise::module::ModuleCommands),

    /// Proxy management
    #[command(subcommand, display_order = 32)]
    Proxy(crate::commands::enterprise::proxy::ProxyCommands),

    /// Service management
    #[command(subcommand, display_order = 33, visible_alias = "svc")]
    Services(crate::commands::enterprise::services::ServicesCommands),

    /// Cluster manager settings
    #[command(subcommand, name = "cm-settings", display_order = 34)]
    CmSettings(crate::commands::enterprise::cm_settings::CmSettingsCommands),

    /// DNS suffix management
    #[command(subcommand, display_order = 35)]
    Suffix(crate::commands::enterprise::suffix::SuffixCommands),

    // -- Advanced --
    /// Active-Active database (CRDB) operations
    #[command(subcommand, display_order = 40)]
    Crdb(EnterpriseCrdbCommands),

    /// CRDB task operations
    #[command(subcommand, name = "crdb-task", display_order = 41)]
    CrdbTask(crate::commands::enterprise::crdb_task::CrdbTaskCommands),

    /// Database group operations
    #[command(subcommand, name = "bdb-group", display_order = 42)]
    BdbGroup(crate::commands::enterprise::bdb_group::BdbGroupCommands),

    /// Migration operations
    #[command(subcommand, display_order = 43)]
    Migration(crate::commands::enterprise::migration::MigrationCommands),

    /// Bootstrap and initialization operations
    #[command(subcommand, display_order = 44)]
    Bootstrap(crate::commands::enterprise::bootstrap::BootstrapCommands),

    /// Job scheduler operations
    #[command(subcommand, name = "job-scheduler", display_order = 45)]
    JobScheduler(crate::commands::enterprise::job_scheduler::JobSchedulerCommands),

    // -- Troubleshooting --
    /// Support package generation for troubleshooting
    #[command(subcommand, name = "support-package", display_order = 50)]
    SupportPackage(crate::commands::enterprise::support_package::SupportPackageCommands),

    /// OCSP certificate validation
    #[command(subcommand, display_order = 51)]
    Ocsp(crate::commands::enterprise::ocsp::OcspCommands),

    /// Usage report operations
    #[command(subcommand, name = "usage-report", display_order = 52)]
    UsageReport(crate::commands::enterprise::usage_report::UsageReportCommands),

    /// Local node operations
    #[command(subcommand, display_order = 53)]
    Local(crate::commands::enterprise::local::LocalCommands),

    // -- Other --
    /// Action (task) operations
    #[command(subcommand, display_order = 60)]
    Action(crate::commands::enterprise::actions::ActionCommands),

    /// JSON schema operations
    #[command(subcommand, display_order = 61)]
    Jsonschema(crate::commands::enterprise::jsonschema::JsonSchemaCommands),

    /// Workflow operations for multi-step tasks
    #[command(subcommand, display_order = 62)]
    Workflow(EnterpriseWorkflowCommands),
}

/// Cloud workflow commands
#[derive(Debug, Subcommand)]
pub enum EnterpriseWorkflowCommands {
    /// List available workflows
    List,
    /// License management workflows
    #[command(subcommand)]
    License(EnterpriseLicenseWorkflowCommands),

    /// Initialize a Redis Enterprise cluster
    #[command(name = "init-cluster")]
    InitCluster {
        /// Cluster name
        #[arg(long, default_value = "redis-cluster")]
        name: String,

        /// Admin username
        #[arg(long, default_value = "admin@redis.local")]
        username: String,

        /// Admin password (required)
        #[arg(long, env = "REDIS_ENTERPRISE_INIT_PASSWORD")]
        password: String,

        /// Skip creating a default database after initialization
        #[arg(long)]
        skip_database: bool,

        /// Name for the default database
        #[arg(long, default_value = "default-db")]
        database_name: String,

        /// Memory size for the default database in GB
        #[arg(long, default_value = "1")]
        database_memory_gb: i64,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },
}

/// Alert management commands
#[derive(Debug, Subcommand)]
pub enum EnterpriseAlertsCommands {
    /// List all alerts
    List {
        /// Filter by alert type (cluster, node, bdb)
        #[arg(long)]
        filter_type: Option<String>,
        /// Filter by severity (info, warning, error, critical)
        #[arg(long)]
        severity: Option<String>,
    },
    /// Get specific alert
    Get {
        /// Alert UID
        uid: u64,
    },
    /// Get cluster alerts
    Cluster {
        /// Specific alert name
        #[arg(long)]
        alert: Option<String>,
    },
    /// Get node alerts
    Node {
        /// Node UID (optional, defaults to all nodes)
        node_uid: Option<u64>,
        /// Specific alert name
        #[arg(long)]
        alert: Option<String>,
    },
    /// Get database alerts
    Database {
        /// Database UID (optional, defaults to all databases)
        bdb_uid: Option<u64>,
        /// Specific alert name
        #[arg(long)]
        alert: Option<String>,
    },
    /// Get alert settings
    #[command(name = "settings-get")]
    SettingsGet,
    /// Update alert settings
    #[command(
        name = "settings-update",
        after_help = "EXAMPLES:
    # Enable cluster alerts
    redisctl enterprise alerts settings-update --cluster-alerts true

    # Set alert thresholds
    redisctl enterprise alerts settings-update --memory-threshold 80 --cpu-threshold 90

    # Using JSON for full configuration
    redisctl enterprise alerts settings-update --data @settings.json"
    )]
    SettingsUpdate {
        /// Enable/disable cluster alerts
        #[arg(long)]
        cluster_alerts: Option<bool>,
        /// Enable/disable node alerts
        #[arg(long)]
        node_alerts: Option<bool>,
        /// Enable/disable database alerts
        #[arg(long)]
        bdb_alerts: Option<bool>,
        /// Memory usage threshold percentage for alerts
        #[arg(long)]
        memory_threshold: Option<u32>,
        /// CPU usage threshold percentage for alerts
        #[arg(long)]
        cpu_threshold: Option<u32>,
        /// JSON data for alert settings (optional, use '-' for stdin)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },
}

/// License management commands
#[derive(Debug, Subcommand)]
pub enum EnterpriseLicenseCommands {
    /// Get current license information
    Get,
    /// Update license with JSON data
    #[command(after_help = "EXAMPLES:
    # Update license with key
    redisctl enterprise license update --license-key ABC123...

    # Using JSON file
    redisctl enterprise license update --data @license.json")]
    Update {
        /// License key string
        #[arg(long)]
        license_key: Option<String>,
        /// License data as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },
    /// Upload license file
    Upload {
        /// Path to license file
        #[arg(long)]
        file: String,
    },
    /// Validate license
    #[command(after_help = "EXAMPLES:
    # Validate license key
    redisctl enterprise license validate --license-key ABC123...

    # Validate from JSON
    redisctl enterprise license validate --data @license.json")]
    Validate {
        /// License key string to validate
        #[arg(long)]
        license_key: Option<String>,
        /// License data as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },
    /// Check license expiration
    Expiry,
    /// List licensed features
    Features,
    /// Show license usage and limits
    Usage,
}

/// License workflow commands
#[derive(Debug, Subcommand)]
pub enum EnterpriseLicenseWorkflowCommands {
    /// Audit licenses across all configured profiles
    Audit {
        /// Only show profiles with expiring licenses (within 30 days)
        #[arg(long)]
        expiring: bool,
        /// Only show profiles with expired licenses
        #[arg(long)]
        expired: bool,
    },
    /// Update license across multiple profiles
    #[command(name = "bulk-update")]
    BulkUpdate {
        /// Profiles to update (comma-separated, or 'all' for all enterprise profiles)
        #[arg(long)]
        profiles: String,
        /// License data as JSON string or @file.json
        #[arg(long)]
        data: String,
        /// Dry run - show what would be updated without making changes
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate license compliance report
    Report {
        /// Output format for report (csv for spreadsheet export)
        #[arg(long, default_value = "table")]
        format: String,
    },
    /// Monitor license expiration and send alerts
    Monitor {
        /// Days before expiration to trigger warning
        #[arg(long, default_value = "30")]
        warning_days: i64,
        /// Exit with error code if any licenses are expiring
        #[arg(long)]
        fail_on_warning: bool,
    },
}

// Placeholder command structures - will be expanded in later PRs

#[derive(Subcommand, Debug)]
pub enum EnterpriseClusterCommands {
    /// Get cluster configuration
    Get,

    /// Update cluster configuration
    #[command(after_help = "EXAMPLES:
    # Update cluster name
    redisctl enterprise cluster update --name my-cluster

    # Enable email alerts
    redisctl enterprise cluster update --email-alerts true

    # Enable rack awareness
    redisctl enterprise cluster update --rack-aware true

    # Update multiple settings
    redisctl enterprise cluster update --email-alerts true --rack-aware true

    # Using JSON for advanced configuration
    redisctl enterprise cluster update --data '{\"cm_session_timeout_minutes\": 30}'")]
    Update {
        /// Cluster name
        #[arg(long)]
        name: Option<String>,
        /// Enable/disable email alerts
        #[arg(long)]
        email_alerts: Option<bool>,
        /// Enable/disable rack awareness
        #[arg(long)]
        rack_aware: Option<bool>,
        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Get cluster policies
    #[command(name = "get-policy")]
    GetPolicy,

    /// Update cluster policies
    #[command(
        name = "update-policy",
        after_help = "EXAMPLES:
    # Set default shards placement
    redisctl enterprise cluster update-policy --default-shards-placement dense

    # Enable rack awareness
    redisctl enterprise cluster update-policy --rack-aware true

    # Set default Redis version
    redisctl enterprise cluster update-policy --default-redis-version 7.2

    # Enable persistent node removal
    redisctl enterprise cluster update-policy --persistent-node-removal true

    # Using JSON for advanced configuration
    redisctl enterprise cluster update-policy --data @policy.json"
    )]
    UpdatePolicy {
        /// Default shards placement strategy (dense, sparse)
        #[arg(long)]
        default_shards_placement: Option<String>,
        /// Enable/disable rack awareness
        #[arg(long)]
        rack_aware: Option<bool>,
        /// Default Redis version for new databases
        #[arg(long)]
        default_redis_version: Option<String>,
        /// Enable/disable persistent node removal
        #[arg(long)]
        persistent_node_removal: Option<bool>,
        /// Policy data (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Get license information
    #[command(name = "get-license")]
    GetLicense,

    /// Update license
    #[command(name = "update-license")]
    UpdateLicense {
        /// License key file or content
        #[arg(long, value_name = "FILE|KEY")]
        license: String,
    },

    /// Bootstrap new cluster
    #[command(after_help = "EXAMPLES:
    # Bootstrap with required parameters
    redisctl enterprise cluster bootstrap --cluster-name mycluster \\
        --username admin@example.com --password mypassword

    # Using JSON for additional options
    redisctl enterprise cluster bootstrap --data @bootstrap.json")]
    Bootstrap {
        /// Cluster name (FQDN)
        #[arg(long)]
        cluster_name: Option<String>,
        /// Admin username (email)
        #[arg(long)]
        username: Option<String>,
        /// Admin password
        #[arg(long)]
        password: Option<String>,
        /// Bootstrap configuration (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
        /// Dry run - show bootstrap config without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// Join node to cluster
    #[command(after_help = "EXAMPLES:
    # Join with required parameters
    redisctl enterprise cluster join --nodes 192.168.1.100 \\
        --username admin@example.com --password mypassword

    # Join multiple nodes
    redisctl enterprise cluster join --nodes 192.168.1.100 --nodes 192.168.1.101 \\
        --username admin@example.com --password mypassword

    # Using JSON for additional options
    redisctl enterprise cluster join --data @join.json")]
    Join {
        /// Node address(es) to connect to (can be specified multiple times)
        #[arg(long)]
        nodes: Vec<String>,
        /// Admin username (email)
        #[arg(long)]
        username: Option<String>,
        /// Admin password
        #[arg(long)]
        password: Option<String>,
        /// Join configuration (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Recover cluster
    #[command(after_help = "EXAMPLES:
    # Recover with default options
    redisctl enterprise cluster recover

    # Using JSON for recovery options
    redisctl enterprise cluster recover --data @recover.json")]
    Recover {
        /// Recovery configuration (JSON file or inline)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Reset cluster (dangerous!)
    Reset {
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Get cluster statistics
    Stats,

    /// Get cluster metrics
    Metrics {
        /// Time interval (e.g., "1h", "5m")
        #[arg(long)]
        interval: Option<String>,
    },

    /// Get active alerts
    Alerts,

    /// Get cluster events
    Events {
        /// Maximum number of events to return
        #[arg(long, default_value = "100")]
        limit: Option<u32>,
    },

    /// Get audit log
    #[command(name = "audit-log")]
    AuditLog {
        /// From date (e.g., "2024-01-01")
        #[arg(long)]
        from: Option<String>,
    },

    /// Enable maintenance mode
    #[command(name = "maintenance-mode-enable")]
    MaintenanceModeEnable,

    /// Disable maintenance mode
    #[command(name = "maintenance-mode-disable")]
    MaintenanceModeDisable,

    /// Collect debug information
    #[command(name = "debug-info")]
    DebugInfo,

    /// Check cluster health status
    #[command(name = "check-status")]
    CheckStatus,

    /// Combined cluster health check (status, balance, rack-awareness)
    #[command(name = "health")]
    Health,

    /// Verify shard distribution balance across nodes
    #[command(name = "verify-balance")]
    VerifyBalance,

    /// Verify rack-aware placement of master/replica pairs
    #[command(name = "verify-rack-awareness")]
    VerifyRackAwareness,

    /// Get cluster certificates
    #[command(name = "get-certificates")]
    GetCertificates,

    /// Update certificates
    #[command(
        name = "update-certificates",
        after_help = "EXAMPLES:
    # Update proxy certificate from file
    redisctl enterprise cluster update-certificates --name proxy --certificate @proxy.pem

    # Update API certificate with key
    redisctl enterprise cluster update-certificates --name api --certificate @api.pem --key @api.key

    # Using JSON for advanced configuration
    redisctl enterprise cluster update-certificates --data @certs.json"
    )]
    UpdateCertificates {
        /// Certificate name (proxy, api, cm, metrics_exporter, syncer)
        #[arg(long)]
        name: Option<String>,
        /// Certificate content or file path (use @filename for files)
        #[arg(long)]
        certificate: Option<String>,
        /// Private key content or file path (use @filename for files)
        #[arg(long)]
        key: Option<String>,
        /// Certificate data (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Rotate certificates
    #[command(name = "rotate-certificates")]
    RotateCertificates,

    /// Get OCSP configuration
    #[command(name = "get-ocsp")]
    GetOcsp,

    /// Update OCSP configuration
    #[command(
        name = "update-ocsp",
        after_help = "EXAMPLES:
    # Enable OCSP with responder URL
    redisctl enterprise cluster update-ocsp --enabled true \\
        --responder-url http://ocsp.example.com

    # Configure OCSP timeouts
    redisctl enterprise cluster update-ocsp --response-timeout 30 \\
        --query-frequency 3600

    # Disable OCSP
    redisctl enterprise cluster update-ocsp --enabled false

    # Using JSON for advanced configuration
    redisctl enterprise cluster update-ocsp --data @ocsp.json"
    )]
    UpdateOcsp {
        /// Enable/disable OCSP
        #[arg(long)]
        enabled: Option<bool>,
        /// OCSP responder URL
        #[arg(long)]
        responder_url: Option<String>,
        /// Response timeout in seconds
        #[arg(long)]
        response_timeout: Option<u32>,
        /// Query frequency in seconds
        #[arg(long)]
        query_frequency: Option<u32>,
        /// Recovery frequency in seconds
        #[arg(long)]
        recovery_frequency: Option<u32>,
        /// Maximum recovery attempts
        #[arg(long)]
        recovery_max_tries: Option<u32>,
        /// OCSP configuration data (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseDatabaseCommands {
    /// List all databases
    List,

    /// Get database details
    Get {
        /// Database ID
        id: u32,
    },

    /// Create a new database
    #[command(after_help = "EXAMPLES:
    # Simple database - just name and size
    redisctl enterprise database create --name mydb --memory 1073741824

    # With replication for high availability
    redisctl enterprise database create --name prod-db --memory 2147483648 --replication

    # With persistence and eviction policy
    redisctl enterprise database create --name cache-db --memory 536870912 \\
      --persistence aof --eviction-policy volatile-lru

    # With sharding for horizontal scaling
    redisctl enterprise database create --name large-db --memory 10737418240 \\
      --sharding --shards-count 4

    # With specific port
    redisctl enterprise database create --name service-db --memory 1073741824 --port 12000

    # With modules (auto-resolves name to module)
    redisctl enterprise database create --name search-db --memory 1073741824 \\
      --module search --module ReJSON

    # Complete configuration from file
    redisctl enterprise database create --data @database.json

    # Dry run to preview without creating
    redisctl enterprise database create --name test-db --memory 1073741824 --dry-run

NOTE: Memory size is in bytes. Common values:
      - 1 GB = 1073741824 bytes
      - 2 GB = 2147483648 bytes
      - 5 GB = 5368709120 bytes
      First-class parameters override values in --data when both are provided.")]
    Create {
        /// Database name (required unless using --data)
        #[arg(long)]
        name: Option<String>,

        /// Memory size in bytes (e.g., 1073741824 for 1GB)
        #[arg(long)]
        memory: Option<u64>,

        /// TCP port (10000-19999, auto-assigned if not specified)
        #[arg(long)]
        port: Option<u16>,

        /// Enable replication for high availability
        #[arg(long)]
        replication: bool,

        /// Data persistence: aof, snapshot, or aof-and-snapshot
        #[arg(long)]
        persistence: Option<String>,

        /// Data eviction policy when memory limit reached
        #[arg(long)]
        eviction_policy: Option<String>,

        /// Enable sharding for horizontal scaling
        #[arg(long)]
        sharding: bool,

        /// Number of shards (requires --sharding)
        #[arg(long)]
        shards_count: Option<u32>,

        /// Proxy policy: single, all-master-shards, or all-nodes
        #[arg(long)]
        proxy_policy: Option<String>,

        /// Enable CRDB (Active-Active)
        #[arg(long)]
        crdb: bool,

        /// Redis password for authentication
        #[arg(long)]
        redis_password: Option<String>,

        /// Module to enable (can be repeated). Format: name[@version][:args]
        /// Use 'enterprise module list' to see available modules.
        /// Examples: --module search  --module search@2.10.27  --module search@2.10.27:PARTITIONS=AUTO
        #[arg(long = "module", value_name = "NAME[@VERSION][:ARGS]")]
        modules: Vec<String>,

        /// Advanced: Full database configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Perform a dry run without creating the database
        #[arg(long)]
        dry_run: bool,
    },

    /// Update database configuration
    #[command(after_help = "EXAMPLES:
    # Update memory size
    redisctl enterprise database update 1 --memory 2147483648

    # Enable replication
    redisctl enterprise database update 1 --replication true

    # Update persistence and eviction policy
    redisctl enterprise database update 1 --persistence aof --eviction-policy volatile-lru

    # Update sharding configuration
    redisctl enterprise database update 1 --shards-count 8

    # Update proxy policy
    redisctl enterprise database update 1 --proxy-policy all-master-shards

    # Update Redis password
    redisctl enterprise database update 1 --redis-password newsecret

    # Advanced: Full update via JSON file
    redisctl enterprise database update 1 --data @updates.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Update {
        /// Database ID
        id: u32,

        /// New database name
        #[arg(long)]
        name: Option<String>,

        /// Memory size in bytes (e.g., 1073741824 for 1GB)
        #[arg(long)]
        memory: Option<u64>,

        /// Enable/disable replication
        #[arg(long)]
        replication: Option<bool>,

        /// Data persistence: disabled, aof, snapshot, or aof-and-snapshot
        #[arg(long)]
        persistence: Option<String>,

        /// Data eviction policy when memory limit reached
        #[arg(long)]
        eviction_policy: Option<String>,

        /// Number of shards
        #[arg(long)]
        shards_count: Option<u32>,

        /// Proxy policy: single, all-master-shards, or all-nodes
        #[arg(long)]
        proxy_policy: Option<String>,

        /// Redis password for authentication
        #[arg(long)]
        redis_password: Option<String>,

        /// Advanced: Full update configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,

        /// Dry run - show what would be changed without applying
        #[arg(long)]
        dry_run: bool,
    },

    /// Delete a database
    Delete {
        /// Database ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
    },

    /// Watch database status changes in real-time
    Watch {
        /// Database ID
        id: u32,
        /// Poll interval in seconds
        #[arg(long, default_value = "5")]
        poll_interval: u64,
    },

    /// Export database to external storage
    #[command(after_help = "EXAMPLES:
    # Export to S3
    redisctl enterprise database export 1 --location s3://bucket/backup.rdb \\
        --aws-access-key AKIAIOSFODNN7EXAMPLE --aws-secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

    # Export to FTP
    redisctl enterprise database export 1 --location ftp://user:pass@ftp.example.com/backup.rdb

    # Export to SFTP
    redisctl enterprise database export 1 --location sftp://user@sftp.example.com/backup.rdb

    # Using JSON for advanced configuration
    redisctl enterprise database export 1 --data @export.json")]
    Export {
        /// Database ID
        id: u32,

        /// Export location (S3, FTP, SFTP, or local path)
        #[arg(long)]
        location: Option<String>,

        /// AWS access key for S3 exports
        #[arg(long)]
        aws_access_key: Option<String>,

        /// AWS secret key for S3 exports
        #[arg(long)]
        aws_secret_key: Option<String>,

        /// Export configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Import data to database from external storage
    #[command(after_help = "EXAMPLES:
    # Import from S3
    redisctl enterprise database import 1 --location s3://bucket/backup.rdb \\
        --aws-access-key AKIAIOSFODNN7EXAMPLE --aws-secret-key wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

    # Import from FTP with flush
    redisctl enterprise database import 1 --location ftp://user:pass@ftp.example.com/backup.rdb --flush

    # Import from HTTP/HTTPS
    redisctl enterprise database import 1 --location https://example.com/backup.rdb

    # Import and wait for completion
    redisctl enterprise database import 1 --location https://example.com/backup.rdb --wait

    # Using JSON for advanced configuration
    redisctl enterprise database import 1 --data @import.json")]
    Import {
        /// Database ID
        id: u32,

        /// Import location (S3, FTP, SFTP, HTTP, or local path)
        #[arg(long)]
        location: Option<String>,

        /// AWS access key for S3 imports
        #[arg(long)]
        aws_access_key: Option<String>,

        /// AWS secret key for S3 imports
        #[arg(long)]
        aws_secret_key: Option<String>,

        /// Flush database before import
        #[arg(long)]
        flush: bool,

        /// Import configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,

        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Trigger database backup
    Backup {
        /// Database ID
        id: u32,
        /// Async operation options
        #[command(flatten)]
        async_ops: crate::commands::cloud::async_utils::AsyncOperationArgs,
    },

    /// Restore database from backup
    #[command(after_help = "EXAMPLES:
    # Restore from latest backup
    redisctl enterprise database restore 1

    # Restore from specific backup
    redisctl enterprise database restore 1 --backup-uid backup-12345

    # Using JSON for advanced configuration
    redisctl enterprise database restore 1 --data @restore.json")]
    Restore {
        /// Database ID
        id: u32,

        /// Specific backup UID to restore from (uses latest if not specified)
        #[arg(long)]
        backup_uid: Option<String>,

        /// Restore configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Flush database data
    Flush {
        /// Database ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Get database shards info
    GetShards {
        /// Database ID
        id: u32,
    },

    /// Update sharding configuration
    #[command(after_help = "EXAMPLES:
    # Update shard count
    redisctl enterprise database update-shards 1 --shards-count 4

    # Update shards placement policy
    redisctl enterprise database update-shards 1 --shards-placement sparse

    # Using JSON for advanced configuration
    redisctl enterprise database update-shards 1 --data @shards.json")]
    UpdateShards {
        /// Database ID
        id: u32,

        /// Number of shards
        #[arg(long)]
        shards_count: Option<u32>,

        /// Shards placement policy (dense, sparse)
        #[arg(long)]
        shards_placement: Option<String>,

        /// Shards configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Get enabled modules
    GetModules {
        /// Database ID
        id: u32,
    },

    /// Update modules configuration
    #[command(after_help = "EXAMPLES:
    # Add a module
    redisctl enterprise database update-modules 1 --add-module search

    # Add module with arguments
    redisctl enterprise database update-modules 1 --add-module 'search:MAXSEARCHRESULTS 100'

    # Remove a module
    redisctl enterprise database update-modules 1 --remove-module ReJSON

    # Using JSON for advanced configuration
    redisctl enterprise database update-modules 1 --data @modules.json")]
    UpdateModules {
        /// Database ID
        id: u32,

        /// Module to add (format: module_name or module_name:args)
        #[arg(long = "add-module")]
        add_modules: Vec<String>,

        /// Module to remove (by name)
        #[arg(long = "remove-module")]
        remove_modules: Vec<String>,

        /// Modules configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Upgrade database Redis version
    Upgrade {
        /// Database ID
        id: u32,
        /// Target Redis version (defaults to latest)
        #[arg(long)]
        version: Option<String>,
        /// Preserve master/replica roles (requires extra failover)
        #[arg(long)]
        preserve_roles: bool,
        /// Restart shards even if no version change
        #[arg(long)]
        force_restart: bool,
        /// Allow data loss in non-replicated, non-persistent databases
        #[arg(long)]
        may_discard_data: bool,
        /// Force data discard even if replicated/persistent
        #[arg(long)]
        force_discard: bool,
        /// Keep current CRDT protocol version
        #[arg(long)]
        keep_crdt_protocol_version: bool,
        /// Maximum parallel shard upgrades
        #[arg(long)]
        parallel_shards_upgrade: Option<u32>,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Get ACL configuration
    GetAcl {
        /// Database ID
        id: u32,
    },

    /// Update ACL configuration
    #[command(after_help = "EXAMPLES:
    # Set default user enabled
    redisctl enterprise database update-acl 1 --default-user true

    # Set ACL by UID
    redisctl enterprise database update-acl 1 --acl-uid 5

    # Using JSON for advanced configuration
    redisctl enterprise database update-acl 1 --data @acl.json")]
    UpdateAcl {
        /// Database ID
        id: u32,

        /// ACL UID to assign to this database
        #[arg(long)]
        acl_uid: Option<u32>,

        /// Enable/disable default user
        #[arg(long)]
        default_user: Option<bool>,

        /// ACL configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Get database statistics
    Stats {
        /// Database ID
        id: u32,
    },

    /// Get database metrics
    Metrics {
        /// Database ID
        id: u32,
        /// Time interval (e.g., "1h", "24h")
        #[arg(long)]
        interval: Option<String>,
    },

    /// Get slow query log
    Slowlog {
        /// Database ID
        id: u32,
        /// Limit number of entries
        #[arg(long)]
        limit: Option<u32>,
    },

    /// Get connected clients
    ClientList {
        /// Database ID
        id: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseNodeCommands {
    /// List all nodes in cluster
    List,

    /// Get node details
    Get {
        /// Node ID
        id: u32,
    },

    /// Add node to cluster
    #[command(after_help = "EXAMPLES:
    # Add node with IP address
    redisctl enterprise node add --address 192.168.1.100

    # Add node with credentials
    redisctl enterprise node add --address 192.168.1.100 \\
        --username admin@example.com --password secret

    # Using JSON for advanced configuration
    redisctl enterprise node add --data @node.json")]
    Add {
        /// Node IP address or hostname
        #[arg(long)]
        address: Option<String>,
        /// Admin username
        #[arg(long)]
        username: Option<String>,
        /// Admin password
        #[arg(long)]
        password: Option<String>,
        /// Node configuration (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Remove node from cluster
    Remove {
        /// Node ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Update node configuration
    #[command(after_help = "EXAMPLES:
    # Update node to accept new shards
    redisctl enterprise node update 1 --accept-servers true

    # Set node's external address
    redisctl enterprise node update 1 --external-addr 10.0.0.1

    # Set rack ID for rack awareness
    redisctl enterprise node update 1 --rack-id rack1

    # Update multiple settings
    redisctl enterprise node update 1 --accept-servers false --rack-id rack2

    # Using JSON for advanced configuration
    redisctl enterprise node update 1 --data '{\"max_redis_servers\": 200}'")]
    Update {
        /// Node ID
        id: u32,
        /// Whether node accepts new shards
        #[arg(long)]
        accept_servers: Option<bool>,
        /// External IP addresses (can be specified multiple times)
        #[arg(long)]
        external_addr: Option<Vec<String>>,
        /// Rack ID for rack-aware placement
        #[arg(long)]
        rack_id: Option<String>,
        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Get node status
    Status {
        /// Node ID
        id: u32,
    },

    /// Get node statistics
    Stats {
        /// Node ID
        id: u32,
    },

    /// Get node metrics
    Metrics {
        /// Node ID
        id: u32,
        /// Time interval (e.g., "1h", "5m")
        #[arg(long)]
        interval: Option<String>,
    },

    /// Run health check on node
    Check {
        /// Node ID
        id: u32,
    },

    /// Get node-specific alerts
    Alerts {
        /// Node ID
        id: u32,
    },

    /// Put node in maintenance mode
    #[command(name = "maintenance-enable")]
    MaintenanceEnable {
        /// Node ID
        id: u32,
    },

    /// Remove node from maintenance mode
    #[command(name = "maintenance-disable")]
    MaintenanceDisable {
        /// Node ID
        id: u32,
    },

    /// Rebalance shards on node
    Rebalance {
        /// Node ID
        id: u32,
    },

    /// Drain node before removal
    Drain {
        /// Node ID
        id: u32,
    },

    /// Restart node services
    Restart {
        /// Node ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Get node configuration
    #[command(name = "get-config")]
    GetConfig {
        /// Node ID
        id: u32,
    },

    /// Update node configuration
    #[command(
        name = "update-config",
        after_help = "EXAMPLES:
    # Set max Redis servers
    redisctl enterprise node update-config 1 --max-redis-servers 200

    # Set bigstore driver
    redisctl enterprise node update-config 1 --bigstore-driver rocksdb

    # Using JSON for advanced configuration
    redisctl enterprise node update-config 1 --data @config.json"
    )]
    UpdateConfig {
        /// Node ID
        id: u32,
        /// Maximum Redis servers on this node
        #[arg(long)]
        max_redis_servers: Option<u32>,
        /// BigStore driver (rocksdb, speedb)
        #[arg(long)]
        bigstore_driver: Option<String>,
        /// Configuration data (JSON file or inline, overridden by other flags)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Get rack awareness configuration
    #[command(name = "get-rack")]
    GetRack {
        /// Node ID
        id: u32,
    },

    /// Set rack ID
    #[command(name = "set-rack")]
    SetRack {
        /// Node ID
        id: u32,
        /// Rack identifier
        #[arg(long)]
        rack: String,
    },

    /// Get node role
    #[command(name = "get-role")]
    GetRole {
        /// Node ID
        id: u32,
    },

    /// Get resource utilization
    Resources {
        /// Node ID
        id: u32,
    },

    /// Get memory usage details
    Memory {
        /// Node ID
        id: u32,
    },

    /// Get CPU usage details
    Cpu {
        /// Node ID
        id: u32,
    },

    /// Get storage usage details
    Storage {
        /// Node ID
        id: u32,
    },

    /// Get network statistics
    Network {
        /// Node ID
        id: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseUserCommands {
    /// List all users
    List,

    /// Get user details
    Get {
        /// User ID
        id: u32,
    },

    /// Create new user
    #[command(after_help = "EXAMPLES:
    # Create user with email and password
    redisctl enterprise user create --email admin@example.com --password secret123 --role admin

    # Create user with display name
    redisctl enterprise user create --email user@example.com --password secret123 --role db_viewer --name \"John Doe\"

    # Create user with email alerts enabled
    redisctl enterprise user create --email ops@example.com --password secret123 --role db_member --email-alerts

    # Create user with RBAC role IDs
    redisctl enterprise user create --email rbac@example.com --password secret123 --role db_viewer --role-uid 1 --role-uid 2

    # Advanced: Full configuration via JSON file
    redisctl enterprise user create --data @user.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Create {
        /// User's email address (used as login)
        #[arg(long)]
        email: Option<String>,

        /// User's password
        #[arg(long)]
        password: Option<String>,

        /// User's role (admin, db_viewer, db_member, cluster_viewer, cluster_member, none)
        #[arg(long)]
        role: Option<String>,

        /// User's display name
        #[arg(long)]
        name: Option<String>,

        /// Enable email alerts for this user
        #[arg(long)]
        email_alerts: bool,

        /// Role UID for RBAC (can be repeated)
        #[arg(long = "role-uid")]
        role_uids: Vec<u32>,

        /// Authentication method (regular, external, certificate)
        #[arg(long)]
        auth_method: Option<String>,

        /// Advanced: Full user configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Update user
    #[command(after_help = "EXAMPLES:
    # Update user's name
    redisctl enterprise user update 1 --name \"Jane Doe\"

    # Update user's role
    redisctl enterprise user update 1 --role admin

    # Update user's password
    redisctl enterprise user update 1 --password newsecret123

    # Enable email alerts
    redisctl enterprise user update 1 --email-alerts true

    # Update RBAC role assignments
    redisctl enterprise user update 1 --role-uid 1 --role-uid 3

    # Advanced: Full update via JSON file
    redisctl enterprise user update 1 --data @updates.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Update {
        /// User ID
        id: u32,

        /// New email address
        #[arg(long)]
        email: Option<String>,

        /// New password
        #[arg(long)]
        password: Option<String>,

        /// New role
        #[arg(long)]
        role: Option<String>,

        /// New display name
        #[arg(long)]
        name: Option<String>,

        /// Enable/disable email alerts
        #[arg(long)]
        email_alerts: Option<bool>,

        /// Role UID for RBAC (can be repeated, replaces existing)
        #[arg(long = "role-uid")]
        role_uids: Vec<u32>,

        /// Advanced: Full update configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete user
    Delete {
        /// User ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Reset user password
    #[command(name = "reset-password")]
    ResetPassword {
        /// User ID
        id: u32,
        /// New password (will prompt if not provided)
        #[arg(long)]
        password: Option<String>,
    },

    /// Get user's roles
    #[command(name = "get-roles")]
    GetRoles {
        /// User ID
        #[arg(name = "user-id")]
        user_id: u32,
    },

    /// Assign role to user
    #[command(name = "assign-role")]
    AssignRole {
        /// User ID
        #[arg(name = "user-id")]
        user_id: u32,
        /// Role ID to assign
        #[arg(long)]
        role: u32,
    },

    /// Remove role from user
    #[command(name = "remove-role")]
    RemoveRole {
        /// User ID
        #[arg(name = "user-id")]
        user_id: u32,
        /// Role ID to remove
        #[arg(long)]
        role: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseRoleCommands {
    /// List all roles
    List,

    /// Get role details
    Get {
        /// Role ID
        id: u32,
    },

    /// Create custom role
    #[command(after_help = "EXAMPLES:
    # Create role with management permission
    redisctl enterprise role create --name db-admin --management admin

    # Create role with cluster viewer access
    redisctl enterprise role create --name cluster-viewer --management cluster_viewer

    # Create role with database-specific permissions
    redisctl enterprise role create --name mydb-admin --management db_viewer --data '{\"bdb_roles\": [{\"bdb_uid\": 1, \"role\": \"admin\"}]}'

    # Advanced: Full configuration via JSON file
    redisctl enterprise role create --data @role.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Create {
        /// Role name (required unless using --data)
        #[arg(long)]
        name: Option<String>,

        /// Management permission level (admin, db_viewer, db_member, cluster_viewer, cluster_member, none)
        #[arg(long)]
        management: Option<String>,

        /// Advanced: Full role configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Update role
    #[command(after_help = "EXAMPLES:
    # Update role name
    redisctl enterprise role update 1 --name new-role-name

    # Update management permission
    redisctl enterprise role update 1 --management admin

    # Advanced: Full update via JSON file
    redisctl enterprise role update 1 --data @updates.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Update {
        /// Role ID
        id: u32,

        /// New role name
        #[arg(long)]
        name: Option<String>,

        /// Management permission level
        #[arg(long)]
        management: Option<String>,

        /// Advanced: Full update configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete custom role
    Delete {
        /// Role ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Get role permissions
    #[command(name = "get-permissions")]
    GetPermissions {
        /// Role ID
        id: u32,
    },

    /// Get users with specific role
    #[command(name = "get-users")]
    GetUsers {
        /// Role ID
        #[arg(name = "role-id")]
        role_id: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseAclCommands {
    /// List all ACLs
    List,

    /// Get ACL details
    Get {
        /// ACL ID
        id: u32,
    },

    /// Create ACL
    #[command(after_help = "EXAMPLES:
    # Create ACL with full access
    redisctl enterprise acl create --name full-access --acl '+@all ~*'

    # Create ACL with read-only access
    redisctl enterprise acl create --name read-only --acl '+@read ~*' --description 'Read-only access'

    # Create ACL with specific key patterns
    redisctl enterprise acl create --name app-acl --acl '+@all ~app:*'

    # Advanced: Full configuration via JSON file
    redisctl enterprise acl create --data @acl.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Create {
        /// ACL name (required unless using --data)
        #[arg(long)]
        name: Option<String>,

        /// ACL rules string (e.g., '+@all ~*')
        #[arg(long)]
        acl: Option<String>,

        /// Description of the ACL
        #[arg(long)]
        description: Option<String>,

        /// Advanced: Full ACL configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Update ACL
    #[command(after_help = "EXAMPLES:
    # Update ACL name
    redisctl enterprise acl update 1 --name new-acl-name

    # Update ACL rules
    redisctl enterprise acl update 1 --acl '+@read +@write ~*'

    # Update description
    redisctl enterprise acl update 1 --description 'Updated description'

    # Advanced: Full update via JSON file
    redisctl enterprise acl update 1 --data @updates.json

NOTE: First-class parameters override values in --data when both are provided.")]
    Update {
        /// ACL ID
        id: u32,

        /// New ACL name
        #[arg(long)]
        name: Option<String>,

        /// New ACL rules string
        #[arg(long)]
        acl: Option<String>,

        /// New description
        #[arg(long)]
        description: Option<String>,

        /// Advanced: Full update configuration as JSON string or @file.json
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete ACL
    Delete {
        /// ACL ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Test ACL permissions
    Test {
        /// User ID
        #[arg(long)]
        user: u32,
        /// Redis command to test
        #[arg(long)]
        command: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseLdapCommands {
    /// Get LDAP configuration
    #[command(name = "get-config")]
    GetConfig,

    /// Update LDAP configuration
    #[command(
        name = "update-config",
        after_help = "EXAMPLES:
    # Enable LDAP with server URL
    redisctl enterprise rbac ldap update-config --enabled true --server-url ldap://ldap.example.com

    # Using JSON for full configuration
    redisctl enterprise rbac ldap update-config --data @ldap.json"
    )]
    UpdateConfig {
        /// Enable or disable LDAP
        #[arg(long)]
        enabled: Option<bool>,
        /// LDAP server URL
        #[arg(long)]
        server_url: Option<String>,
        /// Bind DN for LDAP authentication
        #[arg(long)]
        bind_dn: Option<String>,
        /// Bind password for LDAP authentication
        #[arg(long)]
        bind_password: Option<String>,
        /// Base DN for user search
        #[arg(long)]
        base_dn: Option<String>,
        /// User search filter
        #[arg(long)]
        user_filter: Option<String>,
        /// LDAP config data (JSON file or inline, optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Test LDAP connection
    #[command(name = "test-connection")]
    TestConnection,

    /// Sync users from LDAP
    Sync,

    /// Get LDAP role mappings
    #[command(name = "get-mappings")]
    GetMappings,
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseAuthCommands {
    /// Test authentication
    Test {
        /// Username/email to test
        #[arg(long)]
        user: String,
    },

    /// List active sessions
    #[command(name = "session-list")]
    SessionList,

    /// Revoke session
    #[command(name = "session-revoke")]
    SessionRevoke {
        /// Session ID
        #[arg(name = "session-id")]
        session_id: String,
    },

    /// Revoke all user sessions
    #[command(name = "session-revoke-all")]
    SessionRevokeAll {
        /// User ID
        #[arg(long)]
        user: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum EnterpriseCrdbCommands {
    // CRDB Lifecycle Management
    /// List all Active-Active databases
    List,

    /// Get CRDB details
    Get {
        /// CRDB ID
        id: u32,
    },

    /// Create Active-Active database
    #[command(after_help = "EXAMPLES:
    # Create CRDB with name and memory
    redisctl enterprise crdb create --name my-crdb --memory-size 1073741824

    # Create CRDB with default database name
    redisctl enterprise crdb create --name my-crdb --default-db-name mydb

    # Using JSON for advanced configuration
    redisctl enterprise crdb create --data @crdb.json")]
    Create {
        /// CRDB name
        #[arg(long)]
        name: Option<String>,
        /// Memory size in bytes
        #[arg(long)]
        memory_size: Option<u64>,
        /// Default database name
        #[arg(long)]
        default_db_name: Option<String>,
        /// Enable encryption
        #[arg(long)]
        encryption: Option<bool>,
        /// CRDB configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Update CRDB configuration
    #[command(after_help = "EXAMPLES:
    # Update memory size (1GB)
    redisctl enterprise crdb update 1 --memory-size 1073741824

    # Enable encryption
    redisctl enterprise crdb update 1 --encryption true

    # Set data persistence policy
    redisctl enterprise crdb update 1 --data-persistence aof

    # Update multiple settings
    redisctl enterprise crdb update 1 --memory-size 2147483648 --replication true

    # Using JSON for advanced configuration
    redisctl enterprise crdb update 1 --data '{\"causal_consistency\": true}'")]
    Update {
        /// CRDB ID
        id: u32,
        /// Memory size limit in bytes
        #[arg(long)]
        memory_size: Option<u64>,
        /// Enable/disable encryption
        #[arg(long)]
        encryption: Option<bool>,
        /// Data persistence policy (disabled, aof, snapshot)
        #[arg(long)]
        data_persistence: Option<String>,
        /// Enable/disable replication
        #[arg(long)]
        replication: Option<bool>,
        /// Eviction policy (e.g., allkeys-lru, volatile-lru)
        #[arg(long)]
        eviction_policy: Option<String>,
        /// JSON data for advanced configuration (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Delete CRDB
    Delete {
        /// CRDB ID
        id: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    // Participating Clusters Management
    /// Get participating clusters
    #[command(name = "get-clusters")]
    GetClusters {
        /// CRDB ID
        id: u32,
    },

    /// Add cluster to CRDB
    #[command(
        name = "add-cluster",
        after_help = "EXAMPLES:
    # Add cluster with URL
    redisctl enterprise crdb add-cluster 1 --url https://cluster2.example.com:9443

    # Add cluster with credentials
    redisctl enterprise crdb add-cluster 1 --url https://cluster2.example.com:9443 \\
        --username admin@example.com --password mypassword

    # Using JSON for full configuration
    redisctl enterprise crdb add-cluster 1 --data @cluster.json"
    )]
    AddCluster {
        /// CRDB ID
        id: u32,
        /// Cluster URL (e.g., https://cluster2.example.com:9443)
        #[arg(long)]
        url: Option<String>,
        /// Cluster name
        #[arg(long)]
        name: Option<String>,
        /// Admin username for the cluster
        #[arg(long)]
        username: Option<String>,
        /// Admin password for the cluster
        #[arg(long)]
        password: Option<String>,
        /// Enable replication compression
        #[arg(long)]
        compression: Option<bool>,
        /// Cluster configuration as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Remove cluster from CRDB
    #[command(name = "remove-cluster")]
    RemoveCluster {
        /// CRDB ID
        id: u32,
        /// Cluster ID to remove
        #[arg(long)]
        cluster: u32,
    },

    /// Update cluster configuration in CRDB
    #[command(
        name = "update-cluster",
        after_help = "EXAMPLES:
    # Update cluster URL
    redisctl enterprise crdb update-cluster 1 --cluster 2 --url https://newurl.example.com:9443

    # Enable compression
    redisctl enterprise crdb update-cluster 1 --cluster 2 --compression true

    # Using JSON for full configuration
    redisctl enterprise crdb update-cluster 1 --cluster 2 --data @update.json"
    )]
    UpdateCluster {
        /// CRDB ID
        id: u32,
        /// Cluster ID to update
        #[arg(long)]
        cluster: u32,
        /// Cluster URL
        #[arg(long)]
        url: Option<String>,
        /// Enable replication compression
        #[arg(long)]
        compression: Option<bool>,
        /// Proxy policy (e.g., "single", "all-master-shards", "all-nodes")
        #[arg(long)]
        proxy_policy: Option<String>,
        /// Update configuration as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    // Instance Management
    /// Get all CRDB instances
    #[command(name = "get-instances")]
    GetInstances {
        /// CRDB ID
        id: u32,
    },

    /// Get specific CRDB instance
    #[command(name = "get-instance")]
    GetInstance {
        /// CRDB ID
        #[arg(name = "crdb-id")]
        crdb_id: u32,
        /// Instance ID
        #[arg(long)]
        instance: u32,
    },

    /// Update CRDB instance
    #[command(
        name = "update-instance",
        after_help = "EXAMPLES:
    # Update instance memory size
    redisctl enterprise crdb update-instance 1 --instance 2 --memory-size 2147483648

    # Update instance port
    redisctl enterprise crdb update-instance 1 --instance 2 --port 12001

    # Using JSON for full configuration
    redisctl enterprise crdb update-instance 1 --instance 2 --data @instance.json"
    )]
    UpdateInstance {
        /// CRDB ID
        #[arg(name = "crdb-id")]
        crdb_id: u32,
        /// Instance ID
        #[arg(long)]
        instance: u32,
        /// Memory size in bytes
        #[arg(long)]
        memory_size: Option<u64>,
        /// Port number
        #[arg(long)]
        port: Option<u16>,
        /// Enable/disable the instance
        #[arg(long)]
        enabled: Option<bool>,
        /// Update configuration as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Flush CRDB instance data
    #[command(name = "flush-instance")]
    FlushInstance {
        /// CRDB ID
        #[arg(name = "crdb-id")]
        crdb_id: u32,
        /// Instance ID
        #[arg(long)]
        instance: u32,
        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    // Replication & Sync
    /// Get replication status
    #[command(name = "get-replication-status")]
    GetReplicationStatus {
        /// CRDB ID
        id: u32,
    },

    /// Get replication lag metrics
    #[command(name = "get-lag")]
    GetLag {
        /// CRDB ID
        id: u32,
    },

    /// Force synchronization
    #[command(name = "force-sync")]
    ForceSync {
        /// CRDB ID
        id: u32,
        /// Source cluster ID
        #[arg(long)]
        source: u32,
    },

    /// Pause replication
    #[command(name = "pause-replication")]
    PauseReplication {
        /// CRDB ID
        id: u32,
    },

    /// Resume replication
    #[command(name = "resume-replication")]
    ResumeReplication {
        /// CRDB ID
        id: u32,
    },

    // Conflict Resolution
    /// Get conflict history
    #[command(name = "get-conflicts")]
    GetConflicts {
        /// CRDB ID
        id: u32,
        /// Maximum number of conflicts to return
        #[arg(long)]
        limit: Option<u32>,
    },

    /// Get conflict resolution policy
    #[command(name = "get-conflict-policy")]
    GetConflictPolicy {
        /// CRDB ID
        id: u32,
    },

    /// Update conflict resolution policy
    #[command(
        name = "update-conflict-policy",
        after_help = "EXAMPLES:
    # Set conflict policy to last-write-wins
    redisctl enterprise crdb update-conflict-policy 1 --policy last-write-wins

    # Set policy with source preference
    redisctl enterprise crdb update-conflict-policy 1 --policy source-wins --source-id 2

    # Using JSON for full configuration
    redisctl enterprise crdb update-conflict-policy 1 --data @policy.json"
    )]
    UpdateConflictPolicy {
        /// CRDB ID
        id: u32,
        /// Conflict resolution policy (e.g., "last-write-wins", "source-wins")
        #[arg(long)]
        policy: Option<String>,
        /// Source cluster ID for source-wins policy
        #[arg(long)]
        source_id: Option<u32>,
        /// Policy configuration as JSON string or @file.json (optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Manually resolve conflict
    #[command(name = "resolve-conflict")]
    ResolveConflict {
        /// CRDB ID
        id: u32,
        /// Conflict ID
        #[arg(long)]
        conflict: String,
        /// Resolution method
        #[arg(long)]
        resolution: String,
    },

    // Tasks & Jobs
    /// Get CRDB tasks
    #[command(name = "get-tasks")]
    GetTasks {
        /// CRDB ID
        id: u32,
    },

    /// Get specific task details
    #[command(name = "get-task")]
    GetTask {
        /// CRDB ID
        #[arg(name = "crdb-id")]
        crdb_id: u32,
        /// Task ID
        #[arg(long)]
        task: String,
    },

    /// Retry failed task
    #[command(name = "retry-task")]
    RetryTask {
        /// CRDB ID
        #[arg(name = "crdb-id")]
        crdb_id: u32,
        /// Task ID
        #[arg(long)]
        task: String,
    },

    /// Cancel running task
    #[command(name = "cancel-task")]
    CancelTask {
        /// CRDB ID
        #[arg(name = "crdb-id")]
        crdb_id: u32,
        /// Task ID
        #[arg(long)]
        task: String,
    },

    // Monitoring & Metrics
    /// Get CRDB statistics
    Stats {
        /// CRDB ID
        id: u32,
    },

    /// Get CRDB metrics
    Metrics {
        /// CRDB ID
        id: u32,
        /// Time interval (e.g., "1h", "24h")
        #[arg(long)]
        interval: Option<String>,
    },

    /// Get connection details per instance
    #[command(name = "get-connections")]
    GetConnections {
        /// CRDB ID
        id: u32,
    },

    /// Get throughput metrics
    #[command(name = "get-throughput")]
    GetThroughput {
        /// CRDB ID
        id: u32,
    },

    /// Run health check
    #[command(name = "health-check")]
    HealthCheck {
        /// CRDB ID
        id: u32,
    },

    // Backup & Recovery
    /// Create CRDB backup
    #[command(after_help = "EXAMPLES:
    # Backup to S3
    redisctl enterprise crdb backup 1 --location s3://bucket/crdb-backup

    # Using JSON for advanced configuration
    redisctl enterprise crdb backup 1 --data @backup.json")]
    Backup {
        /// CRDB ID
        id: u32,
        /// Backup location (e.g., S3 URL)
        #[arg(long)]
        location: Option<String>,
        /// Backup configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// Restore CRDB
    #[command(after_help = "EXAMPLES:
    # Restore from specific backup
    redisctl enterprise crdb restore 1 --backup-uid backup-12345

    # Restore from location
    redisctl enterprise crdb restore 1 --location s3://bucket/crdb-backup

    # Using JSON for advanced configuration
    redisctl enterprise crdb restore 1 --data @restore.json")]
    Restore {
        /// CRDB ID
        id: u32,
        /// Backup UID to restore from
        #[arg(long)]
        backup_uid: Option<String>,
        /// Restore location (e.g., S3 URL)
        #[arg(long)]
        location: Option<String>,
        /// Restore configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },

    /// List available backups
    #[command(name = "get-backups")]
    GetBackups {
        /// CRDB ID
        id: u32,
    },

    /// Export CRDB data
    #[command(after_help = "EXAMPLES:
    # Export to S3
    redisctl enterprise crdb export 1 --location s3://bucket/crdb-export

    # Using JSON for advanced configuration
    redisctl enterprise crdb export 1 --data @export.json")]
    Export {
        /// CRDB ID
        id: u32,
        /// Export location (e.g., S3 URL)
        #[arg(long)]
        location: Option<String>,
        /// Export configuration as JSON string or @file.json (overridden by other flags)
        #[arg(long)]
        data: Option<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum EnterpriseStatsCommands {
    /// Get database statistics
    Database {
        /// Database ID
        id: u32,
        /// Stream stats continuously
        #[arg(long, short = 'f')]
        follow: bool,
        /// Poll interval in seconds (for --follow)
        #[arg(long, default_value = "5")]
        poll_interval: u64,
    },

    /// Get database shard statistics
    DatabaseShards {
        /// Database ID
        id: u32,
    },

    /// Get database metrics over time
    DatabaseMetrics {
        /// Database ID
        id: u32,
        /// Time interval (1m, 5m, 1h, 1d)
        #[arg(long, default_value = "1h")]
        interval: String,
    },

    /// Get node statistics
    Node {
        /// Node ID
        id: u32,
        /// Stream stats continuously
        #[arg(long, short = 'f')]
        follow: bool,
        /// Poll interval in seconds (for --follow)
        #[arg(long, default_value = "5")]
        poll_interval: u64,
    },

    /// Get node metrics over time
    NodeMetrics {
        /// Node ID
        id: u32,
        /// Time interval (1m, 5m, 1h, 1d)
        #[arg(long, default_value = "1h")]
        interval: String,
    },

    /// Get cluster-wide statistics
    Cluster {
        /// Stream stats continuously
        #[arg(long, short = 'f')]
        follow: bool,
        /// Poll interval in seconds (for --follow)
        #[arg(long, default_value = "5")]
        poll_interval: u64,
    },

    /// Get cluster metrics over time
    ClusterMetrics {
        /// Time interval (1m, 5m, 1h, 1d)
        #[arg(long, default_value = "1h")]
        interval: String,
    },

    /// Get listener statistics
    Listener,

    /// Export statistics in various formats
    Export {
        /// Export format (json, prometheus, csv)
        #[arg(long, default_value = "json")]
        format: String,
        /// Time interval for time-series data (1m, 5m, 1h, 1d)
        #[arg(long)]
        interval: Option<String>,
    },
}
