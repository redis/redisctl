//! The `--cloud` path: resolve a Redis Cloud database before the engine plans.
//!
//! Inventory spans both tiers and reuse is strictly by name; the picker and the
//! free-tier bookkeeping live here. Creation is delegated to the shared
//! `quick_database` engine (which manages its own `redisctl-<name>` marker
//! subscription). A pinned or available free subscription is created into directly.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::workflows::init::engine::{self as engine, Change, CloudFacts, CloudTier, Status};
use dialoguer::Select;
use redis_cloud::{CloudClient, CloudError};
use redisctl_core::cloud::quick_database::{self, QuickDatabaseError, QuickDatabaseParams};

use super::output;
use super::wizard::RedisTheme;
use crate::error::{RedisCtlError, exit_code};

/// Nothing real yet: the placeholder a dry run plans .env around, masked like a URL.
const PLANNED_URL: &str = "redis://default:<generated>@<endpoint-assigned-by-redis-cloud>";

pub(crate) struct CloudOutcome {
    pub url: String,
    pub facts: CloudFacts,
    /// Ledger entries to lead the Changes block with.
    pub changes: Vec<Change>,
}

struct Candidate {
    sub_id: i32,
    db_id: i32,
    name: String,
    tier: CloudTier,
    endpoint: Option<String>,
}

struct Inventory {
    candidates: Vec<Candidate>,
    /// Where a create would land: the pin, or the free subscription with room.
    target: Option<i32>,
    /// The full free subscription (id, database names), when that is why there is no target.
    free_full: Option<(i32, Vec<String>)>,
}

fn api_err(e: CloudError) -> RedisCtlError {
    match e {
        CloudError::AuthenticationFailed { .. } | CloudError::Forbidden { .. } => {
            RedisCtlError::init(
                "authentication_failed",
                exit_code::AUTH,
                format!(
                    "Redis Cloud: {e}\n  Sign in again: redisctl cloud auth login   (or pass -p <profile> with valid API keys)"
                ),
            )
        }
        e => RedisCtlError::ApiError {
            message: format!("Redis Cloud: {e}"),
        },
    }
}

fn other(message: String) -> RedisCtlError {
    RedisCtlError::Other(message)
}

pub(crate) async fn resolve(
    client: &CloudClient,
    cwd: &Path,
    name: Option<&str>,
    pin: Option<i32>,
    profile: Option<&str>,
    dry: bool,
    defaults: bool,
) -> Result<CloudOutcome, RedisCtlError> {
    let mut db_name = name.map(str::to_string).unwrap_or_else(|| {
        default_db_name(
            &cwd.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    });
    let inv = inventory(client, pin).await?;

    // Reuse strictly by name, across both tiers: a database already carrying the
    // name is connected, never recreated. Live even under --dry-run (reads only).
    if let Some(name) = name
        && let Some(cand) = inv.candidates.iter().find(|c| c.name == name)
    {
        return connect(client, cand, profile).await;
    }
    // Not reused, so an explicit name is one init would create.
    if let Some(name) = name {
        valid_db_name(name).map_err(|rule| {
            RedisCtlError::init(
                "invalid_input",
                exit_code::VALIDATION,
                format!("--name \"{name}\" cannot name a new Redis Cloud database: {rule}"),
            )
        })?;
    }

    // No name, databases exist: report the question (dry), ask (tty), refuse (piped).
    if name.is_none() && !inv.candidates.is_empty() {
        if dry {
            let offered = inv
                .candidates
                .iter()
                .map(|c| format!("\"{}\"", c.name))
                .collect::<Vec<_>>()
                .join(", ");
            let create = match pin {
                Some(pin) => format!("a new database \"{db_name}\" in subscription {pin}"),
                None => format!("a new free database \"{db_name}\""),
            };
            return Ok(planned(
                &db_name,
                inv.target,
                profile,
                Change::new(
                    "cloud:choice",
                    Status::Planned,
                    format!("would offer {offered} or {create}"),
                ),
            ));
        }
        // --defaults promised no prompts; there is no safe default among existing
        // databases, so it gets the same listing the non-interactive path does.
        if defaults || !std::io::stdin().is_terminal() {
            return Err(other(listing(&inv)));
        }
        match pick(&inv, &db_name, pin)? {
            Pick::Existing(cand) => return connect(client, cand, profile).await,
            Pick::Create(name) => db_name = name,
        }
    }

    // Nothing to pick from: a person at a terminal still gets to edit the name.
    if name.is_none()
        && inv.candidates.is_empty()
        && !dry
        && !defaults
        && std::io::stdin().is_terminal()
    {
        db_name = ask_db_name(&db_name)?;
    }

    if dry {
        let note = match inv.target {
            Some(target) => {
                format!("would create database \"{db_name}\" in Essentials subscription {target}")
            }
            None => format!("would create a free Essentials subscription + database \"{db_name}\""),
        };
        return Ok(planned(
            &db_name,
            inv.target,
            profile,
            Change::new(format!("cloud:{db_name}"), Status::Planned, note),
        ));
    }

    // The free tier being used up has ways out; say them instead of relaying the
    // create call's rejection.
    if inv.target.is_none()
        && let Some((id, names)) = &inv.free_full
    {
        let held = names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let first = names.first().map(String::as_str).unwrap_or("<name>");
        return Err(RedisCtlError::init(
            "api_error",
            exit_code::CONFLICT,
            format!(
                "the free Essentials subscription ({id}) is full - it holds {held}, and Redis Cloud allows one free subscription per account.\n  Share that database:  --cloud --name {first}\n  Use a paid one:       --cloud-subscription <id>   (redisctl api cloud get /fixed/subscriptions lists them)\n  Stay local instead:   drop --cloud to provision Docker"
            ),
        ));
    }

    match inv.target {
        Some(target) => create_pinned(client, &db_name, target, profile).await,
        None => create_free(client, &db_name, profile).await,
    }
}

/// Record the reuse (before the endpoint poll, so a timeout still reports it),
/// then wait for a usable URL.
async fn connect(
    client: &CloudClient,
    cand: &Candidate,
    profile: Option<&str>,
) -> Result<CloudOutcome, RedisCtlError> {
    let change = Change::new(
        format!("cloud:{}", cand.name),
        Status::Unchanged,
        format!(
            "database {} in {} subscription {}",
            cand.db_id,
            cand.tier.as_str(),
            cand.sub_id
        ),
    );
    let url = database_url(client, cand).await?;
    Ok(CloudOutcome {
        url,
        facts: CloudFacts {
            name: cand.name.clone(),
            subscription_id: cand.sub_id.to_string(),
            database_id: cand.db_id.to_string(),
            tier: cand.tier,
            profile: profile.map(str::to_string),
            created: false,
        },
        changes: vec![change],
    })
}

fn planned(
    db_name: &str,
    target: Option<i32>,
    profile: Option<&str>,
    change: Change,
) -> CloudOutcome {
    CloudOutcome {
        url: PLANNED_URL.to_string(),
        facts: CloudFacts {
            name: db_name.to_string(),
            subscription_id: target
                .map(|t| t.to_string())
                .unwrap_or_else(|| "<new>".to_string()),
            database_id: "<new>".to_string(),
            tier: CloudTier::Essentials,
            profile: profile.map(str::to_string),
            created: true,
        },
        changes: vec![change],
    }
}

async fn inventory(client: &CloudClient, pin: Option<i32>) -> Result<Inventory, RedisCtlError> {
    // A pinned subscription is the entire inventory, addressed as Essentials (the
    // tier init creates in); free-tier bookkeeping never applies to it.
    if let Some(pin) = pin {
        return Ok(Inventory {
            candidates: fixed_candidates(client, pin).await?,
            target: Some(pin),
            free_full: None,
        });
    }
    let mut inv = Inventory {
        candidates: Vec::new(),
        target: None,
        free_full: None,
    };
    let subs = client.fixed_subscriptions().list().await.map_err(api_err)?;
    for sub in subs.subscriptions.unwrap_or_default() {
        let Some(id) = sub.id else { continue };
        let dbs = fixed_candidates(client, id).await?;
        if sub.price == Some(0) {
            if (dbs.len() as i32) < sub.maximum_databases.unwrap_or(1) {
                inv.target = inv.target.or(Some(id));
            } else if inv.free_full.is_none() {
                inv.free_full = Some((id, dbs.iter().map(|c| c.name.clone()).collect()));
            }
        }
        inv.candidates.extend(dbs);
    }
    let pro = client
        .subscriptions()
        .get_all_subscriptions()
        .await
        .map_err(api_err)?;
    for sub in pro.subscriptions.unwrap_or_default() {
        let Some(id) = sub.id else { continue };
        inv.candidates.extend(pro_candidates(client, id).await?);
    }
    Ok(inv)
}

async fn fixed_candidates(
    client: &CloudClient,
    sub_id: i32,
) -> Result<Vec<Candidate>, RedisCtlError> {
    let payload = client
        .fixed_databases()
        .list(sub_id, None, None)
        .await
        .map_err(api_err)?;
    Ok(payload
        .subscription
        .map(|info| info.databases)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|db| {
            Some(Candidate {
                sub_id,
                db_id: db.database_id?,
                name: db.name.clone().unwrap_or_default(),
                tier: CloudTier::Essentials,
                endpoint: db.public_endpoint.clone(),
            })
        })
        .collect())
}

async fn pro_candidates(
    client: &CloudClient,
    sub_id: i32,
) -> Result<Vec<Candidate>, RedisCtlError> {
    let payload = client
        .databases()
        .get_subscription_databases(sub_id, None, None)
        .await
        .map_err(api_err)?;
    Ok(payload
        .subscription
        .into_iter()
        .flat_map(|info| info.databases)
        .map(|db| Candidate {
            sub_id,
            db_id: db.database_id,
            name: db.name.clone().unwrap_or_default(),
            tier: CloudTier::Flexible,
            endpoint: db.public_endpoint.clone(),
        })
        .collect())
}

fn describe(cand: &Candidate) -> String {
    format!(
        "{}  {}  {} subscription {}",
        cand.name,
        cand.endpoint.as_deref().unwrap_or("endpoint pending"),
        cand.tier.as_str(),
        cand.sub_id
    )
}

/// The non-interactive answer to "which database?": name them and the flag to pick one.
fn listing(inv: &Inventory) -> String {
    let plural = if inv.candidates.len() == 1 { "" } else { "s" };
    let mut lines = vec![format!(
        "this account already has {} Redis Cloud database{plural}:",
        inv.candidates.len()
    )];
    for (i, cand) in inv.candidates.iter().enumerate() {
        lines.push(format!("  {}) {}", i + 1, describe(cand)));
    }
    lines.push("  Connect to one:  --cloud --name <its name>".to_string());
    let mut create = "  Create another:  --cloud --name <a new name>".to_string();
    if inv.target.is_none() && inv.free_full.is_some() {
        create.push_str("   (the free plan is already used up)");
    }
    lines.push(create);
    lines.join("\n")
}

enum Pick<'a> {
    Existing(&'a Candidate),
    Create(String),
}

/// The folder's slug, shaped to pass `valid_db_name`: a leading letter, 3-40 chars.
fn default_db_name(folder: &str) -> String {
    let mut name = engine::slug(folder);
    if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        name.insert_str(0, "db-");
    }
    if name.len() < 3 {
        name.push_str("-db");
    }
    name.truncate(40);
    name.trim_end_matches('-').to_string()
}

/// Mirrors `quick_database`'s private `validate_name`: 3-40 chars of `[a-z0-9-]`,
/// starting with a lowercase letter, ending alphanumeric, no `--`.
fn valid_db_name(name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let ok = (3..=40).contains(&name.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.contains("--");
    if ok {
        Ok(())
    } else {
        Err(
            "3-40 lowercase letters, digits or hyphens; starts with a letter, ends alphanumeric, no \"--\""
                .to_string(),
        )
    }
}

/// Registered with `wizard::is_wizard_prompt`, so cancelling gets wizard tips.
pub(crate) const PICKER_PROMPT: &str = "Redis Cloud - this account already has databases";
pub(crate) const NAME_PROMPT: &str = "Name for the new database";

fn ask_db_name(suggested: &str) -> Result<String, RedisCtlError> {
    dialoguer::Input::with_theme(&RedisTheme)
        .with_prompt(NAME_PROMPT)
        .default(suggested.to_string())
        .validate_with(|input: &String| valid_db_name(input))
        .interact_text()
        .map_err(super::wizard::prompt_failed)
}

fn pick<'a>(
    inv: &'a Inventory,
    db_name: &str,
    pin: Option<i32>,
) -> Result<Pick<'a>, RedisCtlError> {
    let mut items: Vec<String> = inv.candidates.iter().map(describe).collect();
    // An unavailable create stays on the list carrying the reason (the wizard's
    // pattern); choosing it re-prompts instead of aborting the session.
    let mut create = match pin {
        Some(pin) => format!("create a new database in subscription {pin}"),
        None => "create a new free database".to_string(),
    };
    if inv.target.is_none() && inv.free_full.is_some() {
        create.push_str("   (unavailable: the free plan is already used up)");
    }
    items.push(create);
    loop {
        let selection = Select::with_theme(&RedisTheme)
            .with_prompt(PICKER_PROMPT)
            .items(&items)
            .default(0)
            .interact_opt()
            .map_err(super::wizard::prompt_failed)?;
        match selection {
            None => {
                return Err(RedisCtlError::Cancelled {
                    prompt: PICKER_PROMPT.to_string(),
                });
            }
            Some(i) if i < inv.candidates.len() => return Ok(Pick::Existing(&inv.candidates[i])),
            Some(_) if inv.target.is_none() && inv.free_full.is_some() => {
                eprintln!(
                    "  The free plan is already used up - connect to an existing database, or press Esc and re-run with --cloud-subscription <id>."
                );
            }
            Some(_) => return Ok(Pick::Create(ask_db_name(db_name)?)),
        }
    }
}

/// Poll the single-database endpoint (the only response carrying the password)
/// until it is usable. The create task reports success seconds before the endpoint
/// exists, hence the wait.
async fn database_url(client: &CloudClient, cand: &Candidate) -> Result<String, RedisCtlError> {
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    let mut progress: Option<output::Progress> = None;
    loop {
        let (endpoint, password, tls, default_disabled, status) = match cand.tier {
            CloudTier::Essentials => {
                let db = client
                    .fixed_databases()
                    .get_by_id(cand.sub_id, cand.db_id)
                    .await
                    .map_err(api_err)?;
                let sec = db.security.as_ref();
                (
                    db.public_endpoint.clone(),
                    sec.and_then(|s| s.password.clone()),
                    sec.and_then(|s| s.enable_tls),
                    sec.and_then(|s| s.default_user_enabled) == Some(false),
                    db.status.clone(),
                )
            }
            CloudTier::Flexible => {
                let db = client
                    .databases()
                    .get_subscription_database_by_id(cand.sub_id, cand.db_id)
                    .await
                    .map_err(api_err)?;
                let sec = db.security.as_ref();
                (
                    db.public_endpoint.clone(),
                    sec.and_then(|s| s.password.clone()),
                    sec.and_then(|s| s.enable_tls),
                    sec.and_then(|s| s.enable_default_user) == Some(false),
                    db.status.clone(),
                )
            }
        };
        if let Some(endpoint) = &endpoint
            && (password.is_some() || default_disabled)
        {
            if let Some(progress) = progress.as_mut() {
                progress.done(" ready");
            }
            let scheme = if tls == Some(true) { "rediss" } else { "redis" };
            return Ok(match password {
                Some(password) => format!(
                    "{scheme}://default:{}@{endpoint}",
                    urlencoding::encode(&password)
                ),
                None => format!("{scheme}://{endpoint}"),
            });
        }
        let last_status = status.unwrap_or_else(|| "unknown".to_string());
        if progress.is_none() {
            progress = Some(output::progress("waiting for the endpoint to come up"));
        }
        if std::time::Instant::now() >= deadline {
            if let Some(progress) = progress.as_mut() {
                progress.done("");
            }
            return Err(other(format!(
                "Redis Cloud database {}{}/databases/{} exposed no endpoint within 180s (status: {last_status}).\n  It is provisioned - re-run the same command once it is active.",
                cand.tier.api_base(),
                cand.sub_id,
                cand.db_id
            )));
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// The engine writes credentials to a file and never returns them; hand it one
/// and read the URL back. It writes into whatever is already at the path, so the
/// file goes in a fresh owner-only directory.
struct TempCredentials(PathBuf);

impl TempCredentials {
    fn new() -> Result<Self, RedisCtlError> {
        let dir = std::env::temp_dir().join(format!("redisctl-init-{}", uuid::Uuid::new_v4()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&dir).map_err(|e| {
            other(format!(
                "could not create a private directory for the credentials: {e}"
            ))
        })?;
        Ok(Self(dir.join("credentials.env")))
    }

    fn read_url(&self) -> Result<String, RedisCtlError> {
        let text = std::fs::read_to_string(&self.0)
            .map_err(|e| other(format!("could not read provisioned credentials back: {e}")))?;
        text.lines()
            .find_map(|line| line.strip_prefix("REDIS_URL="))
            .map(str::to_string)
            .ok_or_else(|| other("provisioned credentials carry no REDIS_URL".to_string()))
    }
}

impl Drop for TempCredentials {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

async fn marker_subscription(
    client: &CloudClient,
    db_name: &str,
) -> Result<Option<i32>, RedisCtlError> {
    let marker = format!("redisctl-{db_name}");
    let subs = client.fixed_subscriptions().list().await.map_err(api_err)?;
    Ok(subs
        .subscriptions
        .unwrap_or_default()
        .into_iter()
        .find(|s| {
            s.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(&marker))
        })
        .and_then(|s| s.id))
}

fn quick_err(e: QuickDatabaseError) -> RedisCtlError {
    let (code, exit) = match &e {
        QuickDatabaseError::InvalidName(_) => ("invalid_input", exit_code::VALIDATION),
        QuickDatabaseError::NameConflict(_)
        | QuickDatabaseError::FreeDbExists(_)
        | QuickDatabaseError::QuotaExceeded(_) => ("api_error", exit_code::CONFLICT),
        QuickDatabaseError::NotAuthenticated(_) => ("authentication_failed", exit_code::AUTH),
        QuickDatabaseError::Transient(_) => ("api_error", exit_code::UPSTREAM),
        QuickDatabaseError::RateLimited(_) => ("api_error", exit_code::RATE_LIMITED),
        QuickDatabaseError::Other(_) => ("error", exit_code::GENERIC),
    };
    RedisCtlError::init(code, exit, e.to_string())
}

/// Free-plan creation, delegated to the shared engine (idempotent via its
/// `redisctl-<name>` subscription marker).
async fn create_free(
    client: &CloudClient,
    db_name: &str,
    profile: Option<&str>,
) -> Result<CloudOutcome, RedisCtlError> {
    let sub_existed = marker_subscription(client, db_name).await?;
    let temp = TempCredentials::new()?;
    let params = QuickDatabaseParams {
        output_credentials: temp.0.clone(),
        ..QuickDatabaseParams::new(db_name)
    };
    let mut progress = output::progress(&format!(
        "creating free Essentials database \"{db_name}\" on Redis Cloud"
    ));
    let report = match quick_database::provision(client, &params).await {
        Ok(report) => {
            progress.done(" done");
            report
        }
        Err(e) => {
            progress.done(" failed");
            return Err(quick_err(e));
        }
    };
    let url = temp.read_url()?;
    let sub_id = marker_subscription(client, db_name)
        .await?
        .ok_or_else(|| other("the created subscription is not listed yet - re-run".to_string()))?;
    let mut changes = Vec::new();
    if sub_existed.is_none() {
        changes.push(Change::new(
            format!("cloud:subscription/{sub_id}"),
            Status::Created,
            "free Essentials plan",
        ));
    }
    let created = report.status != "reused";
    changes.push(Change::new(
        format!("cloud:{db_name}"),
        if created {
            Status::Created
        } else {
            Status::Unchanged
        },
        format!(
            "database {} in Essentials subscription {sub_id}",
            report.database.id
        ),
    ));
    Ok(CloudOutcome {
        url,
        facts: CloudFacts {
            name: db_name.to_string(),
            subscription_id: sub_id.to_string(),
            database_id: report.database.id.clone(),
            tier: CloudTier::Essentials,
            profile: profile.map(str::to_string),
            created,
        },
        changes,
    })
}

/// Create directly in the selected Essentials subscription.
async fn create_pinned(
    client: &CloudClient,
    db_name: &str,
    pin: i32,
    profile: Option<&str>,
) -> Result<CloudOutcome, RedisCtlError> {
    let mut progress = output::progress(&format!(
        "creating database \"{db_name}\" in subscription {pin}"
    ));
    let request = redis_cloud::fixed::databases::FixedDatabaseCreateRequest::builder()
        .name(db_name)
        .build();
    let result: Result<i32, RedisCtlError> = async {
        let task = client
            .fixed_databases()
            .create(pin, &request)
            .await
            .map_err(api_err)?;
        let task_id = task
            .task_id
            .ok_or_else(|| other("Redis Cloud returned no task id for the create".to_string()))?;
        let completed = redisctl_core::poll_task(
            client,
            &task_id,
            Duration::from_secs(600),
            Duration::from_secs(5),
            None,
        )
        .await
        .map_err(|e| other(format!("create task failed: {e}")))?;
        completed
            .response
            .and_then(|r| r.resource_id)
            .ok_or_else(|| other("Redis Cloud created no database id".to_string()))
    }
    .await;
    let db_id = match result {
        Ok(id) => {
            progress.done(" done");
            id
        }
        Err(e) => {
            progress.done(" failed");
            return Err(e);
        }
    };
    let cand = Candidate {
        sub_id: pin,
        db_id,
        name: db_name.to_string(),
        tier: CloudTier::Essentials,
        endpoint: None,
    };
    let url = database_url(client, &cand).await?;
    Ok(CloudOutcome {
        url,
        facts: CloudFacts {
            name: db_name.to_string(),
            subscription_id: pin.to_string(),
            database_id: db_id.to_string(),
            tier: CloudTier::Essentials,
            profile: profile.map(str::to_string),
            created: true,
        },
        changes: vec![Change::new(
            format!("cloud:{db_name}"),
            Status::Created,
            format!("database {db_id} in Essentials subscription {pin}"),
        )],
    })
}

/// `cloud auth login` refuses an unusable keyring by pointing at its own
/// `--allow-plaintext`, which init does not have; name the command that does.
pub(crate) fn sign_in_error(e: RedisCtlError, profile: Option<&str>) -> RedisCtlError {
    if let RedisCtlError::Structured(se) = &e
        && se.code == "keyring_unavailable"
    {
        // Each refusal reads "<reason>. Re-run ... instead.[ <key left behind>]".
        let (reason, hint) = se
            .message
            .split_once(" Re-run")
            .unwrap_or((&se.message, ""));
        let left_behind = hint.split_once("instead.").map_or("", |(_, rest)| rest);
        let login = match profile {
            Some(p) => format!("redisctl --profile {p} cloud auth login --allow-plaintext"),
            None => "redisctl cloud auth login --allow-plaintext".to_string(),
        };
        return RedisCtlError::init(
            "keyring_unavailable",
            exit_code::GENERIC,
            format!(
                "{reason}{left_behind}\n  Store the credentials in the config file (0600) instead: {login}\n  Then re-run redisctl init."
            ),
        );
    }
    e
}

#[cfg(test)]
mod tests {
    use super::{
        QuickDatabaseError, RedisCtlError, TempCredentials, api_err, default_db_name, quick_err,
        sign_in_error, valid_db_name,
    };
    use crate::error::exit_code;
    use crate::structured_error::StructuredError;
    use redis_cloud::CloudError;

    #[test]
    fn create_engine_errors_map_onto_inits_exit_codes() {
        let m = || "x".to_string();
        for (err, code) in [
            (QuickDatabaseError::InvalidName(m()), exit_code::VALIDATION),
            (QuickDatabaseError::NameConflict(m()), exit_code::CONFLICT),
            (QuickDatabaseError::FreeDbExists(m()), exit_code::CONFLICT),
            (QuickDatabaseError::QuotaExceeded(m()), exit_code::CONFLICT),
            (QuickDatabaseError::NotAuthenticated(m()), exit_code::AUTH),
            (QuickDatabaseError::Transient(m()), exit_code::UPSTREAM),
            (
                QuickDatabaseError::RateLimited(m()),
                exit_code::RATE_LIMITED,
            ),
            (QuickDatabaseError::Other(m()), exit_code::GENERIC),
        ] {
            let label = format!("{err:?}");
            assert_eq!(quick_err(err).exit_code(), code, "{label}");
        }
    }

    #[test]
    fn api_errors_map_onto_inits_exit_codes() {
        let m = || "x".to_string();
        for (err, code) in [
            (
                CloudError::AuthenticationFailed { message: m() },
                exit_code::AUTH,
            ),
            (CloudError::Forbidden { message: m() }, exit_code::AUTH),
            (CloudError::NotFound { message: m() }, exit_code::NOT_FOUND),
            (
                CloudError::ApiError {
                    code: 409,
                    message: m(),
                },
                exit_code::CONFLICT,
            ),
            (
                CloudError::RateLimited { message: m() },
                exit_code::RATE_LIMITED,
            ),
            (
                CloudError::InternalServerError { message: m() },
                exit_code::UPSTREAM,
            ),
        ] {
            let label = format!("{err:?}");
            assert_eq!(api_err(err).exit_code(), code, "{label}");
        }
    }

    /// The three shapes `cloud auth login` refuses a keyring with; init has no
    /// `--allow-plaintext`, so the remedy has to be the login command itself.
    #[test]
    fn a_keyring_refusal_names_the_login_command_not_an_init_flag() {
        for (message, reason) in [
            (
                "no OS keyring is available to store the credentials, and storing them in the config file has to be asked for. Re-run with `--allow-plaintext` to store them there (0600) instead.",
                "no OS keyring is available to store the credentials",
            ),
            (
                "the OS keyring cannot store credentials (collection is locked). Re-run with `--allow-plaintext` to store them in the config file (0600) instead.",
                "the OS keyring cannot store credentials (collection is locked).",
            ),
            (
                "failed to store credentials in the OS keyring (no secret service). Re-run `redisctl cloud auth login --allow-plaintext` to store them in the config file (0600) instead. The key redisctl-cli-9 was created but not stored; revoke it in the Redis Cloud console (Access Management > API Keys).",
                "failed to store credentials in the OS keyring (no secret service). The key redisctl-cli-9 was created but not stored; revoke it",
            ),
        ] {
            let e = sign_in_error(
                RedisCtlError::Structured(Box::new(StructuredError::keyring_unavailable(message))),
                None,
            );
            let text = e.to_string();
            assert_eq!(e.exit_code(), exit_code::GENERIC, "{text}");
            assert!(text.contains(reason), "{text}");
            assert!(
                text.contains("redisctl cloud auth login --allow-plaintext"),
                "{text}"
            );
            assert!(text.contains("re-run redisctl init"), "{text}");
            assert!(!text.contains("Re-run"), "{text}");
            assert_eq!(text.matches("--allow-plaintext").count(), 1, "{text}");
        }

        let pinned = sign_in_error(
            RedisCtlError::Structured(Box::new(StructuredError::keyring_unavailable("x"))),
            Some("work"),
        );
        assert!(
            pinned
                .to_string()
                .contains("redisctl --profile work cloud auth login --allow-plaintext"),
            "{pinned}"
        );

        let other = sign_in_error(
            RedisCtlError::Structured(Box::new(StructuredError::not_authenticated("x"))),
            None,
        );
        assert!(matches!(other, RedisCtlError::Structured(_)));
    }

    /// The create engine writes into whatever file is at the path, so the path must be
    /// one nobody else could have prepared: a fresh owner-only directory per run.
    #[test]
    fn credentials_land_in_a_fresh_private_directory() {
        let first = TempCredentials::new().unwrap();
        let second = TempCredentials::new().unwrap();
        let dir = first.0.parent().unwrap().to_path_buf();
        assert_ne!(dir, std::env::temp_dir(), "{}", first.0.display());
        assert_eq!(dir.parent(), Some(std::env::temp_dir().as_path()));
        assert_ne!(first.0.parent(), second.0.parent());
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let id = name.strip_prefix("redisctl-init-").unwrap_or_default();
        assert!(uuid::Uuid::parse_str(id).is_ok(), "{name}");
        assert!(dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{mode:o}");
        }
        assert!(!first.0.exists());
        std::fs::write(&first.0, "REDIS_URL=redis://h:1\n").unwrap();
        drop(first);
        assert!(!dir.exists());
    }

    #[test]
    fn default_names_always_pass_the_rule() {
        for folder in [
            "1-cloud",
            "x",
            "",
            "!!!",
            "My App",
            "9",
            &"a".repeat(60),
            "ab-cd-efgh-ijkl-mnop-qrst-uvwx-yzab-cdef-g",
        ] {
            let name = default_db_name(folder);
            assert!(valid_db_name(&name).is_ok(), "{folder:?} -> {name:?}");
        }
    }

    #[test]
    fn name_rule_mirror_accepts_and_rejects_like_the_engine() {
        assert!(valid_db_name("ask-bot-3").is_ok());
        for bad in ["ab", "3abc", "Abc", "ab_c", "a--b", "abc-"] {
            assert!(valid_db_name(bad).is_err(), "{bad} should be rejected");
        }
        assert!(valid_db_name(&"a".repeat(41)).is_err());
    }
}
