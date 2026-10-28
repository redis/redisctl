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
                                println!("Key retrieved: {}", mask_files_key(&actual_key));
                            }
                            Err(e) => println!("⚠️  Failed to retrieve from keyring: {}", e),
                        }
                    }
                } else {
                    println!("Profile '{}' key: {}", profile_name, mask_files_key(key));
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
                        println!("Key retrieved: {}", mask_files_key(&actual_key));
                    }
                    Err(e) => println!("⚠️  Failed to retrieve from keyring: {}", e),
                }
            }
        } else {
            println!("Global key: {}", mask_files_key(key));
        }
        return Ok(());
    }

    // Check keyring directly
    #[cfg(feature = "secure-storage")]
    {
        let entry = keyring::Entry::new("redisctl", "files-api-key")
            .context("Failed to access system keyring")?;
        if let Ok(key) = entry.get_password() {
            println!("Key found in keyring: {}", mask_files_key(&key));
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

/// Mask a resolved Files.com API key value for display.
///
/// Mirrors the MCP profile credential mask policy for literal values: an
/// empty value renders as `(not set)`, a value of eight or fewer Unicode
/// scalar values renders as `****`, and a longer value shows its first two
/// and last two scalar values separated by `...`.
///
/// Counting and slicing are done per `char`, never by byte offset, so
/// multibyte input cannot panic or split a code point. The input must already
/// be a resolved value; `keyring:` and other references are interpreted by
/// the caller before the value reaches this function.
fn mask_files_key(value: &str) -> String {
    if value.is_empty() {
        return "(not set)".to_string();
    }
    if value.chars().count() <= 8 {
        return "****".to_string();
    }
    let prefix: String = value.chars().take(2).collect();
    let suffix: String = value
        .chars()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}...{suffix}")
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

#[cfg(test)]
mod tests {
    use super::mask_files_key;

    #[test]
    fn masks_empty_value_as_not_set() {
        assert_eq!(mask_files_key(""), "(not set)");
    }

    #[test]
    fn masks_short_ascii_values_fully() {
        for value in ["a", "abc", "abcdefg", "abcdefgh"] {
            assert_eq!(mask_files_key(value), "****", "value: {value:?}");
        }
    }

    #[test]
    fn masks_ascii_length_boundaries_by_scalar_count() {
        for (value, expected) in [
            ("abcdefgh", "****"),
            ("abcdefghi", "ab...hi"),
            ("abcdefghijkl", "ab...kl"),
            ("abcdefghijklm", "ab...lm"),
        ] {
            assert_eq!(mask_files_key(value), expected, "value: {value:?}");
        }
    }

    #[test]
    fn masks_multibyte_prefixes_and_suffixes_by_scalar() {
        for (value, expected) in [
            ("🔒password", "🔒p...rd"),
            ("password🔒", "pa...d🔒"),
            ("épassword", "ép...rd"),
            ("passwordé", "pa...dé"),
            ("密碼abcdefgh密碼", "密碼...密碼"),
        ] {
            assert_eq!(mask_files_key(value), expected, "value: {value:?}");
        }
    }

    #[test]
    fn masks_short_values_whose_byte_length_exceeds_eight() {
        // Each of these is more than 8 bytes but at most 8 scalar values, so
        // the old byte-offset slice would have panicked or leaked; the mask
        // must treat them as short.
        for value in ["🔒🔒🔒", "密碼密碼密", "éééééééé", "🔒🔒🔒🔒🔒🔒🔒🔒"]
        {
            assert!(value.len() > 8, "value: {value:?}");
            assert!(value.chars().count() <= 8, "value: {value:?}");
            assert_eq!(mask_files_key(value), "****", "value: {value:?}");
        }
    }

    #[test]
    fn counts_combining_scalars_individually() {
        // 'e' followed by U+0301 COMBINING ACUTE ACCENT is two scalar values.
        let eight_scalars = "e\u{301}".repeat(4);
        assert_eq!(eight_scalars.chars().count(), 8);
        assert_eq!(mask_files_key(&eight_scalars), "****");

        let nine_scalars = format!("{eight_scalars}x");
        assert_eq!(mask_files_key(&nine_scalars), "e\u{301}...\u{301}x");

        assert_eq!(mask_files_key("e\u{301}abcdefgh"), "e\u{301}...gh");
        assert_eq!(mask_files_key("abcdefghe\u{301}"), "ab...e\u{301}");
    }

    #[test]
    fn long_mask_shows_exactly_two_scalars_on_each_side() {
        assert_eq!(mask_files_key("abcdefghijklmnopqrstuvwxyz"), "ab...yz");
    }

    #[test]
    fn never_reveals_full_nonempty_input() {
        for value in [
            "a",
            "abcdefgh",
            "abcdefghi",
            "abcdefghijklm",
            "🔒password",
            "密碼密碼密",
            "e\u{301}abcdefgh",
            "sk-live-0123456789abcdef",
        ] {
            let masked = mask_files_key(value);
            assert!(!masked.is_empty(), "value: {value:?}");
            assert!(
                !masked.contains(value),
                "value: {value:?}, masked: {masked:?}"
            );
        }
    }
}
