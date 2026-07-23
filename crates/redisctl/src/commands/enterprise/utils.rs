//! Utility functions for Enterprise commands
use crate::error::Result as CliResult;
use serde_json::Value;

pub use crate::commands::cloud::utils::{
    DetailRow, confirm_action, extract_field, format_memory_size, format_status, output_with_pager,
    truncate_string,
};
pub use crate::output::{apply_jmespath, handle_output, print_formatted_output, resolve_auto};

/// Read JSON data from string, file, or stdin
pub fn read_json_data(data: &str) -> CliResult<Value> {
    let json_str = if data == "-" {
        // Read from stdin
        use std::io::Read;
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| anyhow::anyhow!("Failed to read from stdin: {}", e))?;
        buffer
    } else if let Some(file_path) = data.strip_prefix('@') {
        // Read from file
        std::fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Failed to read file {}: {}", file_path, e))?
    } else {
        // Direct JSON string
        data.to_string()
    };

    serde_json::from_str(&json_str).map_err(|e| anyhow::anyhow!("Invalid JSON: {}", e).into())
}

/// Format byte count as human-readable memory size.
///
/// The RE API returns memory in bytes; this converts to GB and delegates
/// to `format_memory_size` for display.
pub fn format_bytes(bytes: u64) -> String {
    format_memory_size(bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}
