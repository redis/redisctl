//! Public embedding and catalog contracts for the experimental database backend.
#![cfg(feature = "database-mcp-pilot")]

use std::collections::HashSet;
use std::time::Duration;

use redisctl_mcp::{
    CredentialSource, DatabaseBackend, McpServerBuilder, PolicyConfig, SafetyTier, ToolsetPolicy,
};
use serde_json::{Value, json};
use tower_mcp::TestClient;

fn builder(policy: PolicyConfig) -> McpServerBuilder {
    McpServerBuilder::new(CredentialSource::Profiles(Vec::new()), policy, "pilot-test")
        .with_database_backend(DatabaseBackend::RedisMcp)
        .with_database_url(Some("redis://127.0.0.1:1".to_string()))
        .with_tool_specs(["database", "app"])
        .unwrap()
}

async fn client(builder: McpServerBuilder) -> TestClient {
    let mut client = TestClient::from_router(builder.build().unwrap().into_router());
    client.initialize().await;
    client
}

fn names(tools: &[Value]) -> HashSet<String> {
    tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

async fn assert_skill_catalog(client: &mut TestClient, actual: &HashSet<String>) {
    let result = client.read_resource("redisctl://skills").await;
    let text = result.contents[0].text.as_ref().unwrap();
    let index: Value = serde_json::from_str(text).unwrap();
    let reported = index["available_tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(
        &reported, actual,
        "skills must describe the composed router"
    );
    assert!(!text.contains("pilot-discovery-secret"));
    assert!(!text.contains("127.0.0.1:1"));
}

#[tokio::test]
async fn pilot_catalog_matches_separate_reviewed_baseline() {
    let mut client = client(builder(PolicyConfig::default())).await;
    let mut tools = client
        .list_tools()
        .await
        .into_iter()
        .filter(|tool| tool["name"].as_str().unwrap().starts_with("redis_"))
        .map(|tool| {
            json!({"name":tool["name"], "inputSchema":tool["inputSchema"],
            "outputSchema":tool["outputSchema"], "annotations":tool["annotations"]})
        })
        .collect::<Vec<_>>();
    tools.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    let actual = json!({"formatVersion":1,"tools":tools});
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mcp-pilot-catalog-v1.json"
    );
    if std::env::var_os("UPDATE_MCP_PILOT_CATALOG").is_some() {
        std::fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&actual).unwrap()),
        )
        .unwrap();
    }
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        actual, expected,
        "Review changes before updating UPDATE_MCP_PILOT_CATALOG=1"
    );
}

#[cfg(all(feature = "cloud", feature = "enterprise"))]
#[tokio::test]
async fn pilot_keeps_host_identity_and_management_toolsets() {
    let server = builder(PolicyConfig::default())
        .with_database_url(Some(
            "redis://default:pilot-discovery-secret@127.0.0.1:1".to_string(),
        ))
        .with_tool_specs(["database", "cloud", "enterprise", "app"])
        .unwrap()
        .build()
        .unwrap();
    let mut client = TestClient::from_router(server.into_router());
    let initialized = client.initialize().await;
    assert_eq!(initialized["serverInfo"]["name"], "redisctl-mcp");
    let instructions = initialized["instructions"].as_str().unwrap();
    assert!(instructions.len() < 2_000);
    assert!(instructions.contains("redisctl://skills"));
    assert!(!instructions.contains("pilot-discovery-secret"));
    assert!(!instructions.contains("127.0.0.1:1"));
    let tools = client.list_tools().await;
    let actual = names(&tools);
    for name in [
        "redis_get",
        "list_subscriptions",
        "list_enterprise_databases",
        "profile_list",
        "show_policy",
        "list_available_tools",
    ] {
        assert!(actual.contains(name), "missing {name}");
    }
    assert_eq!(
        actual.len(),
        tools.len(),
        "duplicate tool names after composition"
    );
    assert_skill_catalog(&mut client, &actual).await;
    let setup = client
        .read_resource("redisctl://skills/redisctl-setup")
        .await;
    assert!(
        setup.contents[0]
            .text
            .as_ref()
            .unwrap()
            .contains("redisctl profile init")
    );
    let prompt = client
        .get_prompt("redisctl-setup", Default::default())
        .await;
    let prompt = serde_json::to_string(&prompt).unwrap();
    assert!(prompt.contains("redisctl://skills"));
    assert!(prompt.contains("never invoke unavailable"));
    assert!(!prompt.contains("pilot-discovery-secret"));
}

#[tokio::test]
async fn pilot_catalog_is_library_owned_read_only_and_credential_free() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::Full;
    policy.allow = vec!["redis_set".to_string(), "redis_command".to_string()];
    let mut client = client(builder(policy)).await;
    let tools = client.list_tools().await;
    let actual = names(&tools);
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        if name.starts_with("redis_") {
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
            assert!(tool["inputSchema"]["properties"].get("url").is_none());
            assert!(tool["inputSchema"]["properties"].get("profile").is_none());
            assert!(tool["outputSchema"].is_object());
        }
    }
    assert!(actual.contains("redis_get"));
    assert!(actual.contains("redis_scan"));
    assert!(actual.contains("profile_list"));
    assert!(!actual.contains("redis_set"));
    assert!(!actual.contains("redis_command"));
    assert!(!actual.contains("redis_info"));
    let expected = redis_mcp::tool_names_for_families(
        redis_mcp::AccessMode::ReadOnly,
        [
            redis_mcp::ToolFamily::Keyspace,
            redis_mcp::ToolFamily::Strings,
        ],
    );
    let database = actual
        .iter()
        .filter(|name| name.starts_with("redis_"))
        .map(String::as_str)
        .collect::<HashSet<_>>();
    assert_eq!(database, expected.into_iter().collect());
}

#[tokio::test]
async fn host_policy_and_visibility_apply_to_discovery_and_invocation() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::Full;
    policy.deny = vec!["redis_get".to_string()];
    policy.tools.exclude = vec!["redis_scan".to_string()];
    let mut database = ToolsetPolicy::default();
    database.deny = vec!["redis_type".to_string()];
    policy.database = Some(database);
    let mut client = client(builder(policy)).await;
    let tools = client.list_tools().await;
    let actual = names(&tools);
    for name in ["redis_get", "redis_scan", "redis_type"] {
        assert!(!actual.contains(name));
        // Denial occurs at the router, before the lazy client could connect.
        let denied = client
            .call_tool_expect_error(name, json!({"key":"k"}))
            .await;
        assert!(!denied.to_string().contains("127.0.0.1:1"));
    }
    assert_skill_catalog(&mut client, &actual).await;
}

#[tokio::test]
async fn disabled_database_requires_no_target_and_keeps_other_toolsets() {
    let mut policy = PolicyConfig::default();
    let mut database = ToolsetPolicy::default();
    database.enabled = Some(false);
    policy.database = Some(database);
    let mut client = client(builder(policy).with_database_url(None)).await;
    let tools = client.list_tools().await;
    let actual = names(&tools);
    assert!(actual.contains("profile_list"));
    assert!(!actual.contains("redis_get"));
    assert_skill_catalog(&mut client, &actual).await;
}

#[tokio::test]
async fn feature_alone_preserves_the_legacy_schema_and_catalog() {
    let server = McpServerBuilder::new(
        CredentialSource::Profiles(Vec::new()),
        PolicyConfig::default(),
        "legacy-test",
    )
    .with_tool_specs(["database"])
    .unwrap();
    let mut client = client(server).await;
    let tools = client.list_tools().await;
    let get = tools
        .iter()
        .find(|tool| tool["name"] == "redis_get")
        .unwrap();
    assert!(get["inputSchema"]["properties"].get("profile").is_some());
    assert!(get["inputSchema"]["properties"].get("url").is_some());
    assert!(names(&tools).contains("redis_info"));
}

#[test]
fn invalid_limits_and_legacy_submodule_names_fail_startup() {
    assert!(
        builder(PolicyConfig::default())
            .with_database_pilot_timeout(Duration::ZERO)
            .build()
            .is_err()
    );
    assert!(
        builder(PolicyConfig::default())
            .with_database_pilot_output_limits(0, 1)
            .build()
            .is_err()
    );
    assert!(
        builder(PolicyConfig::default())
            .with_tool_specs(["database:keys"])
            .unwrap()
            .build()
            .is_err()
    );
    assert!(
        builder(PolicyConfig::default())
            .with_database_url(None)
            .build()
            .is_err()
    );
}

#[tokio::test]
async fn first_connection_failure_is_bounded_and_secret_safe() {
    let mut client = client(
        builder(PolicyConfig::default())
            .with_database_url(Some(
                "redis://default:unique-fake-secret@127.0.0.1:1".to_string(),
            ))
            .with_database_pilot_timeout(Duration::from_millis(150)),
    )
    .await;
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        client.call_tool("redis_get", json!({"key":"missing"})),
    )
    .await
    .unwrap();
    assert!(result.is_error);
    let text = serde_json::to_string(&result).unwrap();
    assert!(!text.contains("unique-fake-secret"));
    assert!(!text.contains("redis://"));
}

#[tokio::test]
async fn lazy_authentication_handshake_obeys_the_command_deadline() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let silent_server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        std::future::pending::<()>().await;
    });
    let mut client = client(
        builder(PolicyConfig::default())
            .with_database_url(Some(format!("redis://default:handshake-secret@{addr}")))
            .with_database_pilot_timeout(Duration::from_millis(100)),
    )
    .await;
    let response = tokio::time::timeout(
        Duration::from_secs(2),
        client.call_tool("redis_get", json!({"key":"k"})),
    )
    .await
    .unwrap();
    silent_server.abort();
    let _ = silent_server.await;
    assert!(response.is_error);
    assert!(
        !serde_json::to_string(&response)
            .unwrap()
            .contains("handshake-secret")
    );
}

// These tests own their Redis processes and data. Run explicitly when
// redis-server/redis-cli are installed; CI runs them in its integration job.
#[cfg(unix)]
mod live {
    use super::*;
    use redis_server_wrapper::{RedisCluster, RedisServer};

    fn tls_provider() {
        // Both crypto-provider features can be enabled in the combined host.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    async fn connection(url: &str) -> redis::aio::MultiplexedConnection {
        redis::Client::open(url)
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap()
    }

    async fn pilot(url: &str) -> TestClient {
        tls_provider();
        client(builder(PolicyConfig::default()).with_database_url(Some(url.to_string()))).await
    }

    async fn result(client: &mut TestClient, name: &str, input: Value) -> Value {
        let response = client.call_tool(name, input).await;
        assert!(!response.is_error, "{response:?}");
        response.structured_content.unwrap()
    }

    #[tokio::test]
    #[ignore = "requires redis-server and redis-cli"]
    async fn standalone_results_match_legacy_and_preserve_binary_missing_and_scan() {
        let server = RedisServer::new()
            .auto_port()
            .bind("127.0.0.1")
            .start()
            .await
            .unwrap();
        let url = format!("redis://{}", server.addr());
        let mut conn = connection(&url).await;
        let _: () = redis::cmd("SET")
            .arg("pilot:string")
            .arg("hello")
            .query_async(&mut conn)
            .await
            .unwrap();
        let _: () = redis::cmd("SET")
            .arg("pilot:binary")
            .arg(&[0xff_u8, 0x00][..])
            .query_async(&mut conn)
            .await
            .unwrap();
        let mut pilot = pilot(&url).await;
        let get = result(&mut pilot, "redis_get", json!({"key":"pilot:string"})).await;
        assert_eq!(get["value"], "hello");
        assert_eq!(get["encoding"], "utf8");
        let mut legacy = client(
            builder(PolicyConfig::default())
                .with_database_backend(DatabaseBackend::Legacy)
                .with_database_url(Some(url)),
        )
        .await;
        let legacy_get = legacy
            .call_tool("redis_get", json!({"key":"pilot:string"}))
            .await;
        assert!(!legacy_get.is_error);
        assert_eq!(legacy_get.content[0].as_text().unwrap(), "hello");
        let binary = result(&mut pilot, "redis_get", json!({"key":"pilot:binary"})).await;
        assert_eq!(binary["encoding"], "base64");
        assert_eq!(binary["value"], "/wA=");
        let missing = result(&mut pilot, "redis_get", json!({"key":"missing"})).await;
        assert_eq!(missing["exists"], false);
        assert_eq!(missing["value"], Value::Null);
        let kind = result(&mut pilot, "redis_type", json!({"key":"pilot:string"})).await;
        assert_eq!(kind["key_type"], "string");
        let ttl = result(&mut pilot, "redis_ttl", json!({"key":"pilot:string"})).await;
        assert_eq!(ttl["ttl_seconds"], -1);
        let scan = result(
            &mut pilot,
            "redis_scan",
            json!({"pattern":"pilot:*", "count":100}),
        )
        .await;
        assert_eq!(scan["count"], 2);
        assert_eq!(scan["page"]["complete"], true);
        let rejected = pilot
            .call_tool(
                "redis_get",
                json!({"key":"pilot:string", "url":"redis://localhost"}),
            )
            .await;
        assert!(rejected.is_error);
        assert!(
            pilot
                .call_tool(
                    "redis_get",
                    json!({"key":"pilot:string", "profile":"another-db"})
                )
                .await
                .is_error
        );
        pilot
            .call_tool_expect_error(
                "redis_set",
                json!({"key":"pilot:string", "value":"changed"}),
            )
            .await;
        assert_eq!(
            result(&mut pilot, "redis_get", json!({"key":"pilot:string"})).await["value"],
            "hello"
        );
    }

    #[tokio::test]
    #[ignore = "requires redis-server and redis-cli"]
    async fn authenticated_target_is_reused_and_auth_errors_are_redacted() {
        let server = RedisServer::new()
            .auto_port()
            .bind("127.0.0.1")
            .password("pilot-secret")
            .start()
            .await
            .unwrap();
        let url = format!("redis://default:pilot-secret@127.0.0.1:{}", server.port());
        let mut conn = connection(&url).await;
        let _: () = redis::cmd("SET")
            .arg("k")
            .arg("v")
            .query_async(&mut conn)
            .await
            .unwrap();
        let mut pilot = pilot(&url).await;
        for _ in 0..3 {
            assert_eq!(
                result(&mut pilot, "redis_get", json!({"key":"k"})).await["value"],
                "v"
            );
        }
        let info: String = redis::cmd("INFO")
            .arg("clients")
            .query_async(&mut conn)
            .await
            .unwrap();
        // One seed connection and one reused pilot connection, not one per call.
        assert!(
            info.lines()
                .any(|line| line.trim() == "connected_clients:2"),
            "{info}"
        );
        let bad_url = format!(
            "redis://default:wrong-pilot-secret@127.0.0.1:{}",
            server.port()
        );
        let mut rejected = client(
            builder(PolicyConfig::default())
                .with_database_url(Some(bad_url))
                .with_database_pilot_timeout(Duration::from_millis(300)),
        )
        .await;
        let error = rejected.call_tool("redis_get", json!({"key":"k"})).await;
        assert!(error.is_error);
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("pilot-secret")
        );
    }

    #[tokio::test]
    #[ignore = "requires redis-server and redis-cli"]
    async fn output_budget_and_command_deadline_are_enforced() {
        let server = RedisServer::new()
            .auto_port()
            .bind("127.0.0.1")
            .start()
            .await
            .unwrap();
        let url = format!("redis://{}", server.addr());
        let mut conn = connection(&url).await;
        let _: () = redis::cmd("SET")
            .arg("large")
            .arg("x".repeat(4096))
            .query_async(&mut conn)
            .await
            .unwrap();
        let mut limited = client(
            builder(PolicyConfig::default())
                .with_database_url(Some(url.clone()))
                .with_database_pilot_output_limits(512, 10),
        )
        .await;
        assert!(
            limited
                .call_tool("redis_get", json!({"key":"large"}))
                .await
                .is_error
        );
        assert!(
            limited
                .call_tool("redis_scan", json!({"count":11}))
                .await
                .is_error
        );
        let mut bounded = client(
            builder(PolicyConfig::default())
                .with_database_url(Some(url))
                .with_database_pilot_timeout(Duration::from_millis(100)),
        )
        .await;
        result(&mut bounded, "redis_get", json!({"key":"missing"})).await;
        let _: () = redis::cmd("CLIENT")
            .arg("PAUSE")
            .arg(1000)
            .arg("ALL")
            .query_async(&mut conn)
            .await
            .unwrap();
        let response = tokio::time::timeout(
            Duration::from_secs(2),
            bounded.call_tool("redis_get", json!({"key":"large"})),
        )
        .await
        .unwrap();
        assert!(response.is_error);
        // Cancellation of one read must not corrupt the reusable connection.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        assert_eq!(
            result(&mut bounded, "redis_get", json!({"key":"large"})).await["exists"],
            true
        );
    }

    fn cluster_port() -> u16 {
        let seed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        for offset in 0..100u16 {
            // macOS can allocate a long consecutive run above 55,000, leaving
            // no room for Redis's +10,000 bus ports. Probe a bounded lower
            // range instead of repeatedly rejecting OS-assigned ports.
            let port = 20_000 + (seed % 20_000 + offset * 3) % 20_000;
            let sockets = [
                port,
                port + 1,
                port + 2,
                port + 10000,
                port + 10001,
                port + 10002,
            ]
            .map(|port| std::net::TcpListener::bind(("127.0.0.1", port)));
            if sockets.iter().all(Result::is_ok) {
                return port;
            }
        }
        panic!("cannot reserve local Redis Cluster ports");
    }

    #[tokio::test]
    #[ignore = "requires redis-server and redis-cli"]
    async fn cluster_fixed_target_routes_reads_across_slots() {
        tls_provider();
        let dir = tempfile::tempdir().unwrap();
        let cluster = RedisCluster::builder()
            .masters(3)
            .replicas_per_master(0)
            .base_port(cluster_port())
            .dir(dir.path())
            .bind("127.0.0.1")
            .start()
            .await
            .unwrap();
        let url = format!("redis://{}", cluster.addr());
        let mut seed = redis::cluster::ClusterClient::new([url.clone()])
            .unwrap()
            .get_async_connection()
            .await
            .unwrap();
        let mut pilot = client(
            builder(PolicyConfig::default())
                .with_database_url(Some(url))
                .with_cluster_mode(true),
        )
        .await;
        for key in ["{a}:pilot", "{b}:pilot", "{c}:pilot"] {
            let _: () = redis::cmd("SET")
                .arg(key)
                .arg(key)
                .query_async(&mut seed)
                .await
                .unwrap();
            assert_eq!(
                result(&mut pilot, "redis_get", json!({"key":key})).await["value"],
                key
            );
        }
        // The underlying transport's SCAN is node-local. Supplying the known
        // Cluster deployment enables the library's guard instead of implying
        // database-wide completeness from a single primary.
        for name in ["redis_scan", "redis_dbsize", "redis_randomkey"] {
            let response = pilot.call_tool(name, json!({})).await;
            assert!(response.is_error);
            assert!(
                response.content[0]
                    .as_text()
                    .unwrap()
                    .contains("DEPLOYMENT_UNAVAILABLE")
            );
        }
    }

    async fn openssl(args: &[&str]) {
        let output = tokio::process::Command::new("openssl")
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    async fn stdio_get(url: &str, ca: &std::path::Path) -> Value {
        use std::process::Stdio;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_redisctl-mcp"))
            .args([
                "--database-backend",
                "redis-mcp",
                "--tools",
                "database",
                "--database-url",
                url,
            ])
            .env_remove("REDISCTL_PROFILE")
            // Explicit CLI URL must override an ambient URL, without falling
            // back to it if the selected target fails.
            .env("REDIS_URL", "redis://default:unused-env-secret@127.0.0.1:1")
            .env("SSL_CERT_FILE", ca)
            .env_remove("SSL_CERT_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
        for message in [
            json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
                "protocolVersion":"2025-11-25", "capabilities":{}, "clientInfo":{"name":"pilot-test","version":"1"}}}),
            json!({"jsonrpc":"2.0", "method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":"redis_get", "arguments":{"key":"tls-key"}}}),
        ] {
            input
                .write_all(format!("{message}\n").as_bytes())
                .await
                .unwrap();
        }
        let response = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let line = output
                    .next_line()
                    .await
                    .unwrap()
                    .expect("MCP process exited before response");
                // Any tracing on stdout breaks this parse and fails the test.
                let response: Value = serde_json::from_str(&line).unwrap();
                if response["id"] == 2 {
                    break response;
                }
            }
        })
        .await
        .unwrap();
        child.kill().await.unwrap();
        child.wait().await.unwrap();
        response
    }

    #[tokio::test]
    #[ignore = "requires TLS-enabled redis-server, redis-cli and openssl"]
    async fn tls_requires_trust_and_stdio_returns_clean_json() {
        tls_provider();
        let dir = tempfile::tempdir().unwrap();
        let ca_key = dir.path().join("ca.key");
        let ca = dir.path().join("ca.pem");
        let key = dir.path().join("server.key");
        let csr = dir.path().join("server.csr");
        let cert = dir.path().join("server.pem");
        openssl(&[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=pilot-test-ca",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-keyout",
            ca_key.to_str().unwrap(),
            "-out",
            ca.to_str().unwrap(),
        ])
        .await;
        openssl(&[
            "req",
            "-new",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-keyout",
            key.to_str().unwrap(),
            "-out",
            csr.to_str().unwrap(),
        ])
        .await;
        openssl(&[
            "x509",
            "-req",
            "-in",
            csr.to_str().unwrap(),
            "-CA",
            ca.to_str().unwrap(),
            "-CAkey",
            ca_key.to_str().unwrap(),
            "-CAcreateserial",
            "-days",
            "1",
            "-copy_extensions",
            "copy",
            "-out",
            cert.to_str().unwrap(),
        ])
        .await;
        let tls_port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let server = RedisServer::new()
            .auto_port()
            .bind("127.0.0.1")
            .tls_port(tls_port)
            .tls_cert_file(cert)
            .tls_key_file(key)
            .tls_ca_cert_file(&ca)
            .tls_auth_clients(false)
            .start()
            .await
            .unwrap();
        server.run(&["SET", "tls-key", "secure"]).await.unwrap();
        let url = format!("rediss://localhost:{tls_port}");
        let mut untrusted = pilot(&url).await;
        assert!(
            untrusted
                .call_tool("redis_get", json!({"key":"tls-key"}))
                .await
                .is_error
        );
        // Trust only the disposable CA in the child process; never change the
        // user's trust store or the parent process environment.
        let trusted = stdio_get(&url, &ca).await;
        assert_ne!(trusted["result"]["isError"], true, "{trusted}");
        assert_eq!(trusted["result"]["structuredContent"]["value"], "secure");
    }
}
