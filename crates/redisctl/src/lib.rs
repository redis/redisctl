//! # redisctl
//!
//! A unified command-line interface for managing Redis deployments across Cloud and Enterprise.
//!
//! ## Overview
//!
//! `redisctl` is a comprehensive CLI tool that unifies management of both Redis Cloud and
//! Redis Enterprise deployments. It automatically detects which API to use based on your
//! configuration profile or explicit command selection, providing a consistent interface
//! for all Redis management tasks.
//!
//! ## Installation
//!
//! Install the CLI tool from crates.io:
//!
//! ```bash
//! cargo install redisctl
//! ```
//!
//! ## Quick Start
//!
//! ### Configure Authentication
//!
//! For Redis Cloud:
//! ```bash
//! export REDIS_CLOUD_API_KEY="your-api-key"
//! export REDIS_CLOUD_SECRET_KEY="your-api-secret"
//! ```
//!
//! For Redis Enterprise:
//! ```bash
//! export REDIS_ENTERPRISE_URL="https://cluster.example.com:9443"
//! export REDIS_ENTERPRISE_USER="admin@example.com"
//! export REDIS_ENTERPRISE_PASSWORD="your-password"
//! ```
//!
//! Or use profiles:
//! ```bash
//! redisctl profile set prod-cloud \
//!   --deployment-type cloud \
//!   --api-key YOUR_KEY \
//!   --api-secret YOUR_SECRET
//! ```
//!
//! ### Basic Usage
//!
//! ```bash
//! # List all profiles
//! redisctl profile list
//!
//! # Cloud-specific commands
//! redisctl cloud subscription list
//! redisctl cloud database list
//!
//! # Enterprise-specific commands
//! redisctl enterprise cluster info
//! redisctl enterprise database list
//!
//! # Smart routing (auto-detects based on profile)
//! redisctl cloud database list --profile prod-cloud
//! ```
//!
//! ## Features
//!
//! - **Unified Interface** - Single CLI for both Redis Cloud and Enterprise
//! - **Smart Command Routing** - Automatically routes commands based on deployment type
//! - **Profile Management** - Save and switch between multiple Redis deployments
//! - **Multiple Output Formats** - JSON, YAML, and Table output with JMESPath queries
//! - **Comprehensive API Coverage** - Full implementation of both Cloud and Enterprise REST APIs
//!
//! ## Using as a Library
//!
//! If you need to programmatically interact with Redis Cloud or Enterprise APIs,
//! use the dedicated library crates instead:
//!
//! - [`redis-cloud`](https://docs.rs/redis-cloud) - Redis Cloud REST API client
//! - [`redis-enterprise`](https://docs.rs/redis-enterprise) - Redis Enterprise REST API client
//!
//! ## Documentation
//!
//! For complete documentation and examples, see the [GitHub repository](https://github.com/redis/redisctl).

// Internal modules for CLI functionality
pub(crate) mod cli;
pub(crate) mod commands;
pub(crate) mod connection;
pub(crate) mod error;
pub(crate) mod output;
pub(crate) mod workflows;
