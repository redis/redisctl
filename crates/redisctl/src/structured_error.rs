//! Machine-branchable error contract for the agent-native surface (starting with `cloud auth *`).
//!
//! Existing commands keep today's 0/1 exit behavior; only the new surface produces
//! [`StructuredError`]. On failure in JSON/YAML mode the CLI prints
//! `{"status":"error","error":{code,message,retryable}}` to **stdout** (agents parse
//! stdout) and exits with the mapped code; in human mode it prints the usual diagnostic to
//! stderr. `message` is always safe to show — it never contains secrets.
//!
//! Exit codes: `1` unknown/backend, `2` usage/precondition the caller must fix, `3`
//! transient/retryable, `4` quota/limit reached.

// The binary target reads these fields/methods (main.rs); the library target can't see that,
// and a few inventory entries (auth_pending, keyring_unavailable) are defined ahead of use.
#![allow(dead_code)] // Used by binary target

use redisctl_core::AuthError;

/// A terminal error from the agent-native surface, carrying a stable code and exit status.
#[derive(Debug, Clone)]
pub struct StructuredError {
    /// Stable, append-only identifier (see the inventory below). Agents branch on this.
    pub code: &'static str,
    /// Human, actionable, secret-free.
    pub message: String,
    /// Whether re-running the same command might succeed (transient failures).
    pub retryable: bool,
    /// Process exit code: 1 | 2 | 3 | 4.
    pub exit_code: u8,
}

impl std::fmt::Display for StructuredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for StructuredError {}

impl StructuredError {
    fn new(code: &'static str, exit_code: u8, retryable: bool, message: impl Into<String>) -> Self {
        Self {
            code,
            exit_code,
            retryable,
            message: message.into(),
        }
    }

    // --- inventory (append-only) ---

    pub fn device_code_expired() -> Self {
        Self::new(
            "device_code_expired",
            3,
            true,
            "the login code expired before it was approved; run `redisctl cloud auth login` again",
        )
    }
    pub fn auth_denied() -> Self {
        Self::new("auth_denied", 2, false, "the login request was denied")
    }
    pub fn auth_pending() -> Self {
        Self::new(
            "auth_pending",
            3,
            true,
            "login has not been approved yet; approve it and retry",
        )
    }
    pub fn not_authenticated(message: impl Into<String>) -> Self {
        Self::new("not_authenticated", 2, false, message)
    }
    pub fn sm_exchange_failed(message: impl Into<String>) -> Self {
        Self::new("sm_exchange_failed", 1, false, message)
    }
    pub fn keyring_unavailable(message: impl Into<String>) -> Self {
        Self::new("keyring_unavailable", 2, false, message)
    }
    pub fn migration_required() -> Self {
        Self::new(
            "migration_required",
            2,
            false,
            "this Redis Cloud account signs in with a password and must be linked to Google or \
             GitHub once, in the Redis Cloud console, before the CLI can use it",
        )
    }
    pub fn mfa_required() -> Self {
        Self::new(
            "mfa_required",
            2,
            false,
            "this account requires multi-factor authentication; run `redisctl cloud auth login` \
             in an interactive terminal to enter the code",
        )
    }
    pub fn mfa_invalid_code() -> Self {
        Self::new(
            "mfa_invalid_code",
            2,
            false,
            "the multi-factor code was not accepted; start login again",
        )
    }
    pub fn mfa_quota_exceeded() -> Self {
        Self::new(
            "mfa_quota_exceeded",
            4,
            false,
            "too many multi-factor attempts; wait before trying again",
        )
    }
    pub fn invalid_name(message: impl Into<String>) -> Self {
        Self::new("invalid_name", 2, false, message)
    }
    pub fn name_conflict(message: impl Into<String>) -> Self {
        Self::new("name_conflict", 2, false, message)
    }
    pub fn free_db_exists(message: impl Into<String>) -> Self {
        Self::new("free_db_exists", 4, false, message)
    }
    pub fn quota_exceeded(message: impl Into<String>) -> Self {
        Self::new("quota_exceeded", 4, false, message)
    }
    pub fn transient_api_error(message: impl Into<String>) -> Self {
        Self::new("transient_api_error", 3, true, message)
    }
    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self::new("rate_limited", 3, true, message)
    }
    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new("unknown", 1, false, message)
    }

    /// The PRD error envelope, printed to stdout in JSON/YAML mode.
    pub fn envelope(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "error",
            "error": {
                "code": self.code,
                "message": self.message,
                "retryable": self.retryable,
            }
        })
    }
}

impl From<redisctl_core::cloud::quick_database::QuickDatabaseError> for StructuredError {
    fn from(err: redisctl_core::cloud::quick_database::QuickDatabaseError) -> Self {
        use redisctl_core::cloud::quick_database::QuickDatabaseError as E;
        match err {
            E::InvalidName(m) => Self::invalid_name(m),
            E::NameConflict(m) => Self::name_conflict(m),
            E::FreeDbExists(m) => Self::free_db_exists(m),
            E::QuotaExceeded(m) => Self::quota_exceeded(m),
            E::NotAuthenticated(m) => Self::not_authenticated(m),
            E::Transient(m) => Self::transient_api_error(m),
            E::RateLimited(m) => Self::rate_limited(m),
            E::Other(m) => Self::unknown(m),
        }
    }
}

impl From<AuthError> for StructuredError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::Expired => Self::device_code_expired(),
            AuthError::Denied => Self::auth_denied(),
            // Transport failures to the IdP are worth retrying.
            AuthError::Network(e) => Self::transient_api_error(format!(
                "network error contacting the identity provider: {e}"
            )),
            // Unexpected IdP / SM response during the login exchange.
            AuthError::Protocol(msg) => {
                Self::sm_exchange_failed(format!("login exchange failed: {msg}"))
            }
            // A one-time console step, not a failure to retry — give it its own code so an agent
            // can tell the user what to do rather than surfacing a generic exchange error.
            AuthError::MigrationRequired => Self::migration_required(),
            // Reached only when there was no terminal to prompt on: the caller must re-run
            // interactively, so this is a precondition to fix rather than a retryable failure.
            AuthError::MfaRequired { .. } => Self::mfa_required(),
            AuthError::MfaInvalidCode => Self::mfa_invalid_code(),
            AuthError::MfaQuotaExceeded => Self::mfa_quota_exceeded(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every constructor maps to a deliberate (exit_code, retryable) pair; exit codes stay in
    /// the documented 1–4 range and retryable is only ever set for the 3-class.
    #[test]
    fn inventory_is_consistent() {
        let all = [
            StructuredError::device_code_expired(),
            StructuredError::auth_denied(),
            StructuredError::auth_pending(),
            StructuredError::not_authenticated("x"),
            StructuredError::sm_exchange_failed("x"),
            StructuredError::keyring_unavailable("x"),
            StructuredError::invalid_name("x"),
            StructuredError::name_conflict("x"),
            StructuredError::free_db_exists("x"),
            StructuredError::quota_exceeded("x"),
            StructuredError::transient_api_error("x"),
            StructuredError::rate_limited("x"),
            StructuredError::unknown("x"),
        ];
        for e in &all {
            assert!(
                (1..=4).contains(&e.exit_code),
                "{}: exit_code {} out of range",
                e.code,
                e.exit_code
            );
            // retryable is reserved for the transient (exit 3) class.
            assert_eq!(
                e.retryable,
                e.exit_code == 3,
                "{}: retryable must align with exit 3",
                e.code
            );
        }
    }

    #[test]
    fn auth_error_mapping() {
        assert_eq!(
            StructuredError::from(AuthError::Expired).code,
            "device_code_expired"
        );
        assert_eq!(StructuredError::from(AuthError::Denied).code, "auth_denied");
        assert_eq!(
            StructuredError::from(AuthError::Protocol("boom".into())).code,
            "sm_exchange_failed"
        );
        let mig = StructuredError::from(AuthError::MigrationRequired);
        assert_eq!(mig.code, "migration_required");
        assert_eq!(mig.exit_code, 2);
        assert!(!mig.retryable);
        // MFA reaches the structured path only when we couldn't prompt, so it's a precondition
        // (exit 2) the caller fixes by re-running interactively — never "retryable".
        let mfa = StructuredError::from(AuthError::MfaRequired { factors: vec![] });
        assert_eq!(mfa.code, "mfa_required");
        assert_eq!(mfa.exit_code, 2);
        assert!(!mfa.retryable);
        assert_eq!(
            StructuredError::from(AuthError::MfaInvalidCode).code,
            "mfa_invalid_code"
        );
        let quota = StructuredError::from(AuthError::MfaQuotaExceeded);
        assert_eq!(quota.code, "mfa_quota_exceeded");
        assert_eq!(quota.exit_code, 4);
    }

    #[test]
    fn quick_database_error_mapping() {
        use redisctl_core::cloud::quick_database::QuickDatabaseError as E;
        // Each core provisioning error maps to its inventory code with the expected exit class.
        let cases = [
            (E::InvalidName("x".into()), "invalid_name", 2u8),
            (E::NameConflict("x".into()), "name_conflict", 2),
            (E::FreeDbExists("x".into()), "free_db_exists", 4),
            (E::QuotaExceeded("x".into()), "quota_exceeded", 4),
            (E::NotAuthenticated("x".into()), "not_authenticated", 2),
            (E::Transient("x".into()), "transient_api_error", 3),
            (E::RateLimited("x".into()), "rate_limited", 3),
            (E::Other("x".into()), "unknown", 1),
        ];
        for (err, code, exit) in cases {
            let se = StructuredError::from(err);
            assert_eq!(se.code, code);
            assert_eq!(se.exit_code, exit, "{code}: exit code");
            assert_eq!(
                se.retryable,
                exit == 3,
                "{code}: retryable aligns with exit 3"
            );
        }
    }

    #[test]
    fn envelope_shape() {
        let e = StructuredError::invalid_name("bad name");
        let env = e.envelope();
        assert_eq!(env["status"], "error");
        assert_eq!(env["error"]["code"], "invalid_name");
        assert_eq!(env["error"]["retryable"], false);
        assert_eq!(env["error"]["message"], "bad name");
    }
}
