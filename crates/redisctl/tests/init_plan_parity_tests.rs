//! Dry-run parity: for the same inputs, `init --dry-run` lists exactly the change
//! lines a real run then reports.

mod init_common;

use assert_cmd::Command;
use init_common::skills_fixture;
use std::path::Path;

struct Sandbox {
    project: tempfile::TempDir,
    home: tempfile::TempDir,
    skills: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            project: tempfile::tempdir().unwrap(),
            home: tempfile::tempdir().unwrap(),
            skills: skills_fixture(),
        }
    }

    fn init(&self, agents: &str, dry_run: bool) -> String {
        let mut cmd = self.command(agents);
        cmd.arg("--skills-repo").arg(self.skills.path());
        run(cmd, dry_run)
    }

    /// No checkout and no npx on PATH: the skills step has nothing to run.
    fn init_without_npx(&self, agents: &str, dry_run: bool) -> String {
        let mut cmd = self.command(agents);
        cmd.env_remove("REDISCTL_INIT_SKILLS_REPO")
            .env("PATH", self.home.path().join("empty-bin"));
        run(cmd, dry_run)
    }

    fn command(&self, agents: &str) -> Command {
        let mut cmd = Command::cargo_bin("redisctl").unwrap();
        cmd.current_dir(self.project.path())
            .env("HOME", self.home.path())
            .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
            .arg("--config-file")
            .arg(self.home.path().join("config.toml"))
            .args([
                "init",
                "--no-telemetry",
                "--no-install-cli",
                "--url",
                "redis://127.0.0.1:1",
                "--agent",
                agents,
            ]);
        cmd
    }
}

fn run(mut cmd: Command, dry_run: bool) -> String {
    if dry_run {
        cmd.arg("--dry-run");
    }
    let output = cmd.output().unwrap();
    // The refused port fails validation after apply; the dry run succeeds.
    assert_eq!(
        output.status.code(),
        Some(if dry_run { 0 } else { 10 }),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// `(status, subject)` for every change line in the section that starts with
/// `header`. A line renders as `  <icon> <status:9> <subject>[  <note>]`.
fn change_lines(stdout: &str, header: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .skip_while(|line| !line.starts_with(header))
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .map(|line| {
            let rest: String = line.chars().skip(4).collect();
            let (status, subject) = rest.split_at(10);
            let subject = subject.split("  ").next().unwrap_or_default();
            (status.trim().to_string(), subject.to_string())
        })
        .collect()
}

fn assert_parity(agents: &str) {
    let sandbox = Sandbox::new();
    let planned = change_lines(&sandbox.init(agents, true), "Plan  (");
    let applied = change_lines(&sandbox.init(agents, false), "Changes  (");
    assert!(!planned.is_empty(), "no plan lines for {agents}");
    assert_eq!(planned, applied, "dry run vs apply for --agent {agents}");

    let mut paths: Vec<&str> = applied
        .iter()
        .map(|(_, subject)| subject.trim_end_matches('/'))
        .collect();
    paths.sort();
    let before = paths.len();
    paths.dedup();
    assert_eq!(
        before,
        paths.len(),
        "a path is reported twice for {agents}: {applied:?}"
    );

    // A re-run over the applied state plans the same lines it reports.
    let replanned = change_lines(&sandbox.init(agents, true), "Plan  (");
    let reapplied = change_lines(&sandbox.init(agents, false), "Changes  (");
    assert_eq!(replanned, reapplied, "re-run {agents}");
    assert!(
        reapplied
            .iter()
            .filter(|(_, subject)| subject.contains("skills/"))
            .all(|(status, _)| status == "unchanged"),
        "re-run {agents}: {reapplied:?}"
    );
}

#[test]
fn solo_claude_dry_run_matches_apply() {
    assert_parity("claude");
}

#[test]
fn claude_and_cursor_dry_run_matches_apply() {
    assert_parity("claude,cursor");
}

#[test]
fn all_agents_dry_run_matches_apply() {
    assert_parity("claude,cursor,vscode,codex");
}

#[test]
fn a_missing_npx_is_planned_as_the_skip_a_real_run_reports() {
    for (agents, dir) in [
        ("claude", ".claude/skills/"),
        ("claude,cursor", ".agents/skills/"),
    ] {
        let sandbox = Sandbox::new();
        let planned = change_lines(&sandbox.init_without_npx(agents, true), "Plan  (");
        let applied = change_lines(&sandbox.init_without_npx(agents, false), "Changes  (");
        assert_eq!(planned, applied, "dry run vs apply for --agent {agents}");
        assert!(
            applied.contains(&("skipped".to_string(), dir.to_string())),
            "--agent {agents}: {applied:?}"
        );
    }
}

#[test]
fn next_steps_name_the_dir_a_skipped_skills_install_targets() {
    let sandbox = Sandbox::new();
    let port = init_common::fake_redis();
    let output = Command::cargo_bin("redisctl")
        .unwrap()
        .current_dir(sandbox.project.path())
        .env("HOME", sandbox.home.path())
        .env("PATH", sandbox.home.path().join("empty-bin"))
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .env_remove("REDISCTL_INIT_SKILLS_REPO")
        .arg("--config-file")
        .arg(sandbox.home.path().join("config.toml"))
        .args([
            "init",
            "--no-telemetry",
            "--no-install-cli",
            "--agent",
            "claude",
        ])
        .args(["--url", &format!("redis://127.0.0.1:{port}")])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.contains("the skills in .claude/skills/."),
        "{stdout}"
    );
}

#[test]
fn solo_claude_copies_skills_into_claudes_own_dir() {
    let sandbox = Sandbox::new();
    sandbox.init("claude", false);
    let project = sandbox.project.path();
    let copied = project.join(".claude/skills/redis-basics");
    assert!(copied.join("SKILL.md").exists());
    assert!(!copied.is_symlink());
    assert!(!Path::new(&project.join(".agents/skills/redis-basics")).exists());
    let link = project.join(".claude/skills/redis-project-setup");
    assert!(link.is_symlink());
    assert!(link.join("SKILL.md").exists());
}
