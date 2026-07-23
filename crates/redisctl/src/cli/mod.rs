//! CLI structure and command definitions
//!
//! Defines the command-line interface using clap with a three-layer architecture:
//! 1. Raw API access (`api` commands)
//! 2. Human-friendly interface (`cloud`/`enterprise` commands)
//! 3. Workflow orchestration (`workflow` commands - future)

use clap::{Parser, Subcommand, ValueHint};
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use redisctl_core::DeploymentType;

pub mod cloud;
pub mod enterprise;

pub use cloud::*;
pub use enterprise::*;

/// Redis management CLI with unified access to Cloud and Enterprise
#[derive(Parser, Debug)]
#[command(name = "redisctl")]
#[command(
    version,
    about = "Redis management CLI for Cloud and Enterprise deployments"
)]
#[command(long_about = "
Redis management CLI for Cloud and Enterprise deployments

Commands infer platform from your profile — no prefix needed:
    redisctl database list              # uses your configured profile
    redisctl subscription list          # cloud-only, no prefix needed
    redisctl cluster get                # enterprise-only, no prefix needed

Or be explicit:
    redisctl cloud database list
    redisctl enterprise database list

PROFILE TYPES:
    redisctl uses profiles to store connection credentials. Each profile has a
    type that determines which commands it can be used with:

    cloud       Redis Cloud API credentials (api-key + api-secret).
                Unlocks: cloud, subscription, database list, task, acl, ...

    enterprise  Redis Enterprise cluster credentials (url + username).
                Unlocks: enterprise, cluster, node, database list, shard, ...

    database    Direct Redis connection (host + port).
                Unlocks: db open (launches redis-cli with saved credentials)

    Create a profile with:  redisctl profile set <name> --type <type> ...
    See all profiles:       redisctl profile list
    Full setup help:        redisctl profile set --help

EXAMPLES:
    # Set up a Cloud profile
    redisctl profile set mycloud --type cloud --api-key KEY --api-secret SECRET

    # Set up an Enterprise profile
    redisctl profile set myenterprise --type enterprise --url https://cluster:9443 --username admin

    # Set up a Database profile (direct Redis connection)
    redisctl profile set mycache --type database --host localhost --port 6379

    # Get JSON output for scripting
    redisctl subscription list -o json

    # Filter output with JMESPath
    redisctl database list -q 'databases[?status==`active`]'

    # Filter with a query from a file
    redisctl cloud sub list -q @queries/active-dbs.jmespath

    # Direct API access
    redisctl api cloud get /subscriptions
    redisctl api enterprise get /v1/cluster

For more help on a specific command, run:
    redisctl <command> --help
")]
pub struct Cli {
    /// Profile to use for this command
    #[arg(long, short, global = true, env = "REDISCTL_PROFILE", add = ArgValueCandidates::new(profile_candidates))]
    pub profile: Option<String>,

    /// Path to alternate configuration file
    #[arg(long, global = true, env = "REDISCTL_CONFIG_FILE", value_hint = ValueHint::FilePath)]
    pub config_file: Option<String>,

    /// Output format
    #[arg(long, short = 'o', global = true, value_enum, default_value = "auto")]
    pub output: OutputFormat,

    /// JMESPath query to filter output (use @file to read from file)
    #[arg(long, short = 'q', global = true)]
    pub query: Option<String>,

    /// Enable verbose logging
    #[arg(long, short, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

/// Output format options
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    /// Automatically choose format based on command and context
    Auto,
    /// JSON output
    Json,
    /// YAML output
    Yaml,
    /// Human-readable table format
    Table,
}

impl OutputFormat {
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }

    pub fn is_yaml(&self) -> bool {
        matches!(self, Self::Yaml)
    }
}

/// Top-level commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Raw API access - direct REST endpoint calls
    #[command(name = "api")]
    #[command(after_help = "EXAMPLES:
    # GET request to Cloud API
    redisctl api cloud get /subscriptions

    # GET request to Enterprise API
    redisctl api enterprise get /v1/cluster

    # POST request with JSON data
    redisctl api cloud post /subscriptions --data '{\"name\":\"my-sub\"}'

    # POST request from file
    redisctl api cloud post /subscriptions --data @subscription.json

    # Output as JSON for scripting
    redisctl api enterprise get /v1/bdbs -o json
")]
    Api {
        /// Platform type (cloud or enterprise)
        #[arg(value_enum)]
        deployment: DeploymentType,

        /// HTTP method
        #[arg(value_parser = parse_http_method)]
        method: HttpMethod,

        /// API endpoint path (e.g., /subscriptions)
        path: String,

        /// Request body (JSON string or @file)
        #[arg(long)]
        data: Option<String>,

        /// Print equivalent curl command instead of executing
        #[arg(long)]
        curl: bool,
    },

    /// Profile management
    #[command(subcommand, visible_alias = "prof", visible_alias = "pr")]
    #[command(after_help = "EXAMPLES:
    # Create a Cloud profile
    redisctl profile set mycloud --type cloud --api-key KEY --api-secret SECRET

    # Create an Enterprise profile
    redisctl profile set myenterprise --type enterprise --url https://cluster:9443 --username admin

    # Create a Database profile
    redisctl profile set mycache --type database --host localhost --port 6379

    # List all profiles
    redisctl profile list

    # Show profile details
    redisctl profile show mycloud

    # Validate configuration
    redisctl profile validate

    # Set default profiles
    redisctl profile default-cloud mycloud
    redisctl profile default-enterprise myenterprise
    redisctl profile default-database mycache
")]
    Profile(ProfileCommands),

    /// Cloud-specific operations
    #[command(subcommand, visible_alias = "cl")]
    #[command(before_long_help = "\
COMMAND GROUPS:
  Core:       database (db), subscription (sub), fixed-database (fixed-db),
              fixed-subscription (fixed-sub)
  Access:     user, acl
  Billing:    account (acct), payment-method, cost-report
  Networking: connectivity (conn), provider-account
  Operations: task, workflow")]
    #[command(after_help = "\
PROFILE REQUIREMENT:
  These commands require a 'cloud' profile. Set one up with:
    redisctl profile set <name> --type cloud --api-key <KEY> --api-secret <SECRET>
  List existing profiles: redisctl profile list")]
    Cloud(CloudCommands),

    /// Enterprise-specific operations
    #[command(subcommand, visible_alias = "ent", visible_alias = "en")]
    #[command(before_long_help = "\
COMMAND GROUPS:
  Core:          database (db), cluster, node, shard, endpoint
  Access:        user, role, acl, ldap, ldap-mappings, auth
  Monitoring:    stats, status, alerts (alert), logs (log), diagnostics (diag),
                 debug-info
  Admin:         license (lic), module, proxy, services (svc), cm-settings, suffix
  Advanced:      crdb, crdb-task, bdb-group, migration, bootstrap, job-scheduler
  Troubleshoot:  support-package, ocsp, usage-report, local
  Other:         action, jsonschema, workflow")]
    #[command(after_help = "\
PROFILE REQUIREMENT:
  These commands require an 'enterprise' profile. Set one up with:
    redisctl profile set <name> --type enterprise --url <URL> --username <USER>
  List existing profiles: redisctl profile list")]
    Enterprise(EnterpriseCommands),

    /// Files.com API key management (for support package uploads)
    #[command(subcommand, visible_alias = "fk")]
    FilesKey(FilesKeyCommands),

    /// Database operations (direct Redis connections)
    #[command(subcommand)]
    #[command(after_help = "\
PROFILE REQUIREMENT:
  These commands require a 'database' profile. Set one up with:
    redisctl profile set <name> --type database --host <HOST> --port <PORT>
  List existing profiles: redisctl profile list")]
    Db(DbCommands),

    /// Version information
    #[command(visible_alias = "ver", visible_alias = "v")]
    Version,

    /// Generate shell completions
    #[command(visible_alias = "comp")]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,

        /// Print dynamic completion registration command instead of static script
        #[arg(long)]
        register: bool,
    },
}

/// Supported shells for completion generation
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[allow(clippy::enum_variant_names)]
pub enum Shell {
    /// Bourne Again Shell
    Bash,
    /// Z Shell
    Zsh,
    /// Friendly Interactive Shell
    Fish,
    /// PowerShell
    #[value(name = "powershell", alias = "power-shell")]
    PowerShell,
    /// Elvish
    Elvish,
}

/// HTTP methods for raw API access
#[derive(Debug, Clone)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// Parse HTTP method case-insensitively
fn parse_http_method(s: &str) -> Result<HttpMethod, String> {
    match s.to_lowercase().as_str() {
        "get" => Ok(HttpMethod::Get),
        "post" => Ok(HttpMethod::Post),
        "put" => Ok(HttpMethod::Put),
        "patch" => Ok(HttpMethod::Patch),
        "delete" => Ok(HttpMethod::Delete),
        _ => Err(format!(
            "invalid HTTP method: {} (valid: get, post, put, patch, delete)",
            s
        )),
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
        }
    }
}

/// Profile management commands
#[derive(Subcommand, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum ProfileCommands {
    /// List all configured profiles
    #[command(visible_alias = "ls", visible_alias = "l")]
    List {
        /// Filter profiles by tag (repeatable, matches any)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tags: Vec<String>,
    },

    /// Show the path to the configuration file
    Path,

    /// Print the active profile name (for shell prompt integration)
    #[command(visible_alias = "active")]
    #[command(
        after_help = "Prints the profile that would be used for the given deployment type.
Useful for embedding in your shell prompt (PS1).

EXAMPLES:
    # Show active cloud profile
    redisctl profile current --type cloud

    # Show active enterprise profile
    redisctl profile current --type enterprise

    # Use in shell prompt (bash)
    PS1='[\\$(redisctl profile current --type cloud 2>/dev/null)]\\$ '"
    )]
    Current {
        /// Deployment type to show the active profile for
        #[arg(long, value_enum)]
        r#type: DeploymentType,
    },

    /// Show details of a specific profile
    #[command(visible_alias = "sh", visible_alias = "get")]
    Show {
        /// Profile name to show
        name: String,
    },

    /// Set or create a profile
    #[command(visible_alias = "add", visible_alias = "create")]
    #[command(after_help = "CHOOSING A PROFILE TYPE:
    cloud       -- You manage databases through the Redis Cloud console/API.
                   Requires an API key and secret from cloud.redis.io.
    enterprise  -- You run Redis Enterprise Software on your own infrastructure.
                   Requires the cluster URL and admin credentials.
    database    -- You want to connect directly to a Redis instance (any provider).
                   Requires host, port, and optionally a password.

EXAMPLES:
    # Create a Cloud profile
    redisctl profile set mycloud --type cloud \\
        --api-key A3qcymrvqpn9rrgdt40sv5f9yfxob26vx64hwddh8vminqnkgfq \\
        --api-secret S3s8ecrrnaguqkvwfvealoe3sn25zqs4wc4lwgo4rb0ud3qm77c

    # Create an Enterprise profile (password will be prompted)
    redisctl profile set prod --type enterprise \\
        --url https://cluster.example.com:9443 \\
        --username admin@example.com

    # Create Enterprise profile with password
    redisctl profile set staging --type enterprise \\
        --url https://staging:9443 \\
        --username admin \\
        --password mypassword

    # Create Enterprise profile allowing insecure connections
    redisctl profile set local --type enterprise \\
        --url https://localhost:9443 \\
        --username admin@redis.local \\
        --insecure

    # Create a Database profile (direct Redis connection)
    redisctl profile set my-cache --type database \\
        --host redis-12345.cloud.redislabs.com \\
        --port 12345 \\
        --password mypassword

    # Create Database profile without TLS (local dev)
    redisctl profile set local-redis --type database \\
        --host localhost \\
        --port 6379 \\
        --no-tls

    # Tag a profile for organization
    redisctl profile set prod-cloud --type cloud \\
        --api-key KEY --api-secret SECRET \\
        --tag prod --tag us-east
")]
    Set {
        /// Profile name
        name: String,

        /// Platform type: 'cloud' for Redis Cloud or 'enterprise' for Redis Enterprise
        #[arg(long, value_enum, visible_alias = "deployment")]
        r#type: DeploymentType,

        /// API key (for Cloud profiles)
        #[arg(long, required_if_eq("type", "cloud"))]
        api_key: Option<String>,

        /// API secret (for Cloud profiles)
        #[arg(long, required_if_eq("type", "cloud"))]
        api_secret: Option<String>,

        /// API URL (for Cloud profiles)
        #[arg(long, default_value = "https://api.redislabs.com/v1", value_hint = ValueHint::Url)]
        api_url: String,

        /// Enterprise URL (for Enterprise profiles)
        #[arg(long, required_if_eq("type", "enterprise"), value_hint = ValueHint::Url)]
        url: Option<String>,

        /// Username (for Enterprise profiles)
        #[arg(long, required_if_eq("type", "enterprise"))]
        username: Option<String>,

        /// Password (for Enterprise profiles)
        #[arg(long)]
        password: Option<String>,

        /// Allow insecure connections (for Enterprise profiles)
        #[arg(long)]
        insecure: bool,

        /// Path to custom CA certificate for TLS verification (for Enterprise/Kubernetes profiles)
        #[arg(long, value_hint = ValueHint::FilePath)]
        ca_cert: Option<String>,

        /// Redis host (for Database profiles)
        #[arg(long, required_if_eq("type", "database"))]
        host: Option<String>,

        /// Redis port (for Database profiles)
        #[arg(long, required_if_eq("type", "database"))]
        port: Option<u16>,

        /// Disable TLS (for Database profiles, TLS is enabled by default)
        #[arg(long)]
        no_tls: bool,

        /// Redis database number (for Database profiles, default: 0)
        #[arg(long)]
        db: Option<u8>,

        /// Store credentials in OS keyring instead of config file
        #[cfg(feature = "secure-storage")]
        #[arg(long)]
        use_keyring: bool,

        /// Tags for organizing profiles (repeatable)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tags: Vec<String>,
    },

    /// Remove a profile
    #[command(visible_alias = "rm", visible_alias = "del", visible_alias = "delete")]
    Remove {
        /// Profile name to remove
        name: String,
    },

    /// Set the default profile for enterprise commands
    #[command(name = "default-enterprise", visible_alias = "def-ent")]
    DefaultEnterprise {
        /// Profile name to set as default for enterprise commands
        name: String,
    },

    /// Set the default profile for cloud commands
    #[command(name = "default-cloud", visible_alias = "def-cloud")]
    DefaultCloud {
        /// Profile name to set as default for cloud commands
        name: String,
    },

    /// Set the default profile for database commands
    #[command(name = "default-database", visible_alias = "def-db")]
    DefaultDatabase {
        /// Profile name to set as default for database commands
        name: String,
    },

    /// Validate configuration file and profiles
    #[command(visible_alias = "check")]
    #[command(after_help = "EXAMPLES:
    # Validate all profiles and configuration
    redisctl profile validate

    # Validate and test connectivity to all profiles
    redisctl profile validate --connect

    # Machine-readable validation output
    redisctl profile validate --connect -o json

    # Example output:
    # Configuration file: /Users/user/.config/redisctl/config.toml
    # ✓ Configuration file exists and is readable
    # ✓ Found 2 profile(s)
    #
    # Profile 'mycloud' (cloud): ✓ Valid
    # Profile 'myenterprise' (enterprise): ✓ Valid
    #
    # ✓ Default enterprise profile: myenterprise
    # ✓ Default cloud profile: mycloud
    #
    # ✓ Configuration is valid
")]
    Validate {
        /// Test actual API/database connectivity for each profile
        #[arg(long, short = 'c')]
        connect: bool,
    },

    /// Interactive wizard to create a new profile
    #[command(visible_alias = "setup")]
    #[command(after_help = "Walks you through creating a profile step by step.
Prompts for the profile type, name, and required credentials.
Optionally tests connectivity before saving.")]
    Init,
}

/// Files.com API key management commands
#[derive(Subcommand, Debug)]
pub enum FilesKeyCommands {
    /// Store Files.com API key (globally or in config)
    #[command(visible_alias = "add")]
    Set {
        /// The Files.com API key provided by Redis Support
        api_key: String,

        /// Store in system keyring (most secure - recommended)
        #[cfg(feature = "secure-storage")]
        #[arg(long)]
        use_keyring: bool,

        /// Store globally in config file (plaintext - not recommended)
        #[arg(long, conflicts_with = "use_keyring")]
        global: bool,

        /// Store in specific profile's config (plaintext - not recommended)
        #[arg(long, conflicts_with_all = ["use_keyring", "global"])]
        profile: Option<String>,
    },

    /// Get the currently configured Files.com API key
    #[command(visible_alias = "show")]
    Get {
        /// Show for specific profile
        #[arg(long)]
        profile: Option<String>,
    },

    /// Remove Files.com API key
    #[command(visible_alias = "rm", visible_alias = "delete")]
    Remove {
        /// Remove from keyring
        #[cfg(feature = "secure-storage")]
        #[arg(long)]
        keyring: bool,

        /// Remove from global config
        #[arg(long, conflicts_with = "keyring")]
        global: bool,

        /// Remove from specific profile
        #[arg(long, conflicts_with_all = ["keyring", "global"])]
        profile: Option<String>,
    },
}

/// Database commands for direct Redis operations
#[derive(Subcommand, Debug)]
pub enum DbCommands {
    /// Open redis-cli with profile credentials
    #[command(visible_alias = "connect", visible_alias = "cli")]
    #[command(after_help = "EXAMPLES:
    # Open redis-cli using a database profile
    redisctl db open --profile my-cache

    # Print the command without executing (for debugging)
    redisctl db open --profile my-cache --dry-run

    # Pass additional arguments to redis-cli
    redisctl db open --profile my-cache -- -n 1

    # Use a specific redis-cli binary
    redisctl db open --profile my-cache --redis-cli /usr/local/bin/redis-cli
")]
    Open {
        /// Database profile to use (must be a 'database' type profile)
        #[arg(long, short)]
        profile: String,

        /// Print the redis-cli command without executing
        #[arg(long)]
        dry_run: bool,

        /// Path to redis-cli binary (defaults to 'redis-cli' in PATH)
        #[arg(long, default_value = "redis-cli", value_hint = ValueHint::ExecutablePath)]
        redis_cli: String,

        /// Additional arguments to pass to redis-cli
        #[arg(last = true)]
        args: Vec<String>,
    },
}

fn profile_candidates() -> Vec<CompletionCandidate> {
    let config = redisctl_core::Config::load().unwrap_or_default();
    config
        .list_profiles()
        .into_iter()
        .map(|(name, profile)| {
            CompletionCandidate::new(name.as_str())
                .help(Some(format!("{}", profile.deployment_type).into()))
        })
        .collect()
}
