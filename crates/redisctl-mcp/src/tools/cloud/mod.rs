//! Redis Cloud API tools

mod account;
mod fixed;
mod networking;
mod provisioning;
mod raw;
mod subscriptions;

#[allow(unused_imports)]
pub use account::*;
#[allow(unused_imports)]
pub use fixed::*;
#[allow(unused_imports)]
pub use networking::*;
#[allow(unused_imports)]
pub use provisioning::*;
#[allow(unused_imports)]
pub use raw::*;
#[allow(unused_imports)]
pub use subscriptions::*;

use std::sync::Arc;

use tower_mcp::McpRouter;

use super::SubModule;
use crate::state::AppState;

/// Sub-modules within the Cloud toolset, each with its own tool names and router.
pub const SUB_MODULES: &[SubModule] = &[
    SubModule {
        name: "subscriptions",
        tool_names: subscriptions::TOOL_NAMES,
    },
    SubModule {
        name: "account",
        tool_names: account::TOOL_NAMES,
    },
    SubModule {
        name: "networking",
        tool_names: networking::TOOL_NAMES,
    },
    SubModule {
        name: "fixed",
        tool_names: fixed::TOOL_NAMES,
    },
    SubModule {
        name: "raw",
        tool_names: raw::TOOL_NAMES,
    },
    SubModule {
        name: "provisioning",
        tool_names: provisioning::TOOL_NAMES,
    },
];

/// Get all Cloud tool names as owned strings.
pub fn tool_names() -> Vec<String> {
    SUB_MODULES
        .iter()
        .flat_map(|sm| sm.tool_names.iter().map(|s| (*s).to_string()))
        .collect()
}

/// Get tool names for a specific sub-module by name.
pub fn sub_tool_names(name: &str) -> Option<&'static [&'static str]> {
    SUB_MODULES
        .iter()
        .find(|sm| sm.name == name)
        .map(|sm| sm.tool_names)
}

/// Build an MCP sub-router for a specific sub-module by name.
pub fn sub_router(name: &str, state: Arc<AppState>) -> Option<McpRouter> {
    match name {
        "subscriptions" => Some(subscriptions::router(state)),
        "account" => Some(account::router(state)),
        "networking" => Some(networking::router(state)),
        "fixed" => Some(fixed::router(state)),
        "raw" => Some(raw::router(state)),
        "provisioning" => Some(provisioning::router(state)),
        _ => None,
    }
}

/// Build an MCP sub-router containing all Cloud tools
pub fn router(state: Arc<AppState>) -> McpRouter {
    McpRouter::new()
        .merge(subscriptions::router(state.clone()))
        .merge(account::router(state.clone()))
        .merge(networking::router(state.clone()))
        .merge(fixed::router(state.clone()))
        .merge(raw::router(state.clone()))
        .merge(provisioning::router(state))
}
