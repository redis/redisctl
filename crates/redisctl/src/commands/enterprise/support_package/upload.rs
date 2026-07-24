//! Support package upload functionality using Files.com
//!
//! This module handles uploading support packages to Files.com for Redis Support.

#[cfg(feature = "upload")]
use anyhow::{Context, Result};
#[cfg(feature = "upload")]
use files_sdk::{FileHandler, FilesClient};
#[cfg(feature = "upload")]
use redisctl_core::Config;

/// Upload a support package to Files.com
///
/// # Arguments
///
/// * `api_key` - Files.com API key
/// * `package_data` - The support package bytes
/// * `filename` - Filename for the upload
/// * `remote_path` - Remote path (default: /RLEC_Customers/Uploads)
#[cfg(feature = "upload")]
pub async fn upload_package(
    api_key: &str,
    package_data: &[u8],
    filename: &str,
    remote_path: Option<&str>,
) -> Result<String> {
    let client = FilesClient::builder()
        .api_key(api_key)
        .build()
        .context("Failed to create Files.com client")?;

    let handler = FileHandler::new(client);

    // Default to Redis Enterprise customer uploads path (matching spfetch)
    let upload_path = remote_path.unwrap_or("/RLEC_Customers/Uploads");
    let full_path = format!("{}/{}", upload_path, filename);

    println!("Uploading to Files.com: {}", full_path);
    println!("Size: {} bytes", package_data.len());

    let file = handler
        .upload_file(&full_path, package_data)
        .await
        .context("Failed to upload file to Files.com")?;

    Ok(file.path.unwrap_or_else(|| full_path.clone()))
}

/// Get Files.com API key from environment, config, or keyring
///
/// Priority (highest to lowest):
/// 1. REDIS_ENTERPRISE_FILES_API_KEY environment variable
/// 2. Profile-specific files_api_key in config
/// 3. Global files_api_key in config
/// 4. System keyring (if secure-storage enabled)
/// 5. REDIS_FILES_API_KEY environment variable (fallback)
#[cfg(feature = "upload")]
pub fn get_files_api_key(profile_name: Option<&str>) -> Result<String> {
    // 1. Try environment variable first (highest priority - for CI/CD)
    if let Ok(key) = std::env::var("REDIS_ENTERPRISE_FILES_API_KEY") {
        return Ok(key);
    }

    // 2 & 3. Try config file (profile-specific, then global)
    if let Ok(config) = Config::load() {
        // 2. Try profile-specific key
        if let Some(profile_name) = profile_name
            && let Some(profile) = config.profiles.get(profile_name)
            && let Some(key) = &profile.files_api_key
        {
            return resolve_config_key(key);
        }

        // 3. Try global key
        if let Some(key) = &config.files_api_key {
            return resolve_config_key(key);
        }
    }

    // 4. Try keyring directly (for CLI-stored keys)
    #[cfg(feature = "secure-storage")]
    {
        if let Ok(key) = get_from_keyring() {
            return Ok(key);
        }
    }

    // 5. Fallback environment variable
    if let Ok(key) = std::env::var("REDIS_FILES_API_KEY") {
        return Ok(key);
    }

    // Build helpful error message based on available features
    #[cfg(feature = "secure-storage")]
    let error_msg = "Files.com API key not found. Options:\n\
         1. Set REDIS_ENTERPRISE_FILES_API_KEY environment variable\n\
         2. Store securely: redisctl files-key set <key> --use-keyring\n\
         3. Add to config: files_api_key = \"...\" (global) or per-profile\n\
         4. Set REDIS_FILES_API_KEY environment variable (fallback)";

    #[cfg(not(feature = "secure-storage"))]
    let error_msg = "Files.com API key not found. Options:\n\
         1. Set REDIS_ENTERPRISE_FILES_API_KEY environment variable\n\
         2. Add to config: files_api_key = \"...\" (global) or per-profile\n\
         3. Set REDIS_FILES_API_KEY environment variable (fallback)";

    anyhow::bail!(error_msg)
}

/// Resolve a config key value, handling keyring: prefix
///
/// If the value starts with "keyring:", resolves it from the keyring.
/// Otherwise returns the value as-is.
#[cfg(feature = "upload")]
fn resolve_config_key(key: &str) -> Result<String> {
    if let Some(keyring_key) = key.strip_prefix("keyring:") {
        #[cfg(feature = "secure-storage")]
        {
            let entry = keyring::Entry::new("redisctl", keyring_key)
                .context("Failed to access system keyring")?;
            entry
                .get_password()
                .context(format!("Failed to retrieve '{}' from keyring", keyring_key))
        }

        #[cfg(not(feature = "secure-storage"))]
        anyhow::bail!(
            "Config references keyring ('{}'), but secure-storage feature not enabled",
            key
        )
    } else {
        Ok(key.to_string())
    }
}

/// Retrieve Files.com API key from system keyring
#[cfg(all(feature = "upload", feature = "secure-storage"))]
pub fn get_from_keyring() -> Result<String> {
    let entry = keyring::Entry::new("redisctl", "files-api-key")
        .context("Failed to access system keyring")?;

    entry.get_password().context("API key not found in keyring")
}
