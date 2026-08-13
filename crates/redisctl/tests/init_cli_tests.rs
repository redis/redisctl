//! CLI-level tests for `redisctl init`: wiring, detection, dry-run, env contract.
//!
//! Everything here is hermetic: temp directories only, no Docker (the dry-run tests
//! pass --url so the database step never probes the Docker daemon), and network
//! contact limited to refused loopback connections.

use assert_cmd::Command;
use predicates::prelude::*;

fn redisctl() -> Command {
    Command::cargo_bin("redisctl").unwrap()
}

/// A fake redis/agent-skills checkout, so non-dry runs never reach npx or the
/// network for the skills step.
fn skills_fixture() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    let skill = repo.path().join("skills/redis-basics");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "# basics\n").unwrap();
    repo
}

#[test]
fn init_is_listed_in_top_level_help() {
    redisctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("init"));
}

#[test]
fn init_help_documents_the_flags() {
    redisctl()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--url"))
        .stdout(predicate::str::contains("--agent"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn unknown_agent_is_a_usage_error() {
    redisctl()
        .args(["init", "--agent", "bogus"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("possible values"));
}

#[test]
fn dry_run_detects_a_node_project_and_plans_the_env_wiring() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("package.json"),
        r#"{"name":"demo-shop","dependencies":{"express":"^4"}}"#,
    )
    .unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis://localhost:6379",
            "--agent",
            "claude",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("demo-shop"))
        .stdout(predicate::str::contains("(node, npm, express)"))
        .stdout(predicate::str::contains("Plan"))
        .stdout(predicate::str::contains("created   .env"))
        .stdout(predicate::str::contains(".gitignore"))
        .stdout(predicate::str::contains(
            ".agents/skills/redis-project-setup/SKILL.md",
        ))
        .stdout(predicate::str::contains(".mcp.json"))
        .stdout(predicate::str::contains("Dry run complete"));
    // Nothing written: the manifest is still the only file.
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn unknown_runtime_gets_a_note_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run", "--url", "redis://localhost:6379"])
        .assert()
        .success()
        .stdout(predicate::str::contains("no package manifest detected"));
}

#[test]
fn provided_url_is_masked_in_the_plan_subject() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis-cli -u redis://default:s3cret@host.example:12000",
            "--name",
            "my-db",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "redis://default:****@host.example:12000",
        ))
        .stdout(predicate::str::contains("[my-db]"))
        .stdout(predicate::str::contains("s3cret").not());
}

#[test]
fn url_without_a_redis_url_is_a_validation_error() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "http://not-redis"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains(
            "no redis:// or rediss:// URL found",
        ))
        // The generic file-format tips make no sense for a connection string.
        .stderr(predicate::str::contains("JSON/YAML").not());
}

#[test]
fn rejected_url_input_is_credential_masked_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "redisx://default:s3cret@host:6379"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("redisx://default:****@host:6379"))
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn rejected_url_input_is_credential_masked_in_the_json_envelope() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "-o",
            "json",
            "--url",
            "redisx://default:s3cret@host:6379",
        ])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("****"))
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn verbatim_unquoted_console_paste_is_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    // The shell-split form of an unquoted paste: -u must not parse as a redisctl
    // flag. The URL points at a refused loopback port, so the run proceeds past
    // parsing, writes the contract, and fails at validation - proving both halves.
    redisctl()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args([
            "init",
            "redis-cli",
            "-u",
            "redis://default:s3cret@127.0.0.1:9",
        ])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("redis://default:****@127.0.0.1:9"))
        .stdout(predicate::str::contains("via provided URL"))
        .stdout(predicate::str::contains("s3cret").not())
        .stderr(predicate::str::contains("could not talk to Redis"))
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn dead_url_writes_env_then_fails_validation_with_the_stale_hint() {
    let dir = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    redisctl()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--url", "redis://127.0.0.1:9"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("Validate"))
        .stderr(predicate::str::contains("could not talk to Redis"))
        .stderr(predicate::str::contains("remove REDIS_URL from .env"))
        // The message carries its own remedy; the profile-oriented connection tips
        // do not apply to init and must stay out.
        .stderr(predicate::str::contains("profile").not());
    // .env is written before validation, so the failure can name it and the user
    // can fix or remove the stale URL.
    let env = std::fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env.contains("REDIS_URL=\"redis://127.0.0.1:9\""), "{env}");
    let gitignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.contains(".env"), "{gitignore}");
    let example = std::fs::read_to_string(dir.path().join(".env.example")).unwrap();
    assert!(
        example.contains("REDIS_URL=\"redis://localhost:6379\""),
        "{example}"
    );
}

#[test]
fn rerun_with_a_url_reports_env_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "REDIS_URL=\"redis://127.0.0.1:9\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join(".gitignore"), ".env\n").unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run", "--url", "redis://127.0.0.1:9"])
        .assert()
        .success()
        .stdout(predicate::str::contains("unchanged .env"))
        .stdout(predicate::str::contains("unchanged .gitignore"));
}

#[test]
fn a_different_existing_redis_url_is_kept_not_clobbered() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), "REDIS_URL=\"redis://keep-me:1\"\n").unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis://default:s3cret@other:2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("kept"))
        .stdout(predicate::str::contains("left untouched"))
        // A short token here once collided with a random tempdir name printed in the
        // Project line; whole-output negative assertions need collision-proof tokens.
        .stdout(predicate::str::contains("s3cret").not());
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        "REDIS_URL=\"redis://keep-me:1\"\n"
    );
}

#[test]
fn rust_project_plans_cargo_add_and_the_example_contract() {
    let dir = tempfile::tempdir().unwrap();
    // cargo is guaranteed on PATH wherever these tests run.
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--dry-run", "--url", "redis://localhost:6379"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".env.example"))
        .stdout(predicate::str::contains("would run: cargo add redis"))
        .stdout(predicate::str::contains("redis-cli"));
}

#[test]
fn no_install_cli_is_respected() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--no-install-cli",
            "--url",
            "redis://localhost:6379",
        ])
        .assert()
        .success()
        // Whether redis-cli is installed here or not, the line must never plan the
        // installer under --no-install-cli.
        .stdout(predicate::str::contains("would install via").not())
        .stdout(predicate::str::contains("redis-cli"));
}

#[test]
fn dry_run_plans_the_standard_skills_install() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--url",
            "redis://localhost:6379",
            "--agent",
            "claude,codex",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "would run: npx -y skills@latest add redis/agent-skills -s * -a claude-code -a codex -y",
        ));
}

/// The installer's own words must reach the user; a generic "(offline?)" guess sent
/// a real demo failure down the wrong path.
#[test]
#[cfg(unix)]
fn a_failed_npx_run_surfaces_the_installers_error() {
    let dir = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let fake_npx = bin.path().join("npx");
    std::fs::write(
        &fake_npx,
        "#!/bin/sh\necho 'Unknown agent: codex' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(
        &fake_npx,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    // PATH holds only the fake, so the skills step runs it and nothing else on the
    // machine can answer has_bin probes. Validation still fails on the refused port.
    redisctl()
        .current_dir(dir.path())
        .env("PATH", bin.path())
        .args([
            "init",
            "--url",
            "redis://127.0.0.1:9",
            "--no-install-cli",
            "--agent",
            "claude,codex",
        ])
        .assert()
        .code(10)
        .stdout(predicate::str::contains(
            "npx skills add failed: Unknown agent: codex - re-run it yourself",
        ))
        .stdout(predicate::str::contains("(offline?)").not());
}

#[test]
fn an_explicit_checkout_installs_skills_offline() {
    let dir = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    // Validation fails (refused port) but the apply half has already landed.
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--url",
            "redis://127.0.0.1:9",
            "--agent",
            "claude,codex",
            "--skills-repo",
            repo.path().to_str().unwrap(),
        ])
        .assert()
        .code(10)
        .stdout(predicate::str::contains(".agents/skills/redis-basics/"))
        .stdout(predicate::str::contains(
            ".agents/skills/redis-project-setup/SKILL.md",
        ))
        .stdout(predicate::str::contains(
            ".claude/skills/redis-project-setup",
        ))
        // Checkout copies place no symlinks themselves, so the copied skill gets one.
        .stdout(predicate::str::contains(".claude/skills/redis-basics"));
    assert!(
        dir.path()
            .join(".agents/skills/redis-basics/SKILL.md")
            .exists()
    );
    let skill = std::fs::read_to_string(
        dir.path()
            .join(".agents/skills/redis-project-setup/SKILL.md"),
    )
    .unwrap();
    assert!(skill.contains("redis-basics"), "{skill}");
    assert!(
        skill.contains("(external, e.g. Redis Cloud)") || skill.contains("external (not managed"),
        "{skill}"
    );
    assert!(!skill.contains("redis://"), "no URL in the skill: {skill}");
    // Absent agent docs stay absent - the skill is the only artifact.
    assert!(!dir.path().join("AGENTS.md").exists());
    assert!(!dir.path().join("CLAUDE.md").exists());
    let mcp = std::fs::read_to_string(dir.path().join(".mcp.json")).unwrap();
    assert!(mcp.contains("$REDIS_URL"), "{mcp}");
    assert!(!mcp.contains("redis://"), "credential-free: {mcp}");
}

#[test]
fn preexisting_agents_and_claude_md_stay_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    std::fs::write(
        dir.path().join("CLAUDE.md"),
        "mine
",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("AGENTS.md"),
        "also mine
",
    )
    .unwrap();
    redisctl()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--url", "redis://127.0.0.1:9", "--no-install-cli"])
        .assert()
        .code(10);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
        "mine
"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
        "also mine
"
    );
}

#[test]
#[ignore = "requires npx + network (real skills CLI against redis/agent-skills)"]
fn npx_path_installs_the_official_skills_with_a_lock() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "redis://127.0.0.1:9", "--no-install-cli"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("npx skills add"))
        .stdout(predicate::str::contains("skills-lock.json"));
    assert!(dir.path().join("skills-lock.json").exists());
    // Re-run: the lock diff reads everything back as unchanged.
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "redis://127.0.0.1:9", "--no-install-cli"])
        .assert()
        .code(10)
        .stdout(predicate::str::contains("unchanged .agents/skills/"));
}

#[test]
fn defaults_flag_parses_and_stays_non_interactive() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--defaults",
            "--dry-run",
            "--url",
            "redis://localhost:6379",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run complete"));
    redisctl()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--defaults"));
}
