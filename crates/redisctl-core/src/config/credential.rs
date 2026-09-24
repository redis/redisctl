//! Credential storage abstraction with optional keyring support
//!
//! This module provides a unified interface for storing and retrieving credentials,
//! with support for:
//! - OS keyring (when feature enabled)
//! - Plaintext storage (fallback)
//! - Environment variable override

use super::error::{ConfigError, Result};
use std::env;

/// Whether process environment variables may override stored profile values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvironmentOverrides {
    /// Resolve supported environment variables before stored profile values.
    Enabled,
    /// Resolve only stored profile values and keyring references.
    Disabled,
}

/// Prefix that indicates a value should be retrieved from the keyring
const KEYRING_PREFIX: &str = "keyring:";

/// Service name for keyring entries
#[cfg(feature = "secure-storage")]
const SERVICE_NAME: &str = "redisctl";

/// Entry name the availability read uses, and the stem `probe_writable` builds its per-call key
/// from. One name for both so there is a single reserved key rather than a magic one per check.
#[cfg(feature = "secure-storage")]
const PROBE_ENTRY: &str = "__probe__";

/// Distinguishes concurrent probes within one process, so parallel tests don't collide either.
#[cfg(feature = "secure-storage")]
static PROBE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Storage backend for credentials
#[derive(Debug, Clone)]
pub enum CredentialStorage {
    /// Store in OS keyring
    #[cfg(feature = "secure-storage")]
    Keyring,
    /// Store as plaintext
    Plaintext,
}

/// Credential store abstraction
pub struct CredentialStore {
    #[cfg(feature = "secure-storage")]
    storage: CredentialStorage,
}

impl Default for CredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore {
    /// Create a new credential store with automatic backend selection
    pub fn new() -> Self {
        #[cfg(feature = "secure-storage")]
        {
            // Try to use keyring if available
            if Self::is_keyring_available() {
                Self {
                    storage: CredentialStorage::Keyring,
                }
            } else {
                Self {
                    storage: CredentialStorage::Plaintext,
                }
            }
        }
        #[cfg(not(feature = "secure-storage"))]
        {
            Self {}
        }
    }

    /// Create a store that always uses plaintext (no keyring). Useful for tests and for the
    /// explicit `--allow-plaintext` opt-in when no keyring is available.
    pub fn plaintext() -> Self {
        #[cfg(feature = "secure-storage")]
        {
            Self {
                storage: CredentialStorage::Plaintext,
            }
        }
        #[cfg(not(feature = "secure-storage"))]
        {
            Self {}
        }
    }

    /// Whether the keyring backend answers at all, from a read.
    ///
    /// Reads the probe entry and judges the answer rather than discarding it: `Entry::new` only
    /// builds a handle, so returning `true` whenever it succeeded made this unconditional and
    /// left `new()` selecting `Keyring` on every machine with the feature compiled in. `NoEntry`
    /// is the healthy answer — the entry is absent, and the backend said so. Anything else
    /// (`NoStorageAccess`, `PlatformFailure`, …) is the backend refusing to talk.
    ///
    /// A read is all this does, so constructing a store never writes. Confirming the backend can
    /// *hold* a value needs [`CredentialStore::probe_writable`].
    #[cfg(feature = "secure-storage")]
    fn is_keyring_available() -> bool {
        match keyring::Entry::new(SERVICE_NAME, PROBE_ENTRY) {
            Ok(entry) => !matches!(
                entry.get_password(),
                Err(keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_))
            ),
            Err(_) => false,
        }
    }

    /// Confirm the backend can actually hold a credential, by storing a throwaway value and
    /// reading it back.
    ///
    /// [`CredentialStore::is_keyring_available`] only reads, and a backend can answer a read and
    /// still refuse a write — a locked macOS keychain, a Windows credential store the session
    /// cannot write, keyutils in a container without `CONFIG_KEYS`. Callers about to create
    /// something they cannot recreate — a minted API key, whose secret is returned once — should
    /// ask here first.
    ///
    /// What this does *not* establish: on Linux the backend is keyutils (see the `keyring`
    /// features in the workspace `Cargo.toml`), where the value lives in an in-memory kernel
    /// keyring. A write and read-back inside one process succeeds there even when the value will
    /// not be visible to the next `redisctl` run — the same absence [`Self::get_credential`]
    /// reports after a reboot.
    pub fn probe_writable(&self) -> Result<()> {
        #[cfg(feature = "secure-storage")]
        {
            if !matches!(self.storage, CredentialStorage::Keyring) {
                return Ok(());
            }
            const PROBE_VALUE: &str = "redisctl-probe";

            // Per call, not a shared constant: two runs probing the same entry race, and the one
            // that reads after the other's cleanup sees `NoEntry` and refuses a login its own
            // keyring would have served.
            let key = format!(
                "{PROBE_ENTRY}-{}-{}",
                std::process::id(),
                PROBE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let entry = keyring::Entry::new(SERVICE_NAME, &key)
                .map_err(|e| ConfigError::KeyringError(e.to_string()))?;
            entry.set_password(PROBE_VALUE).map_err(|e| {
                ConfigError::KeyringError(format!("the keyring rejected a test write: {e}"))
            })?;
            let read_back = entry.get_password().map_err(|e| {
                ConfigError::KeyringError(format!("the keyring did not return a test write: {e}"))
            });
            // Leave nothing behind whatever the read said.
            let _ = entry.delete_credential();
            if read_back? != PROBE_VALUE {
                return Err(ConfigError::KeyringError(
                    "the keyring returned a different value than was written".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Store a credential value
    pub fn store_credential(&self, key: &str, value: &str) -> Result<String> {
        #[cfg(feature = "secure-storage")]
        {
            match self.storage {
                CredentialStorage::Keyring => {
                    let entry = keyring::Entry::new(SERVICE_NAME, key)
                        .map_err(|e| ConfigError::KeyringError(e.to_string()))?;
                    entry.set_password(value).map_err(|e| {
                        ConfigError::KeyringError(format!(
                            "Failed to store credential in keyring: {}",
                            e
                        ))
                    })?;
                    // Return the reference string that will be stored in config
                    Ok(format!("{}{}", KEYRING_PREFIX, key))
                }
                CredentialStorage::Plaintext => Ok(value.to_string()),
            }
        }
        #[cfg(not(feature = "secure-storage"))]
        {
            // Without secure-storage feature, always use plaintext
            let _ = key; // Not used without secure-storage
            Ok(value.to_string())
        }
    }

    /// Retrieve a credential value
    ///
    /// Resolution order:
    /// 1. Check environment variables in order (if env vars provided)
    /// 2. If value starts with "keyring:", retrieve from keyring
    /// 3. Otherwise, return the value as-is (plaintext)
    pub fn get_credential(&self, value: &str, env_var: Option<&str>) -> Result<String> {
        match env_var {
            Some(env_var) => self.get_credential_with_environment(
                value,
                &[env_var],
                EnvironmentOverrides::Enabled,
            ),
            None => self.get_credential_with_environment(value, &[], EnvironmentOverrides::Enabled),
        }
    }

    /// Retrieve a credential value with support for multiple environment variable aliases.
    ///
    /// Environment variables are checked in order, and the first set value wins.
    pub fn get_credential_with_env_vars(&self, value: &str, env_vars: Vec<&str>) -> Result<String> {
        self.get_credential_with_environment(value, &env_vars, EnvironmentOverrides::Enabled)
    }

    /// Retrieve a credential with an explicit environment override policy.
    ///
    /// This is used by callers that load an explicit configuration file and
    /// require its credential values to be isolated from the process
    /// environment.
    pub fn get_credential_with_environment(
        &self,
        value: &str,
        env_vars: &[&str],
        environment_overrides: EnvironmentOverrides,
    ) -> Result<String> {
        if environment_overrides == EnvironmentOverrides::Enabled {
            for var in env_vars {
                if let Ok(env_value) = env::var(var) {
                    return Ok(env_value);
                }
            }
        }

        // Check if this is a keyring reference
        if value.starts_with(KEYRING_PREFIX) {
            #[cfg(feature = "secure-storage")]
            {
                let key = value.trim_start_matches(KEYRING_PREFIX);
                let entry = keyring::Entry::new(SERVICE_NAME, key)
                    .map_err(|e| ConfigError::KeyringError(e.to_string()))?;
                entry.get_password().map_err(|e| match e {
                    // The entry is gone rather than unreadable. On Linux the backing store is an
                    // in-memory kernel keyring, so this is expected after a reboot and the config
                    // itself is fine — say so, because the generic wording sends people to check
                    // their config file syntax.
                    keyring::Error::NoEntry => ConfigError::KeyringError(format!(
                        "credential '{key}' is no longer in the OS keyring, so any profile \
                         referencing it cannot be used until it is stored again. On Linux the \
                         keyring does not survive a reboot. Store it again with \
                         `redisctl profile set <name> --type <cloud|enterprise>`, or for a Redis \
                         Cloud profile sign in again with \
                         `redisctl --profile <name> cloud auth login`."
                    )),
                    other => ConfigError::KeyringError(format!(
                        "Failed to retrieve credential '{key}' from keyring: {other}"
                    )),
                })
            }
            #[cfg(not(feature = "secure-storage"))]
            {
                Err(ConfigError::CredentialError(
                    "Credential references keyring but secure-storage feature is not enabled"
                        .to_string(),
                ))
            }
        } else {
            // Plain text value
            Ok(value.to_string())
        }
    }

    /// Delete a credential from storage
    pub fn delete_credential(&self, key: &str) -> Result<()> {
        #[cfg(feature = "secure-storage")]
        {
            match self.storage {
                CredentialStorage::Keyring => {
                    let entry = keyring::Entry::new(SERVICE_NAME, key)
                        .map_err(|e| ConfigError::KeyringError(e.to_string()))?;
                    match entry.delete_credential() {
                        Ok(()) => Ok(()),
                        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted
                        Err(e) => Err(ConfigError::KeyringError(format!(
                            "Failed to delete credential from keyring: {}",
                            e
                        ))),
                    }
                }
                CredentialStorage::Plaintext => Ok(()), // Nothing to delete for plaintext
            }
        }
        #[cfg(not(feature = "secure-storage"))]
        {
            let _ = key; // Not used without secure-storage
            Ok(()) // Nothing to delete for plaintext
        }
    }

    /// Check if a value is a keyring reference
    pub fn is_keyring_reference(value: &str) -> bool {
        value.starts_with(KEYRING_PREFIX)
    }

    /// Get the current storage backend
    pub fn storage_backend(&self) -> &str {
        #[cfg(feature = "secure-storage")]
        {
            match self.storage {
                CredentialStorage::Keyring => "keyring",
                CredentialStorage::Plaintext => "plaintext",
            }
        }
        #[cfg(not(feature = "secure-storage"))]
        {
            "plaintext"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plaintext_storage() {
        let store = CredentialStore::new();

        // Plaintext values should be returned as-is
        let result = store.get_credential("my-api-key", None).unwrap();
        assert_eq!(result, "my-api-key");
    }

    #[test]
    fn test_env_var_override() {
        unsafe {
            env::set_var("TEST_CREDENTIAL", "env-value");
        }

        let store = CredentialStore::new();
        let result = store
            .get_credential("config-value", Some("TEST_CREDENTIAL"))
            .unwrap();
        assert_eq!(result, "env-value");

        unsafe {
            env::remove_var("TEST_CREDENTIAL");
        }
    }

    #[test]
    #[serial_test::serial(credential_alias_env)]
    fn test_env_var_alias_override_uses_first_available() {
        unsafe {
            env::set_var("TEST_CREDENTIAL_ALIAS_2", "alias-value");
        }

        let store = CredentialStore::new();
        let result = store
            .get_credential_with_env_vars(
                "config-value",
                vec!["TEST_CREDENTIAL_ALIAS_1", "TEST_CREDENTIAL_ALIAS_2"],
            )
            .unwrap();
        assert_eq!(result, "alias-value");

        unsafe {
            env::remove_var("TEST_CREDENTIAL_ALIAS_2");
        }
    }

    #[test]
    #[serial_test::serial(credential_alias_env)]
    fn test_env_var_alias_override_prefers_first_set() {
        unsafe {
            env::set_var("TEST_CREDENTIAL_ALIAS_1", "preferred-value");
            env::set_var("TEST_CREDENTIAL_ALIAS_2", "fallback-value");
        }

        let store = CredentialStore::new();
        let result = store
            .get_credential_with_env_vars(
                "config-value",
                vec!["TEST_CREDENTIAL_ALIAS_1", "TEST_CREDENTIAL_ALIAS_2"],
            )
            .unwrap();
        assert_eq!(result, "preferred-value");

        unsafe {
            env::remove_var("TEST_CREDENTIAL_ALIAS_1");
            env::remove_var("TEST_CREDENTIAL_ALIAS_2");
        }
    }

    #[test]
    fn test_keyring_reference_detection() {
        assert!(CredentialStore::is_keyring_reference("keyring:my-key"));
        assert!(!CredentialStore::is_keyring_reference("my-key"));
        assert!(!CredentialStore::is_keyring_reference(""));
    }

    #[cfg(feature = "secure-storage")]
    #[test]
    #[ignore = "Requires keyring service to be available"]
    fn test_keyring_storage() {
        let store = CredentialStore::new();

        // Store a credential
        let key = "test-credential";
        let value = "test-value";
        let reference = store.store_credential(key, value).unwrap();

        // Should return a keyring reference
        assert!(reference.starts_with(KEYRING_PREFIX));

        // Retrieve it back
        let retrieved = store.get_credential(&reference, None).unwrap();
        assert_eq!(retrieved, value);

        // Clean up
        let _ = store.delete_credential(key);
    }

    /// Nothing to probe without a keyring, and nothing may be written looking.
    #[test]
    fn probe_writable_is_a_no_op_for_plaintext() {
        assert!(CredentialStore::plaintext().probe_writable().is_ok());
    }

    /// The probe has to be repeatable — it runs on every login, and each run must leave the
    /// keyring able to serve the next one.
    ///
    /// Repeatability is what is asserted rather than the absence of a specific entry: the key is
    /// derived per call, so there is no fixed name to look for, and an `is_err()` on a read would
    /// pass just as well against a keyring that answers nothing at all.
    #[cfg(feature = "secure-storage")]
    #[test]
    #[ignore = "Requires keyring service to be available"]
    fn probe_writable_accepts_a_working_keyring_repeatedly() {
        let store = CredentialStore::new();
        store.probe_writable().unwrap();
        store.probe_writable().unwrap();

        // A real credential still round-trips afterwards: the probe took nothing with it.
        let reference = store.store_credential("probe-neighbour", "kept").unwrap();
        assert_eq!(store.get_credential(&reference, None).unwrap(), "kept");
        let _ = store.delete_credential("probe-neighbour");
    }
}
