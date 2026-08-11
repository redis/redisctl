//! MCP tools for Redis Cloud, Enterprise, and direct database operations

pub(crate) mod macros;

/// Metadata for a toolset sub-module (e.g. `cloud:subscriptions`).
///
/// Each sub-module within a toolset declares a static `TOOL_NAMES` array and
/// exposes a `router()` function. This struct lets the parent toolset `mod.rs`
/// advertise its sub-modules for selective CLI loading via `--tools cloud:subscriptions`.
#[allow(dead_code)]
pub struct SubModule {
    /// Sub-module name as used on the CLI (e.g. `"subscriptions"`).
    pub name: &'static str,
    /// Tool names registered by this sub-module.
    pub tool_names: &'static [&'static str],
}

#[cfg(any(feature = "cloud", feature = "enterprise"))]
use tower_mcp::ToolError;

#[cfg(feature = "cloud")]
pub mod cloud;
#[cfg(feature = "enterprise")]
pub mod enterprise;
pub mod profile;
#[cfg(feature = "database")]
pub mod redis;

/// Format a client creation error with structured remediation guidance for LLMs.
///
/// Inspects the error chain to identify common credential issues and provides
/// actionable MCP tool calls the LLM can use to diagnose and fix the problem.
#[cfg(any(feature = "cloud", feature = "enterprise"))]
pub fn credential_error(platform: &str, err: anyhow::Error) -> ToolError {
    let msg = format!("{:#}", err);
    let err_lower = msg.to_lowercase();

    let mut output = String::new();

    if err_lower.contains("no redisctl config available") {
        output.push_str("No profiles configured.\n\n");
        output.push_str("Suggested actions:\n");
        output.push_str(&format!(
            "- Call profile_create with profile_type='{}' to create a profile\n",
            platform
        ));
        output.push_str("- Call profile_path to check the config file location\n");
    } else if err_lower.contains("failed to resolve") && err_lower.contains("profile") {
        output.push_str(&format!("No {} profile available.\n\n", platform));
        output.push_str("Suggested actions:\n");
        output.push_str("- Call profile_list to see available profiles\n");
        output.push_str(&format!(
            "- Call profile_create with profile_type='{}' to create one\n",
            platform
        ));
        output.push_str("- Pass the 'profile' parameter to specify an existing profile by name\n");
    } else if err_lower.contains("not found") {
        output.push_str("The specified profile does not exist.\n\n");
        output.push_str("Suggested actions:\n");
        output.push_str("- Call profile_list to see available profiles\n");
        output.push_str(&format!(
            "- Call profile_create with profile_type='{}' to create a new profile\n",
            platform
        ));
    } else if err_lower.contains("no cloud credentials")
        || err_lower.contains("no enterprise credentials")
    {
        output.push_str(&format!(
            "The resolved profile does not have {} credentials.\n\n",
            platform
        ));
        output.push_str("Suggested actions:\n");
        output.push_str("- Call profile_list to find profiles of the correct type\n");
        output.push_str("- Pass the 'profile' parameter with a profile name of the correct type\n");
        output.push_str(&format!(
            "- Call profile_create with profile_type='{}' to create one\n",
            platform
        ));
    } else if err_lower.contains("credential") {
        output.push_str(
            "Credential resolution failed (keyring or environment variable lookup may have failed).\n\n",
        );
        output.push_str("Suggested actions:\n");
        output.push_str("- Call profile_show to inspect credential sources\n");
        output.push_str("- Call profile_validate with connect=true to diagnose issues\n");
    } else {
        output.push_str(&format!("Failed to initialize {} client.\n\n", platform));
        output.push_str("Suggested actions:\n");
        output.push_str("- Call profile_list to check configured profiles\n");
        output.push_str("- Call profile_validate with connect=true to test connectivity\n");
        output.push_str("- Call profile_show <name> to inspect a specific profile\n");
    }

    output.push_str(&format!("\nError details: {}", msg));

    ToolError::new(output)
}
