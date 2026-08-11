//! The project side: what it is built with and which coding agents to configure.

use std::path::Path;

use crate::cli::AgentArg;

use super::util::{exists, has_bin, read_if};

/// The coding agents `redisctl init` can configure, in canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Cursor,
    Vscode,
    Codex,
}

pub const KNOWN_AGENTS: [Agent; 4] = [Agent::Claude, Agent::Cursor, Agent::Vscode, Agent::Codex];

impl Agent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Cursor => "cursor",
            Agent::Vscode => "vscode",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Node,
    Python,
    Go,
    Rust,
    Java,
    Unknown,
}

impl Runtime {
    pub fn as_str(&self) -> &'static str {
        match self {
            Runtime::Node => "node",
            Runtime::Python => "python",
            Runtime::Go => "go",
            Runtime::Rust => "rust",
            Runtime::Java => "java",
            Runtime::Unknown => "unknown",
        }
    }
}

#[derive(Debug)]
pub struct Project {
    pub name: String,
    pub runtime: Runtime,
    pub pm: Option<&'static str>,
    pub framework: Option<String>,
    /// Existing agent-config markers in the project, in display order.
    pub agent_markers: Vec<(&'static str, bool)>,
}

/// Lockfiles first, so the bare manifest cannot shadow the real package manager.
const PACKAGE_MANAGERS: [(&str, &str); 6] = [
    ("bun.lockb", "bun"),
    ("bun.lock", "bun"),
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "yarn"),
    ("package-lock.json", "npm"),
    ("package.json", "npm"),
];

const FRAMEWORKS: [&str; 11] = [
    "next",
    "nuxt",
    "astro",
    "remix",
    "@remix-run/node",
    "express",
    "fastify",
    "hono",
    "koa",
    "nest",
    "@nestjs/core",
];

/// Detect what the project in `dir` is built with.
pub fn detect(dir: &Path) -> Project {
    let mut pm = PACKAGE_MANAGERS
        .iter()
        .find(|(file, _)| exists(dir, file))
        .map(|(_, pm)| *pm);

    let mut runtime = Runtime::Unknown;
    let mut name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    let mut framework = None;
    if exists(dir, "package.json") {
        runtime = Runtime::Node;
        if let Some(pkg) = read_if(dir, "package.json")
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        {
            if let Some(pkg_name) = pkg["name"].as_str() {
                name = pkg_name.to_string();
            }
            framework = FRAMEWORKS
                .iter()
                .find(|f| !pkg["dependencies"][f].is_null() || !pkg["devDependencies"][f].is_null())
                .map(|f| f.to_string());
        }
        // Unparseable package.json: keep defaults.
    } else if exists(dir, "pyproject.toml") || exists(dir, "requirements.txt") {
        runtime = Runtime::Python;
    } else if exists(dir, "go.mod") {
        runtime = Runtime::Go;
    } else if exists(dir, "Cargo.toml") {
        runtime = Runtime::Rust;
    } else if exists(dir, "pom.xml")
        || exists(dir, "build.gradle")
        || exists(dir, "build.gradle.kts")
    {
        runtime = Runtime::Java;
        pm = Some(if exists(dir, "pom.xml") {
            "maven"
        } else {
            "gradle"
        });
    }

    let agent_markers = vec![
        ("AGENTS.md", exists(dir, "AGENTS.md")),
        ("CLAUDE.md", exists(dir, "CLAUDE.md")),
        (".claude/", exists(dir, ".claude")),
        (".mcp.json", exists(dir, ".mcp.json")),
        (".cursor/", exists(dir, ".cursor")),
    ];
    Project {
        name,
        runtime,
        pm,
        framework,
        agent_markers,
    }
}

/// The agents to configure: an explicit `--agent` list wins (canonical order, `all`
/// expands), otherwise whatever was detected, otherwise all of them - having no agent
/// tooling detected is not a reason to configure none.
pub fn choose_agents(requested: &[AgentArg], detected: &[Agent]) -> Vec<Agent> {
    if !requested.is_empty() {
        if requested.contains(&AgentArg::All) {
            return KNOWN_AGENTS.to_vec();
        }
        return KNOWN_AGENTS
            .into_iter()
            .filter(|agent| {
                requested.iter().any(|r| {
                    matches!(
                        (r, agent),
                        (AgentArg::Claude, Agent::Claude)
                            | (AgentArg::Cursor, Agent::Cursor)
                            | (AgentArg::Vscode, Agent::Vscode)
                            | (AgentArg::Codex, Agent::Codex)
                    )
                })
            })
            .collect();
    }
    if detected.is_empty() {
        KNOWN_AGENTS.to_vec()
    } else {
        detected.to_vec()
    }
}

/// Detect which agent tools this machine/project uses.
pub fn detect_agents(dir: &Path) -> Vec<Agent> {
    let home = std::env::home_dir().unwrap_or_default();
    let detectors: [(Agent, bool); 4] = [
        (
            Agent::Claude,
            has_bin("claude")
                || exists(dir, ".claude")
                || exists(dir, "CLAUDE.md")
                || exists(dir, ".mcp.json"),
        ),
        (
            Agent::Cursor,
            has_bin("cursor") || exists(dir, ".cursor") || home.join(".cursor").exists(),
        ),
        (Agent::Vscode, has_bin("code") || exists(dir, ".vscode")),
        (
            Agent::Codex,
            has_bin("codex") || home.join(".codex").exists(),
        ),
    ];
    detectors
        .into_iter()
        .filter_map(|(agent, found)| found.then_some(agent))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn detects_node_with_npm_name_and_framework() {
        let dir = dir_with(&[
            (
                "package.json",
                r#"{"name":"demo-shop","dependencies":{"express":"^4.18.2"}}"#,
            ),
            ("package-lock.json", "{}"),
        ]);
        let p = detect(dir.path());
        assert_eq!(p.runtime, Runtime::Node);
        assert_eq!(p.pm, Some("npm"));
        assert_eq!(p.name, "demo-shop");
        assert_eq!(p.framework.as_deref(), Some("express"));
    }

    #[test]
    fn lockfile_beats_bare_manifest_for_package_manager() {
        let dir = dir_with(&[("package.json", r#"{"name":"x"}"#), ("pnpm-lock.yaml", "")]);
        assert_eq!(detect(dir.path()).pm, Some("pnpm"));
    }

    #[test]
    fn framework_found_in_dev_dependencies() {
        let dir = dir_with(&[(
            "package.json",
            r#"{"name":"x","devDependencies":{"fastify":"^5"}}"#,
        )]);
        assert_eq!(detect(dir.path()).framework.as_deref(), Some("fastify"));
    }

    #[test]
    fn unparseable_package_json_keeps_node_with_defaults() {
        let dir = dir_with(&[("package.json", "not json")]);
        let p = detect(dir.path());
        assert_eq!(p.runtime, Runtime::Node);
        let basename = dir.path().file_name().unwrap().to_string_lossy();
        assert_eq!(p.name, basename);
        assert_eq!(p.framework, None);
    }

    #[test]
    fn detects_python_go_rust_java() {
        assert_eq!(
            detect(dir_with(&[("pyproject.toml", "")]).path()).runtime,
            Runtime::Python
        );
        assert_eq!(
            detect(dir_with(&[("requirements.txt", "")]).path()).runtime,
            Runtime::Python
        );
        assert_eq!(
            detect(dir_with(&[("go.mod", "")]).path()).runtime,
            Runtime::Go
        );
        assert_eq!(
            detect(dir_with(&[("Cargo.toml", "")]).path()).runtime,
            Runtime::Rust
        );
        let maven = detect(dir_with(&[("pom.xml", "")]).path());
        assert_eq!((maven.runtime, maven.pm), (Runtime::Java, Some("maven")));
        let gradle = detect(dir_with(&[("build.gradle.kts", "")]).path());
        assert_eq!((gradle.runtime, gradle.pm), (Runtime::Java, Some("gradle")));
    }

    #[test]
    fn empty_dir_is_unknown_named_after_the_directory() {
        let dir = tempfile::tempdir().unwrap();
        let p = detect(dir.path());
        assert_eq!(p.runtime, Runtime::Unknown);
        assert_eq!(p.pm, None);
        let basename = dir.path().file_name().unwrap().to_string_lossy();
        assert_eq!(p.name, basename);
    }

    #[test]
    fn agent_markers_reflect_existing_files() {
        let dir = dir_with(&[("AGENTS.md", ""), (".mcp.json", "{}")]);
        fs::create_dir(dir.path().join(".cursor")).unwrap();
        let markers = detect(dir.path()).agent_markers;
        assert_eq!(
            markers,
            vec![
                ("AGENTS.md", true),
                ("CLAUDE.md", false),
                (".claude/", false),
                (".mcp.json", true),
                (".cursor/", true),
            ]
        );
    }

    #[test]
    fn explicit_agents_win_in_canonical_order() {
        let chosen = choose_agents(&[AgentArg::Codex, AgentArg::Claude], &[Agent::Vscode]);
        assert_eq!(chosen, vec![Agent::Claude, Agent::Codex]);
    }

    #[test]
    fn all_expands_to_every_agent() {
        assert_eq!(choose_agents(&[AgentArg::All], &[]), KNOWN_AGENTS.to_vec());
    }

    #[test]
    fn detected_agents_used_when_no_flag() {
        assert_eq!(choose_agents(&[], &[Agent::Cursor]), vec![Agent::Cursor]);
    }

    #[test]
    fn nothing_detected_configures_all() {
        assert_eq!(choose_agents(&[], &[]), KNOWN_AGENTS.to_vec());
    }
}
