//! # redisctl-core
//!
//! Layer 2: Higher-level interface on top of redis-cloud and redis-enterprise clients.
//!
//! This crate provides:
//! - **Unified error handling** - CoreError wrapping both platform errors
//! - **Progress callbacks** - For Cloud's async task polling
//! - **Module resolution** - Validate Enterprise modules before creation
//! - **Workflows** - Multi-step operations (create + wait, etc.)
//!
//! ## Philosophy
//!
//! **Don't rebuild Layer 1. Use it and add value.**
//!
//! - Simple operations: Use Layer 1 directly (`redis_cloud::DatabaseHandler`, etc.)
//! - Operations with progress: Use Layer 2 workflows
//! - Operations with validation: Use Layer 2 helpers
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    Layer 3: Consumers                           │
//! │           CLI (redisctl)        MCP (redisctl-mcp)             │
//! └──────────────────────────┬──────────────────────────────────────┘
//!                            │
//!                            ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                 Layer 2: redisctl-core                          │
//! │  - Unified errors (CoreError)                                   │
//! │  - Progress callbacks (poll_task)                               │
//! │  - Module resolution (resolve_modules)                          │
//! │  - Workflows (create_and_wait, etc.)                            │
//! └──────────────────────────┬──────────────────────────────────────┘
//!                            │
//!                            ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │               Layer 1: Raw API Clients                          │
//! │         redis-cloud              redis-enterprise               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use redis_cloud::{CloudClient, DatabaseHandler};
//! use redisctl_core::{poll_task, ProgressEvent};
//! use std::time::Duration;
//!
//! // Simple operation: use Layer 1 directly
//! let handler = DatabaseHandler::new(client.clone());
//! let databases = handler.list(subscription_id).await?;
//!
//! // Operation with progress: use Layer 2
//! let task = handler.create(subscription_id, &request).await?;
//! let completed = poll_task(
//!     &client,
//!     &task.task_id.unwrap(),
//!     Duration::from_secs(600),
//!     Duration::from_secs(10),
//!     Some(Box::new(|event| {
//!         if let ProgressEvent::Polling { status, elapsed, .. } = event {
//!             println!("Status: {} ({:.0}s)", status, elapsed.as_secs());
//!         }
//!     })),
//! ).await?;
//! ```

/// `User-Agent` sent by every redisctl HTTP client.
///
/// The Redis Cloud API recognises the `redisctl/` prefix as a trusted client for some operations
/// (free-tier provisioning among them), so all consumers — CLI and MCP alike — must send it.
pub const USER_AGENT: &str = concat!("redisctl/", env!("CARGO_PKG_VERSION"));

pub mod auth;
pub mod config;
pub mod error;
pub mod progress;

pub mod cloud;
pub mod enterprise;

// Re-export commonly used items
pub use auth::{
    AuthError, CapiKey, CloudAuthenticator, DeviceAuthorization, DeviceFlowClient,
    LoopbackFlowClient, MintedCredentials, SmAccount, SmApiClient, SmUser, TokenSet,
};
pub use error::{CoreError, Result};
pub use progress::{ProgressCallback, ProgressEvent, poll_task};

// Re-export config types for convenience
pub use config::{
    CloudAuthConfig, Config, ConfigError, CredentialStorage, CredentialStore, DeploymentType,
    Profile, ProfileCredentials,
};

// Re-export Layer 1 for convenience (but consumers can also import directly)
pub use redis_cloud;
pub use redis_enterprise;
