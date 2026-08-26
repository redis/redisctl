use clap::{ArgGroup, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum ModuleCommands {
    /// List all available modules
    #[command(visible_alias = "ls")]
    List,

    /// Get module details by UID or name
    #[command(group(ArgGroup::new("identifier").required(true).args(["uid", "name"])))]
    Get {
        /// Module UID
        #[arg(conflicts_with = "name")]
        uid: Option<String>,

        /// Module name (e.g., "ReJSON", "search")
        #[arg(long, conflicts_with = "uid")]
        name: Option<String>,
    },

    /// Upload new module
    Upload {
        /// Module file path (e.g., @module.zip)
        #[arg(long)]
        file: String,
    },

    /// Delete module
    #[command(visible_alias = "rm")]
    Delete {
        /// Module UID
        uid: String,

        /// Force deletion without confirmation
        #[arg(long)]
        force: bool,
    },

    /// Configure module for database
    #[command(
        name = "config-bdb",
        after_help = "EXAMPLES:
    # Configure module with name
    redisctl enterprise module config-bdb 1 --module-name ReJSON --module-args '--maxarrsize 1000'

    # Using JSON for full configuration
    redisctl enterprise module config-bdb 1 --data @module-config.json"
    )]
    ConfigBdb {
        /// Database UID
        bdb_uid: u32,

        /// Module name (e.g., ReJSON, search)
        #[arg(long)]
        module_name: Option<String>,

        /// Module arguments
        #[arg(long)]
        module_args: Option<String>,

        /// Configuration data (JSON file or inline, optional)
        #[arg(long, value_name = "FILE|JSON")]
        data: Option<String>,
    },

    /// Validate module.json against Redis Enterprise schema
    #[command(after_help = "EXAMPLES:
    # Validate a module.json file
    redisctl enterprise module validate ./module.json

    # Validate with strict mode (all recommended fields required)
    redisctl enterprise module validate ./module.json --strict")]
    Validate {
        /// Path to module.json file
        file: PathBuf,

        /// Strict validation (require all recommended fields)
        #[arg(long)]
        strict: bool,
    },

    /// Inspect a packaged module zip file
    #[command(after_help = "EXAMPLES:
    # Inspect a module package
    redisctl enterprise module inspect ./redis-jmespath.Linux-x86_64.0.3.0.zip

    # Show full metadata including all commands
    redisctl enterprise module inspect ./module.zip --full")]
    Inspect {
        /// Path to module zip file
        file: PathBuf,

        /// Show full metadata including all commands
        #[arg(long)]
        full: bool,
    },

    /// Package a module and metadata into an RE8-compatible zip
    #[command(after_help = "EXAMPLES:
    # Package a module
    redisctl enterprise module package \\
      --module ./libredis_jmespath.so \\
      --metadata ./module.json \\
      --out ./dist/redis-jmespath.Linux-x86_64.0.3.0.zip

    # Package with validation
    redisctl enterprise module package \\
      --module ./module.so \\
      --metadata ./module.json \\
      --out ./package.zip \\
      --validate")]
    Package {
        /// Path to compiled module binary (.so file)
        #[arg(long)]
        module: PathBuf,

        /// Path to module.json metadata file
        #[arg(long)]
        metadata: PathBuf,

        /// Output zip file path
        #[arg(long = "out")]
        output_path: PathBuf,

        /// Validate module.json before packaging
        #[arg(long)]
        validate: bool,
    },
}
