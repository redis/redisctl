//! Parent-directory refusals must precede project writes and Cloud requests.

mod init_common;

use predicates::prelude::*;

const CONTENT: &str = "user content\n";
#[cfg(feature = "cloud")]
const CASES: &[(&str, &str)] = &[
    (".agents", "claude"),
    #[cfg(unix)]
    (".claude", "claude"),
    (".cursor", "cursor"),
    (".vscode", "vscode"),
    (".agents/skills", "codex"),
    #[cfg(unix)]
    (".claude/skills", "claude"),
];

fn assert_preserved(project: &std::path::Path, blocked: &str) {
    assert_eq!(
        std::fs::read_to_string(project.join(blocked)).unwrap(),
        CONTENT
    );
    for file in [
        ".gitignore",
        ".env",
        ".env.example",
        ".mcp.json",
        ".agents/skills/redis-project-setup/SKILL.md",
    ] {
        assert!(!project.join(file).exists(), "unexpected write: {file}");
    }
}

fn refused_parent(blocked: &str, agent: &str) {
    for dry_run in [true, false] {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join(blocked);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, CONTENT).unwrap();
        let mut cmd = init_common::hermetic_redisctl();
        let config = cmd.home().join("config.toml");
        let path = cmd.downloaders();
        cmd.env("PATH", path)
            .timeout(std::time::Duration::from_secs(15));
        cmd.current_dir(project.path())
            .arg("--config-file")
            .arg(config)
            .args([
                "init",
                "--agent",
                agent,
                "--url",
                "redis://127.0.0.1:1",
                "--no-skills",
                "--no-install-cli",
                "--no-telemetry",
            ]);
        if dry_run {
            cmd.arg("--dry-run");
        }
        cmd.assert()
            .code(1)
            .stderr(predicate::str::contains(format!("'{blocked}'")))
            .stderr(predicate::str::contains("not a directory"))
            .stdout(predicate::str::contains("created").not());
        assert_preserved(project.path(), blocked);
    }
}

#[test]
fn non_directory_parent_agents_is_refused() {
    refused_parent(".agents", "claude");
}

#[test]
#[cfg(unix)]
fn non_directory_parent_claude_is_refused() {
    refused_parent(".claude", "claude");
}

#[test]
fn non_directory_parent_cursor_is_refused() {
    refused_parent(".cursor", "cursor");
}

#[test]
fn non_directory_parent_vscode_is_refused() {
    refused_parent(".vscode", "vscode");
}

#[test]
fn non_directory_parent_nested_paths_are_refused() {
    refused_parent(".agents/skills", "codex");
    #[cfg(unix)]
    refused_parent(".claude/skills", "claude");
}

#[cfg(feature = "cloud")]
#[tokio::test(flavor = "multi_thread")]
async fn non_directory_parent_stops_cloud_before_any_request() {
    let server = wiremock::MockServer::start().await;
    for &(blocked, agent) in CASES {
        for dry_run in [true, false] {
            let project = tempfile::tempdir().unwrap();
            let path = project.path().join(blocked);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, CONTENT).unwrap();
            let mut cmd = init_common::hermetic_redisctl();
            let config = cmd.home().join("config.toml");
            let path = cmd.downloaders();
            cmd.env("PATH", path)
                .timeout(std::time::Duration::from_secs(15));
            std::fs::write(&config, format!(
                "[profiles.test]\ndeployment_type = \"cloud\"\napi_key = \"synthetic-key\"\napi_secret = \"synthetic-secret\"\napi_url = \"{}\"\n",
                server.uri()
            )).unwrap();
            for variable in [
                "REDIS_CLOUD_API_KEY",
                "REDIS_CLOUD_SECRET_KEY",
                "REDIS_CLOUD_API_URL",
                "REDISCTL_PROFILE",
            ] {
                cmd.env_remove(variable);
            }
            cmd.current_dir(project.path())
                .arg("--config-file")
                .arg(&config)
                .args([
                    "--profile",
                    "test",
                    "init",
                    "--cloud",
                    "--name",
                    "new-db",
                    "--agent",
                    agent,
                    "--no-skills",
                    "--no-install-cli",
                    "--no-telemetry",
                ]);
            if dry_run {
                cmd.arg("--dry-run");
            }
            cmd.assert()
                .code(1)
                .stderr(predicate::str::contains(format!("'{blocked}'")))
                .stderr(predicate::str::contains("not a directory"));
            assert_preserved(project.path(), blocked);
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[test]
fn an_unselected_agent_directory_is_untouched() {
    for blocked in [".claude", ".cursor", ".vscode"] {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join(blocked), CONTENT).unwrap();
        let mut cmd = init_common::hermetic_redisctl();
        let config = cmd.home().join("config.toml");
        let path = cmd.downloaders();
        cmd.env("PATH", path)
            .timeout(std::time::Duration::from_secs(15))
            .current_dir(project.path())
            .arg("--config-file")
            .arg(config)
            .args([
                "init",
                "--agent",
                "codex",
                "--url",
                "redis://127.0.0.1:1",
                "--no-skills",
                "--no-install-cli",
                "--no-telemetry",
                "--dry-run",
            ])
            .assert()
            .success();
        assert_preserved(project.path(), blocked);
    }
}
