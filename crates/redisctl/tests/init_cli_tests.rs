//! CLI-level tests for `redisctl init`: wiring, detection, dry-run, env contract.
//!
//! Everything here is hermetic: temp directories only, no Docker (the dry-run tests
//! pass --url so the database step never probes the Docker daemon), and network
//! contact limited to refused loopback connections.

use assert_cmd::Command;
use predicates::prelude::*;

fn redisctl() -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    // Explicitly off: a key in the developer's (or CI's) environment must not
    // make test runs send telemetry.
    cmd.env("REDISCTL_INIT_AMPLITUDE_KEY", "");
    cmd
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
fn an_empty_folder_reads_plainly_in_the_header() {
    let dir = tempfile::tempdir().unwrap();
    let output = redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--no-telemetry",
            "--url",
            "redis://localhost:6379",
            "--agent",
            "claude",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("(no package manifest)"), "{stdout}");
    assert!(!stdout.contains("(unknown)"), "{stdout}");
    assert!(
        stdout.contains("Agents    Claude Code   existing: none"),
        "{stdout}"
    );
    assert!(!stdout.contains('✗'), "{stdout}");
    assert_eq!(
        stdout.matches("no package manifest detected").count(),
        1,
        "{stdout}"
    );
}

#[test]
fn the_header_says_when_no_agent_was_detected() {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let output = redisctl()
        .current_dir(dir.path())
        .env("HOME", home.path())
        .env("PATH", home.path().join("empty-bin"))
        .args([
            "init",
            "--dry-run",
            "--no-telemetry",
            "--url",
            "redis://localhost:6379",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let agents = stdout
        .lines()
        .find(|l| l.contains("Agents"))
        .unwrap_or_else(|| panic!("no Agents row: {stdout}"));
    assert!(agents.contains("Codex (none detected, so all)"), "{agents}");
}

#[test]
fn the_header_rows_line_up_on_the_rail() {
    let dir = tempfile::tempdir().unwrap();
    let output = redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--no-telemetry",
            "--url",
            "redis://localhost:6379",
            "--agent",
            "claude",
        ])
        .assert()
        .success()
        .get_output()
        .clone();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = |label: &str| {
        stdout
            .lines()
            .find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("no {label} row: {stdout}"))
            .to_string()
    };
    let value_column = |line: &str, label: &str| {
        let end = line.find(label).unwrap() + label.len();
        let gap = line[end..].len() - line[end..].trim_start().len();
        line[..end].chars().count() + gap
    };
    let (project, agents) = (row("Project"), row("Agents"));
    assert!(project.starts_with('◇'), "{stdout}");
    assert!(agents.starts_with('◇'), "{stdout}");
    assert_eq!(
        value_column(&project, "Project"),
        value_column(&agents, "Agents"),
        "{stdout}"
    );
}

/// The wizard's "use the local Redis" answer arrives as a localhost URL; the plan
/// names it as the local server, while a remote URL stays a provided one.
#[test]
fn a_localhost_url_is_named_as_the_local_redis() {
    for (url, source) in [
        ("redis://localhost:6379", "via local Redis)"),
        ("redis://127.0.0.1:6390", "via local Redis)"),
        ("redis://cache.example.com:6379", "via provided URL)"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        redisctl()
            .current_dir(dir.path())
            .args([
                "init",
                "--dry-run",
                "--no-telemetry",
                "--agent",
                "claude",
                "--url",
                url,
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains(source));
    }
}

#[test]
fn the_header_lists_only_the_agent_files_that_exist() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CLAUDE.md"), "# notes\n").unwrap();
    redisctl()
        .current_dir(dir.path())
        .args([
            "init",
            "--dry-run",
            "--no-telemetry",
            "--url",
            "redis://localhost:6379",
            "--agent",
            "claude,cursor",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Agents    Claude Code, Cursor   existing: CLAUDE.md",
        ))
        .stdout(predicate::str::contains("✓").not());
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
fn rejected_url_input_is_not_echoed_on_stderr() {
    let dir = tempfile::tempdir().unwrap();
    redisctl()
        .current_dir(dir.path())
        .args(["init", "--url", "redisx://default:s3cret@host:6379"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains(
            "no redis:// or rediss:// URL found",
        ))
        .stderr(predicate::str::contains("redisx://").not())
        .stderr(predicate::str::contains("s3cret").not());
}

#[test]
fn rejected_url_input_is_not_echoed_in_the_json_envelope() {
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
        .stderr(predicate::str::contains(
            "no redis:// or rediss:// URL found",
        ))
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
        .stdout(predicate::str::contains("via local Redis"))
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
fn a_dns_failure_gets_the_propagation_hint_not_the_stale_one() {
    // .invalid is RFC-reserved: resolution always fails, distinguishing the DNS
    // remedy from the refused-port stale-URL remedy the test above pins.
    let dir = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    redisctl()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args([
            "init",
            "--url",
            "redis://no-such-host.invalid:6379",
            "--no-install-cli",
            "--agent",
            "claude",
        ])
        .assert()
        .code(10)
        .stderr(predicate::str::contains(
            "DNS record may still be propagating",
        ))
        .stderr(predicate::str::contains("remove REDIS_URL from .env").not());
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
fn an_explicit_url_replaces_a_different_existing_redis_url() {
    // Explicit beats implicit: --url is consent to supersede what .env carries.
    // The note names the old value masked; only the implicit no-flags default
    // keeps an existing REDIS_URL.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "REDIS_URL=\"redis://default:0ldpw@keep-me:1\"\n",
    )
    .unwrap();
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
        .stdout(predicate::str::contains(
            "REDIS_URL replaced (was redis://default:****@keep-me:1)",
        ))
        // Whole-output negative assertions need tokens no random tempdir name in the
        // Project line can contain.
        .stdout(predicate::str::contains("s3cret").not())
        .stdout(predicate::str::contains("0ldpw").not());
    // Dry run: nothing written yet.
    assert_eq!(
        std::fs::read_to_string(dir.path().join(".env")).unwrap(),
        "REDIS_URL=\"redis://default:0ldpw@keep-me:1\"\n"
    );
}

#[test]
fn a_placeholder_env_completes_with_action_required() {
    // The wizard's "skip for now" leaves this placeholder; --complete must treat
    // it as still pending instead of validating garbage, and exit 0.
    let dir = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    std::fs::write(
        dir.path().join(".env"),
        "REDIS_URL=\"<paste-your-redis-url>\"\n",
    )
    .unwrap();
    redisctl()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args(["init", "--complete", "--no-install-cli"])
        .assert()
        .success()
        .stdout(predicate::str::contains("waiting for REDIS_URL"))
        .stdout(predicate::str::contains("Action required"))
        .stdout(predicate::str::contains("redisctl init --complete"));
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
