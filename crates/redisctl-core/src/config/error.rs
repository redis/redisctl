//! Error types for configuration operations

use thiserror::Error;

/// Errors that can occur during configuration operations
#[derive(Error)]
pub enum ConfigError {
    #[error("Failed to load config from {path}: {source}")]
    LoadError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to save config to {path}: {source}")]
    SaveError {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "Failed to parse config: invalid TOML syntax or field value; check quoted strings and field types"
    )]
    ParseError(toml::de::Error),

    #[error("Failed to serialize config: invalid configuration value")]
    SerializeError(toml::ser::Error),

    #[error("Profile '{name}' not found")]
    ProfileNotFound { name: String },

    #[error("No {deployment_type} profiles configured. {suggestion}")]
    NoProfilesOfType {
        deployment_type: String,
        suggestion: String,
    },

    #[error("Failed to resolve credential: {0}")]
    CredentialError(String),

    #[cfg(feature = "secure-storage")]
    #[error("Keyring error: {0}")]
    KeyringError(String),

    #[error("Environment variable expansion failed: {0}")]
    EnvExpansionError(String),

    #[error("ambiguous deployment type — both cloud and enterprise profiles exist")]
    AmbiguousDeployment,

    #[error("Failed to determine config directory")]
    ConfigDirError,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

// TOML errors can contain the input document, field names and rejected values. Removing only
// the source excerpt is insufficient. Keep the public variant payload types, but do not expose
// their contents through Debug or Error::source (including explicitly constructed variants).
impl std::fmt::Debug for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ParseError(_) => f.write_str("ParseError(<redacted>)"),
            Self::SerializeError(_) => f.write_str("SerializeError(<redacted>)"),
            _ => f
                .debug_tuple("ConfigError")
                .field(&self.to_string())
                .finish(),
        }
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(error: toml::de::Error) -> Self {
        // Retain a non-sensitive location where available, without copying parser messages.
        let message = match error.span() {
            Some(span) => format!("Invalid configuration at byte offset {}", span.start),
            None => "Invalid configuration syntax or field value".to_string(),
        };
        Self::ParseError(<toml::de::Error as serde::de::Error>::custom(message))
    }
}

impl From<toml::ser::Error> for ConfigError {
    fn from(_: toml::ser::Error) -> Self {
        Self::SerializeError(<toml::ser::Error as serde::ser::Error>::custom(
            "Invalid configuration value",
        ))
    }
}

/// Result type for configuration operations
pub type Result<T> = std::result::Result<T, ConfigError>;
