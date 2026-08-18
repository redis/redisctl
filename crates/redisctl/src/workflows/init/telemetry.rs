//! Anonymous usage counting: one `cli_run` event per run, so we can see how many
//! people run this command and which paths they take. This module owns the
//! transport - the device id, the opt-outs, the send timeout, and never letting a
//! failure escape. The property list is a closed allowlist of booleans, counts and
//! detector-owned vocabularies: paths, names, URLs and credentials structurally
//! cannot land in it.

use std::time::Instant;

use futures::FutureExt;
use serde::Serialize;

use super::output::dim;
use crate::cli::InitArgs;
use crate::error::RedisCtlError;

const DEFAULT_ENDPOINT: &str = "https://api2.amplitude.com/2/httpapi";
const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

const NOTICE: &str = "  Anonymous usage data (which paths get used, never paths, names or credentials).\n  Opt out with --no-telemetry, REDISCTL_INIT_TELEMETRY=0, or DO_NOT_TRACK=1.";

/// Release builds carry the key compiled in (the CI build step exports it); a
/// runtime value overrides it, and an exported-but-empty value is the off switch -
/// the compiled-in key does not resurface. Dev builds have neither and stay inert.
fn api_key() -> Option<String> {
    match std::env::var("REDISCTL_INIT_AMPLITUDE_KEY") {
        Ok(value) if value.trim().is_empty() => None,
        Ok(value) => Some(value),
        Err(_) => option_env!("REDISCTL_INIT_AMPLITUDE_KEY")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

fn opted_out(args: &InitArgs) -> bool {
    args.no_telemetry
        || std::env::var("REDISCTL_INIT_TELEMETRY").as_deref() == Ok("0")
        || matches!(
            std::env::var("DO_NOT_TRACK")
                .unwrap_or_default()
                .to_lowercase()
                .as_str(),
            "1" | "true"
        )
}

/// Debug echo (stderr): the exact body with the key redacted, and the send
/// outcome. Off for the usual falsy spellings, so CI can pass the variable verbatim.
fn debug_enabled() -> bool {
    !matches!(
        std::env::var("REDISCTL_INIT_TELEMETRY_DEBUG")
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
        "" | "0" | "false" | "no" | "off"
    )
}

fn debug(line: &str) {
    if debug_enabled() {
        eprintln!("{}", dim(&format!("telemetry: {line}")));
    }
}

/// A random id in the cache directory. Random rather than derived, so it cannot be
/// traced back to a machine or a user. `first` drives the one-time notice.
fn identify() -> (String, bool) {
    let path = directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".cache/redisctl/id"));
    if let Some(path) = &path
        && let Ok(existing) = std::fs::read_to_string(path)
        && !existing.trim().is_empty()
    {
        return (existing.trim().to_string(), false);
    }
    let id = uuid::Uuid::new_v4().to_string();
    // A read-only home is not a reason to fail the run.
    if let Some(path) = &path {
        let _ = path.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(path, format!("{id}\n"));
    }
    (id, true)
}

/// The closed property allowlist. Every field is a bool, a count, or a closed
/// vocabulary our own code produces; a golden test pins the exact key set.
#[derive(Default, Serialize)]
pub(crate) struct Properties {
    // Flag usage: presence and choice only, never values.
    pub url_provided: bool,
    pub name_provided: bool,
    pub agents_explicit: bool,
    pub cloud: bool,
    pub defaults: bool,
    pub no_install_cli: bool,
    pub skills_global: bool,
    pub skills_repo_overridden: bool,
    pub dry_run: bool,
    pub ci: bool,
    // Run shape.
    pub interactive: bool,
    pub wizard_questions_asked: usize,
    // Resolved facts, each from a vocabulary the detectors own.
    pub runtime: Option<&'static str>,
    pub package_manager: Option<&'static str>,
    pub framework: Option<String>,
    pub database_source: Option<&'static str>,
    pub cloud_created: bool,
    pub agents: Vec<&'static str>,
    pub agent_count: usize,
    pub skills_installed_count: usize,
    // Outcome, filled at finish.
    outcome: &'static str,
    failed_step: Option<&'static str>,
    exit_code: i32,
    duration_ms: u64,
    first_run: bool,
    version: &'static str,
    platform: &'static str,
}

/// Collects properties during the run and reports once at the end. Everything in
/// here is fallible-but-silent: telemetry never affects the run or its exit code.
pub(crate) struct Telemetry {
    started: Instant,
    step: &'static str,
    pub props: Properties,
}

impl Telemetry {
    pub fn start(args: &InitArgs) -> Self {
        let props = Properties {
            url_provided: args.url.is_some() || !args.pasted.is_empty(),
            name_provided: args.name.is_some(),
            agents_explicit: !args.agents.is_empty(),
            cloud: args.cloud,
            defaults: args.defaults,
            no_install_cli: args.no_install_cli,
            skills_global: args.skills_global,
            skills_repo_overridden: args.skills_repo.is_some(),
            dry_run: args.dry_run,
            ci: !std::env::var("CI").unwrap_or_default().is_empty(),
            ..Properties::default()
        };
        Self {
            started: Instant::now(),
            step: "start",
            props,
        }
    }

    /// The phase the run is in, reported as `failed_step` when it dies.
    pub fn step(&mut self, name: &'static str) {
        self.step = name;
    }

    /// Send the one event. Failures are swallowed: no retry, no message, no exit
    /// code - and without a key (or with an opt-out) nothing leaves the machine.
    pub async fn finish(mut self, args: &InitArgs, result: &Result<(), RedisCtlError>) {
        if opted_out(args) {
            return;
        }
        let Some(api_key) = api_key() else {
            debug("nothing shared, REDISCTL_INIT_AMPLITUDE_KEY is unset or blank");
            return;
        };
        let (outcome, failed_step) = match result {
            Ok(()) => ("success", None),
            Err(RedisCtlError::Cancelled { .. }) => ("cancelled", Some(self.step)),
            Err(_) => ("failed", Some(self.step)),
        };
        let (device_id, first) = identify();
        if first {
            // A diagnostic about the tool, not output of the run: stderr, so it
            // never lands in anything that parses stdout.
            eprintln!("\n{}", dim(NOTICE));
        }
        self.props.outcome = outcome;
        self.props.failed_step = failed_step;
        self.props.exit_code = result.as_ref().err().map(|e| e.exit_code()).unwrap_or(0);
        self.props.duration_ms = self.started.elapsed().as_millis() as u64;
        self.props.first_run = first;
        self.props.version = env!("CARGO_PKG_VERSION");
        self.props.platform = std::env::consts::OS;
        let body = serde_json::json!({
            "api_key": api_key,
            "events": [{
                "device_id": device_id,
                "event_type": "cli_run",
                // Amplitude's documented way to suppress IP-based geolocation:
                // without this it stores a city/region derived from the source IP.
                "ip": "0.0.0.0",
                "event_properties": self.props,
            }]
        });
        if debug_enabled() {
            let mut echoed = body.clone();
            echoed["api_key"] = "<redacted>".into();
            debug(&echoed.to_string());
        }
        let endpoint = std::env::var("REDISCTL_INIT_AMPLITUDE_URL")
            .unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string());
        let send = async {
            let client = reqwest::Client::builder().timeout(SEND_TIMEOUT).build()?;
            client.post(endpoint).json(&body).send().await
        };
        match std::panic::AssertUnwindSafe(send).catch_unwind().await {
            Ok(Ok(response)) => debug(&format!("shared, {}", response.status())),
            Ok(Err(e)) => debug(&format!("not shared, {e}")),
            Err(_) => debug("not shared, send panicked"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The golden schema: adding or renaming a property must show up here, so
    /// nothing ships unnoticed. Values are all bools, counts, or closed vocab.
    #[test]
    fn the_property_allowlist_is_exactly_this() {
        let value = serde_json::to_value(Properties::default()).unwrap();
        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "agent_count",
                "agents",
                "agents_explicit",
                "ci",
                "cloud",
                "cloud_created",
                "database_source",
                "defaults",
                "dry_run",
                "duration_ms",
                "exit_code",
                "failed_step",
                "first_run",
                "framework",
                "interactive",
                "name_provided",
                "no_install_cli",
                "outcome",
                "package_manager",
                "platform",
                "runtime",
                "skills_global",
                "skills_installed_count",
                "skills_repo_overridden",
                "url_provided",
                "version",
                "wizard_questions_asked",
            ]
        );
    }
}
