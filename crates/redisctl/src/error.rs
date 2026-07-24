//! Error types for redisctl
//!
//! Defines structured error types using thiserror for better error handling and user experience.

use colored::Colorize;
use thiserror::Error;

/// Cargo-style diagnostic formatter for CLI errors.
///
/// Produces structured output like:
/// ```text
/// error: cannot determine platform for 'database'
///   You have both cloud and enterprise profiles.
///
///   tip: be explicit about the platform:
///       redisctl cloud database list
///       redisctl enterprise database list
/// ```
pub struct CliDiagnostic {
    message: String,
    detail: Option<String>,
    tips: Vec<(String, Vec<String>)>,
}

impl CliDiagnostic {
    /// Start a new error diagnostic with the given message.
    pub fn error(message: &str) -> Self {
        Self {
            message: message.to_string(),
            detail: None,
            tips: Vec::new(),
        }
    }

    /// Add a detail line below the error message.
    pub fn detail(mut self, text: &str) -> Self {
        self.detail = Some(text.to_string());
        self
    }

    /// Add a tip with optional example commands.
    pub fn tip(mut self, description: &str, commands: &[&str]) -> Self {
        self.tips.push((
            description.to_string(),
            commands.iter().map(|s| s.to_string()).collect(),
        ));
        self
    }

    /// Print the diagnostic to stderr with colored formatting.
    pub fn print(&self) {
        eprint!("{}{}", "error".red().bold(), ": ".bold());
        eprintln!("{}", self.message);

        if let Some(detail) = &self.detail {
            eprintln!("  {}", detail);
        }

        for (description, commands) in &self.tips {
            eprintln!();
            eprint!("  {}{}", "tip".yellow().bold(), ": ".bold());
            eprintln!("{}", description);
            for cmd in commands {
                eprintln!("      {}", cmd);
            }
        }
    }
}

/// Main error type for the redisctl application
#[derive(Error, Debug)]
pub enum RedisCtlError {
    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("{0}")]
    Other(String),

    #[error("Profile '{name}' not found")]
    ProfileNotFound { name: String },

    #[error("Profile '{name}' is type '{actual_type}' but command requires '{expected_type}'")]
    ProfileTypeMismatch {
        name: String,
        actual_type: String,
        expected_type: String,
        available_profiles: Vec<String>,
    },

    // Intended error types that no code path constructs yet: the CLI currently
    // reports these conditions with generic errors instead. Kept (with their
    // messages and suggestions) to be wired up rather than cut; see #1049.
    #[allow(dead_code)]
    #[error("No profile configured. Use 'redisctl profile set' to configure a profile.")]
    NoProfileConfigured,

    #[allow(dead_code)]
    #[error("Missing credentials for profile '{name}': {missing_fields}")]
    MissingCredentials {
        name: String,
        missing_fields: String,
    },

    #[error("Authentication failed for profile '{profile_name}': {message}")]
    AuthenticationFailed {
        message: String,
        profile_name: String,
    },

    #[error("API error: {message}")]
    ApiError { message: String },

    #[error("Invalid input: {message}")]
    InvalidInput { message: String },

    #[error(
        "Confirmation required: {prompt} Re-run with --force to proceed in a non-interactive session."
    )]
    Cancelled { prompt: String },

    #[allow(dead_code)] // intended but not yet produced by any code path; see #1049
    #[error("Command not supported for deployment type '{deployment_type}'")]
    UnsupportedDeploymentType { deployment_type: String },
    #[error("File error for '{path}': {message}")]
    FileError { path: String, message: String },

    #[error("Connection error: {message}")]
    ConnectionError { message: String },

    #[error("Timeout: {message}")]
    Timeout { message: String },

    #[error("Output formatting error: {message}")]
    OutputError { message: String },

    /// Agent-native surface error carrying a stable code + exit code (see
    /// [`crate::structured_error`]). Handled specially in `main` (JSON envelope to stdout,
    /// mapped exit code) rather than the generic 0/1 path.
    #[error("{0}")]
    Structured(Box<crate::structured_error::StructuredError>),
}

/// Result type for redisctl operations
pub type Result<T> = std::result::Result<T, RedisCtlError>;

impl RedisCtlError {
    /// Get helpful suggestions for resolving this error
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            RedisCtlError::ProfileNotFound { name } => vec![
                format!("List available profiles: redisctl profile list"),
                format!("Create profile '{}': redisctl profile set {}", name, name),
                format!("Check profile name spelling"),
            ],
            RedisCtlError::NoProfileConfigured => vec![
                "Run the setup wizard: redisctl profile init".to_string(),
                "Or create manually: redisctl profile set <name> --type <cloud|enterprise|database> ...".to_string(),
                "View profile help: redisctl profile set --help".to_string(),
            ],
            RedisCtlError::MissingCredentials { name, .. } => vec![
                format!(
                    "Update the profile: redisctl profile set {} --type <type> ...",
                    name
                ),
                format!("Check current values: redisctl profile show {}", name),
                "If using environment variables, check they are exported in your shell".to_string(),
            ],
            RedisCtlError::AuthenticationFailed { profile_name, .. } => {
                let mut suggestions = vec![format!(
                    "Check credentials: redisctl profile show {}",
                    profile_name,
                )];
                suggestions.push(format!(
                    "Refresh credentials: redisctl profile set {} --type <type> ... (preserves other settings)",
                    profile_name,
                ));
                suggestions.push(
                    "Test connectivity: redisctl profile validate --connect".to_string(),
                );
                suggestions
            }
            RedisCtlError::ConnectionError { message } if message.contains("certificate") || message.contains("SSL") || message.contains("tls") => vec![
                "For self-signed certificates, recreate profile with --insecure".to_string(),
                "Or provide a CA cert: --ca-cert /path/to/ca.pem".to_string(),
                "Verify the URL uses the correct port (9443 for Enterprise admin)".to_string(),
            ],
            RedisCtlError::ConnectionError { message } if message.contains("Connection refused") => vec![
                "The server is not accepting connections on this address/port".to_string(),
                "Verify the URL: redisctl profile show <profile>".to_string(),
                "Check that the server is running and the port is correct".to_string(),
            ],
            RedisCtlError::ConnectionError { message } if message.contains("timed out") => vec![
                "The server did not respond in time".to_string(),
                "Check network connectivity and firewall rules".to_string(),
                "Verify the URL: redisctl profile show <profile>".to_string(),
            ],
            RedisCtlError::ConnectionError { .. } => vec![
                "Check network connectivity to the server".to_string(),
                "Verify the URL: redisctl profile show <profile>".to_string(),
                "Test connectivity: redisctl profile validate --connect".to_string(),
            ],
            RedisCtlError::ApiError { message } if message.contains("404") => vec![
                "Verify the resource ID is correct".to_string(),
                "List available resources to find the correct ID".to_string(),
                "Check that you're using the correct profile".to_string(),
            ],
            RedisCtlError::ProfileTypeMismatch {
                expected_type,
                available_profiles,
                ..
            } => {
                let mut suggestions = Vec::new();
                if available_profiles.is_empty() {
                    suggestions.push(format!(
                        "No {} profiles found. Create one with: redisctl profile set <name> --type {}",
                        expected_type, expected_type
                    ));
                } else {
                    suggestions.push(format!(
                        "Available {} profiles: {}",
                        expected_type,
                        available_profiles.join(", ")
                    ));
                    suggestions.push(format!(
                        "Use one with: redisctl --profile {} <command>",
                        available_profiles[0]
                    ));
                }
                suggestions.push("List all profiles: redisctl profile list".to_string());
                suggestions
            }
            RedisCtlError::UnsupportedDeploymentType { .. } => vec![
                "Check the command documentation: redisctl <command> --help".to_string(),
                "Use the appropriate command for your deployment type".to_string(),
            ],
            RedisCtlError::InvalidInput { .. } => vec![
                "Check the command syntax: redisctl <command> --help".to_string(),
                "Verify input file format is correct (JSON/YAML)".to_string(),
            ],
            RedisCtlError::FileError { path, .. } => vec![
                format!("Check that file exists: {}", path),
                "Verify file permissions are correct".to_string(),
                "Ensure file path is correct (use absolute path if needed)".to_string(),
            ],
            RedisCtlError::Cancelled { .. } => vec![
                "Re-run with --force to skip the confirmation prompt".to_string(),
                "Confirmation prompts require an interactive terminal".to_string(),
            ],
            RedisCtlError::Configuration(_) => vec![
                "Check the profile: redisctl profile show <name>".to_string(),
                "Verify the config file syntax and required fields".to_string(),
            ],
            _ => vec![],
        }
    }

    /// Print a cargo-style diagnostic to stderr using colored formatting.
    /// A stable, machine-readable error code for the -o json/yaml envelope.
    /// Exhaustive on purpose: a new variant must declare its code.
    pub fn code(&self) -> &'static str {
        match self {
            RedisCtlError::Configuration(_) => "configuration",
            RedisCtlError::Other(_) => "error",
            RedisCtlError::ProfileNotFound { .. } => "profile_not_found",
            RedisCtlError::ProfileTypeMismatch { .. } => "profile_type_mismatch",
            RedisCtlError::NoProfileConfigured => "no_profile_configured",
            RedisCtlError::MissingCredentials { .. } => "missing_credentials",
            RedisCtlError::AuthenticationFailed { .. } => "authentication_failed",
            RedisCtlError::ApiError { .. } => "api_error",
            RedisCtlError::InvalidInput { .. } => "invalid_input",
            RedisCtlError::Cancelled { .. } => "cancelled",
            RedisCtlError::UnsupportedDeploymentType { .. } => "unsupported_deployment_type",
            RedisCtlError::FileError { .. } => "file_error",
            RedisCtlError::ConnectionError { .. } => "connection_error",
            RedisCtlError::Timeout { .. } => "timeout",
            RedisCtlError::OutputError { .. } => "output_error",
            // Agent-native surface errors carry their own stable code.
            RedisCtlError::Structured(se) => se.code,
        }
    }

    /// The structured error object emitted under -o json/-o yaml.
    fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": self.code(),
                "message": self.to_string(),
                "tips": self.suggestions(),
            }
        })
    }

    /// Print an error to stderr. Under -o json/-o yaml this is a structured
    /// envelope a script can parse; otherwise a cargo-style human diagnostic.
    /// Always stderr, so the -o json success payload on stdout stays clean.
    pub fn print_diagnostic(&self, format: crate::cli::OutputFormat) {
        use crate::cli::OutputFormat;
        match format {
            OutputFormat::Json => {
                let envelope = self.envelope();
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&envelope)
                        .unwrap_or_else(|_| envelope.to_string())
                );
            }
            OutputFormat::Yaml => match serde_yaml::to_string(&self.envelope()) {
                Ok(s) => eprint!("{}", s),
                Err(_) => self.print_human_diagnostic(),
            },
            OutputFormat::Auto | OutputFormat::Table => self.print_human_diagnostic(),
        }
    }

    fn print_human_diagnostic(&self) {
        let mut diag = CliDiagnostic::error(&format!("{}", self));

        for suggestion in self.suggestions() {
            diag = diag.tip(&suggestion, &[]);
        }

        diag.print();
    }
}

impl From<redis_cloud::CloudError> for RedisCtlError {
    fn from(err: redis_cloud::CloudError) -> Self {
        match err {
            redis_cloud::CloudError::AuthenticationFailed { message } => {
                RedisCtlError::AuthenticationFailed {
                    message,
                    profile_name: "<unknown>".to_string(),
                }
            }
            redis_cloud::CloudError::ConnectionError(message) => {
                RedisCtlError::ConnectionError { message }
            }
            _ => RedisCtlError::ApiError {
                message: err.to_string(),
            },
        }
    }
}

impl From<redis_enterprise::RestError> for RedisCtlError {
    fn from(err: redis_enterprise::RestError) -> Self {
        match err {
            redis_enterprise::RestError::AuthenticationFailed => {
                RedisCtlError::AuthenticationFailed {
                    message: "Authentication failed".to_string(),
                    profile_name: "<unknown>".to_string(),
                }
            }
            redis_enterprise::RestError::Unauthorized => RedisCtlError::AuthenticationFailed {
                message: "401 Unauthorized: Invalid username or password. Check your credentials."
                    .to_string(),
                profile_name: "<unknown>".to_string(),
            },
            redis_enterprise::RestError::NotFound => RedisCtlError::ApiError {
                message: "404 Not Found: The requested resource does not exist".to_string(),
            },
            redis_enterprise::RestError::ApiError { code, message } => RedisCtlError::ApiError {
                message: format!("HTTP {}: {}", code, message),
            },
            redis_enterprise::RestError::ServerError(msg) => RedisCtlError::ApiError {
                message: format!("Server error (5xx): {}", msg),
            },
            redis_enterprise::RestError::RequestFailed(reqwest_err) => {
                RedisCtlError::ConnectionError {
                    message: reqwest_err.to_string(),
                }
            }
            redis_enterprise::RestError::ConnectionError(msg) => {
                RedisCtlError::ConnectionError { message: msg }
            }
            redis_enterprise::RestError::ValidationError(msg) => {
                RedisCtlError::InvalidInput { message: msg }
            }
            _ => RedisCtlError::ApiError {
                message: err.to_string(),
            },
        }
    }
}

impl From<serde_json::Error> for RedisCtlError {
    fn from(err: serde_json::Error) -> Self {
        RedisCtlError::OutputError {
            message: format!("JSON error: {}", err),
        }
    }
}

impl From<std::io::Error> for RedisCtlError {
    fn from(err: std::io::Error) -> Self {
        RedisCtlError::OutputError {
            message: format!("IO error: {}", err),
        }
    }
}

impl From<anyhow::Error> for RedisCtlError {
    fn from(err: anyhow::Error) -> Self {
        // {:#} renders the full anyhow context chain (message: cause: cause),
        // and Other keeps the message as-is, instead of dropping the cause and
        // mislabeling every anyhow-wrapped API, IO, or JSON error as a
        // "Configuration error".
        RedisCtlError::Other(format!("{:#}", err))
    }
}

impl From<redisctl_core::AuthError> for RedisCtlError {
    fn from(err: redisctl_core::AuthError) -> Self {
        match err {
            redisctl_core::AuthError::Network(e) => RedisCtlError::ConnectionError {
                message: e.to_string(),
            },
            other => RedisCtlError::AuthenticationFailed {
                message: other.to_string(),
                profile_name: "<login>".to_string(),
            },
        }
    }
}

impl From<redisctl_core::ConfigError> for RedisCtlError {
    fn from(err: redisctl_core::ConfigError) -> Self {
        RedisCtlError::Configuration(err.to_string())
    }
}

impl From<redisctl_core::error::CoreError> for RedisCtlError {
    fn from(err: redisctl_core::error::CoreError) -> Self {
        match err {
            redisctl_core::error::CoreError::TaskTimeout(duration) => RedisCtlError::Timeout {
                message: format!("Operation timed out after {} seconds", duration.as_secs()),
            },
            redisctl_core::error::CoreError::TaskFailed(msg) => RedisCtlError::ApiError {
                message: format!("Task failed: {}", msg),
            },
            redisctl_core::error::CoreError::Validation(msg) => {
                RedisCtlError::InvalidInput { message: msg }
            }
            redisctl_core::error::CoreError::Config(msg) => RedisCtlError::Configuration(msg),
            redisctl_core::error::CoreError::Cloud(cloud_err) => RedisCtlError::from(cloud_err),
            redisctl_core::error::CoreError::Enterprise(enterprise_err) => {
                RedisCtlError::from(enterprise_err)
            }
        }
    }
}
