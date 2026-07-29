//! Curl command formatting for the `api --curl` flag

use crate::cli::HttpMethod;
use crate::connection::{CloudConnectionInfo, EnterpriseConnectionInfo};
use serde_json::Value;

/// Format a curl command for a Cloud API request.
///
/// Auth headers are redacted by default.
pub fn format_cloud_curl(
    info: &CloudConnectionInfo,
    method: &HttpMethod,
    path: &str,
    body: Option<&Value>,
) -> String {
    let mut parts = vec![
        "curl".to_string(),
        "-s".to_string(),
        format!("-X {}", method),
    ];

    parts.push(format!("'{}{}'", info.base_url, path));
    parts.push("-H 'Accept: application/json'".to_string());
    parts.push(format!("-H 'User-Agent: {}'", info.user_agent));
    parts.push("-H 'x-api-key: <REDACTED>'".to_string());
    parts.push("-H 'x-api-secret-key: <REDACTED>'".to_string());

    if let Some(body) = body {
        parts.push("-H 'Content-Type: application/json'".to_string());
        parts.push(format!("-d '{}'", body));
    }

    parts.join(" \\\n  ")
}

/// Format a curl command for an Enterprise API request.
///
/// Auth credentials are redacted by default.
pub fn format_enterprise_curl(
    info: &EnterpriseConnectionInfo,
    method: &HttpMethod,
    path: &str,
    body: Option<&Value>,
) -> String {
    let mut parts = vec!["curl".to_string(), "-s".to_string()];

    if info.insecure {
        parts.push("-k".to_string());
    }

    parts.push(format!("-X {}", method));

    parts.push(format!("'{}{}'", info.base_url, path));
    parts.push("-H 'Accept: application/json'".to_string());
    parts.push(format!("-H 'User-Agent: {}'", info.user_agent));
    parts.push("-u '<REDACTED>:<REDACTED>'".to_string());

    if let Some(ref ca_cert_path) = info.ca_cert {
        parts.push(format!("--cacert '{}'", ca_cert_path));
    }

    if let Some(body) = body {
        parts.push("-H 'Content-Type: application/json'".to_string());
        parts.push(format!("-d '{}'", body));
    }

    parts.join(" \\\n  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_get_no_body() {
        let info = CloudConnectionInfo {
            base_url: "https://api.redislabs.com/v1".to_string(),
            user_agent: "redisctl/test".to_string(),
        };
        let result = format_cloud_curl(&info, &HttpMethod::Get, "/subscriptions", None);
        assert!(result.starts_with("curl"));
        assert!(result.contains("-X GET"));
        assert!(result.contains("'https://api.redislabs.com/v1/subscriptions'"));
        assert!(result.contains("x-api-key: <REDACTED>"));
        assert!(result.contains("x-api-secret-key: <REDACTED>"));
        assert!(result.contains("User-Agent: redisctl/test"));
        assert!(!result.contains("-d "));
        assert!(!result.contains("Content-Type"));
    }

    #[test]
    fn cloud_post_with_body() {
        let info = CloudConnectionInfo {
            base_url: "https://api.redislabs.com/v1".to_string(),
            user_agent: "redisctl/test".to_string(),
        };
        let body = serde_json::json!({"name": "test"});
        let result = format_cloud_curl(&info, &HttpMethod::Post, "/subscriptions", Some(&body));
        assert!(result.contains("-X POST"));
        assert!(result.contains("-d '{\"name\":\"test\"}'"));
        assert!(result.contains("Content-Type: application/json"));
    }

    #[test]
    fn enterprise_get_insecure() {
        let info = EnterpriseConnectionInfo {
            base_url: "https://cluster:9443".to_string(),
            insecure: true,
            ca_cert: None,
            user_agent: "redisctl/test".to_string(),
        };
        let result = format_enterprise_curl(&info, &HttpMethod::Get, "/v1/cluster", None);
        assert!(result.contains("-k"));
        assert!(result.contains("-X GET"));
        assert!(result.contains("'https://cluster:9443/v1/cluster'"));
        assert!(result.contains("-u '<REDACTED>:<REDACTED>'"));
        assert!(result.contains("User-Agent: redisctl/test"));
        assert!(!result.contains("--cacert"));
    }

    #[test]
    fn enterprise_with_ca_cert() {
        let info = EnterpriseConnectionInfo {
            base_url: "https://cluster:9443".to_string(),
            insecure: false,
            ca_cert: Some("/path/to/ca.crt".to_string()),
            user_agent: "redisctl/test".to_string(),
        };
        let result = format_enterprise_curl(&info, &HttpMethod::Get, "/v1/bdbs", None);
        assert!(!result.contains("-k"));
        assert!(result.contains("--cacert '/path/to/ca.crt'"));
    }

    #[test]
    fn enterprise_post_with_body() {
        let info = EnterpriseConnectionInfo {
            base_url: "https://cluster:9443".to_string(),
            insecure: true,
            ca_cert: None,
            user_agent: "redisctl/test".to_string(),
        };
        let body = serde_json::json!({"name": "db1"});
        let result = format_enterprise_curl(&info, &HttpMethod::Post, "/v1/bdbs", Some(&body));
        assert!(result.contains("-X POST"));
        assert!(result.contains("-d '{\"name\":\"db1\"}'"));
        assert!(result.contains("Content-Type: application/json"));
    }
}
