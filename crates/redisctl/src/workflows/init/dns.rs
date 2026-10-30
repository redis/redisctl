//! DNS readiness for freshly published cloud endpoints.
//!
//! A new database's hostname can lag its API "active" status by a minute, and the
//! cloud zones tell resolvers to cache a failed lookup for up to 30 minutes - so a
//! single premature lookup through the OS resolver poisons every later attempt.
//! This gate asks the zone's authoritative server directly (no cache anywhere in
//! the path) and only lets the run proceed to a normal connect once the record
//! exists. Any infrastructure hiccup here degrades to proceeding; validation still
//! reports the truth.

use std::net::IpAddr;
use std::time::Duration;

use hickory_resolver::TokioResolver;
use hickory_resolver::config::{ConnectionConfig, NameServerConfig, ResolverConfig};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::{RData, RecordType};

use super::output;

const DEADLINE: Duration = Duration::from_secs(150);
const POLL: Duration = Duration::from_secs(3);

/// Loopback and literal addresses never need a record.
fn needs_gate(host: &str) -> bool {
    host.parse::<IpAddr>().is_err() && host != "localhost"
}

/// The candidate zones a host could live in, most specific first, never shorter
/// than two labels ("db-qa.redis.io" -> ["db-qa.redis.io", "redis.io"]).
fn zone_candidates(host: &str) -> Vec<String> {
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    (1..labels.len().saturating_sub(1))
        .map(|i| labels[i..].join("."))
        .collect()
}

fn host_of(url: &str) -> Option<String> {
    Some(url::Url::parse(url).ok()?.host_str()?.to_string())
}

/// Whether a validation failure is a name-resolution failure (as opposed to a
/// refused/timed-out connection): Rust's lookup errors carry this prefix on every
/// platform.
pub(crate) fn is_dns_error(error: &str) -> bool {
    error.contains("failed to lookup address")
        || error.contains("nodename nor servname")
        || error.contains("Name or service not known")
}

/// A resolver pointed straight at one of the zone's authoritative servers, with
/// caching off - answers reflect the zone right now. Discovery goes through NS
/// records, not SOA: filtering resolvers (observed on a corporate network) drop
/// SOA queries but answer NS.
async fn authoritative_resolver(host: &str) -> Option<TokioResolver> {
    let system = TokioResolver::builder_tokio().ok()?.build().ok()?;
    for zone in zone_candidates(host) {
        let Ok(ns) = system.lookup(zone.as_str(), RecordType::NS).await else {
            continue;
        };
        let Some(ns_name) = ns.answers().iter().find_map(|record| match &record.data {
            RData::NS(name) => Some(name.0.to_utf8()),
            _ => None,
        }) else {
            continue;
        };
        // IPv4 only: v6 routes are absent on enough networks that a v6-first
        // answer would read as the record not existing.
        let ns_ip = system
            .lookup_ip(ns_name.as_str())
            .await
            .ok()?
            .iter()
            .find(|ip| ip.is_ipv4())?;
        let config = ResolverConfig::from_parts(
            None,
            Vec::new(),
            vec![NameServerConfig::new(
                ns_ip,
                false,
                vec![ConnectionConfig::udp()],
            )],
        );
        let mut builder =
            TokioResolver::builder_with_config(config, TokioRuntimeProvider::default());
        builder.options_mut().cache_size = 0;
        builder.options_mut().attempts = 1;
        return builder.build().ok();
    }
    None
}

/// Any answer counts as published - an A record directly (what the cloud zones
/// serve), or a CNAME the recursive resolvers will chase once it exists.
async fn resolves(resolver: &TokioResolver, host: &str) -> bool {
    resolver
        .lookup(host, RecordType::A)
        .await
        .map(|answer| !answer.answers().is_empty())
        .unwrap_or(false)
}

/// Block until the endpoint's DNS record is visible on the authoritative server,
/// bounded by [`DEADLINE`]. Called on cloud-resolved URLs only: a user-pasted URL
/// must keep failing fast when its host is stale.
pub(crate) async fn wait_for_endpoint_dns(url: &str) {
    let Some(host) = host_of(url) else { return };
    if !needs_gate(&host) {
        return;
    }
    let Some(resolver) = authoritative_resolver(&host).await else {
        return;
    };
    if resolves(&resolver, &host).await {
        return;
    }
    let mut progress =
        output::progress("waiting for the database's DNS record (new endpoints take a moment)");
    let deadline = tokio::time::Instant::now() + DEADLINE;
    loop {
        tokio::time::sleep(POLL).await;
        if resolves(&resolver, &host).await {
            progress.done(" ready");
            return;
        }
        if tokio::time::Instant::now() > deadline {
            progress.done(" still not visible - validation may fail");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_addresses_and_localhost_skip_the_gate() {
        assert!(!needs_gate("127.0.0.1"));
        assert!(!needs_gate("::1"));
        assert!(!needs_gate("localhost"));
        assert!(needs_gate("brand-new-63625.db-qa.redis.io"));
    }

    #[test]
    fn zone_candidates_walk_up_but_stop_above_the_tld() {
        assert_eq!(
            zone_candidates("db-1.db-qa.redis.io"),
            vec!["db-qa.redis.io", "redis.io"]
        );
        assert!(zone_candidates("redis.io").is_empty());
    }

    #[test]
    fn hosts_come_out_of_credentialed_urls() {
        assert_eq!(
            host_of("redis://default:pw@x-1.db-qa.redis.io:12871").as_deref(),
            Some("x-1.db-qa.redis.io")
        );
        assert_eq!(host_of("redis://127.0.0.1:9").as_deref(), Some("127.0.0.1"));
    }

    #[tokio::test]
    #[ignore = "requires network (live authoritative DNS query)"]
    async fn a_published_record_passes_the_gate_quickly() {
        // A host with a direct A record at its zone's authority - the same shape
        // the cloud endpoint zones serve.
        let resolver = authoritative_resolver("www.cloudflare.com")
            .await
            .expect("resolver");
        assert!(resolves(&resolver, "www.cloudflare.com").await);
    }

    #[test]
    fn dns_errors_are_told_apart_from_refused_connections() {
        assert!(is_dns_error(
            "failed to lookup address information: nodename nor servname provided, or not known"
        ));
        assert!(is_dns_error(
            "failed to lookup address information: Name or service not known"
        ));
        assert!(!is_dns_error("Connection refused (os error 61)"));
    }
}
