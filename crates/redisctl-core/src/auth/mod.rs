//! Cloud authentication: OIDC flows that bootstrap a Redis Cloud CAPI key.
//!
//! `cloud auth login` is a credential *bootstrapper*: obtain Okta tokens, exchange them with
//! the SM API, mint a CAPI key, and hand it to the config layer to persist as a normal cloud
//! profile. This module holds the OIDC token-acquisition clients plus the SM exchange.
//!
//! Two front doors, one back end:
//! - [`device_flow::DeviceFlowClient`] — device authorization grant (headless / agent).
//! - [`auth_code_loopback::LoopbackFlowClient`] — auth-code + PKCE via a loopback redirect
//!   (interactive human login).
//!
//! Both yield a [`TokenSet`] and share the common token-endpoint plumbing in `oidc`. No
//! JWT/JWKS validation happens here — the SM API is the verifier of the token downstream.

pub mod auth_code_loopback;
pub mod authenticator;
pub mod device_flow;
pub mod sm_api;

mod oidc;

pub use auth_code_loopback::LoopbackFlowClient;
pub use authenticator::{CloudAuthenticator, MintedCredentials};
pub use device_flow::{DeviceAuthorization, DeviceFlowClient};
pub use oidc::{AuthError, TokenSet};
pub use sm_api::{CapiKey, SmAccount, SmApiClient, SmUser};

// Crate-private OIDC plumbing shared by the SM exchange (referenced as `super::*`). The flow
// clients pull their `oauth2` helpers directly from `super::oidc`.
pub(crate) use oidc::{default_http_client, endpoint, truncate};
