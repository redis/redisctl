//! Cloud command implementations
//!
//! This module contains all cloud-specific command handlers organized into submodules:
//! - `account`: Account management commands
//! - `subscription`: Subscription management commands
//! - `user`: User management commands
//! - `database`: Database management commands
//! - `cost_report`: Cost report generation and download
//! - `utils`: Shared utilities and helper functions

pub mod account;
pub mod acl;
pub mod acl_impl;
pub mod async_utils;
pub mod auth;
pub mod cloud_account;
pub mod cloud_account_impl;
pub mod connectivity;
pub mod cost_report;
pub mod database;
pub mod database_impl;
pub mod fixed_database;
pub mod fixed_subscription;
pub mod payment_method;
pub mod subscription;
pub mod subscription_impl;
pub mod task;
pub mod user;
pub mod utils;

// Re-export all handler functions for backward compatibility
#[allow(unused_imports)]
pub use account::handle_account_command;
#[allow(unused_imports)]
pub use connectivity::handle_connectivity_command;
#[allow(unused_imports)]
pub use cost_report::handle_cost_report_command;
#[allow(unused_imports)]
pub use database::handle_database_command;
#[allow(unused_imports)]
pub use payment_method::handle_payment_method_command;
#[allow(unused_imports)]
pub use subscription::handle_subscription_command;
#[allow(unused_imports)]
pub use user::handle_user_command;
