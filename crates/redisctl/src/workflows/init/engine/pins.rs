//! The tools `redisctl init` installs with, or writes into committed MCP configs, at the
//! exact version they were checked with. Only the top-level package is pinned: uvx and
//! npx still resolve its dependencies when a machine first runs it. (The local Redis
//! container image is chosen in `docker.rs`, not here.) Dependabot and Renovate cannot
//! see these strings: bump them with the release checklist in CONTRIBUTING.md. A pin
//! reaches an existing project when init runs there again, because a differing entry is
//! replaced.

/// The data-plane MCP server, run by uvx. The REDIS_PWD handoff in `mcp::SPLIT_PASSWORD`
/// was verified against this version.
pub(crate) const REDIS_MCP_SERVER: &str = "redis-mcp-server==0.5.1";

/// The Docker fallback. Docker's MCP catalog builds `mcp/redis` from a redis/mcp-redis
/// commit (`source.commit` in docker/mcp-registry's `servers/redis/server.yaml`) and
/// publishes only `latest`, so it is pinned by digest. This build is commit 53df2bcb:
/// it reports 0.5.0, has the same `src/` as 0.5.1, and its own locked dependencies.
/// The image has no ENTRYPOINT, so the launcher names `uv run redis-mcp-server`.
pub(crate) const REDIS_MCP_IMAGE: &str =
    "mcp/redis@sha256:e886a7e9990a084a20d46adcea8e7c89d539271f1b453ffabdce4c3f96f27fa0";

/// The Context Retriever bridge. Must stay at 0.1.16 or later (CVE-2025-6514).
pub(crate) const MCP_REMOTE: &str = "mcp-remote@0.15.2";

/// The skills CLI. It still installs redis/agent-skills from its default branch,
/// which has no tags to pin to.
pub(crate) const SKILLS_CLI: &str = "skills@1.7.4";

/// The official redis-cli installer, and the version it is told to install through
/// its documented `REDIS_CLI_VERSION` variable (unless the user already set one).
pub(crate) const REDIS_CLI_INSTALLER: &str = "https://packages.redis.io/redis-cli/install.sh";
pub(crate) const REDIS_CLI_VERSION: &str = "8.10.2";

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(version: &str) -> bool {
        let parts: Vec<&str> = version.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    }

    fn version_of<'a>(spec: &'a str, separator: &str) -> &'a str {
        spec.rsplit_once(separator).map(|(_, v)| v).unwrap_or("")
    }

    #[test]
    fn every_pin_is_exact() {
        assert!(
            exact(version_of(REDIS_MCP_SERVER, "==")),
            "{REDIS_MCP_SERVER}"
        );
        assert!(exact(version_of(MCP_REMOTE, "@")), "{MCP_REMOTE}");
        assert!(exact(version_of(SKILLS_CLI, "@")), "{SKILLS_CLI}");
        assert!(exact(REDIS_CLI_VERSION), "{REDIS_CLI_VERSION}");
        let digest = version_of(REDIS_MCP_IMAGE, "@sha256:");
        assert!(
            digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()),
            "{REDIS_MCP_IMAGE}"
        );
    }

    #[test]
    fn mcp_remote_stays_past_cve_2025_6514() {
        let version: Vec<u32> = version_of(MCP_REMOTE, "@")
            .split('.')
            .map(|part| part.parse().unwrap())
            .collect();
        assert!(version >= vec![0, 1, 16], "{MCP_REMOTE}");
    }

    #[test]
    fn the_installer_is_fetched_over_https() {
        assert!(REDIS_CLI_INSTALLER.starts_with("https://"));
    }
}
