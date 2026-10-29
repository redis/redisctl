//! Safe diagnostics for test fixtures, not a general Docker-output sanitizer.

use std::fmt;

use docker_wrapper::Error as DockerError;
use docker_wrapper::template::TemplateError;

const MAX_INSPECTED_STDERR_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug)]
pub enum FixtureStage {
    RedisStart,
    RedisPort,
    StackStart,
    StackPort,
}

impl FixtureStage {
    fn label(self) -> &'static str {
        match self {
            Self::RedisStart => "Redis startup",
            Self::RedisPort => "Redis port resolution",
            Self::StackStart => "Redis Stack startup",
            Self::StackPort => "Redis Stack port resolution",
        }
    }
}

pub struct FixtureError {
    stage: FixtureStage,
    original: TemplateError,
}

impl FixtureError {
    /// Retain structured evidence for explicit inspection without printing it.
    pub fn original_error(&self) -> &TemplateError {
        &self.original
    }
}

// The upstream Display prints command arguments, and Debug also prints captured
// stdout/stderr. Neither is safe for an automatically rendered public CI chain.
// Keep the typed error accessible above instead of exposing it through source().
impl std::error::Error for FixtureError {}

impl fmt::Debug for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Fixture {} failed: ", self.stage.label())?;
        if let TemplateError::DockerError(DockerError::CommandFailed {
            exit_code, stderr, ..
        }) = &self.original
        {
            let (category, inspected) = classify_stderr(stderr);
            let truncated = if inspected < stderr.len() {
                " (truncated)"
            } else {
                ""
            };
            write!(
                f,
                "docker exit {exit_code}; cause={category}; stderr inspection={inspected}/{} bytes{truncated}; command/output withheld",
                stderr.len()
            )
        } else {
            let category = match &self.original {
                TemplateError::DockerError(DockerError::DockerNotFound) => "docker-runtime-missing",
                TemplateError::DockerError(DockerError::DaemonNotRunning) => {
                    "docker-daemon-unavailable"
                }
                TemplateError::DockerError(DockerError::Timeout { .. })
                | TemplateError::Timeout(_) => "timeout",
                TemplateError::DockerError(_) => "docker-runtime-error",
                TemplateError::InvalidConfig(_) => "invalid-fixture-configuration",
                TemplateError::AlreadyRunning(_) => "fixture-already-running",
                TemplateError::NotRunning(_) => "fixture-not-running",
            };
            write!(f, "cause={category}; details withheld")
        }
    }
}

pub fn fixture_error(stage: FixtureStage, original: TemplateError) -> anyhow::Error {
    anyhow::Error::new(FixtureError { stage, original })
}

fn classify_stderr(stderr: &str) -> (&'static str, usize) {
    let mut end = stderr.len().min(MAX_INSPECTED_STDERR_BYTES);
    while !stderr.is_char_boundary(end) {
        end -= 1;
    }
    let prefix = stderr[..end].to_ascii_lowercase();
    // Only fixed labels leave this function. Even unknown text is never echoed.
    let category = if prefix.contains("port is already allocated")
        || prefix.contains("address already in use")
    {
        "port-in-use"
    } else if prefix.contains("container name") && prefix.contains("already in use") {
        "container-name-in-use"
    } else if prefix.contains("cannot connect to the docker daemon")
        || prefix.contains("is the docker daemon running")
    {
        "docker-daemon-unreachable"
    } else if prefix.contains("pull access denied")
        || prefix.contains("manifest unknown")
        || prefix.contains("no matching manifest")
        || prefix.contains("repository does not exist")
    {
        "image-unavailable"
    } else if prefix.contains("permission denied") {
        "permission-denied"
    } else {
        "unclassified"
    };
    (category, end)
}
