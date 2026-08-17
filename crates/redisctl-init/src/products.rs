//! Redis Iris products: the registry, discovery, and key resolution. Wiring is
//! decided at plan time (read-only); the resolved key stays inside the engine and
//! never lands in a change note, a skill, or an error message.

use std::path::Path;

use crate::InitError;
use crate::env::read_env_key;

/// What a human replaces by hand in `.env`; anything angle-bracketed reads as unset.
pub const SECRET_PLACEHOLDER: &str = "<paste-from-redis-cloud>";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductKey {
    AgentMemory,
    LangCache,
    ContextRetriever,
}

/// The per-product contract. The env names are the SDKs' own and never change.
pub(crate) struct ProductSpec {
    pub(crate) key: ProductKey,
    pub(crate) label: &'static str,
    pub(crate) id_name: Option<&'static str>,
    pub(crate) env_url: &'static str,
    pub(crate) env_id: Option<&'static str>,
    pub(crate) env_key: &'static str,
    pub(crate) example_url: &'static str,
    /// The coding agent consumes this product through MCP, not an SDK.
    pub(crate) mcp: bool,
}

pub(crate) const SPECS: [ProductSpec; 3] = [
    ProductSpec {
        key: ProductKey::AgentMemory,
        label: "Agent Memory",
        id_name: Some("store id"),
        env_url: "AGENT_MEMORY_URL",
        env_id: Some("AGENT_MEMORY_STORE_ID"),
        env_key: "AGENT_MEMORY_API_KEY",
        example_url: "https://<region>.memory.redis.io",
        mcp: false,
    },
    ProductSpec {
        key: ProductKey::LangCache,
        label: "LangCache",
        id_name: Some("cache id"),
        env_url: "LANGCACHE_URL",
        env_id: Some("LANGCACHE_CACHE_ID"),
        env_key: "LANGCACHE_API_KEY",
        example_url: "https://<region>.langcache.redis.io",
        mcp: false,
    },
    ProductSpec {
        key: ProductKey::ContextRetriever,
        label: "Context Retriever",
        id_name: None,
        env_url: "CONTEXT_RETRIEVER_MCP_URL",
        env_id: None,
        env_key: "CONTEXT_RETRIEVER_AGENT_KEY",
        example_url: "https://mcp.cloud.redis.io",
        mcp: true,
    },
];

/// What the caller asked for: one product endpoint (and its id, where the product
/// has one), straight from the flags.
#[derive(Debug, Clone)]
pub struct ProductRequest {
    pub key: ProductKey,
    pub url: String,
    pub id: Option<String>,
}

/// A product the run works with: inputs resolved, key found or left as the
/// placeholder a human fills in.
pub struct WiredProduct {
    pub(crate) spec: &'static ProductSpec,
    pub(crate) url: String,
    pub(crate) id: Option<String>,
    pub(crate) key: String,
    pub(crate) ready: bool,
}

// Manual, because the resolved key is a real credential and `{:?}` must never
// carry it.
impl std::fmt::Debug for WiredProduct {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WiredProduct")
            .field("label", &self.spec.label)
            .field("url", &self.url)
            .field("id", &self.id)
            .field("key", &"<redacted>")
            .field("ready", &self.ready)
            .finish()
    }
}

impl WiredProduct {
    pub fn label(&self) -> &'static str {
        self.spec.label
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn env_key(&self) -> &'static str {
        self.spec.env_key
    }

    pub fn id_name(&self) -> Option<&'static str> {
        self.spec.id_name
    }

    /// The env name whose value is still the placeholder, for the caller's
    /// Action-required epilogue and pending Validate line.
    pub fn pending_env(&self) -> Option<&'static str> {
        (!self.ready).then_some(self.spec.env_key)
    }

    /// The `.env` block: real values, with the key as resolved (a placeholder when
    /// the human still has to paste it).
    pub(crate) fn env_entries(&self) -> Vec<(String, String)> {
        let mut entries = vec![(self.spec.env_url.to_string(), self.url.clone())];
        if let (Some(env_id), Some(id)) = (self.spec.env_id, &self.id) {
            entries.push((env_id.to_string(), id.clone()));
        }
        entries.push((self.spec.env_key.to_string(), self.key.clone()));
        entries
    }

    /// The `.env.example` block: shapes only, never values.
    pub(crate) fn example_entries(&self) -> Vec<(String, String)> {
        let mut entries = vec![(
            self.spec.env_url.to_string(),
            self.spec.example_url.to_string(),
        )];
        if let (Some(env_id), Some(id_name)) = (self.spec.env_id, self.spec.id_name) {
            entries.push((env_id.to_string(), format!("<{id_name}>")));
        }
        entries.push((
            self.spec.env_key.to_string(),
            format!("<{}>", self.spec.env_key),
        ));
        entries
    }
}

/// The self-check identity: a fixed session id, so re-runs overwrite one tiny
/// record in the customer's store instead of accumulating.
const CHECK_SESSION: &str = "redisctl-check";
const CHECK_TEXT: &str = "redisctl init connectivity check";

enum Auth<'a> {
    None,
    Bearer(&'a str),
    /// The Context Retriever convention; also flips error hints to "agent key".
    ApiKey(&'a str),
}

/// One JSON call with a 15s deadline. Error messages carry the method, path, and
/// status - never the key, and never a response body (it can echo the key back).
async fn api(
    client: &reqwest::Client,
    method: &str,
    url: &str,
    auth: Auth<'_>,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let pathname = reqwest::Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.to_string());
    let mut request = match method {
        "POST" => client.post(url),
        _ => client.get(url),
    };
    let (agent_key, mcp) = matches!(auth, Auth::ApiKey(_))
        .then_some((true, true))
        .unwrap_or((false, false));
    request = match auth {
        Auth::None => request,
        Auth::Bearer(key) => request.bearer_auth(key),
        Auth::ApiKey(key) => request.header("X-API-Key", key),
    };
    if let Some(body) = &body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|e| format!("{method} {pathname} failed: {}", e.without_url()))?;
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        let hint = match status {
            401 | 403 if agent_key => " - check the agent key",
            401 | 403 => " - check the API key",
            404 => " - check the endpoint and the id",
            _ => "",
        };
        return Err(format!("{method} {pathname} returned {status}{hint}"));
    }
    if text.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&text).map_err(|_| {
        format!(
            "{method} {pathname} returned a non-JSON body - is that the {} endpoint?",
            if mcp { "MCP" } else { "service" }
        )
    })
}

/// Live proof the product works: reads only, plus the one self-check session write
/// the health check needs. Returns the proof text for the Validate line.
pub async fn validate_product(product: &WiredProduct) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let base = &product.url;
    let id = product.id.as_deref().unwrap_or_default();
    match product.spec.key {
        ProductKey::AgentMemory => {
            // Health is unauthenticated by design; the write/read pair proves the
            // key and the store id together.
            api(&client, "GET", &format!("{base}/health"), Auth::None, None).await?;
            api(
                &client,
                "POST",
                &format!("{base}/v1/stores/{id}/session-memory/events"),
                Auth::Bearer(&product.key),
                Some(serde_json::json!({
                    "sessionId": CHECK_SESSION,
                    "actorId": "redisctl",
                    "role": "USER",
                    "content": [{ "text": CHECK_TEXT }],
                    "createdAt": chrono::Utc::now().to_rfc3339(),
                })),
            )
            .await?;
            let back = api(
                &client,
                "GET",
                &format!("{base}/v1/stores/{id}/session-memory/{CHECK_SESSION}"),
                Auth::Bearer(&product.key),
                None,
            )
            .await?;
            if back["events"]
                .as_array()
                .is_none_or(|events| events.is_empty())
            {
                return Err("session read back empty".to_string());
            }
            Ok("health, session write + read".to_string())
        }
        ProductKey::LangCache => {
            // Read-only on purpose: nothing synthetic lands in the user's cache,
            // and a zero-hit search still proves endpoint + id + key.
            api(
                &client,
                "POST",
                &format!("{base}/v1/caches/{id}/entries/search"),
                Auth::Bearer(&product.key),
                Some(serde_json::json!({ "prompt": CHECK_TEXT, "maxResults": 1 })),
            )
            .await?;
            Ok("entry search".to_string())
        }
        ProductKey::ContextRetriever => {
            let result = api(
                &client,
                "POST",
                base,
                Auth::ApiKey(&product.key),
                Some(serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}
                })),
            )
            .await?;
            if let Some(error) = result.get("error") {
                let code = error
                    .get("code")
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "an error".to_string());
                return Err(format!("MCP tools/list returned {code}"));
            }
            let Some(tools) = result["result"]["tools"].as_array() else {
                return Err("MCP tools/list returned no tool list".to_string());
            };
            let plural = if tools.len() == 1 { "" } else { "s" };
            Ok(format!("{} governed MCP tool{plural} listed", tools.len()))
        }
    }
}

/// Anything wrapped in angle brackets counts as unset - that is what placeholders
/// look like, both ours and the .env.example ones.
pub(crate) fn is_configured(value: &str) -> bool {
    !value.is_empty() && !(value.starts_with('<') && value.ends_with('>') && value.len() > 2)
}

/// The flag is the last resort by design: the env var and `.env` paths keep the
/// key out of shell history, and a stored key must win so validation always tests
/// the value `.env` actually holds (never-clobber would keep it anyway).
fn resolve_key(
    spec: &ProductSpec,
    api_key: Option<&str>,
    cwd: &Path,
    getenv: &dyn Fn(&str) -> Option<String>,
) -> String {
    [
        getenv(spec.env_key),
        read_env_key(cwd, ".env", spec.env_key),
        api_key.map(str::to_string),
    ]
    .into_iter()
    .flatten()
    .find(|value| is_configured(value))
    .unwrap_or_else(|| SECRET_PLACEHOLDER.to_string())
}

/// Resolve the run's products: explicit requests first, and under `complete` the
/// products already recorded in `.env`. A half-recorded product is an error, not a
/// silent skip. `getenv` is injected so tests never depend on the process
/// environment.
pub(crate) fn wire(
    cwd: &Path,
    requests: &[ProductRequest],
    api_key: Option<&str>,
    complete: bool,
    getenv: &dyn Fn(&str) -> Option<String>,
) -> Result<Vec<WiredProduct>, InitError> {
    let mut wired = Vec::new();
    for spec in &SPECS {
        let request = requests.iter().find(|r| r.key == spec.key);
        let url = request.map(|r| r.url.clone()).or_else(|| {
            complete
                .then(|| read_env_key(cwd, ".env", spec.env_url))
                .flatten()
        });
        let id = request
            .and_then(|r| r.id.clone())
            .or_else(|| match spec.env_id {
                Some(env_id) if complete => read_env_key(cwd, ".env", env_id),
                _ => None,
            });
        let (url, id) = match (url, id) {
            (None, None) => continue,
            (Some(url), id) if spec.env_id.is_none() || id.is_some() => (url, id),
            _ => {
                let needed = [Some(spec.env_url), spec.env_id]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" and ");
                return Err(InitError::ProductIncomplete {
                    label: spec.label.to_string(),
                    needed,
                });
            }
        };
        // The flag belongs to the one explicitly requested product only - a
        // rediscovered product must never authenticate with another product's key.
        let flag_key = request.and(api_key);
        let key = resolve_key(spec, flag_key, cwd, getenv);
        wired.push(WiredProduct {
            spec,
            url: url.trim_end_matches('/').to_string(),
            id,
            ready: is_configured(&key),
            key,
        });
    }
    Ok(wired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_and_empties_read_as_unconfigured() {
        assert!(!is_configured(""));
        assert!(!is_configured("<paste-from-redis-cloud>"));
        assert!(!is_configured("<LANGCACHE_API_KEY>"));
        assert!(is_configured("real-key"));
        assert!(is_configured("<>")); // not a placeholder shape
    }

    #[test]
    fn key_resolution_prefers_env_var_then_env_file_then_flag() {
        let none = |_: &str| None;
        let exported = |key: &str| (key == "LANGCACHE_API_KEY").then(|| "from-env-var".to_string());
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "LANGCACHE_API_KEY=\"from-env-file\"\n",
        )
        .unwrap();
        let spec = &SPECS[1];
        // A stored key wins over the flag, so validation always tests what .env holds.
        assert_eq!(
            resolve_key(spec, Some("from-flag"), dir.path(), &exported),
            "from-env-var"
        );
        assert_eq!(
            resolve_key(spec, Some("from-flag"), dir.path(), &none),
            "from-env-file"
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_key(spec, Some("from-flag"), empty.path(), &none),
            "from-flag"
        );
        assert_eq!(
            resolve_key(spec, None, empty.path(), &none),
            SECRET_PLACEHOLDER
        );
    }

    #[test]
    fn complete_discovers_from_env_and_rejects_half_configured_products() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "LANGCACHE_URL=\"https://l/\"\nLANGCACHE_CACHE_ID=\"c1\"\nLANGCACHE_API_KEY=\"k\"\n",
        )
        .unwrap();
        let wired = wire(dir.path(), &[], None, true, &|_| None).unwrap();
        assert_eq!(wired.len(), 1);
        assert_eq!(wired[0].label(), "LangCache");
        assert_eq!(wired[0].url, "https://l");
        assert!(wired[0].ready);

        std::fs::write(dir.path().join(".env"), "AGENT_MEMORY_URL=\"https://m\"\n").unwrap();
        let err = wire(dir.path(), &[], None, true, &|_| None).unwrap_err();
        assert_eq!(
            err.to_string(),
            "cannot complete Agent Memory: .env needs AGENT_MEMORY_URL and AGENT_MEMORY_STORE_ID."
        );
    }

    #[test]
    fn a_missing_key_wires_pending_with_the_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let requests = vec![ProductRequest {
            key: ProductKey::LangCache,
            url: "https://l".into(),
            id: Some("c1".into()),
        }];
        let wired = wire(dir.path(), &requests, None, false, &|_| None).unwrap();
        assert_eq!(wired[0].key, SECRET_PLACEHOLDER);
        assert_eq!(wired[0].pending_env(), Some("LANGCACHE_API_KEY"));
    }
}
