//! Hermetic tests for the Iris product flows: a wiremock server plays all three
//! product APIs on one base URL (health unauthenticated, Bearer for Agent Memory
//! and LangCache, X-API-Key for the Context Retriever MCP), and a PATH-shimmed
//! `npm` keeps SDK installs offline. Unix-only for the shim.
#![cfg(unix)]

mod init_common;

use assert_cmd::Command;
use init_common::skills_fixture;
use predicates::prelude::*;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

const KEY: &str = "s3cret-product-key";

/// The happy-path product API: same shapes the PoC's fake server pinned.
async fn product_api() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"ok"})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/v1/stores/[^/]+/session-memory/events$"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"event":{"eventId":"fake-event"}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/stores/[^/]+/session-memory/[^/]+$"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"sessionId":"redisctl-check","events":[{"eventId":"fake-event"}]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/v1/caches/[^/]+/entries/search$"))
        .and(header("authorization", format!("Bearer {KEY}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(header("x-api-key", KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc":"2.0","id":1,
            "result":{"tools":[{"name":"get_customer_by_id","description":"Get one customer"}]}
        })))
        .mount(&server)
        .await;
    // Anything else (wrong key included) is rejected like the real services do.
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error":"forbidden"})))
        .mount(&server)
        .await;
    server
}

/// A fake `npm` on PATH: SDK installs succeed without touching the network.
fn npm_shim() -> (TempDir, String) {
    let shim = tempfile::tempdir().unwrap();
    use std::os::unix::fs::PermissionsExt;
    let bin = shim.path().join("npm");
    std::fs::write(&bin, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path_env = format!(
        "{}:{}",
        shim.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (shim, path_env)
}

fn run_init(project: &std::path::Path, repo: &TempDir, path_env: &str, extra: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("redisctl").unwrap();
    for var in [
        "AGENT_MEMORY_API_KEY",
        "LANGCACHE_API_KEY",
        "CONTEXT_RETRIEVER_AGENT_KEY",
        "DO_NOT_TRACK",
    ] {
        cmd.env_remove(var);
    }
    cmd.current_dir(project)
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .env("PATH", path_env)
        .args(["init", "--no-install-cli", "--agent", "claude"])
        .args(extra);
    cmd
}

fn read(project: &std::path::Path, rel: &str) -> String {
    std::fs::read_to_string(project.join(rel)).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn both_products_wire_validate_and_never_leak_the_key() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("package.json"),
        r#"{"name":"k","type":"module"}"#,
    )
    .unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;

    let output = run_init(
        project.path(),
        &repo,
        &path_env,
        &[
            "--agent-memory",
            &api.uri(),
            "--store",
            "store-123",
            "--langcache",
            &api.uri(),
            "--cache",
            "cache-456",
        ],
    )
    .env("AGENT_MEMORY_API_KEY", KEY)
    .env("LANGCACHE_API_KEY", KEY)
    .assert()
    .success()
    .stdout(predicate::str::contains("✓ Agent Memory"))
    .stdout(predicate::str::contains("✓ LangCache"))
    .get_output()
    .clone();

    // Credentials land in .env under the SDKs' own env names, one provenance
    // block per product; the key never travels anywhere else.
    let env = read(project.path(), ".env");
    assert!(env.contains("AGENT_MEMORY_STORE_ID=\"store-123\""), "{env}");
    assert!(
        env.contains(&format!("AGENT_MEMORY_API_KEY=\"{KEY}\"")),
        "{env}"
    );
    assert!(env.contains("LANGCACHE_CACHE_ID=\"cache-456\""), "{env}");
    assert_eq!(env.matches("# Added by redisctl init").count(), 2, "{env}");
    assert!(!env.contains("REDIS_URL"), "{env}");
    let skill = read(
        project.path(),
        ".claude/skills/redis-project-setup/SKILL.md",
    );
    for (text, name) in [
        (&read(project.path(), ".env.example"), ".env.example"),
        (&skill, "SKILL.md"),
        (
            &String::from_utf8_lossy(&output.stdout).to_string(),
            "stdout",
        ),
        (
            &String::from_utf8_lossy(&output.stderr).to_string(),
            "stderr",
        ),
    ] {
        assert!(!text.contains(KEY), "key leaked into {name}");
    }

    // The skill carries both products, their ids, and the env contract.
    assert!(skill.contains("Agent memory (Redis Iris)"), "{skill}");
    assert!(skill.contains("store-123"), "{skill}");
    assert!(skill.contains("AGENT_MEMORY_API_KEY"), "{skill}");
    assert!(skill.contains("Semantic cache (LangCache)"), "{skill}");
    assert!(skill.contains("cache-456"), "{skill}");
    assert!(skill.contains("search-before-generate"), "{skill}");

    // A product-only run provisions no database and registers no MCP server.
    assert!(!project.path().join(".mcp.json").exists());

    // The example lands next to the source, references env names, and holds no
    // secret; an edited example is never overwritten.
    let example = read(project.path(), "redis-agent-memory.js");
    assert!(
        example.contains("process.env.AGENT_MEMORY_STORE_ID"),
        "{example}"
    );
    assert!(example.contains("yours: edit it, move it, or delete it"));
    assert!(!example.contains(KEY));
    std::fs::write(project.path().join("redis-langcache.js"), "// mine now\n").unwrap();
    run_init(
        project.path(),
        &repo,
        &path_env,
        &["--langcache", &api.uri(), "--cache", "cache-456"],
    )
    .env("LANGCACHE_API_KEY", KEY)
    .assert()
    .success()
    .stdout(predicate::str::contains("existing file left untouched"));
    assert_eq!(read(project.path(), "redis-langcache.js"), "// mine now\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_key_fails_and_names_the_value_to_fix() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;

    run_init(
        project.path(),
        &repo,
        &path_env,
        &["--agent-memory", &api.uri(), "--store", "store-123"],
    )
    .env("AGENT_MEMORY_API_KEY", "wrong-key")
    .assert()
    .failure()
    .stdout(predicate::str::contains("✗ Agent Memory"))
    .stdout(predicate::str::contains("returned 403"))
    .stderr(predicate::str::contains("AGENT_MEMORY_API_KEY"))
    .stderr(predicate::str::contains(".env already holds"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_key_scaffolds_pending_and_complete_validates_after_the_fill() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;

    run_init(
        project.path(),
        &repo,
        &path_env,
        &["--langcache", &api.uri(), "--cache", "cache-456"],
    )
    .env("LANGCACHE_API_KEY", "")
    .assert()
    .success()
    .stdout(predicate::str::contains("waiting for LANGCACHE_API_KEY"))
    .stdout(predicate::str::contains(
        "Do not paste service keys into chat",
    ))
    .stdout(predicate::str::contains("redisctl init --complete"));
    let env = read(project.path(), ".env");
    assert!(
        env.contains("LANGCACHE_API_KEY=\"<paste-from-redis-cloud>\""),
        "{env}"
    );

    // The human fills the key; --complete rediscovers from .env and validates.
    std::fs::write(
        project.path().join(".env"),
        env.replace("<paste-from-redis-cloud>", KEY),
    )
    .unwrap();
    run_init(project.path(), &repo, &path_env, &["--complete"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ LangCache"));
    let env = read(project.path(), ".env");
    assert!(!env.contains("<paste-from-redis-cloud>"), "{env}");
    assert!(!env.contains("REDIS_URL"), "{env}");
}

#[tokio::test(flavor = "multi_thread")]
async fn iris_installs_guidance_without_product_runtime() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();

    run_init(project.path(), &repo, &path_env, &["--iris"])
        .assert()
        .success()
        .stdout(predicate::str::contains("product runtime"))
        .stdout(predicate::str::contains(
            "no .env, SDK, example, or MCP server until you approve a product",
        ))
        .stdout(predicate::str::contains(
            "recommend the smallest Redis Iris setup",
        ));
    for absent in [".env", ".env.example", ".mcp.json", "redis-agent-memory.js"] {
        assert!(
            !project.path().join(absent).exists(),
            "{absent} should not exist"
        );
    }
    let skill = read(
        project.path(),
        ".claude/skills/redis-project-setup/SKILL.md",
    );
    assert!(
        skill.contains("Agent Memory when an agent must retain"),
        "{skill}"
    );
    assert!(
        skill.contains("LangCache when semantically repeated"),
        "{skill}"
    );
    assert!(
        skill.contains("Context Retriever when an agent needs governed"),
        "{skill}"
    );
    assert!(skill.contains("Redis Data Integration when the source of truth is relational"));
    assert!(skill.contains("Do not add a product"), "{skill}");
    assert!(skill.contains("redisctl init --agent-memory <endpoint> --store <store-id>"));
}

#[tokio::test(flavor = "multi_thread")]
async fn all_three_products_compose_and_complete_validates_every_one() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;

    run_init(
        project.path(),
        &repo,
        &path_env,
        &[
            "--agent-memory",
            &api.uri(),
            "--store",
            "store-123",
            "--langcache",
            &api.uri(),
            "--cache",
            "cache-456",
            "--context-retriever",
            &api.uri(),
        ],
    )
    .env("AGENT_MEMORY_API_KEY", "")
    .env("LANGCACHE_API_KEY", "")
    .env("CONTEXT_RETRIEVER_AGENT_KEY", "")
    .assert()
    .success();
    let env = read(project.path(), ".env");
    assert_eq!(env.matches("<paste-from-redis-cloud>").count(), 3, "{env}");
    // The bridge entry reads the key from .env at launch; the config never holds it.
    let mcp = read(project.path(), ".mcp.json");
    assert!(mcp.contains("context-retriever"), "{mcp}");
    assert!(mcp.contains("${CONTEXT_RETRIEVER_AGENT_KEY}"), "{mcp}");
    assert!(!mcp.contains(KEY), "{mcp}");

    std::fs::write(
        project.path().join(".env"),
        read(project.path(), ".env").replace("<paste-from-redis-cloud>", KEY),
    )
    .unwrap();
    run_init(project.path(), &repo, &path_env, &["--complete"])
        .assert()
        .success()
        .stdout(predicate::str::contains("✓ Agent Memory"))
        .stdout(predicate::str::contains("✓ LangCache"))
        .stdout(predicate::str::contains("✓ Context Retriever"))
        .stdout(predicate::str::contains("1 governed MCP tool listed"));
}

#[test]
fn flag_rules_teach_where_values_come_from() {
    let cases: [(&[&str], &str); 6] = [
        (
            &["--iris", "--complete"],
            "--iris discovers what the project needs",
        ),
        (
            &["--agent-memory", "not-a-url", "--store", "s1"],
            "copy it from the console (https://...)",
        ),
        (
            &["--agent-memory", "https://x.io"],
            "--agent-memory also needs --store <store id>",
        ),
        (
            &["--store", "s1"],
            "--store only applies with --agent-memory",
        ),
        (
            &["--iris", "--langcache", "https://x.io", "--cache", "c1"],
            "--iris is discovery-only",
        ),
        (
            &["--api-key", "k"],
            "--api-key applies to an Iris product flag",
        ),
    ];
    for (extra, expected) in cases {
        Command::cargo_bin("redisctl")
            .unwrap()
            .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
            .arg("init")
            .args(extra)
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn no_example_writes_no_example_file() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;

    run_init(
        project.path(),
        &repo,
        &path_env,
        &["--langcache", &api.uri(), "--cache", "c1", "--no-example"],
    )
    .env("LANGCACHE_API_KEY", KEY)
    .assert()
    .success()
    .stdout(predicate::str::contains("--no-example"));
    assert!(!project.path().join("redis-langcache.js").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn api_key_scopes_to_the_flagged_product_and_stored_keys_win() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;
    // LangCache accepts its own key, distinct from the Agent Memory one.
    Mock::given(method("POST"))
        .and(path_regex(r"^/v1/caches/[^/]+/entries/search$"))
        .and(header("authorization", "Bearer langcache-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        // Outranks the helper's catch-all 403, which mounted first.
        .with_priority(1)
        .mount(&api)
        .await;
    // .env already holds a complete, correct Agent Memory setup.
    std::fs::write(
        project.path().join(".env"),
        format!(
            "AGENT_MEMORY_URL=\"{0}\"\nAGENT_MEMORY_STORE_ID=\"store-123\"\nAGENT_MEMORY_API_KEY=\"{KEY}\"\n",
            api.uri()
        ),
    )
    .unwrap();

    // The flag key belongs to the LangCache being wired; the rediscovered Agent
    // Memory must keep authenticating with its own stored key.
    run_init(
        project.path(),
        &repo,
        &path_env,
        &[
            "--complete",
            "--langcache",
            &api.uri(),
            "--cache",
            "cache-456",
            "--api-key",
            "langcache-key",
        ],
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("✓ Agent Memory"))
    .stdout(predicate::str::contains("✓ LangCache"));
    let env = read(project.path(), ".env");
    assert!(env.contains("LANGCACHE_API_KEY=\"langcache-key\""), "{env}");
    assert!(
        env.contains(&format!("AGENT_MEMORY_API_KEY=\"{KEY}\"")),
        "{env}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_flag_key_never_fills_another_products_placeholder() {
    let project = tempfile::tempdir().unwrap();
    let repo = skills_fixture();
    let (_shim, path_env) = npm_shim();
    let api = product_api().await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/v1/caches/[^/]+/entries/search$"))
        .and(header("authorization", "Bearer langcache-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .with_priority(1)
        .mount(&api)
        .await;
    // Agent Memory is rediscovered still pending; the LangCache flag key must not
    // fill its placeholder (and then fail validating with the wrong key).
    std::fs::write(
        project.path().join(".env"),
        format!(
            "AGENT_MEMORY_URL=\"{0}\"\nAGENT_MEMORY_STORE_ID=\"store-123\"\nAGENT_MEMORY_API_KEY=\"<paste-from-redis-cloud>\"\n",
            api.uri()
        ),
    )
    .unwrap();

    run_init(
        project.path(),
        &repo,
        &path_env,
        &[
            "--complete",
            "--langcache",
            &api.uri(),
            "--cache",
            "cache-456",
            "--api-key",
            "langcache-key",
        ],
    )
    .assert()
    .success()
    .stdout(predicate::str::contains("✓ LangCache"))
    .stdout(predicate::str::contains("waiting for AGENT_MEMORY_API_KEY"));
    let env = read(project.path(), ".env");
    assert!(
        env.contains("AGENT_MEMORY_API_KEY=\"<paste-from-redis-cloud>\""),
        "{env}"
    );
}
