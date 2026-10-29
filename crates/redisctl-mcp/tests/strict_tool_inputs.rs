//! Offline input contracts: no tool execution, connections, credentials or policy changes.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn rejects_extra_field<T: DeserializeOwned>(mut valid: Value) {
    assert!(serde_json::from_value::<T>(valid.clone()).is_ok());
    valid
        .as_object_mut()
        .unwrap()
        .insert("__unexpected".into(), json!(true));
    let err = serde_json::from_value::<T>(valid)
        .err()
        .expect("extra field must fail");
    assert!(
        err.to_string().contains("unknown field `__unexpected`"),
        "{err}"
    );
}

#[test]
fn standalone_profile_input_is_strict() {
    rejects_extra_field::<redisctl_mcp::tools::profile::ListProfilesInput>(json!({}));
}

#[cfg(feature = "database")]
#[test]
fn both_database_macro_variants_are_strict() {
    use redisctl_mcp::tools::redis::{AliasSetInput, FtAggregateInput};
    rejects_extra_field::<FtAggregateInput>(json!({"index":"idx", "query":"*"}));
    rejects_extra_field::<AliasSetInput>(json!({"name":"example", "commands":[{"args":["PING"]}]}));
    let input: FtAggregateInput = serde_json::from_value(json!({
        "index":"idx", "query":"*", "limit_num":"10"
    }))
    .unwrap();
    assert_eq!(input.limit_num, Some(10));
    assert!(input.group_by.is_none());
}

#[cfg(feature = "database")]
#[test]
fn nested_objects_are_strict_but_user_maps_are_not_closed() {
    use redisctl_mcp::tools::redis::{AliasSetInput, HsetInput};
    let err = serde_json::from_value::<AliasSetInput>(json!({
        "name":"example", "commands":[{"args":["PING"], "metadata":"not an argument"}]
    }))
    .unwrap_err();
    assert!(
        err.to_string().contains("unknown field `metadata`"),
        "{err}"
    );
    let input: HsetInput = serde_json::from_value(json!({
        "key":"example", "fields":{"metadata":"user data", "arbitrary:key":"kept"}
    }))
    .unwrap();
    assert_eq!(
        input.fields.get("metadata").map(String::as_str),
        Some("user data")
    );
    assert_eq!(
        input.fields.get("arbitrary:key").map(String::as_str),
        Some("kept")
    );
}

#[cfg(feature = "cloud")]
#[test]
fn cloud_macro_and_raw_envelope_are_strict_but_payload_is_preserved() {
    use redisctl_mcp::tools::cloud::{CloudRawApiInput, GetAccountInput};
    rejects_extra_field::<GetAccountInput>(json!({}));
    let body = json!({"future_field": {"arbitrary": [1, null, {"metadata":"user data"}]}});
    let input = json!({"method":"POST", "path":"/synthetic", "body":body});
    rejects_extra_field::<CloudRawApiInput>(input.clone());
    let decoded: CloudRawApiInput = serde_json::from_value(input).unwrap();
    assert_eq!(decoded.body, Some(body));
    assert!(!decoded.dry_run);
}

#[cfg(feature = "enterprise")]
#[test]
fn enterprise_macro_and_raw_envelope_are_strict_but_payload_is_preserved() {
    use redisctl_mcp::tools::enterprise::{EnterpriseRawApiInput, GetClusterInput};
    rejects_extra_field::<GetClusterInput>(json!({}));
    let body = json!({"future_field": {"arbitrary": [1, null, {"metadata":"user data"}]}});
    let input = json!({"method":"POST", "path":"/synthetic", "body":body});
    rejects_extra_field::<EnterpriseRawApiInput>(input.clone());
    let decoded: EnterpriseRawApiInput = serde_json::from_value(input).unwrap();
    assert_eq!(decoded.body, Some(body));
    assert!(!decoded.dry_run);
}
