# Cloud auth: where the pieces belong

> **Status: historical design record.** This was a layering review written during the 0.11.x health assessment, before PR #1036 merged on 2026-07-27. It is kept as the rationale behind the current `redisctl-core::auth` structure. Outcome: the `oauth2` crate was added as a `redisctl-core` dependency, and the auth code was kept consolidated in `redisctl-core::auth` rather than split into the separate crates recommended below. The open questions at the end were effectively settled by the merged implementation.

A layering review of PR #1036 (`feat(core): add Cloud OIDC + SM auth library`), the first PR in the chained series that will power `redisctl cloud auth login`. The question this answers: what belongs in redisctl proper, what belongs in redis-cloud-rs, and what should be its own library.

Date: 2026-07-23. Reviewed against redisctl `main` and redis-cloud-rs 0.11.0.

## Summary

The PR is well built, but it puts three concerns that live at three different layers into one module (`redisctl_core::auth`). The recommended split:

- **redisctl proper** keeps the `cloud auth login` command, the thin orchestration, and persisting the minted key as a profile.
- **A generic OIDC library** (prefer the existing `oauth2` crate) owns the device-flow and auth-code + PKCE plumbing. This should not be hand-rolled inside the CLI's shared crate.
- **A dedicated Cloud signin crate** owns the SM token-exchange (the reverse-engineered console backend that mints an API key). This belongs neither in redisctl nor inside the clean redis-cloud-rs REST client.

So two pieces want to become their own libraries, redisctl keeps only the command plus persistence, and redis-cloud-rs stays as it is.

## What the PR contains

`redisctl_core::auth` folds together three concerns:

1. **Generic OIDC/OAuth** (`device_flow`, `auth_code_loopback`, `oidc`, PKCE). OAuth Device Authorization Grant (RFC 8628) and Authorization Code + PKCE S256 over a loopback redirect, against an Okta issuer. Nothing is Redis-specific except the issuer URL. Roughly 900 lines of security-critical code.
2. **The SM token-exchange** (`sm_api`). This talks to the Redis Cloud console / session-management backend using session cookie plus CSRF auth. Its endpoints are `/login`, `/csrf`, `/users/me`, `/accounts`, `/accounts/cloud-api/cloudApiAccessKey`, `/accounts/cloud-api/cloudApiKeys`. Its purpose is to turn an OIDC session into a minted public-API key and secret.
3. **Orchestration and result** (`authenticator`, `MintedCredentials`). Sequences login then exchange and returns a credential to persist.

## For contrast: redis-cloud-rs today

`redis-cloud` 0.11.0 (repo redis-developer/redis-cloud-rs, published to crates.io) is the typed client for the Redis Cloud **public REST API** at `https://api.redislabs.com/v1`. It authenticates with an `api_key` plus `api_secret` that the caller already has. Its modules are account, acl, cloud_accounts, connectivity, cost_report, fixed, flexible, tasks, users. It has no auth, signin, OIDC, or signup anywhere, and its harmonization plan does not mention any.

The consequence: the SM/console API in the PR is a **sibling surface** to what redis-cloud-rs models, not a fit inside it. It is a different host, a different auth model (cookie plus CSRF rather than API key), and it is internal and undocumented rather than the public OpenAPI-modeled REST surface.

## Recommended layering

### 1. redisctl proper: the command, orchestration, and persistence

The `cloud auth login` command, the thin authenticator that sequences login then exchange, and turning `MintedCredentials` into a stored cloud profile all belong here. `redisctl-core` already owns `config/config.rs` and `config/credential.rs` (including keyring storage), so the persistence half is unambiguous. Keep the orchestrator thin: it should consume the two libraries below and hand the result to the existing credential storage, not contain the flows itself.

### 2. Its own library: the generic OIDC flows

This is the strongest recommendation. Device grant plus auth-code plus PKCE S256 is provider-agnostic and security-critical. As written, a small team would own hand-rolled OAuth (state and nonce handling, token redaction, timing, refresh) indefinitely.

Prefer not to hand-roll it: depend on the mature `oauth2` crate, which implements all three flows. If there is a concrete reason to hand-roll (loopback UX control, avoiding the dependency), then it should be its own small crate, not buried in `redisctl-core`, so it is independently testable and does not couple the CLI's shared layer to OAuth internals.

### 3. Its own library: the Cloud signin / mint client

This is the subtle one. The SM exchange is reusable Redis Cloud domain code, so it does not belong in redisctl. But it should also stay out of redis-cloud-rs proper: that crate's identity is the clean, OpenAPI-modeled, public-REST client, and the SM API is a reverse-engineered, undocumented, cookie and CSRF console backend that can change without notice. Mixing a fragile internal surface into the public-API SDK muddies its stability guarantees.

Give it its own small crate (for example `redis-cloud-signin`), published alongside redis-cloud-rs with its own release cadence, so the fragile part is isolated and both redis-cloud-rs and redisctl stay uncontaminated.

### Crate layout sketch

```
oauth2 (external crate)            redis-cloud-signin (new, own crate)
  device grant                       POST /login, /csrf, /users/me
  auth code + PKCE S256              mint cloudApiKeys -> (api_key, api_secret, api_url)
  token endpoint + refresh          session cookie + CSRF handling
        \                              /
         \                            /
          v                          v
   redisctl-core::auth (thin orchestration)
     CloudAuthenticator: login (oauth2) -> exchange (signin) -> MintedCredentials
     persist via existing config::credential storage
                    |
                    v
   redisctl cli: `cloud auth login`
```

Dependency edges point one way: the CLI depends on core, core depends on the two libraries, and neither library depends on redisctl. redis-cloud-rs is untouched.

## Why decide this now

This is PR #1 of a chain. The follow-ups will build the CLI on top of `redisctl_core::auth::SmApiClient`, `CloudAuthenticator`, and the other public types. Once the CLI depends on those, moving them across a crate boundary is a breaking change. The layering is cheapest to fix now, while it is still a pure library with no consumers. This is the same now-or-never logic that applies to the rest of the pre-1.0 surface.

There is also a governance angle. redis-cloud-rs is solely owned, so concentrating more security-critical, undocumented surface into your crates raises the bus-factor and audit burden. Offloading the generic half to `oauth2` and isolating the Cloud-specific half in a clearly scoped crate reduces both.

## What the PR gets right

The feedback should be calibrated: the hygiene here is genuinely good. The PR is fully additive with no surface changes, `Debug` is manually redacted on both `MintedCredentials` and `TokenSet`, there is no secret logging anywhere, `AuthError` is a typed error rather than `anyhow`, the new dependencies are wired through the workspace, and there are 34 wiremock tests covering success and error paths for each flow. Every `unwrap` is in test code. The only issue is layering: it is a monolith spanning three homes.

Its red CI is the pre-existing advisories, not its own code; that clears once the advisory fix lands.

## Open questions for the team

1. Is redis-cloud-rs intended to eventually be the SDK for all Cloud APIs (in which case the signin client could be a feature-gated module there), or should it stay strictly the public REST client (which argues for the separate `redis-cloud-signin` crate)?
2. Is there a reason not to use the `oauth2` crate for the generic flows?
3. Which OIDC flow is the default for `cloud auth login`: device grant (agent and headless friendly) or loopback (interactive browser)? This affects the CLI UX and the profile-persistence follow-up.
