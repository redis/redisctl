//! Files.com API key management commands
//!
//! This module provides commands for managing Files.com API keys used for
//! support package uploads. Keys can be stored in the system keyring (secure)
//! or in the config file (plaintext).

use anyhow::{Context, Result};
use redisctl_core::Config;

/// Handle the files-key set command
pub async fn handle_set(
    api_key: String,
    #[cfg(feature = "secure-storage")] use_keyring: bool,
    global: bool,
    profile: Option<String>,
) -> Result<()> {
    #[cfg(feature = "secure-storage")]
    if use_keyring {
        // Store in system keyring
        let entry = keyring::Entry::new("redisctl", "files-api-key")
            .context("Failed to access system keyring")?;

        entry
            .set_password(&api_key)
            .context("Failed to store API key in keyring")?;

        println!("✓ Files.com API key stored securely in system keyring");
        println!("\nThe key will be used automatically for support package uploads.");
        println!("To use it in config, reference it as: files_api_key = \"keyring:files-api-key\"");
        return Ok(());
    }

    // Store in config file
    let mut config = Config::load().unwrap_or_default();

    if let Some(profile_name) = profile {
        // Store in specific profile
        if let Some(prof) = config.profiles.get_mut(&profile_name) {
            prof.files_api_key = Some(api_key);
            config.save()?;
            println!("✓ Files.com API key stored in profile '{}'", profile_name);
            println!("\n⚠️  Warning: Key is stored in plaintext in config file");
            #[cfg(feature = "secure-storage")]
            println!("   For better security, use: redisctl files-key set <key> --use-keyring");
        } else {
            anyhow::bail!("Profile '{}' not found", profile_name);
        }
    } else if global {
        // Store globally
        config.files_api_key = Some(api_key);
        config.save()?;
        println!("✓ Files.com API key stored globally in config");
        println!("\n⚠️  Warning: Key is stored in plaintext in config file");
        #[cfg(feature = "secure-storage")]
        println!("   For better security, use: redisctl files-key set <key> --use-keyring");
    } else {
        // Default behavior
        #[cfg(feature = "secure-storage")]
        {
            println!("Please specify where to store the key:");
            println!("  --use-keyring    Store securely in system keyring (recommended)");
            println!("  --global         Store in global config (plaintext)");
            println!("  --profile <name> Store in specific profile (plaintext)");
            anyhow::bail!("No storage location specified");
        }

        #[cfg(not(feature = "secure-storage"))]
        {
            // Without secure-storage, default to global
            config.files_api_key = Some(api_key);
            config.save()?;
            println!("✓ Files.com API key stored globally in config");
        }
    }

    Ok(())
}

/// Handle the files-key get command
pub async fn handle_get(profile: Option<String>) -> Result<()> {
    let config = Config::load().context("Failed to load config")?;

    // Check profile-specific key
    if let Some(profile_name) = &profile {
        if let Some(prof) = config.profiles.get(profile_name) {
            if let Some(key) = &prof.files_api_key {
                if key.starts_with("keyring:") {
                    println!("Profile '{}' uses keyring: {}", profile_name, key);

                    #[cfg(feature = "secure-storage")]
                    {
                        let keyring_key = key.strip_prefix("keyring:").unwrap();
                        let entry = keyring::Entry::new("redisctl", keyring_key)
                            .context("Failed to access system keyring")?;
                        match entry.get_password() {
                            Ok(actual_key) => {
                                println!(
                                    "Key retrieved: {}...{}",
                                    &actual_key[..8.min(actual_key.len())],
                                    if actual_key.len() > 8 {
                                        &actual_key[actual_key.len() - 4..]
                                    } else {
                                        ""
                                    }
                                );
                            }
                            Err(e) => println!("⚠️  Failed to retrieve from keyring: {}", e),
                        }
                    }
                } else {
                    println!(
                        "Profile '{}' key: {}...{}",
                        profile_name,
                        &key[..8.min(key.len())],
                        if key.len() > 8 {
                            &key[key.len() - 4..]
                        } else {
                            ""
                        }
                    );
                }
                return Ok(());
            } else {
                println!("No Files.com API key set for profile '{}'", profile_name);
            }
        } else {
            anyhow::bail!("Profile '{}' not found", profile_name);
        }
    }

    // Check global key
    if let Some(key) = &config.files_api_key {
        if key.starts_with("keyring:") {
            println!("Global key uses keyring: {}", key);

            #[cfg(feature = "secure-storage")]
            {
                let keyring_key = key.strip_prefix("keyring:").unwrap();
                let entry = keyring::Entry::new("redisctl", keyring_key)
                    .context("Failed to access system keyring")?;
                match entry.get_password() {
                    Ok(actual_key) => {
                        println!(
                            "Key retrieved: {}...{}",
                            &actual_key[..8.min(actual_key.len())],
                            if actual_key.len() > 8 {
                                &actual_key[actual_key.len() - 4..]
                            } else {
                                ""
                            }
                        );
                    }
                    Err(e) => println!("⚠️  Failed to retrieve from keyring: {}", e),
                }
            }
        } else {
            println!(
                "Global key: {}...{}",
                &key[..8.min(key.len())],
                if key.len() > 8 {
                    &key[key.len() - 4..]
                } else {
                    ""
                }
            );
        }
        return Ok(());
    }

    // Check keyring directly
    #[cfg(feature = "secure-storage")]
    {
        let entry = keyring::Entry::new("redisctl", "files-api-key")
            .context("Failed to access system keyring")?;
        if let Ok(key) = entry.get_password() {
            println!(
                "Key found in keyring: {}...{}",
                &key[..8.min(key.len())],
                if key.len() > 8 {
                    &key[key.len() - 4..]
                } else {
                    ""
                }
            );
            return Ok(());
        }
    }

    println!("No Files.com API key configured");
    println!("\nTo set a key:");
    println!("  redisctl files-key set <key> --use-keyring    (recommended)");
    println!("  redisctl files-key set <key> --global");
    println!("  redisctl files-key set <key> --profile <name>");

    Ok(())
}

/// Handle the files-key remove command
pub async fn handle_remove(
    #[cfg(feature = "secure-storage")] keyring: bool,
    global: bool,
    profile: Option<String>,
) -> Result<()> {
    #[cfg(feature = "secure-storage")]
    if keyring {
        let entry = keyring::Entry::new("redisctl", "files-api-key")
            .context("Failed to access system keyring")?;

        entry
            .delete_credential()
            .context("Failed to delete API key from keyring")?;

        println!("✓ Files.com API key removed from keyring");
        return Ok(());
    }

    let mut config = Config::load().context("Failed to load config")?;
    let mut modified = false;

    if let Some(profile_name) = profile {
        // Remove from specific profile
        if let Some(prof) = config.profiles.get_mut(&profile_name) {
            if prof.files_api_key.is_some() {
                prof.files_api_key = None;
                modified = true;
                println!(
                    "✓ Files.com API key removed from profile '{}'",
                    profile_name
                );
            } else {
                println!("No Files.com API key set for profile '{}'", profile_name);
            }
        } else {
            anyhow::bail!("Profile '{}' not found", profile_name);
        }
    } else if global {
        // Remove global key
        if config.files_api_key.is_some() {
            config.files_api_key = None;
            modified = true;
            println!("✓ Files.com API key removed from global config");
        } else {
            println!("No global Files.com API key set");
        }
    } else {
        println!("Please specify what to remove:");
        #[cfg(feature = "secure-storage")]
        println!("  --keyring        Remove from system keyring");
        println!("  --global         Remove from global config");
        println!("  --profile <name> Remove from specific profile");
        anyhow::bail!("No removal target specified");
    }

    if modified {
        config.save()?;
    }

    Ok(())
}
