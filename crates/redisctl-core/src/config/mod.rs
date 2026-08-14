//! Configuration and profile management for Redis CLI tools
//!
// Allow nested config module - this is intentional for the config subsystem

#![allow(clippy::module_inception)]
//!
//! This module provides a reusable configuration system for managing
//! credentials and settings for Redis Cloud and Redis Enterprise deployments.
//!
//! # Features
//!
//! - Multiple named profiles for different Redis deployments
//! - Secure credential storage using OS keyring (optional)
//! - Environment variable expansion in config files
//! - Platform-specific config file locations
//! - Support for both Redis Cloud and Redis Enterprise

pub mod config;
pub mod credential;
pub mod error;

// Re-export main types for convenience
pub use config::{CloudAuthConfig, Config, DeploymentType, Profile, ProfileCredentials};
pub use credential::{CredentialStorage, CredentialStore, EnvironmentOverrides};
pub use error::{ConfigError, Result};
