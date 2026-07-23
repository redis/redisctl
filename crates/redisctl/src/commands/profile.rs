//! Profile management command implementations

#![allow(dead_code)] // Functions called from bin target

use crate::cli::{OutputFormat, ProfileCommands};
use crate::connection::ConnectionManager;
use crate::error::RedisCtlError;
use crate::output;
use anyhow::Context;
use colored::Colorize;
use redisctl_core::Config;
use serde::Serialize;
use std::time::{Duration, Instant};
use tracing::{debug, info, trace};

/// Handle profile management commands
pub async fn handle_profile_command(
    profile_cmd: &ProfileCommands,
    conn_mgr: &ConnectionManager,
    output_format: OutputFormat,
) -> Result<(), RedisCtlError> {
    use ProfileCommands::*;

    match profile_cmd {
        List { tags } => handle_list(conn_mgr, output_format, tags).await,
        Path => handle_path(output_format).await,
        Current { r#type } => handle_current(conn_mgr, r#type).await,
        Show { name } => handle_show(conn_mgr, name, output_format).await,
        Set {
            name,
            r#type,
            api_key,
            api_secret,
            api_url,
            url,
            username,
            password,
            insecure,
            ca_cert,
            host,
            port,
            no_tls,
            db,
            #[cfg(feature = "secure-storage")]
            use_keyring,
            tags,
        } => {
            handle_set(
                conn_mgr,
                name,
                r#type,
                api_key,
                api_secret,
                api_url,
                url,
                username,
                password,
                insecure,
                ca_cert,
                host,
                port,
                no_tls,
                db,
                #[cfg(feature = "secure-storage")]
                use_keyring,
                tags,
            )
            .await
        }
        Remove { name } => handle_remove(conn_mgr, name).await,
        DefaultEnterprise { name } => handle_default_enterprise(conn_mgr, name).await,
        DefaultCloud { name } => handle_default_cloud(conn_mgr, name).await,
        DefaultDatabase { name } => handle_default_database(conn_mgr, name).await,
        Validate { connect } => handle_validate(conn_mgr, *connect, output_format).await,
        Init => handle_init(conn_mgr).await,
    }
}

async fn handle_list(
    conn_mgr: &ConnectionManager,
    output_format: OutputFormat,
    tag_filter: &[String],
) -> Result<(), RedisCtlError> {
    debug!("Listing all configured profiles");
    let all_profiles = conn_mgr.config.list_profiles();
    let profiles: Vec<_> = if tag_filter.is_empty() {
        all_profiles
    } else {
        all_profiles
            .into_iter()
            .filter(|(_, profile)| profile.tags.iter().any(|t| tag_filter.contains(t)))
            .collect()
    };
    trace!("Found {} profiles", profiles.len());

    match output_format {
        OutputFormat::Json | OutputFormat::Yaml => {
            let config_path = conn_mgr
                .config_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| {
                    Config::config_path()
                        .ok()
                        .and_then(|p| p.to_str().map(String::from))
                });

            let profile_list: Vec<serde_json::Value> = profiles
                .iter()
                .map(|(name, profile)| {
                    let is_default_enterprise =
                        conn_mgr.config.default_enterprise.as_deref() == Some(name);
                    let is_default_cloud = conn_mgr.config.default_cloud.as_deref() == Some(name);

                    let mut obj = serde_json::json!({
                        "name": name,
                        "deployment_type": profile.deployment_type.to_string(),
                        "is_default_enterprise": is_default_enterprise,
                        "is_default_cloud": is_default_cloud,
                    });

                    if !profile.tags.is_empty() {
                        obj["tags"] = serde_json::json!(&profile.tags);
                    }

                    match profile.deployment_type {
                        redisctl_core::DeploymentType::Cloud => {
                            if let Some((_, _, url)) = profile.cloud_credentials() {
                                obj["api_url"] = serde_json::json!(url);
                            }
                        }
                        redisctl_core::DeploymentType::Enterprise => {
                            if let Some((url, username, _, insecure, ca_cert)) =
                                profile.enterprise_credentials()
                            {
                                obj["url"] = serde_json::json!(url);
                                obj["username"] = serde_json::json!(username);
                                obj["insecure"] = serde_json::json!(insecure);
                                if let Some(cert_path) = ca_cert {
                                    obj["ca_cert"] = serde_json::json!(cert_path);
                                }
                            }
                        }
                        redisctl_core::DeploymentType::Database => {
                            if let Some((host, port, _, tls, username, database)) =
                                profile.database_credentials()
                            {
                                obj["host"] = serde_json::json!(host);
                                obj["port"] = serde_json::json!(port);
                                obj["tls"] = serde_json::json!(tls);
                                obj["username"] = serde_json::json!(username);
                                obj["database"] = serde_json::json!(database);
                            }
                        }
                    }

                    obj
                })
                .collect();

            let output_data = serde_json::json!({
                "config_path": config_path,
                "profiles": profile_list,
                "count": profiles.len()
            });

            output::print_output(&output_data, output_format, None)?;
        }
        _ => {
            // Show config file path at the top
            if let Some(ref path) = conn_mgr.config_path {
                println!("Configuration file: {}", path.display());
                println!();
            } else if let Ok(config_path) = Config::config_path() {
                println!("Configuration file: {}", config_path.display());
                println!();
            }

            if profiles.is_empty() {
                info!("No profiles configured");
                println!("No profiles configured.");
                println!("Use 'redisctl profile set' to create a profile.");
                return Ok(());
            }

            // Group profiles by deployment type
            let mut cloud_profiles = Vec::new();
            let mut enterprise_profiles = Vec::new();
            let mut database_profiles = Vec::new();

            for (name, profile) in &profiles {
                match profile.deployment_type {
                    redisctl_core::DeploymentType::Cloud => cloud_profiles.push((*name, *profile)),
                    redisctl_core::DeploymentType::Enterprise => {
                        enterprise_profiles.push((*name, *profile))
                    }
                    redisctl_core::DeploymentType::Database => {
                        database_profiles.push((*name, *profile))
                    }
                }
            }

            let print_section = |header: &str,
                                 group: &[(&String, &redisctl_core::Profile)],
                                 default_name: Option<&str>,
                                 first: &mut bool| {
                if group.is_empty() {
                    return;
                }
                if !*first {
                    println!();
                }
                *first = false;
                println!("{}", header.bold());
                for (name, profile) in group {
                    let tag_suffix = if profile.tags.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", profile.tags.join(", "))
                            .dimmed()
                            .to_string()
                    };
                    if default_name == Some(name.as_str()) {
                        println!(
                            "  {} {}{}",
                            name.bold().cyan(),
                            "(default)".green(),
                            tag_suffix
                        );
                    } else {
                        println!("  {}{}", name.bold().cyan(), tag_suffix);
                    }
                    match profile.deployment_type {
                        redisctl_core::DeploymentType::Cloud => {
                            if let Some((_, _, url)) = profile.cloud_credentials() {
                                println!("    {} {}", "URL:".dimmed(), url);
                            }
                        }
                        redisctl_core::DeploymentType::Enterprise => {
                            if let Some((url, username, _, insecure, _ca_cert)) =
                                profile.enterprise_credentials()
                            {
                                println!("    {}  {}", "URL:".dimmed(), url);
                                println!(
                                    "    {} {}{}",
                                    "User:".dimmed(),
                                    username,
                                    if insecure { " (insecure)" } else { "" }
                                );
                            }
                        }
                        redisctl_core::DeploymentType::Database => {
                            if let Some((host, port, _, tls, _, _)) = profile.database_credentials()
                            {
                                println!(
                                    "    {} {}:{} {}",
                                    "Host:".dimmed(),
                                    host,
                                    port,
                                    if tls { "(TLS)" } else { "(no TLS)" }
                                );
                            }
                        }
                    }
                }
            };

            let mut first = true;
            print_section(
                "Cloud",
                &cloud_profiles,
                conn_mgr.config.default_cloud.as_deref(),
                &mut first,
            );
            print_section(
                "Enterprise",
                &enterprise_profiles,
                conn_mgr.config.default_enterprise.as_deref(),
                &mut first,
            );
            print_section(
                "Database",
                &database_profiles,
                conn_mgr.config.default_database.as_deref(),
                &mut first,
            );
        }
    }

    Ok(())
}

async fn handle_path(output_format: OutputFormat) -> Result<(), RedisCtlError> {
    let config_path = Config::config_path()?;

    match output_format {
        OutputFormat::Json | OutputFormat::Yaml => {
            let output_data = serde_json::json!({
                "config_path": config_path.to_str()
            });

            output::print_output(&output_data, output_format, None)?;
        }
        _ => {
            println!("{}", config_path.display());
        }
    }
    Ok(())
}

async fn handle_current(
    conn_mgr: &ConnectionManager,
    deployment_type: &redisctl_core::DeploymentType,
) -> Result<(), RedisCtlError> {
    let resolved = match deployment_type {
        redisctl_core::DeploymentType::Cloud => conn_mgr.config.resolve_cloud_profile(None)?,
        redisctl_core::DeploymentType::Enterprise => {
            conn_mgr.config.resolve_enterprise_profile(None)?
        }
        redisctl_core::DeploymentType::Database => {
            conn_mgr.config.resolve_database_profile(None)?
        }
    };
    println!("{}", resolved);
    Ok(())
}

async fn handle_show(
    conn_mgr: &ConnectionManager,
    name: &str,
    output_format: OutputFormat,
) -> Result<(), RedisCtlError> {
    match conn_mgr.config.profiles.get(name) {
        Some(profile) => {
            let is_default_enterprise = conn_mgr.config.default_enterprise.as_deref() == Some(name);
            let is_default_cloud = conn_mgr.config.default_cloud.as_deref() == Some(name);

            match output_format {
                OutputFormat::Json | OutputFormat::Yaml => {
                    let mut output_data = serde_json::json!({
                        "name": name,
                        "deployment_type": profile.deployment_type.to_string(),
                        "is_default_enterprise": is_default_enterprise,
                        "is_default_cloud": is_default_cloud,
                    });

                    if !profile.tags.is_empty() {
                        output_data["tags"] = serde_json::json!(&profile.tags);
                    }

                    match profile.deployment_type {
                        redisctl_core::DeploymentType::Cloud => {
                            if let Some((api_key, _, api_url)) = profile.cloud_credentials() {
                                output_data["api_key_preview"] = serde_json::json!(format!(
                                    "{}...",
                                    &api_key[..std::cmp::min(8, api_key.len())]
                                ));
                                output_data["api_url"] = serde_json::json!(api_url);
                            }
                        }
                        redisctl_core::DeploymentType::Enterprise => {
                            if let Some((url, username, has_password, insecure, ca_cert)) =
                                profile.enterprise_credentials()
                            {
                                output_data["url"] = serde_json::json!(url);
                                output_data["username"] = serde_json::json!(username);
                                output_data["password_configured"] =
                                    serde_json::json!(has_password.is_some());
                                output_data["insecure"] = serde_json::json!(insecure);
                                if let Some(cert_path) = ca_cert {
                                    output_data["ca_cert"] = serde_json::json!(cert_path);
                                }
                            }
                        }
                        redisctl_core::DeploymentType::Database => {
                            if let Some((host, port, has_password, tls, username, database)) =
                                profile.database_credentials()
                            {
                                output_data["host"] = serde_json::json!(host);
                                output_data["port"] = serde_json::json!(port);
                                output_data["password_configured"] =
                                    serde_json::json!(has_password.is_some());
                                output_data["tls"] = serde_json::json!(tls);
                                output_data["username"] = serde_json::json!(username);
                                output_data["database"] = serde_json::json!(database);
                            }
                        }
                    }

                    let fmt = match output_format {
                        OutputFormat::Json => output::OutputFormat::Json,
                        OutputFormat::Yaml => output::OutputFormat::Yaml,
                        _ => output::OutputFormat::Json,
                    };

                    output::print_output(&output_data, fmt, None)?;
                }
                _ => {
                    println!("Profile: {}", name);
                    println!("Type: {}", profile.deployment_type);
                    if !profile.tags.is_empty() {
                        println!("Tags: {}", profile.tags.join(", "));
                    }

                    match profile.deployment_type {
                        redisctl_core::DeploymentType::Cloud => {
                            if let Some((api_key, _, api_url)) = profile.cloud_credentials() {
                                println!(
                                    "API Key: {}...",
                                    &api_key[..std::cmp::min(8, api_key.len())]
                                );
                                println!("API URL: {}", api_url);
                            }
                        }
                        redisctl_core::DeploymentType::Enterprise => {
                            if let Some((url, username, has_password, insecure, ca_cert)) =
                                profile.enterprise_credentials()
                            {
                                println!("URL: {}", url);
                                println!("Username: {}", username);
                                println!(
                                    "Password: {}",
                                    if has_password.is_some() {
                                        "configured"
                                    } else {
                                        "not set"
                                    }
                                );
                                println!("Insecure: {}", insecure);
                                if let Some(cert_path) = ca_cert {
                                    println!("CA Cert: {}", cert_path);
                                }
                            }
                        }
                        redisctl_core::DeploymentType::Database => {
                            if let Some((host, port, has_password, tls, username, database)) =
                                profile.database_credentials()
                            {
                                println!("Host: {}", host);
                                println!("Port: {}", port);
                                println!("Username: {}", username);
                                println!(
                                    "Password: {}",
                                    if has_password.is_some() {
                                        "configured"
                                    } else {
                                        "not set"
                                    }
                                );
                                println!("TLS: {}", tls);
                                println!("Database: {}", database);
                            }
                        }
                    }

                    if is_default_enterprise {
                        println!("Default for enterprise: yes");
                    }
                    if is_default_cloud {
                        println!("Default for cloud: yes");
                    }
                    if conn_mgr.config.default_database.as_deref() == Some(name) {
                        println!("Default for database: yes");
                    }
                }
            }

            Ok(())
        }
        None => Err(RedisCtlError::ProfileNotFound { name: name.into() }),
    }
}

#[allow(clippy::too_many_arguments)]
async fn handle_set(
    conn_mgr: &ConnectionManager,
    name: &str,
    deployment: &redisctl_core::DeploymentType,
    api_key: &Option<String>,
    api_secret: &Option<String>,
    api_url: &str,
    url: &Option<String>,
    username: &Option<String>,
    password: &Option<String>,
    insecure: &bool,
    ca_cert: &Option<String>,
    host: &Option<String>,
    port: &Option<u16>,
    no_tls: &bool,
    db: &Option<u8>,
    #[cfg(feature = "secure-storage")] use_keyring: &bool,
    tags: &[String],
) -> Result<(), RedisCtlError> {
    debug!("Setting profile: {}", name);

    // Check if profile already exists
    if conn_mgr.config.profiles.contains_key(name) {
        // Ask for confirmation before updating
        println!(
            "Profile '{}' already exists. Credentials will be updated (other settings preserved).",
            name
        );
        print!("Continue? (y/N): ");
        use std::io::{self, Write};
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("Profile update cancelled.");
            return Ok(());
        }
    }

    // Determine effective tags: use provided tags, or preserve existing if none given
    let effective_tags = if tags.is_empty() {
        conn_mgr
            .config
            .profiles
            .get(name)
            .map(|p| p.tags.clone())
            .unwrap_or_default()
    } else {
        tags.to_vec()
    };

    // Create the profile based on deployment type
    let profile = match deployment {
        redisctl_core::DeploymentType::Cloud => {
            let api_key = api_key
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API key is required for Cloud profiles"))?;
            let api_secret = api_secret
                .clone()
                .ok_or_else(|| anyhow::anyhow!("API secret is required for Cloud profiles"))?;

            // Handle keyring storage if requested
            #[cfg(feature = "secure-storage")]
            let (stored_key, stored_secret) = if *use_keyring {
                use redisctl_core::CredentialStore;
                let store = CredentialStore::new();

                // Store credentials in keyring and get references
                let key_ref = store
                    .store_credential(&format!("{}-api-key", name), &api_key)
                    .context("Failed to store API key in keyring")?;
                let secret_ref = store
                    .store_credential(&format!("{}-api-secret", name), &api_secret)
                    .context("Failed to store API secret in keyring")?;

                println!("Credentials stored securely in OS keyring");
                (key_ref, secret_ref)
            } else {
                (api_key.clone(), api_secret.clone())
            };

            #[cfg(not(feature = "secure-storage"))]
            let (stored_key, stored_secret) = (api_key.clone(), api_secret.clone());

            redisctl_core::Profile {
                deployment_type: redisctl_core::DeploymentType::Cloud,
                credentials: redisctl_core::ProfileCredentials::Cloud {
                    api_key: stored_key,
                    api_secret: stored_secret,
                    api_url: api_url.to_string(),
                },
                files_api_key: None,
                tags: effective_tags.clone(),
            }
        }
        redisctl_core::DeploymentType::Enterprise => {
            let url = url
                .clone()
                .ok_or_else(|| anyhow::anyhow!("URL is required for Enterprise profiles"))?;
            let username = username
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Username is required for Enterprise profiles"))?;

            // Prompt for password if not provided
            let password = match password {
                Some(p) => Some(p.clone()),
                None => {
                    let pass = rpassword::prompt_password("Enter password: ")
                        .context("Failed to read password")?;
                    Some(pass)
                }
            };

            // Handle keyring storage if requested
            #[cfg(feature = "secure-storage")]
            let (stored_username, stored_password) = if *use_keyring {
                use redisctl_core::CredentialStore;
                let store = CredentialStore::new();

                // Store credentials in keyring and get references
                let user_ref = store
                    .store_credential(&format!("{}-username", name), &username)
                    .context("Failed to store username in keyring")?;

                let pass_ref = if let Some(ref p) = password {
                    Some(
                        store
                            .store_credential(&format!("{}-password", name), p)
                            .context("Failed to store password in keyring")?,
                    )
                } else {
                    None
                };

                println!("Credentials stored securely in OS keyring");
                (user_ref, pass_ref)
            } else {
                (username.clone(), password.clone())
            };

            #[cfg(not(feature = "secure-storage"))]
            let (stored_username, stored_password) = (username.clone(), password.clone());

            redisctl_core::Profile {
                deployment_type: redisctl_core::DeploymentType::Enterprise,
                credentials: redisctl_core::ProfileCredentials::Enterprise {
                    url: url.clone(),
                    username: stored_username,
                    password: stored_password,
                    insecure: *insecure,
                    ca_cert: ca_cert.clone(),
                },
                files_api_key: None,
                tags: effective_tags.clone(),
            }
        }
        redisctl_core::DeploymentType::Database => {
            let host = host
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Host is required for Database profiles"))?;
            let port =
                port.ok_or_else(|| anyhow::anyhow!("Port is required for Database profiles"))?;

            // Prompt for password if not provided (optional for database profiles)
            let password = match password {
                Some(p) if !p.is_empty() => Some(p.clone()),
                _ => {
                    print!("Enter password (press Enter for none): ");
                    use std::io::{self, Write};
                    io::stdout().flush().unwrap();
                    let pass = rpassword::read_password().context("Failed to read password")?;
                    if pass.is_empty() { None } else { Some(pass) }
                }
            };

            // Use username if provided, otherwise default to "default"
            let username = username.clone().unwrap_or_else(|| "default".to_string());

            // Handle keyring storage if requested
            #[cfg(feature = "secure-storage")]
            let stored_password = if *use_keyring {
                if let Some(ref p) = password {
                    use redisctl_core::CredentialStore;
                    let store = CredentialStore::new();
                    let pass_ref = store
                        .store_credential(&format!("{}-password", name), p)
                        .context("Failed to store password in keyring")?;
                    println!("Password stored securely in OS keyring");
                    Some(pass_ref)
                } else {
                    None
                }
            } else {
                password.clone()
            };

            #[cfg(not(feature = "secure-storage"))]
            let stored_password = password.clone();

            redisctl_core::Profile {
                deployment_type: redisctl_core::DeploymentType::Database,
                credentials: redisctl_core::ProfileCredentials::Database {
                    host,
                    port,
                    password: stored_password,
                    tls: !*no_tls,
                    username,
                    database: db.unwrap_or(0),
                },
                files_api_key: None,
                tags: effective_tags,
            }
        }
    };

    // Preserve non-credential settings from existing profile when updating
    let profile = if let Some(existing) = conn_mgr.config.profiles.get(name) {
        redisctl_core::Profile {
            files_api_key: profile.files_api_key.or(existing.files_api_key.clone()),
            ..profile
        }
    } else {
        profile
    };

    // Update the configuration
    let mut config = conn_mgr.config.clone();
    config.profiles.insert(name.to_string(), profile);

    // Save the configuration to the appropriate location
    if let Some(ref path) = conn_mgr.config_path {
        config
            .save_to_path(path)
            .context("Failed to save configuration")?;
        println!("Profile '{}' saved successfully to:", name);
        println!("  {}", path.display());
    } else {
        config.save().context("Failed to save configuration")?;
        if let Ok(config_path) = Config::config_path() {
            println!("Profile '{}' saved successfully to:", name);
            println!("  {}", config_path.display());
        } else {
            println!("Profile '{}' saved successfully.", name);
        }
    }

    // Suggest setting as default if it's the only profile of its type
    let profiles_of_type = config.get_profiles_of_type(*deployment);
    if profiles_of_type.len() == 1 {
        println!();
        match deployment {
            redisctl_core::DeploymentType::Enterprise => {
                println!("Tip: Set as default for enterprise commands with:");
                println!("  redisctl profile default-enterprise {}", name);
            }
            redisctl_core::DeploymentType::Cloud => {
                println!("Tip: Set as default for cloud commands with:");
                println!("  redisctl profile default-cloud {}", name);
            }
            redisctl_core::DeploymentType::Database => {
                println!("Tip: Set as default for database commands with:");
                println!("  redisctl profile default-database {}", name);
            }
        }
    }

    Ok(())
}

async fn handle_init(conn_mgr: &ConnectionManager) -> Result<(), RedisCtlError> {
    use dialoguer::{Input, Select};

    println!("Welcome to redisctl profile setup!");
    println!();

    // Step 1: Choose profile type
    let type_options = &["cloud", "enterprise", "database"];
    let type_descriptions = &[
        "cloud       - Redis Cloud API (requires api-key + api-secret from cloud.redis.io)",
        "enterprise  - Redis Enterprise Software cluster (requires URL + admin credentials)",
        "database    - Direct Redis connection (requires host + port)",
    ];

    let type_index = Select::new()
        .with_prompt("What type of profile do you want to create?")
        .items(type_descriptions)
        .default(0)
        .interact()
        .map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Selection cancelled: {}", e),
        })?;

    let deployment_type = match type_options[type_index] {
        "cloud" => redisctl_core::DeploymentType::Cloud,
        "enterprise" => redisctl_core::DeploymentType::Enterprise,
        "database" => redisctl_core::DeploymentType::Database,
        _ => unreachable!(),
    };

    println!();

    // Step 2: Choose profile name
    let default_name = match deployment_type {
        redisctl_core::DeploymentType::Cloud => "mycloud",
        redisctl_core::DeploymentType::Enterprise => "myenterprise",
        redisctl_core::DeploymentType::Database => "mydb",
    };

    let name: String = Input::new()
        .with_prompt("Profile name")
        .default(default_name.to_string())
        .interact_text()
        .map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Input cancelled: {}", e),
        })?;

    // Check if profile already exists
    if conn_mgr.config.profiles.contains_key(&name) {
        println!("Profile '{}' already exists.", name);
        let overwrite = dialoguer::Confirm::new()
            .with_prompt("Overwrite?")
            .default(false)
            .interact()
            .map_err(|e| RedisCtlError::InvalidInput {
                message: format!("Input cancelled: {}", e),
            })?;
        if !overwrite {
            println!("Setup cancelled.");
            return Ok(());
        }
    }

    println!();

    // Step 3: Collect credentials based on type
    let profile = match deployment_type {
        redisctl_core::DeploymentType::Cloud => {
            let api_key: String = Input::new()
                .with_prompt("API key")
                .interact_text()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            let api_secret: String = rpassword::prompt_password("API secret: ").map_err(|e| {
                RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                }
            })?;

            let api_url: String = Input::new()
                .with_prompt("API URL")
                .default("https://api.redislabs.com/v1".to_string())
                .interact_text()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            redisctl_core::Profile {
                deployment_type: redisctl_core::DeploymentType::Cloud,
                credentials: redisctl_core::ProfileCredentials::Cloud {
                    api_key,
                    api_secret,
                    api_url,
                },
                files_api_key: None,
                tags: vec![],
            }
        }
        redisctl_core::DeploymentType::Enterprise => {
            let url: String = Input::new()
                .with_prompt("Cluster URL (e.g., https://cluster:9443)")
                .interact_text()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            let username: String = Input::new()
                .with_prompt("Username")
                .interact_text()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            let password = rpassword::prompt_password("Password: ").map_err(|e| {
                RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                }
            })?;

            let insecure = dialoguer::Confirm::new()
                .with_prompt("Allow insecure TLS (self-signed certificates)?")
                .default(false)
                .interact()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            redisctl_core::Profile {
                deployment_type: redisctl_core::DeploymentType::Enterprise,
                credentials: redisctl_core::ProfileCredentials::Enterprise {
                    url,
                    username,
                    password: Some(password),
                    insecure,
                    ca_cert: None,
                },
                files_api_key: None,
                tags: vec![],
            }
        }
        redisctl_core::DeploymentType::Database => {
            let host: String = Input::new()
                .with_prompt("Redis host")
                .default("localhost".to_string())
                .interact_text()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            let port: u16 = Input::new()
                .with_prompt("Redis port")
                .default(6379)
                .interact_text()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            let password =
                rpassword::prompt_password("Password (Enter for none): ").map_err(|e| {
                    RedisCtlError::InvalidInput {
                        message: format!("Input cancelled: {}", e),
                    }
                })?;
            let password = if password.is_empty() {
                None
            } else {
                Some(password)
            };

            let tls = dialoguer::Confirm::new()
                .with_prompt("Use TLS?")
                .default(true)
                .interact()
                .map_err(|e| RedisCtlError::InvalidInput {
                    message: format!("Input cancelled: {}", e),
                })?;

            redisctl_core::Profile {
                deployment_type: redisctl_core::DeploymentType::Database,
                credentials: redisctl_core::ProfileCredentials::Database {
                    host,
                    port,
                    password,
                    tls,
                    username: "default".to_string(),
                    database: 0,
                },
                files_api_key: None,
                tags: vec![],
            }
        }
    };

    // Step 4: Optionally test connectivity
    let test_connect = dialoguer::Confirm::new()
        .with_prompt("Test connectivity before saving?")
        .default(true)
        .interact()
        .map_err(|e| RedisCtlError::InvalidInput {
            message: format!("Input cancelled: {}", e),
        })?;

    if test_connect {
        print!("Testing connectivity... ");
        use std::io::{self, Write};
        io::stdout().flush().unwrap();

        // Build a temporary config with the new profile so we can reuse the
        // existing connectivity test functions.
        let mut temp_config = conn_mgr.config.clone();
        temp_config.profiles.insert(name.clone(), profile.clone());
        let temp_mgr = ConnectionManager::new(temp_config);

        let result = match deployment_type {
            redisctl_core::DeploymentType::Cloud => test_cloud_connectivity(&temp_mgr, &name).await,
            redisctl_core::DeploymentType::Enterprise => {
                test_enterprise_connectivity(&temp_mgr, &name).await
            }
            redisctl_core::DeploymentType::Database => test_database_connectivity(&profile).await,
        };

        match result.status {
            ConnectStatus::Ok => {
                println!(
                    "{}{}",
                    "ok".green(),
                    result
                        .latency_ms
                        .map(|ms| format!(" ({}ms)", ms))
                        .unwrap_or_default()
                );
            }
            _ => {
                println!("{}", "failed".red());
                println!("  {}", result.detail);
                println!();
                let save_anyway = dialoguer::Confirm::new()
                    .with_prompt("Save profile anyway?")
                    .default(false)
                    .interact()
                    .map_err(|e| RedisCtlError::InvalidInput {
                        message: format!("Input cancelled: {}", e),
                    })?;
                if !save_anyway {
                    println!("Setup cancelled.");
                    return Ok(());
                }
            }
        }
    }

    // Step 5: Save
    let mut config = conn_mgr.config.clone();
    config.profiles.insert(name.clone(), profile);

    // Auto-set as default if first profile of this type
    let is_first = config.get_profiles_of_type(deployment_type).len() == 1;
    if is_first {
        match deployment_type {
            redisctl_core::DeploymentType::Cloud => {
                config.default_cloud = Some(name.clone());
            }
            redisctl_core::DeploymentType::Enterprise => {
                config.default_enterprise = Some(name.clone());
            }
            redisctl_core::DeploymentType::Database => {
                config.default_database = Some(name.clone());
            }
        }
    }

    if let Some(ref path) = conn_mgr.config_path {
        config
            .save_to_path(path)
            .context("Failed to save configuration")?;
        println!();
        println!("Profile '{}' saved to: {}", name, path.display());
    } else {
        config.save().context("Failed to save configuration")?;
        if let Ok(config_path) = Config::config_path() {
            println!();
            println!("Profile '{}' saved to: {}", name, config_path.display());
        } else {
            println!();
            println!("Profile '{}' saved.", name);
        }
    }

    if is_first {
        let type_name = match deployment_type {
            redisctl_core::DeploymentType::Cloud => "cloud",
            redisctl_core::DeploymentType::Enterprise => "enterprise",
            redisctl_core::DeploymentType::Database => "database",
        };
        println!("Set as default {} profile.", type_name);
    }

    Ok(())
}

async fn handle_remove(conn_mgr: &ConnectionManager, name: &str) -> Result<(), RedisCtlError> {
    debug!("Removing profile: {}", name);

    // Check if profile exists
    if !conn_mgr.config.profiles.contains_key(name) {
        return Err(RedisCtlError::ProfileNotFound { name: name.into() });
    }

    // Check if it's a default profile
    let is_default_enterprise = conn_mgr.config.default_enterprise.as_deref() == Some(name);
    let is_default_cloud = conn_mgr.config.default_cloud.as_deref() == Some(name);
    if is_default_enterprise {
        println!(
            "Warning: '{}' is the default profile for enterprise commands.",
            name
        );
    }
    if is_default_cloud {
        println!(
            "Warning: '{}' is the default profile for cloud commands.",
            name
        );
    }

    // Ask for confirmation
    print!(
        "Are you sure you want to remove profile '{}'? (y/N): ",
        name
    );
    use std::io::{self, Write};
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_lowercase();

    if input != "y" && input != "yes" {
        println!("Profile removal cancelled.");
        return Ok(());
    }

    // Remove the profile
    let mut config = conn_mgr.config.clone();
    config.profiles.remove(name);

    // Clear defaults if this was a default profile
    if is_default_enterprise {
        config.default_enterprise = None;
        println!("Default enterprise profile cleared.");
    }
    if is_default_cloud {
        config.default_cloud = None;
        println!("Default cloud profile cleared.");
    }

    // Save the configuration to the appropriate location
    if let Some(ref path) = conn_mgr.config_path {
        config
            .save_to_path(path)
            .context("Failed to save configuration")?;
    } else {
        config.save().context("Failed to save configuration")?;
    }

    println!("Profile '{}' removed successfully.", name);
    Ok(())
}

async fn handle_default_enterprise(
    conn_mgr: &ConnectionManager,
    name: &str,
) -> Result<(), RedisCtlError> {
    debug!("Setting default enterprise profile: {}", name);

    // Check if profile exists and is an enterprise profile
    match conn_mgr.config.profiles.get(name) {
        Some(profile) => {
            if profile.deployment_type != redisctl_core::DeploymentType::Enterprise {
                return Err(anyhow::anyhow!(
                    "Profile '{}' is a cloud profile, not an enterprise profile",
                    name
                )
                .into());
            }
        }
        None => return Err(RedisCtlError::ProfileNotFound { name: name.into() }),
    }

    // Update the configuration
    let mut config = conn_mgr.config.clone();
    config.default_enterprise = Some(name.to_string());

    // Save the configuration to the appropriate location
    if let Some(ref path) = conn_mgr.config_path {
        config
            .save_to_path(path)
            .context("Failed to save configuration")?;
    } else {
        config.save().context("Failed to save configuration")?;
    }

    println!("Default enterprise profile set to '{}'.", name);
    Ok(())
}

async fn handle_default_cloud(
    conn_mgr: &ConnectionManager,
    name: &str,
) -> Result<(), RedisCtlError> {
    debug!("Setting default cloud profile: {}", name);

    // Check if profile exists and is a cloud profile
    match conn_mgr.config.profiles.get(name) {
        Some(profile) => {
            if profile.deployment_type != redisctl_core::DeploymentType::Cloud {
                return Err(anyhow::anyhow!(
                    "Profile '{}' is an enterprise profile, not a cloud profile",
                    name
                )
                .into());
            }
        }
        None => return Err(RedisCtlError::ProfileNotFound { name: name.into() }),
    }

    // Update the configuration
    let mut config = conn_mgr.config.clone();
    config.default_cloud = Some(name.to_string());

    // Save the configuration to the appropriate location
    if let Some(ref path) = conn_mgr.config_path {
        config
            .save_to_path(path)
            .context("Failed to save configuration")?;
    } else {
        config.save().context("Failed to save configuration")?;
    }

    println!("Default cloud profile set to '{}'.", name);
    Ok(())
}

async fn handle_default_database(
    conn_mgr: &ConnectionManager,
    name: &str,
) -> Result<(), RedisCtlError> {
    debug!("Setting default database profile: {}", name);

    // Check if profile exists and is a database profile
    match conn_mgr.config.profiles.get(name) {
        Some(profile) => {
            if profile.deployment_type != redisctl_core::DeploymentType::Database {
                return Err(anyhow::anyhow!("Profile '{}' is not a database profile", name).into());
            }
        }
        None => return Err(RedisCtlError::ProfileNotFound { name: name.into() }),
    }

    // Update the configuration
    let mut config = conn_mgr.config.clone();
    config.default_database = Some(name.to_string());

    // Save the configuration to the appropriate location
    if let Some(ref path) = conn_mgr.config_path {
        config
            .save_to_path(path)
            .context("Failed to save configuration")?;
    } else {
        config.save().context("Failed to save configuration")?;
    }

    println!("Default database profile set to '{}'.", name);
    Ok(())
}

/// Result of a connectivity test for a single profile
#[derive(Debug, Serialize)]
struct ConnectResult {
    status: ConnectStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    latency_ms: Option<u64>,
    detail: String,
}

/// Connectivity test outcome
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectStatus {
    Ok,
    AuthFailed,
    ConnectionRefused,
    Timeout,
    TlsError,
    Error,
}

/// Structured result for a single profile validation
#[derive(Debug, Serialize)]
struct ProfileValidationResult {
    name: String,
    deployment_type: String,
    structural: StructuralResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    connectivity: Option<ConnectResult>,
}

/// Structural validation result
#[derive(Debug, Serialize)]
struct StructuralResult {
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

/// Full validation output
#[derive(Debug, Serialize)]
struct ValidationOutput {
    config_path: String,
    config_exists: bool,
    profile_count: usize,
    profiles: Vec<ProfileValidationResult>,
    defaults: DefaultsValidation,
    overall_valid: bool,
}

/// Default profile validation
#[derive(Debug, Serialize)]
struct DefaultsValidation {
    #[serde(skip_serializing_if = "Option::is_none")]
    cloud: Option<DefaultValidation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enterprise: Option<DefaultValidation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    database: Option<DefaultValidation>,
}

#[derive(Debug, Serialize)]
struct DefaultValidation {
    name: String,
    valid: bool,
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Test connectivity for a Cloud profile
async fn test_cloud_connectivity(conn_mgr: &ConnectionManager, name: &str) -> ConnectResult {
    let start = Instant::now();
    match conn_mgr.create_cloud_client(Some(name)).await {
        Ok(client) => {
            use redis_cloud::flexible::SubscriptionHandler;
            let handler = SubscriptionHandler::new(client);
            match tokio::time::timeout(CONNECT_TIMEOUT, handler.get_all_subscriptions()).await {
                Ok(Ok(_)) => ConnectResult {
                    status: ConnectStatus::Ok,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    detail: "Successfully authenticated and listed subscriptions".to_string(),
                },
                Ok(Err(e)) => classify_cloud_error(e, start.elapsed()),
                Err(_) => ConnectResult {
                    status: ConnectStatus::Timeout,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    detail: format!("Connection timed out after {}s", CONNECT_TIMEOUT.as_secs()),
                },
            }
        }
        Err(e) => ConnectResult {
            status: ConnectStatus::Error,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            detail: format!("Failed to create client: {}", e),
        },
    }
}

/// Classify a Cloud API error into a ConnectResult
fn classify_cloud_error(err: redis_cloud::CloudError, elapsed: Duration) -> ConnectResult {
    let latency_ms = Some(elapsed.as_millis() as u64);
    let msg = err.to_string();

    if matches!(err, redis_cloud::CloudError::AuthenticationFailed { .. }) {
        ConnectResult {
            status: ConnectStatus::AuthFailed,
            latency_ms,
            detail: msg,
        }
    } else if msg.contains("tls") || msg.contains("certificate") || msg.contains("SSL") {
        ConnectResult {
            status: ConnectStatus::TlsError,
            latency_ms,
            detail: msg,
        }
    } else if msg.contains("Connection refused") {
        ConnectResult {
            status: ConnectStatus::ConnectionRefused,
            latency_ms,
            detail: msg,
        }
    } else {
        ConnectResult {
            status: ConnectStatus::Error,
            latency_ms,
            detail: msg,
        }
    }
}

/// Test connectivity for an Enterprise profile
async fn test_enterprise_connectivity(conn_mgr: &ConnectionManager, name: &str) -> ConnectResult {
    let start = Instant::now();
    match conn_mgr.create_enterprise_client(Some(name)).await {
        Ok(client) => {
            use redis_enterprise::cluster::ClusterHandler;
            let handler = ClusterHandler::new(client);
            match tokio::time::timeout(CONNECT_TIMEOUT, handler.info()).await {
                Ok(Ok(cluster)) => ConnectResult {
                    status: ConnectStatus::Ok,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    detail: format!("Connected to cluster '{}'", cluster.name),
                },
                Ok(Err(e)) => classify_enterprise_error(e, start.elapsed()),
                Err(_) => ConnectResult {
                    status: ConnectStatus::Timeout,
                    latency_ms: Some(start.elapsed().as_millis() as u64),
                    detail: format!("Connection timed out after {}s", CONNECT_TIMEOUT.as_secs()),
                },
            }
        }
        Err(e) => ConnectResult {
            status: ConnectStatus::Error,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            detail: format!("Failed to create client: {}", e),
        },
    }
}

/// Classify an Enterprise REST error into a ConnectResult
fn classify_enterprise_error(err: redis_enterprise::RestError, elapsed: Duration) -> ConnectResult {
    let latency_ms = Some(elapsed.as_millis() as u64);
    let msg = err.to_string();

    match err {
        redis_enterprise::RestError::AuthenticationFailed
        | redis_enterprise::RestError::Unauthorized => ConnectResult {
            status: ConnectStatus::AuthFailed,
            latency_ms,
            detail: msg,
        },
        redis_enterprise::RestError::RequestFailed(ref reqwest_err) => {
            let inner = reqwest_err.to_string();
            if inner.contains("tls") || inner.contains("certificate") || inner.contains("SSL") {
                ConnectResult {
                    status: ConnectStatus::TlsError,
                    latency_ms,
                    detail: inner,
                }
            } else if inner.contains("Connection refused") {
                ConnectResult {
                    status: ConnectStatus::ConnectionRefused,
                    latency_ms,
                    detail: inner,
                }
            } else {
                ConnectResult {
                    status: ConnectStatus::Error,
                    latency_ms,
                    detail: inner,
                }
            }
        }
        redis_enterprise::RestError::ConnectionError(ref e) if e.contains("Connection refused") => {
            ConnectResult {
                status: ConnectStatus::ConnectionRefused,
                latency_ms,
                detail: msg,
            }
        }
        _ => ConnectResult {
            status: ConnectStatus::Error,
            latency_ms,
            detail: msg,
        },
    }
}

/// Test connectivity for a Database profile via Redis PING
async fn test_database_connectivity(profile: &redisctl_core::Profile) -> ConnectResult {
    let start = Instant::now();

    let (host, port, password, tls, username, database) =
        match profile.resolve_database_credentials() {
            Ok(Some(creds)) => creds,
            Ok(None) => {
                return ConnectResult {
                    status: ConnectStatus::Error,
                    latency_ms: None,
                    detail: "No database credentials in profile".to_string(),
                };
            }
            Err(e) => {
                return ConnectResult {
                    status: ConnectStatus::Error,
                    latency_ms: None,
                    detail: format!("Failed to resolve credentials: {}", e),
                };
            }
        };

    // Build Redis URL
    let scheme = if tls { "rediss" } else { "redis" };
    let auth = match (&password, username.as_str()) {
        (Some(pwd), "default") => format!(":{}@", urlencoding::encode(pwd)),
        (Some(pwd), user) => {
            format!(
                "{}:{}@",
                urlencoding::encode(user),
                urlencoding::encode(pwd)
            )
        }
        (None, "default") => String::new(),
        (None, user) => format!("{}@", urlencoding::encode(user)),
    };
    let url = format!("{}://{}{}:{}/{}", scheme, auth, host, port, database);

    let client = match redis::Client::open(url.as_str()) {
        Ok(c) => c,
        Err(e) => {
            return ConnectResult {
                status: ConnectStatus::Error,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                detail: format!("Invalid connection URL: {}", e),
            };
        }
    };

    match tokio::time::timeout(CONNECT_TIMEOUT, client.get_multiplexed_async_connection()).await {
        Ok(Ok(mut conn)) => match redis::cmd("PING").query_async::<String>(&mut conn).await {
            Ok(response) => ConnectResult {
                status: ConnectStatus::Ok,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                detail: format!("PING response: {}", response),
            },
            Err(e) => ConnectResult {
                status: ConnectStatus::Error,
                latency_ms: Some(start.elapsed().as_millis() as u64),
                detail: format!("PING failed: {}", e),
            },
        },
        Ok(Err(e)) => {
            let msg = e.to_string();
            let latency_ms = Some(start.elapsed().as_millis() as u64);
            if msg.contains("WRONGPASS")
                || msg.contains("NOAUTH")
                || msg.contains("AUTH")
                || msg.contains("invalid username-password")
            {
                ConnectResult {
                    status: ConnectStatus::AuthFailed,
                    latency_ms,
                    detail: msg,
                }
            } else if msg.contains("tls")
                || msg.contains("certificate")
                || msg.contains("SSL")
                || msg.contains("HandshakeFailure")
            {
                ConnectResult {
                    status: ConnectStatus::TlsError,
                    latency_ms,
                    detail: msg,
                }
            } else if msg.contains("Connection refused") {
                ConnectResult {
                    status: ConnectStatus::ConnectionRefused,
                    latency_ms,
                    detail: msg,
                }
            } else {
                ConnectResult {
                    status: ConnectStatus::Error,
                    latency_ms,
                    detail: msg,
                }
            }
        }
        Err(_) => ConnectResult {
            status: ConnectStatus::Timeout,
            latency_ms: Some(start.elapsed().as_millis() as u64),
            detail: format!("Connection timed out after {}s", CONNECT_TIMEOUT.as_secs()),
        },
    }
}

/// Perform structural validation of a single profile
fn validate_profile_structure(name: &str, profile: &redisctl_core::Profile) -> StructuralResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    match profile.deployment_type {
        redisctl_core::DeploymentType::Cloud => match profile.cloud_credentials() {
            Some((api_key, api_secret, api_url)) => {
                if api_key.is_empty() || api_secret.is_empty() {
                    errors.push("Missing API key or secret".to_string());
                }
                if !api_url.starts_with("http://") && !api_url.starts_with("https://") {
                    warnings.push("API URL should start with http:// or https://".to_string());
                }
                if !api_url.contains("api.redislabs.com") && api_url.starts_with("https://") {
                    warnings.push(format!(
                        "Non-standard Cloud API URL: {} (expected api.redislabs.com)",
                        api_url
                    ));
                }
            }
            None => {
                errors.push("Missing Cloud credentials".to_string());
            }
        },
        redisctl_core::DeploymentType::Enterprise => match profile.enterprise_credentials() {
            Some((url, username, password, _insecure, ca_cert)) => {
                if username.is_empty() {
                    errors.push("Missing username".to_string());
                }
                if password.is_none() || password.as_ref().is_none_or(|p: &&str| p.is_empty()) {
                    warnings.push("Missing password (will be prompted)".to_string());
                }
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    warnings.push("URL should start with http:// or https://".to_string());
                }
                if url.starts_with("http://") && !url.contains("localhost") {
                    warnings.push(
                        "Using HTTP (not HTTPS) for non-localhost Enterprise URL".to_string(),
                    );
                }
                if let Some(cert_path) = ca_cert
                    && !std::path::Path::new(cert_path).exists()
                {
                    warnings.push(format!("CA certificate path does not exist: {}", cert_path));
                }
            }
            None => {
                errors.push("Missing Enterprise credentials".to_string());
            }
        },
        redisctl_core::DeploymentType::Database => match profile.database_credentials() {
            Some((host, port, password, _tls, _username, _database)) => {
                if host.is_empty() {
                    errors.push("Missing host".to_string());
                }
                if port == 0 {
                    errors.push("Invalid port (0)".to_string());
                }
                if password.is_none() || password.as_ref().is_none_or(|p| p.is_empty()) {
                    warnings.push("No password configured".to_string());
                }
            }
            None => {
                errors.push("Missing Database credentials".to_string());
            }
        },
    }

    debug!(
        "Profile '{}' structural validation: {} errors, {} warnings",
        name,
        errors.len(),
        warnings.len()
    );

    StructuralResult {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

async fn handle_validate(
    conn_mgr: &ConnectionManager,
    connect: bool,
    output_format: OutputFormat,
) -> Result<(), RedisCtlError> {
    debug!("Validating configuration (connect={})", connect);

    let config_path = Config::config_path()?;
    let config_exists = config_path.exists();
    let config_path_str = config_path.display().to_string();

    if !config_exists {
        let result = ValidationOutput {
            config_path: config_path_str.clone(),
            config_exists: false,
            profile_count: 0,
            profiles: vec![],
            defaults: DefaultsValidation {
                cloud: None,
                enterprise: None,
                database: None,
            },
            overall_valid: false,
        };

        return output_validation(result, output_format);
    }

    let profiles = conn_mgr.config.list_profiles();
    let mut profile_results = Vec::new();

    for (name, profile) in &profiles {
        let structural = validate_profile_structure(name, profile);

        let connectivity = if connect && structural.valid {
            Some(match profile.deployment_type {
                redisctl_core::DeploymentType::Cloud => {
                    test_cloud_connectivity(conn_mgr, name).await
                }
                redisctl_core::DeploymentType::Enterprise => {
                    test_enterprise_connectivity(conn_mgr, name).await
                }
                redisctl_core::DeploymentType::Database => {
                    test_database_connectivity(profile).await
                }
            })
        } else {
            None
        };

        profile_results.push(ProfileValidationResult {
            name: (*name).clone(),
            deployment_type: profile.deployment_type.to_string(),
            structural,
            connectivity,
        });
    }

    // Validate defaults
    let cloud_default = conn_mgr
        .config
        .default_cloud
        .as_ref()
        .map(|name| DefaultValidation {
            name: name.clone(),
            valid: conn_mgr.config.profiles.contains_key(name),
        });
    let enterprise_default =
        conn_mgr
            .config
            .default_enterprise
            .as_ref()
            .map(|name| DefaultValidation {
                name: name.clone(),
                valid: conn_mgr.config.profiles.contains_key(name),
            });
    let database_default =
        conn_mgr
            .config
            .default_database
            .as_ref()
            .map(|name| DefaultValidation {
                name: name.clone(),
                valid: conn_mgr.config.profiles.contains_key(name),
            });

    let overall_valid = profile_results.iter().all(|r| r.structural.valid)
        && cloud_default.as_ref().is_none_or(|d| d.valid)
        && enterprise_default.as_ref().is_none_or(|d| d.valid)
        && database_default.as_ref().is_none_or(|d| d.valid);

    let result = ValidationOutput {
        config_path: config_path_str,
        config_exists: true,
        profile_count: profiles.len(),
        profiles: profile_results,
        defaults: DefaultsValidation {
            cloud: cloud_default,
            enterprise: enterprise_default,
            database: database_default,
        },
        overall_valid,
    };

    output_validation(result, output_format)
}

/// Output validation results in the requested format
fn output_validation(
    result: ValidationOutput,
    output_format: OutputFormat,
) -> Result<(), RedisCtlError> {
    match output_format {
        OutputFormat::Json | OutputFormat::Yaml => {
            output::print_output(&result, output_format, None)?;
        }
        _ => {
            print_validation_human(&result);
        }
    }
    Ok(())
}

/// Print validation results in human-readable format
fn print_validation_human(result: &ValidationOutput) {
    println!("Configuration file: {}", result.config_path);

    if !result.config_exists {
        println!("{} Configuration file does not exist", "x".red());
        println!("\nTry:");
        println!("  Create a profile: redisctl profile set <name> --type <type>");
        return;
    }

    println!("{} Configuration file exists and is readable", "ok".green());
    println!("{} Found {} profile(s)", "ok".green(), result.profile_count);

    if result.profiles.is_empty() {
        println!("\n{} No profiles configured", "!!".yellow());
        println!("\nTry:");
        println!(
            "  Create a Cloud profile: redisctl profile set mycloud --type cloud --api-key <key> --api-secret <secret>"
        );
        println!(
            "  Create an Enterprise profile: redisctl profile set myenterprise --type enterprise --url <url> --username <user>"
        );
        return;
    }

    println!();

    for p in &result.profiles {
        // Structural result
        print!("Profile '{}' ({}): ", p.name, p.deployment_type);
        if p.structural.valid {
            println!("{}", "ok".green());
        } else {
            println!("{}", "FAIL".red());
        }

        for err in &p.structural.errors {
            println!("  {} {}", "x".red(), err);
        }
        for warn in &p.structural.warnings {
            println!("  {} {}", "!!".yellow(), warn);
        }

        // Connectivity result
        if let Some(ref conn) = p.connectivity {
            match conn.status {
                ConnectStatus::Ok => {
                    let latency = conn
                        .latency_ms
                        .map(|ms| format!(" ({}ms)", ms))
                        .unwrap_or_default();
                    println!("  {} {}{}", "ok".green(), conn.detail, latency);
                }
                ConnectStatus::AuthFailed => {
                    println!("  {} Authentication failed: {}", "x".red(), conn.detail);
                }
                ConnectStatus::ConnectionRefused => {
                    println!("  {} Connection refused: {}", "x".red(), conn.detail);
                }
                ConnectStatus::Timeout => {
                    println!("  {} {}", "x".red(), conn.detail);
                }
                ConnectStatus::TlsError => {
                    println!("  {} TLS error: {}", "x".red(), conn.detail);
                }
                ConnectStatus::Error => {
                    println!("  {} {}", "x".red(), conn.detail);
                }
            }
        }
    }

    // Default profiles
    println!();
    if let Some(ref d) = result.defaults.enterprise {
        if d.valid {
            println!("{} Default enterprise profile: {}", "ok".green(), d.name);
        } else {
            println!(
                "{} Default enterprise profile '{}' not found",
                "x".red(),
                d.name
            );
        }
    }
    if let Some(ref d) = result.defaults.cloud {
        if d.valid {
            println!("{} Default cloud profile: {}", "ok".green(), d.name);
        } else {
            println!("{} Default cloud profile '{}' not found", "x".red(), d.name);
        }
    }
    if let Some(ref d) = result.defaults.database {
        if d.valid {
            println!("{} Default database profile: {}", "ok".green(), d.name);
        } else {
            println!(
                "{} Default database profile '{}' not found",
                "x".red(),
                d.name
            );
        }
    }

    // Overall summary
    println!();
    let has_errors = result.profiles.iter().any(|p| !p.structural.valid);
    let has_warnings = result
        .profiles
        .iter()
        .any(|p| !p.structural.warnings.is_empty());
    let has_conn_failures = result.profiles.iter().any(|p| {
        p.connectivity
            .as_ref()
            .is_some_and(|c| !matches!(c.status, ConnectStatus::Ok))
    });

    if has_errors {
        println!(
            "{} Configuration has errors. Fix them before using affected profiles.",
            "!!".yellow()
        );
    } else if has_conn_failures {
        println!(
            "{} Structural checks passed but connectivity tests failed for some profiles.",
            "!!".yellow()
        );
    } else if has_warnings {
        println!(
            "{} Configuration has warnings but should work.",
            "!!".yellow()
        );
    } else {
        println!("{} Configuration is valid", "ok".green());
    }
}
