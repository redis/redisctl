//! Implementation of enterprise module commands

use crate::error::RedisCtlError;

use crate::cli::OutputFormat;
use crate::commands::enterprise::module::ModuleCommands;
use crate::commands::enterprise::utils::{
    DetailRow, extract_field, output_with_pager, resolve_auto, truncate_string,
};
use crate::connection::ConnectionManager;
use crate::error::Result as CliResult;
use anyhow::Context;
use redis_enterprise::ModuleHandler;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tabled::{Table, Tabled, settings::Style};

/// Module row for clean table display
#[derive(Tabled)]
struct ModuleRow {
    #[tabled(rename = "UID")]
    uid: String,
    #[tabled(rename = "MODULE")]
    module_name: String,
    #[tabled(rename = "VERSION")]
    version: String,
    #[tabled(rename = "DISPLAY")]
    display_name: String,
}

pub async fn handle_module_commands(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    cmd: &ModuleCommands,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    match cmd {
        ModuleCommands::List => handle_list(conn_mgr, profile_name, output_format, query).await,
        ModuleCommands::Get { uid, name } => {
            handle_get(
                conn_mgr,
                profile_name,
                uid.as_deref(),
                name.as_deref(),
                output_format,
                query,
            )
            .await
        }
        ModuleCommands::Upload { file } => {
            handle_upload(conn_mgr, profile_name, file, output_format, query).await
        }
        ModuleCommands::Delete { uid, force } => {
            handle_delete(conn_mgr, profile_name, uid, *force, output_format, query).await
        }
        ModuleCommands::ConfigBdb {
            bdb_uid,
            module_name,
            module_args,
            data,
        } => {
            handle_config_bdb(
                conn_mgr,
                profile_name,
                *bdb_uid,
                module_name.as_deref(),
                module_args.as_deref(),
                data.as_deref(),
                output_format,
                query,
            )
            .await
        }
        ModuleCommands::Validate { file, strict } => {
            handle_validate(file, *strict, output_format).await
        }
        ModuleCommands::Inspect { file, full } => handle_inspect(file, *full, output_format).await,
        ModuleCommands::Package {
            module,
            metadata,
            output_path,
            validate,
        } => handle_package(module, metadata, output_path, *validate, output_format).await,
    }
}

async fn handle_list(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = ModuleHandler::new(client);

    let modules = handler.list().await.map_err(RedisCtlError::from)?;

    let modules_json = serde_json::to_value(&modules)?;
    let output_data = if let Some(q) = query {
        crate::commands::enterprise::utils::apply_jmespath(&modules_json, q)?
    } else {
        modules_json
    };

    if matches!(resolve_auto(output_format), OutputFormat::Table) {
        print_modules_table(&output_data)?;
    } else {
        crate::commands::enterprise::utils::print_formatted_output(output_data, output_format)?;
    }
    Ok(())
}

async fn handle_get(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: Option<&str>,
    name: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = ModuleHandler::new(client);

    // Resolve module UID from name if provided
    let resolved_uid = if let Some(module_name) = name {
        let modules = handler.list().await.map_err(RedisCtlError::from)?;
        let matching: Vec<_> = modules
            .iter()
            .filter(|m| {
                m.module_name
                    .as_ref()
                    .map(|n| n.eq_ignore_ascii_case(module_name))
                    .unwrap_or(false)
            })
            .collect();

        match matching.len() {
            0 => {
                // No exact match - try partial match and suggest
                let partial_matches: Vec<_> = modules
                    .iter()
                    .filter(|m| {
                        m.module_name
                            .as_ref()
                            .map(|n| n.to_lowercase().contains(&module_name.to_lowercase()))
                            .unwrap_or(false)
                    })
                    .collect();

                if partial_matches.is_empty() {
                    return Err(anyhow::anyhow!(
                        "No module found with name '{}'. Use 'module list' to see available modules.",
                        module_name
                    )
                    .into());
                } else {
                    let suggestions: Vec<_> = partial_matches
                        .iter()
                        .filter_map(|m| m.module_name.as_deref())
                        .collect();
                    return Err(anyhow::anyhow!(
                        "No module found with name '{}'. Did you mean one of: {}?",
                        module_name,
                        suggestions.join(", ")
                    )
                    .into());
                }
            }
            1 => matching[0].uid.clone(),
            _ => {
                // Multiple matches - show versions and ask user to be specific
                let versions: Vec<_> = matching
                    .iter()
                    .map(|m| {
                        format!(
                            "{} (uid: {}, version: {})",
                            m.module_name.as_deref().unwrap_or("unknown"),
                            m.uid,
                            m.semantic_version.as_deref().unwrap_or("unknown")
                        )
                    })
                    .collect();
                return Err(anyhow::anyhow!(
                    "Multiple modules found with name '{}'. Please use --uid to specify:\n  {}",
                    module_name,
                    versions.join("\n  ")
                )
                .into());
            }
        }
    } else {
        uid.expect("Either uid or name must be provided")
            .to_string()
    };

    let module = handler
        .get(&resolved_uid)
        .await
        .map_err(RedisCtlError::from)?;

    let module_json = serde_json::to_value(&module)?;
    let output_data = if let Some(q) = query {
        crate::commands::enterprise::utils::apply_jmespath(&module_json, q)?
    } else {
        module_json
    };

    if matches!(resolve_auto(output_format), OutputFormat::Table) {
        print_module_detail(&output_data)?;
    } else {
        crate::commands::enterprise::utils::print_formatted_output(output_data, output_format)?;
    }
    Ok(())
}

/// Print modules in clean table format
fn print_modules_table(data: &Value) -> CliResult<()> {
    let modules = match data {
        Value::Array(arr) => arr.clone(),
        _ => {
            println!("No modules found");
            return Ok(());
        }
    };

    if modules.is_empty() {
        println!("No modules found");
        return Ok(());
    }

    let mut rows = Vec::new();
    for module in &modules {
        rows.push(ModuleRow {
            uid: extract_field(module, "uid", "-"),
            module_name: truncate_string(&extract_field(module, "module_name", "-"), 25),
            version: extract_field(
                module,
                "semantic_version",
                &extract_field(module, "version", "-"),
            ),
            display_name: truncate_string(&extract_field(module, "display_name", "-"), 25),
        });
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());
    Ok(())
}

/// Print module detail in key-value format
fn print_module_detail(data: &Value) -> CliResult<()> {
    let mut rows = Vec::new();

    let fields = [
        ("UID", "uid"),
        ("Module Name", "module_name"),
        ("Display Name", "display_name"),
        ("Version", "version"),
        ("Semantic Version", "semantic_version"),
        ("Min Redis Version", "min_redis_version"),
        ("Description", "description"),
        ("Author", "author"),
        ("Email", "email"),
        ("Homepage", "homepage"),
        ("License", "license"),
    ];

    for (label, key) in &fields {
        if let Some(val) = data.get(*key) {
            let display = match val {
                Value::Null => continue,
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                _ => val.to_string(),
            };
            rows.push(DetailRow {
                field: label.to_string(),
                value: display,
            });
        }
    }

    // Capabilities
    if let Some(caps) = data.get("capabilities").and_then(|v| v.as_array()) {
        let cap_strs: Vec<&str> = caps.iter().filter_map(|v| v.as_str()).collect();
        if !cap_strs.is_empty() {
            rows.push(DetailRow {
                field: "Capabilities".to_string(),
                value: cap_strs.join(", "),
            });
        }
    }

    // Commands count
    if let Some(cmds) = data.get("commands").and_then(|v| v.as_array()) {
        rows.push(DetailRow {
            field: "Commands".to_string(),
            value: cmds.len().to_string(),
        });
    }

    if rows.is_empty() {
        println!("No module information available");
        return Ok(());
    }

    let mut table = Table::new(&rows);
    table.with(Style::blank());
    output_with_pager(&table.to_string());
    Ok(())
}

async fn handle_upload(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    file: &str,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    // Handle @file syntax
    let file_path = if let Some(path) = file.strip_prefix('@') {
        path
    } else {
        file
    };

    // Check if file exists
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(anyhow::anyhow!("Module file not found: {}", file_path).into());
    }

    // Read file contents
    let module_data = fs::read(file_path)
        .with_context(|| format!("Failed to read module file: {}", file_path))?;

    // Get the filename for the upload
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("module.zip");

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = ModuleHandler::new(client);

    // Upload module using v2 API (returns action_uid for async tracking)
    let response = handler
        .upload(module_data, file_name)
        .await
        .map_err(RedisCtlError::from)?;

    // Check if response contains action_uid (v2 async operation)
    if response.get("action_uid").is_some() {
        println!("Module upload initiated. Response:");
        crate::commands::enterprise::utils::print_formatted_output(
            response.clone(),
            output_format,
        )?;

        // Note: Full async tracking would require polling /v1/actions/{action_uid}
        // For now, we just return the response with action_uid
        println!("\nNote: Module upload is processing. Use the action_uid to check status.");
    } else {
        // Direct response (shouldn't happen with v2, but handle it)
        let output_data = if let Some(q) = query {
            crate::commands::enterprise::utils::apply_jmespath(&response, q)?
        } else {
            response
        };
        crate::commands::enterprise::utils::print_formatted_output(output_data, output_format)?;
    }

    Ok(())
}

async fn handle_delete(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    uid: &str,
    force: bool,
    output_format: OutputFormat,
    _query: Option<&str>,
) -> CliResult<()> {
    // Confirm deletion if not forced
    if !force {
        let message = format!("Are you sure you want to delete module '{}'?", uid);
        crate::commands::enterprise::utils::confirm_action(&message)?;
    }

    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = ModuleHandler::new(client);

    handler.delete(uid).await.map_err(RedisCtlError::from)?;

    // Print success message
    let result = serde_json::json!({
        "status": "success",
        "message": format!("Module '{}' deleted successfully", uid)
    });

    crate::commands::enterprise::utils::print_formatted_output(result, output_format)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_config_bdb(
    conn_mgr: &ConnectionManager,
    profile_name: Option<&str>,
    bdb_uid: u32,
    module_name: Option<&str>,
    module_args: Option<&str>,
    data: Option<&str>,
    output_format: OutputFormat,
    query: Option<&str>,
) -> CliResult<()> {
    let client = conn_mgr.create_enterprise_client(profile_name).await?;
    let handler = ModuleHandler::new(client);

    // Start with JSON from --data if provided, otherwise empty object
    let mut config = if let Some(data_str) = data {
        crate::commands::enterprise::utils::read_json_data(data_str)?
    } else {
        serde_json::json!({})
    };

    let config_obj = config.as_object_mut().unwrap();

    // CLI parameters override JSON values
    if let Some(name) = module_name {
        config_obj.insert("module_name".to_string(), serde_json::json!(name));
    }
    if let Some(args) = module_args {
        config_obj.insert("module_args".to_string(), serde_json::json!(args));
    }

    let result = handler
        .config_bdb(bdb_uid, config)
        .await
        .map_err(RedisCtlError::from)?;

    let result_json = serde_json::to_value(&result)?;
    let output_data = if let Some(q) = query {
        crate::commands::enterprise::utils::apply_jmespath(&result_json, q)?
    } else {
        result_json
    };

    crate::commands::enterprise::utils::print_formatted_output(output_data, output_format)?;
    Ok(())
}

// ============================================================================
// Module metadata structures for validation, inspection, and packaging
// ============================================================================

/// Module command definition in module.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleCommand {
    /// Command name (e.g., "JMESPATH.QUERY")
    pub command_name: String,

    /// Command arity (negative means variable, e.g., -3 means at least 2 args)
    #[serde(default)]
    pub command_arity: i32,

    /// Position of the first key argument (1-indexed, 0 means no keys)
    #[serde(default)]
    pub first_key: i32,

    /// Position of the last key argument (1-indexed, 0 means no keys)
    #[serde(default)]
    pub last_key: i32,

    /// Key step (for commands with multiple keys)
    #[serde(default = "default_step")]
    pub step: i32,

    /// Command flags (e.g., ["readonly", "fast"])
    #[serde(default)]
    pub flags: Vec<String>,
}

fn default_step() -> i32 {
    1
}

/// Module metadata from module.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleMetadata {
    /// Module name (required)
    pub module_name: String,

    /// Display name for UI
    #[serde(default)]
    pub display_name: Option<String>,

    /// Numeric version (e.g., 300 for 0.3.0)
    #[serde(default)]
    pub version: Option<u32>,

    /// Semantic version string (e.g., "0.3.0")
    #[serde(default)]
    pub semantic_version: Option<String>,

    /// Minimum Redis version required
    #[serde(default)]
    pub min_redis_version: Option<String>,

    /// Compatible Redis version (important for RE8 upgrade tests)
    #[serde(default)]
    pub compatible_redis_version: Option<String>,

    /// Module author
    #[serde(default)]
    pub author: Option<String>,

    /// Author email
    #[serde(default)]
    pub email: Option<String>,

    /// Module description
    #[serde(default)]
    pub description: Option<String>,

    /// Homepage URL
    #[serde(default)]
    pub homepage: Option<String>,

    /// License
    #[serde(default)]
    pub license: Option<String>,

    /// Command line arguments for module loading
    #[serde(default)]
    pub command_line_args: Option<String>,

    /// Module capabilities
    #[serde(default)]
    pub capabilities: Vec<String>,

    /// Module commands
    #[serde(default)]
    pub commands: Vec<ModuleCommand>,
}

/// Validation result for a single field
#[derive(Debug)]
struct ValidationResult {
    field: String,
    status: ValidationStatus,
    message: String,
}

#[derive(Debug, PartialEq)]
enum ValidationStatus {
    Ok,
    Warning,
    Error,
}

impl ValidationResult {
    fn ok(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            status: ValidationStatus::Ok,
            message: message.to_string(),
        }
    }

    fn warning(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            status: ValidationStatus::Warning,
            message: message.to_string(),
        }
    }

    fn error(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            status: ValidationStatus::Error,
            message: message.to_string(),
        }
    }

    fn symbol(&self) -> &str {
        match self.status {
            ValidationStatus::Ok => "v",
            ValidationStatus::Warning => "!",
            ValidationStatus::Error => "x",
        }
    }
}

/// Validate module metadata
fn validate_module_metadata(metadata: &ModuleMetadata, strict: bool) -> Vec<ValidationResult> {
    let mut results = Vec::new();

    // Required fields
    if metadata.module_name.is_empty() {
        results.push(ValidationResult::error(
            "module_name",
            "Module name is required",
        ));
    } else {
        results.push(ValidationResult::ok("module_name", &metadata.module_name));
    }

    // Version fields
    if let Some(version) = metadata.version {
        results.push(ValidationResult::ok("version", &version.to_string()));
    } else if strict {
        results.push(ValidationResult::error(
            "version",
            "Numeric version is required in strict mode",
        ));
    } else {
        results.push(ValidationResult::warning(
            "version",
            "Numeric version not specified (recommended)",
        ));
    }

    if let Some(ref sv) = metadata.semantic_version {
        results.push(ValidationResult::ok("semantic_version", sv));
    } else if strict {
        results.push(ValidationResult::error(
            "semantic_version",
            "Semantic version is required in strict mode",
        ));
    } else {
        results.push(ValidationResult::warning(
            "semantic_version",
            "Semantic version not specified (recommended)",
        ));
    }

    // Redis version compatibility
    if let Some(ref min_ver) = metadata.min_redis_version {
        results.push(ValidationResult::ok("min_redis_version", min_ver));
    } else if strict {
        results.push(ValidationResult::error(
            "min_redis_version",
            "Minimum Redis version is required in strict mode",
        ));
    } else {
        results.push(ValidationResult::warning(
            "min_redis_version",
            "Minimum Redis version not specified (recommended)",
        ));
    }

    if let Some(ref compat_ver) = metadata.compatible_redis_version {
        results.push(ValidationResult::ok("compatible_redis_version", compat_ver));
    } else {
        // This is important for RE8 upgrade tests
        results.push(ValidationResult::warning(
            "compatible_redis_version",
            "Compatible Redis version not specified (required for RE8 upgrade tests)",
        ));
    }

    // Commands
    if metadata.commands.is_empty() {
        if strict {
            results.push(ValidationResult::error(
                "commands",
                "No commands defined (required in strict mode)",
            ));
        } else {
            results.push(ValidationResult::warning("commands", "No commands defined"));
        }
    } else {
        results.push(ValidationResult::ok(
            "commands",
            &format!("{} commands defined", metadata.commands.len()),
        ));

        // Validate individual commands
        for cmd in &metadata.commands {
            if cmd.command_name.is_empty() {
                results.push(ValidationResult::error(
                    "commands",
                    "Command with empty name found",
                ));
            }
        }
    }

    // Capabilities
    if metadata.capabilities.is_empty() {
        results.push(ValidationResult::warning(
            "capabilities",
            "No capabilities defined",
        ));
    } else {
        results.push(ValidationResult::ok(
            "capabilities",
            &format!("{} capabilities", metadata.capabilities.len()),
        ));
    }

    // Optional but recommended fields in strict mode
    if strict {
        if metadata.display_name.is_none() {
            results.push(ValidationResult::warning(
                "display_name",
                "Display name not specified",
            ));
        }
        if metadata.description.is_none() {
            results.push(ValidationResult::warning(
                "description",
                "Description not specified",
            ));
        }
        if metadata.author.is_none() {
            results.push(ValidationResult::warning("author", "Author not specified"));
        }
        if metadata.license.is_none() {
            results.push(ValidationResult::warning(
                "license",
                "License not specified",
            ));
        }
    }

    results
}

async fn handle_validate(file: &Path, strict: bool, output_format: OutputFormat) -> CliResult<()> {
    // Check file exists
    if !file.exists() {
        return Err(anyhow::anyhow!("File not found: {}", file.display()).into());
    }

    // Read and parse module.json
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    let metadata: ModuleMetadata = serde_json::from_str(&content)
        .with_context(|| "Failed to parse module.json: invalid JSON format")?;

    // Validate
    let results = validate_module_metadata(&metadata, strict);

    // Check for errors
    let has_errors = results.iter().any(|r| r.status == ValidationStatus::Error);
    let has_warnings = results
        .iter()
        .any(|r| r.status == ValidationStatus::Warning);

    // Output based on format
    match output_format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "file": file.display().to_string(),
                "valid": !has_errors,
                "strict": strict,
                "module_name": metadata.module_name,
                "version": metadata.version,
                "semantic_version": metadata.semantic_version,
                "commands_count": metadata.commands.len(),
                "capabilities_count": metadata.capabilities.len(),
                "results": results.iter().map(|r| {
                    serde_json::json!({
                        "field": r.field,
                        "status": format!("{:?}", r.status).to_lowercase(),
                        "message": r.message
                    })
                }).collect::<Vec<_>>()
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            println!("Validating: {}\n", file.display());

            for result in &results {
                println!("  {} {}: {}", result.symbol(), result.field, result.message);
            }

            println!();
            if has_errors {
                println!("x Module metadata has validation errors");
            } else if has_warnings {
                println!(
                    "! Module metadata is valid with warnings{}",
                    if strict { " (strict mode)" } else { "" }
                );
            } else {
                println!(
                    "v Module metadata is valid for Redis Enterprise 8.x{}",
                    if strict { " (strict mode)" } else { "" }
                );
            }
        }
    }

    if has_errors {
        Err(anyhow::anyhow!("Validation failed").into())
    } else {
        Ok(())
    }
}

async fn handle_inspect(file: &Path, verbose: bool, output_format: OutputFormat) -> CliResult<()> {
    // Check file exists
    if !file.exists() {
        return Err(anyhow::anyhow!("File not found: {}", file.display()).into());
    }

    // Open zip file
    let zip_file =
        fs::File::open(file).with_context(|| format!("Failed to open file: {}", file.display()))?;

    let mut archive = zip::ZipArchive::new(zip_file)
        .with_context(|| format!("Failed to read zip archive: {}", file.display()))?;

    // Collect file info
    let mut files_info: Vec<(String, u64)> = Vec::new();
    let mut module_json_content: Option<String> = None;
    let mut has_so_file = false;
    let mut so_file_name: Option<String> = None;

    for i in 0..archive.len() {
        let mut zip_entry = archive
            .by_index(i)
            .with_context(|| format!("Failed to read zip entry {}", i))?;
        let name = zip_entry.name().to_string();
        let size = zip_entry.size();

        files_info.push((name.clone(), size));

        if name == "module.json" {
            let mut content = String::new();
            zip_entry.read_to_string(&mut content)?;
            module_json_content = Some(content);
        }

        if name.ends_with(".so") {
            has_so_file = true;
            so_file_name = Some(name);
        }
    }

    // Check for flat structure (files at root)
    let has_subdirs = files_info.iter().any(|(name, _)| name.contains('/'));

    // Parse module.json if found
    let metadata: Option<ModuleMetadata> = module_json_content
        .as_ref()
        .and_then(|content| serde_json::from_str(content).ok());

    // Determine validity
    let is_valid = module_json_content.is_some() && has_so_file && !has_subdirs;

    match output_format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "file": file.display().to_string(),
                "valid": is_valid,
                "files": files_info.iter().map(|(name, size)| {
                    serde_json::json!({
                        "name": name,
                        "size": size
                    })
                }).collect::<Vec<_>>(),
                "has_module_json": module_json_content.is_some(),
                "has_so_file": has_so_file,
                "so_file": so_file_name,
                "has_subdirectories": has_subdirs,
                "metadata": metadata.as_ref().map(|m| serde_json::json!({
                    "module_name": m.module_name,
                    "display_name": m.display_name,
                    "version": m.version,
                    "semantic_version": m.semantic_version,
                    "min_redis_version": m.min_redis_version,
                    "compatible_redis_version": m.compatible_redis_version,
                    "commands_count": m.commands.len(),
                    "capabilities": m.capabilities
                }))
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            let filename = file
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            println!("Package: {}\n", filename);

            println!("Files:");
            for (name, size) in &files_info {
                println!("  {} ({})", name, format_size(*size));
            }

            if let Some(ref m) = metadata {
                println!("\nMetadata:");
                println!("  Name: {}", m.module_name);
                if let Some(ref dn) = m.display_name {
                    println!("  Display: {}", dn);
                }
                if let (Some(sv), Some(v)) = (&m.semantic_version, m.version) {
                    println!("  Version: {} ({})", sv, v);
                } else if let Some(ref sv) = m.semantic_version {
                    println!("  Version: {}", sv);
                } else if let Some(v) = m.version {
                    println!("  Version: {}", v);
                }
                if let Some(ref min_ver) = m.min_redis_version {
                    println!("  Min Redis: {}", min_ver);
                }
                if let Some(ref compat_ver) = m.compatible_redis_version {
                    println!("  Compatible: {}", compat_ver);
                }
                println!("  Commands: {}", m.commands.len());
                if !m.capabilities.is_empty() {
                    if verbose {
                        println!("  Capabilities: {}", m.capabilities.join(", "));
                    } else {
                        let display_caps: Vec<_> = m.capabilities.iter().take(5).collect();
                        let suffix = if m.capabilities.len() > 5 {
                            format!(", ... ({} total)", m.capabilities.len())
                        } else {
                            String::new()
                        };
                        println!(
                            "  Capabilities: {}{}",
                            display_caps
                                .iter()
                                .map(|s| s.as_str())
                                .collect::<Vec<_>>()
                                .join(", "),
                            suffix
                        );
                    }
                }

                if verbose && !m.commands.is_empty() {
                    println!("\nCommands:");
                    for cmd in &m.commands {
                        println!(
                            "  {} (arity: {}, keys: {}-{}, flags: [{}])",
                            cmd.command_name,
                            cmd.command_arity,
                            cmd.first_key,
                            cmd.last_key,
                            cmd.flags.join(", ")
                        );
                    }
                }
            } else if module_json_content.is_some() {
                println!("\nMetadata: Failed to parse module.json");
            } else {
                println!("\nMetadata: module.json not found");
            }

            println!();
            if is_valid {
                println!("v Package structure is valid for RE8 user_defined_modules");
            } else {
                println!("x Package structure is INVALID:");
                if module_json_content.is_none() {
                    println!("  - Missing module.json at root");
                }
                if !has_so_file {
                    println!("  - Missing .so module binary");
                }
                if has_subdirs {
                    println!("  - Contains subdirectories (files must be at zip root)");
                }
            }
        }
    }

    Ok(())
}

async fn handle_package(
    module_path: &Path,
    metadata_path: &Path,
    output_path: &Path,
    validate: bool,
    output_format: OutputFormat,
) -> CliResult<()> {
    // Check input files exist
    if !module_path.exists() {
        return Err(anyhow::anyhow!("Module file not found: {}", module_path.display()).into());
    }
    if !metadata_path.exists() {
        return Err(anyhow::anyhow!("Metadata file not found: {}", metadata_path.display()).into());
    }

    // Validate if requested
    if validate {
        let content = fs::read_to_string(metadata_path)
            .with_context(|| format!("Failed to read metadata: {}", metadata_path.display()))?;

        let metadata: ModuleMetadata = serde_json::from_str(&content)
            .with_context(|| "Failed to parse module.json: invalid JSON format")?;

        let results = validate_module_metadata(&metadata, false);
        let has_errors = results.iter().any(|r| r.status == ValidationStatus::Error);

        if has_errors {
            println!("Validation failed:");
            for result in results
                .iter()
                .filter(|r| r.status == ValidationStatus::Error)
            {
                println!("  x {}: {}", result.field, result.message);
            }
            return Err(anyhow::anyhow!("Module metadata validation failed").into());
        }
    }

    // Create output directory if needed
    if let Some(parent) = output_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Read input files
    let module_data = fs::read(module_path)
        .with_context(|| format!("Failed to read module: {}", module_path.display()))?;
    let metadata_data = fs::read(metadata_path)
        .with_context(|| format!("Failed to read metadata: {}", metadata_path.display()))?;

    // Get filenames for zip entries
    let module_filename = module_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("module.so");

    // Create zip file
    let zip_file = fs::File::create(output_path)
        .with_context(|| format!("Failed to create output file: {}", output_path.display()))?;

    let mut zip = zip::ZipWriter::new(zip_file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    // Add module.json first (convention)
    zip.start_file("module.json", options)
        .with_context(|| "Failed to add module.json to zip")?;
    zip.write_all(&metadata_data)?;

    // Add module binary
    zip.start_file(module_filename, options)
        .with_context(|| format!("Failed to add {} to zip", module_filename))?;
    zip.write_all(&module_data)?;

    zip.finish()
        .with_context(|| "Failed to finalize zip file")?;

    // Calculate output size
    let output_size = fs::metadata(output_path)?.len();

    match output_format {
        OutputFormat::Json => {
            let output = serde_json::json!({
                "status": "success",
                "output": output_path.display().to_string(),
                "size": output_size,
                "files": ["module.json", module_filename]
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            println!("v Package created: {}", output_path.display());
            println!("  Size: {}", format_size(output_size));
            println!("  Contents: module.json, {}", module_filename);
        }
    }

    Ok(())
}

/// Format file size in human-readable format
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;

    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}
