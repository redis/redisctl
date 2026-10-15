//! Check the tour's actual example inputs and discovery surfaces, not model obedience.

use redisctl_mcp::{CredentialSource, McpServerBuilder, PolicyConfig, SafetyTier};
use serde_json::{Value, json};
use tower_mcp::TestClient;

const TOUR: &str = include_str!("../skills/redisctl-tour/SKILL.md");

fn examples() -> Vec<Value> {
    TOUR.split("```json\n")
        .skip(1)
        .map(|block| {
            let (json, _) = block.split_once("\n```").expect("Unclosed tour example");
            serde_json::from_str(json).expect("Invalid tour JSON")
        })
        .collect()
}

#[cfg(feature = "database")]
fn validate_example(example: &Value) -> anyhow::Result<()> {
    use anyhow::{Context, bail};
    use redisctl_mcp::tools::redis::{
        DbsizeInput, GetInput, InfoInput, KeyTypeInput, PingInput, TtlInput,
    };
    use serde::de::DeserializeOwned;

    fn arguments<T: schemars::JsonSchema + DeserializeOwned>(args: &Value) -> anyhow::Result<()> {
        let schema = serde_json::to_value(schemars::schema_for!(T))?;
        let properties = schema["properties"].as_object().context("No properties")?;
        for field in args.as_object().context("Arguments must be object")?.keys() {
            if !properties.contains_key(field) {
                bail!("Undeclared tour input: {field}");
            }
            if matches!(field.as_str(), "url" | "profile") {
                bail!("Tour must not override the target");
            }
        }
        let _: T = serde_json::from_value(args.clone())?;
        Ok(())
    }

    let envelope = example.as_object().context("Example must be object")?;
    if envelope.len() != 2 || !envelope.contains_key("arguments") {
        bail!("Expected name and arguments only");
    }
    let args = &example["arguments"];
    match example["name"].as_str().context("No tool name")? {
        "redis_ping" => arguments::<PingInput>(args),
        "redis_info" => {
            arguments::<InfoInput>(args)?;
            if args["section"] != "server" {
                bail!("Tour INFO must use server section");
            }
            Ok(())
        }
        "redis_dbsize" => arguments::<DbsizeInput>(args),
        "redis_type" => arguments::<KeyTypeInput>(args),
        "redis_ttl" => arguments::<TtlInput>(args),
        "redis_get" => arguments::<GetInput>(args),
        _ => bail!("Tool outside the bounded tour"),
    }
}

#[test]
fn examples_are_exactly_the_six_bounded_reads() {
    let examples = examples();
    assert_eq!(
        examples,
        [
            json!({"name":"redis_ping","arguments":{}}),
            json!({"name":"redis_info","arguments":{"section":"server"}}),
            json!({"name":"redis_dbsize","arguments":{}}),
            json!({"name":"redis_type","arguments":{"key":"demo:catalog:1"}}),
            json!({"name":"redis_ttl","arguments":{"key":"demo:session:1"}}),
            json!({"name":"redis_get","arguments":{"key":"demo:catalog:1"}}),
        ]
    );
    #[cfg(feature = "database")]
    for example in examples {
        validate_example(&example).unwrap();
    }
}

#[cfg(feature = "database")]
#[test]
fn example_checks_reject_ignored_fields_overrides_and_out_of_scope_calls() {
    for example in examples() {
        let mut invented = example.clone();
        invented["arguments"]["count"] = json!(10);
        assert!(validate_example(&invented).is_err());
        for (field, value) in [("url", "redis://127.0.0.1"), ("profile", "other")] {
            let mut overridden = example.clone();
            overridden["arguments"][field] = json!(value);
            assert!(validate_example(&overridden).is_err());
        }
    }
    for example in [
        json!({"name":"redis_get","arguments":{}}),
        json!({"name":"redis_get","arguments":{"key":5}}),
        json!({"name":"redis_info","arguments":{"section":"all"}}),
        json!({"name":"redis_set","arguments":{"key":"demo:catalog:1","value":"changed"}}),
        json!({"name":"redis_scan","arguments":{"limit":2}}),
        json!({"name":"redis_ping","arguments":{},"extra":true}),
    ] {
        assert!(validate_example(&example).is_err(), "{example}");
    }
}

async fn client(policy: PolicyConfig, tools: &[&str]) -> TestClient {
    let server = McpServerBuilder::new(CredentialSource::Profiles(Vec::new()), policy, "tour-test")
        .with_tool_specs(tools)
        .unwrap()
        .build()
        .unwrap();
    let mut client = TestClient::from_router(server.into_router());
    client.initialize().await;
    client
}

async fn index(client: &mut TestClient) -> Value {
    let result = client.read_resource("redisctl://skills").await;
    serde_json::from_str(result.contents[0].text.as_ref().unwrap()).unwrap()
}

#[tokio::test]
async fn tour_resource_and_prompt_work_without_database_tools() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::ReadOnly;
    let mut client = client(policy, &["app"]).await;
    let index = index(&mut client).await;
    assert!(
        index["available_tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|name| { !name.as_str().unwrap().starts_with("redis_") })
    );
    assert!(
        index["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|skill| { skill["name"] == "redisctl-tour" && skill["source"] == "bundled" })
    );
    let resource = client
        .read_resource("redisctl://skills/redisctl-tour")
        .await;
    assert_eq!(resource.contents[0].text.as_deref(), Some(TOUR));
    let prompt = client.get_prompt("redisctl-tour", Default::default()).await;
    let prompt = serde_json::to_value(prompt).unwrap();
    let body = TOUR.split_once("---\n\n").unwrap().1;
    assert!(
        prompt["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .ends_with(body.trim())
    );
    assert!(body.contains("explain the\nmissing capabilities without database calls"));
}

#[tokio::test]
async fn denied_discovery_is_not_reported_as_callable() {
    let mut policy = PolicyConfig::default();
    policy.deny = vec!["show_policy".into(), "list_available_tools".into()];
    let mut client = client(policy, &["app"]).await;
    let index = index(&mut client).await;
    let actual = client.list_tools().await;
    for name in ["show_policy", "list_available_tools"] {
        assert!(
            !index["available_tools"]
                .as_array()
                .unwrap()
                .contains(&json!(name))
        );
        assert!(actual.iter().all(|tool| tool["name"] != name));
    }
    assert!(TOUR.contains("or denied, stop with advice only"));
}

#[cfg(feature = "database")]
#[tokio::test]
async fn global_read_only_can_expose_an_explicitly_allowed_write() {
    let mut policy = PolicyConfig::default();
    policy.tier = SafetyTier::ReadOnly;
    policy.allow = vec!["redis_set".into()];
    let mut client = client(policy, &["database:keys"]).await;
    let index = index(&mut client).await;
    assert!(
        index["available_tools"]
            .as_array()
            .unwrap()
            .contains(&json!("redis_set"))
    );
    assert!(
        client
            .list_tools()
            .await
            .iter()
            .any(|tool| tool["name"] == "redis_set")
    );
    // Discovery only: no Redis target is configured and the write is never called.
    let policy = client.call_tool_json("show_policy", json!({})).await;
    assert_eq!(policy["global_tier"], "read-only");
    assert_eq!(policy["global_allow"], json!(["redis_set"]));
    assert!(TOUR.contains("the server may permit other operations"));
}

#[test]
fn guide_preserves_stop_conditions_and_separates_later_tracks() {
    for guard in [
        "If deployment mode is clustered or uncertain, stop the sample",
        "If the key is missing, the type is not string",
        "An empty string is not a missing key",
        "An empty database does not authorize writes",
        "Do not infer module support",
        "For a request to change, delete or seed data, decline within this tour",
        "do not claim an attempted denial or an audit event",
        "Do not send cleanup or shutdown commands to Redis",
    ] {
        assert!(TOUR.contains(guard), "Missing tour guard: {guard}");
    }
}
