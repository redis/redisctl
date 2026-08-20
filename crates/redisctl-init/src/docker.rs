//! The local database: a Docker container this tool owns, reused across runs when it
//! can be. Probing happens at plan time (read-only), starting and creating at apply
//! time. All Docker interaction shells out to the `docker` CLI.

use std::path::Path;
use std::time::Duration;

use crate::change::{Change, Status};
use crate::env::read_env_key;
use crate::util::{sh, slug};
use crate::{Event, InitError};

/// The decided database resolution, fixed at plan time.
#[derive(Debug)]
pub(crate) enum DatabaseAction {
    /// A URL the caller supplied; nothing to provision.
    Provided { url: String },
    /// `.env` already carries `REDIS_URL`; a stopped local container gets a
    /// best-effort restart (validation reports the truth either way).
    ExistingEnv {
        url: String,
        container: Option<String>,
        restart: bool,
    },
    /// A container from an earlier run exists but is stopped.
    StartExisting { name: String, url: String },
    /// A container from an earlier run is already serving.
    AlreadyRunning {
        name: String,
        port: u16,
        url: String,
    },
    /// The user chose to connect later: a placeholder REDIS_URL gets written and
    /// the run ends with instructions instead of validation.
    Placeholder,
    /// No container yet: run the image on the chosen free port.
    RunNew {
        name: String,
        image: String,
        image_local: bool,
        port: u16,
        url: String,
    },
}

impl DatabaseAction {
    pub(crate) fn container(&self) -> Option<&str> {
        match self {
            DatabaseAction::Provided { .. } | DatabaseAction::Placeholder => None,
            DatabaseAction::ExistingEnv { container, .. } => container.as_deref(),
            DatabaseAction::StartExisting { name, .. }
            | DatabaseAction::AlreadyRunning { name, .. }
            | DatabaseAction::RunNew { name, .. } => Some(name),
        }
    }

    pub(crate) fn url(&self) -> &str {
        match self {
            DatabaseAction::Placeholder => PLACEHOLDER_URL,
            DatabaseAction::Provided { url }
            | DatabaseAction::ExistingEnv { url, .. }
            | DatabaseAction::StartExisting { url, .. }
            | DatabaseAction::AlreadyRunning { url, .. }
            | DatabaseAction::RunNew { url, .. } => url,
        }
    }

    pub(crate) fn source(&self, applied: bool) -> &'static str {
        match self {
            DatabaseAction::Provided { .. } => "provided URL",
            DatabaseAction::Placeholder => "placeholder - fill .env",
            DatabaseAction::ExistingEnv { .. } => "existing .env",
            DatabaseAction::StartExisting { .. } | DatabaseAction::AlreadyRunning { .. } => {
                "existing Docker container"
            }
            DatabaseAction::RunNew { .. } if applied => "new Docker container",
            DatabaseAction::RunNew { .. } => "Docker (planned)",
        }
    }

    pub(crate) fn preview(&self) -> Option<Change> {
        match self {
            DatabaseAction::ExistingEnv {
                restart: true,
                container: Some(name),
                ..
            } => Some(Change::new(
                format!("docker:{name}"),
                Status::Planned,
                "would start existing container",
            )),
            DatabaseAction::Placeholder => Some(placeholder_change()),
            DatabaseAction::Provided { .. } | DatabaseAction::ExistingEnv { .. } => None,
            DatabaseAction::StartExisting { name, .. } => Some(Change::new(
                format!("docker:{name}"),
                Status::Planned,
                "would start existing container",
            )),
            DatabaseAction::AlreadyRunning { name, port, .. } => Some(Change::new(
                format!("docker:{name}"),
                Status::Unchanged,
                format!("already running on port {port}"),
            )),
            DatabaseAction::RunNew {
                name, image, port, ..
            } => Some(Change::new(
                format!("docker:{name}"),
                Status::Planned,
                format!("would run: docker run -d --name {name} -p 127.0.0.1:{port}:6379 {image}"),
            )),
        }
    }
}

/// The value the "connect later" path writes to .env; a run that finds it treats
/// the database as still pending instead of validating garbage.
pub const PLACEHOLDER_URL: &str = "<paste-your-redis-url>";

/// The same line in the plan preview and the apply report - dry-run parity.
fn placeholder_change() -> Change {
    Change::new(
        "database",
        Status::Skipped,
        "no connection yet - fill REDIS_URL in .env, then run redisctl init --complete",
    )
}

pub fn docker_ok() -> bool {
    sh("docker", &["info", "--format", "{{.ServerVersion}}"]).status == 0
}

struct ContainerInfo {
    running: bool,
    port: u16,
}

const INSPECT_FORMAT: &str =
    r#"{{.State.Running}} {{(index (index .HostConfig.PortBindings "6379/tcp") 0).HostPort}}"#;

fn container_info(name: &str) -> Option<ContainerInfo> {
    let r = sh("docker", &["inspect", "-f", INSPECT_FORMAT, name]);
    if r.status != 0 {
        return None;
    }
    parse_container_info(&r.stdout)
}

fn parse_container_info(stdout: &str) -> Option<ContainerInfo> {
    let mut parts = stdout.trim().split(' ');
    let running = parts.next()? == "true";
    let port = parts.next()?.parse().ok()?;
    Some(ContainerInfo { running, port })
}

fn container_name(cwd: &Path) -> String {
    let basename = cwd
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("redis-init-{}", slug(&basename))
}

/// Prefer an image that is already local - offline-safe and instant. Pull only as a
/// last resort.
const IMAGE_PREFS: [&str; 3] = ["redis:8-alpine", "redis:8", "redis:latest"];

fn resolve_image() -> (String, bool) {
    match IMAGE_PREFS
        .iter()
        .find(|img| sh("docker", &["image", "inspect", img]).status == 0)
    {
        Some(img) => ((*img).to_string(), true),
        None => (IMAGE_PREFS[0].to_string(), false),
    }
}

/// Probed on loopback and both wildcard families: Docker publishes ports via a
/// dual-stack [::] listener an IPv4-only probe does not see, while SO_REUSEADDR
/// (which the std listener sets) lets a wildcard probe succeed over a
/// loopback-only listener.
fn port_is_free(port: u16) -> bool {
    let in_use = |result: std::io::Result<std::net::TcpListener>| matches!(result, Err(e) if e.kind() == std::io::ErrorKind::AddrInUse);
    !in_use(std::net::TcpListener::bind(("127.0.0.1", port)))
        && !in_use(std::net::TcpListener::bind(("0.0.0.0", port)))
        && !in_use(std::net::TcpListener::bind(("::", port)))
}

fn free_port(start: u16) -> Option<u16> {
    (start..start + 100).find(|p| port_is_free(*p))
}

/// Decide the action for a `.env` that already carries a URL. A leftover container
/// only counts when the URL still points at it - after a move to a remote database
/// it must not poison the skill or get restarted. Split from the probe so that gate
/// is testable without Docker.
fn decide_existing_env(url: String, name: String, info: Option<ContainerInfo>) -> DatabaseAction {
    let local = url.contains("localhost") || url.contains("127.0.0.1");
    let restart = info.as_ref().is_some_and(|info| !info.running && local);
    DatabaseAction::ExistingEnv {
        url,
        container: info.filter(|_| local).map(|_| name),
        restart,
    }
}

/// Probe (read-only) how this project gets its local database.
pub(crate) fn plan_local_database(
    cwd: &Path,
    ignore_env: bool,
) -> Result<DatabaseAction, InitError> {
    let name = container_name(cwd);

    // `ignore_env` is the user's explicit choice of a fresh source superseding
    // whatever .env carries.
    if let Some(url) = read_env_key(cwd, ".env", "REDIS_URL").filter(|_| !ignore_env) {
        if url == PLACEHOLDER_URL {
            return Ok(DatabaseAction::Placeholder);
        }
        // Second run typically lands here: revive our container if it is just stopped.
        let info = container_info(&name);
        return Ok(decide_existing_env(url, name, info));
    }

    if !docker_ok() {
        return Err(InitError::DockerUnavailable);
    }

    if let Some(info) = container_info(&name) {
        let url = format!("redis://localhost:{}", info.port);
        return Ok(if info.running {
            DatabaseAction::AlreadyRunning {
                name,
                port: info.port,
                url,
            }
        } else {
            DatabaseAction::StartExisting { name, url }
        });
    }

    let port = free_port(6379).ok_or(InitError::NoFreePort)?;
    let (image, image_local) = resolve_image();
    Ok(DatabaseAction::RunNew {
        url: format!("redis://localhost:{port}"),
        name,
        image,
        image_local,
        port,
    })
}

/// Execute the decided action. Returns the change to report, if the action has one.
pub(crate) async fn apply_database(
    action: &DatabaseAction,
    on_event: &mut dyn FnMut(Event),
) -> Result<Option<Change>, InitError> {
    match action {
        DatabaseAction::Provided { .. } => Ok(None),
        DatabaseAction::Placeholder => Ok(Some(placeholder_change())),
        DatabaseAction::ExistingEnv {
            url,
            container,
            restart,
        } => {
            let (true, Some(name)) = (*restart, container.as_deref()) else {
                return Ok(None);
            };
            // A failed start must not read as updated; validation reports the truth.
            if sh("docker", &["start", name]).status != 0 {
                return Ok(None);
            }
            wait_for_ping(url, Duration::from_secs(30)).await?;
            Ok(Some(Change::new(
                format!("docker:{name}"),
                Status::Updated,
                "restarted stopped container",
            )))
        }
        DatabaseAction::AlreadyRunning { name, port, .. } => Ok(Some(Change::new(
            format!("docker:{name}"),
            Status::Unchanged,
            format!("already running on port {port}"),
        ))),
        DatabaseAction::StartExisting { name, url } => {
            let r = sh("docker", &["start", name]);
            if r.status != 0 {
                return Err(InitError::DockerCommand {
                    command: format!("docker start {name}"),
                    stderr: r.stderr.trim().to_string(),
                });
            }
            wait_for_ping(url, Duration::from_secs(30)).await?;
            Ok(Some(Change::new(
                format!("docker:{name}"),
                Status::Updated,
                "restarted stopped container",
            )))
        }
        DatabaseAction::RunNew {
            name,
            image,
            image_local,
            port,
            url,
        } => {
            if !image_local {
                on_event(Event::Note(format!(
                    "  pulling {image} (first run - may take a minute)..."
                )));
            }
            on_event(Event::ProgressStart(format!(
                "starting {image} as {name} on port {port}"
            )));
            let r = sh(
                "docker",
                &[
                    "run",
                    "-d",
                    "--name",
                    name,
                    "-p",
                    // Loopback only: the image runs without authentication, and a
                    // wildcard bind would expose a writable Redis to the local network.
                    &format!("127.0.0.1:{port}:6379"),
                    image,
                ],
            );
            if r.status != 0 {
                on_event(Event::ProgressDone(String::new()));
                return Err(InitError::DockerCommand {
                    command: "docker run".to_string(),
                    stderr: r.stderr.trim().to_string(),
                });
            }
            if let Err(e) = wait_for_ping(url, Duration::from_secs(30)).await {
                on_event(Event::ProgressDone(String::new()));
                return Err(e);
            }
            on_event(Event::ProgressDone(" ready".to_string()));
            Ok(Some(Change::new(
                format!("docker:{name}"),
                Status::Created,
                format!("{image} on port {port}"),
            )))
        }
    }
}

/// Connect with a deadline; errors come back as one-line strings for the caller's
/// failure message.
async fn connect(
    url: &str,
    timeout: Duration,
) -> Result<redis::aio::MultiplexedConnection, String> {
    let client = redis::Client::open(url).map_err(|e| e.to_string())?;
    match tokio::time::timeout(timeout, client.get_multiplexed_async_connection()).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!(
            "timed out connecting to {}",
            crate::util::mask_url(url)
        )),
    }
}

async fn ping(url: &str, timeout: Duration) -> Result<(), String> {
    let mut conn = connect(url, timeout).await?;
    let pong: String = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .map_err(|e| e.to_string())?;
    if pong == "PONG" {
        Ok(())
    } else {
        Err(format!("PING returned {pong:?}"))
    }
}

async fn wait_for_ping(url: &str, timeout: Duration) -> Result<(), InitError> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let error = match ping(url, Duration::from_millis(1500)).await {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        if tokio::time::Instant::now() > deadline {
            return Err(InitError::NotReady {
                url: url.to_string(),
                error,
            });
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

/// The default endpoint a native local Redis listens on.
pub const LOCAL_REDIS_URL: &str = "redis://localhost:6379";

/// What answers (or does not) at a local Redis endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalRedis {
    Ready,
    NeedsAuth,
    NotFound,
}

/// A quick look before the wizard offers to reuse a running server: PONG means
/// usable as-is, an auth error still counts as running, anything else (refused,
/// timeout, non-Redis chatter) reads as absent.
pub async fn probe_local_redis(url: &str) -> LocalRedis {
    const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
    let Ok(client) = redis::Client::open(url) else {
        return LocalRedis::NotFound;
    };
    let mut conn = match tokio::time::timeout(
        PROBE_TIMEOUT,
        client.get_multiplexed_async_connection(),
    )
    .await
    {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) if is_auth_error(&e) => return LocalRedis::NeedsAuth,
        _ => return LocalRedis::NotFound,
    };
    match tokio::time::timeout(
        PROBE_TIMEOUT,
        redis::cmd("PING").query_async::<String>(&mut conn),
    )
    .await
    {
        Ok(Ok(_)) => LocalRedis::Ready,
        Ok(Err(e)) if is_auth_error(&e) => LocalRedis::NeedsAuth,
        _ => LocalRedis::NotFound,
    }
}

fn is_auth_error(e: &redis::RedisError) -> bool {
    e.kind() == redis::ErrorKind::AuthenticationFailed
        || matches!(e.code(), Some("NOAUTH") | Some("WRONGPASS"))
}

/// Prove the database actually works: a PING and a SET/GET round trip on a
/// short-lived key.
pub async fn validate(url: &str) -> Result<(), String> {
    let mut conn = connect(url, Duration::from_secs(3)).await?;
    let pong: String = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .map_err(|e| e.to_string())?;
    if pong != "PONG" {
        return Err(format!("PING returned {pong:?}"));
    }
    let key = "redis-init:selfcheck";
    let value = format!("ok {}", chrono::Utc::now().to_rfc3339());
    redis::cmd("SET")
        .arg(key)
        .arg(&value)
        .arg("EX")
        .arg(60)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| e.to_string())?;
    let got: Option<String> = redis::cmd("GET")
        .arg(key)
        .query_async(&mut conn)
        .await
        .map_err(|e| e.to_string())?;
    if got.as_deref() != Some(value.as_str()) {
        return Err(format!("SET/GET round trip failed (got {got:?})"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake Redis that answers every incoming command with `reply` - the client
    /// pipelines CLIENT SETINFO during setup before the PING the probe cares about,
    /// so one reply per `*`-led command keeps the protocol in sync.
    fn fake_redis(reply: &'static [u8]) -> String {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!(
            "redis://127.0.0.1:{}",
            listener.local_addr().unwrap().port()
        );
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                use std::io::{Read, Write};
                let _ = sock.set_read_timeout(Some(Duration::from_secs(3)));
                let mut buf = [0u8; 4096];
                while let Ok(n) = sock.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    let commands = buf[..n].iter().filter(|b| **b == b'*').count().max(1);
                    for _ in 0..commands {
                        let _ = sock.write_all(reply);
                    }
                    let _ = sock.flush();
                }
            }
        });
        url
    }

    #[tokio::test]
    async fn probe_finds_a_ready_local_redis() {
        let url = fake_redis(b"+PONG\r\n");
        assert_eq!(probe_local_redis(&url).await, LocalRedis::Ready);
    }

    #[tokio::test]
    async fn probe_reads_an_auth_error_as_a_running_server() {
        let url = fake_redis(b"-NOAUTH Authentication required.\r\n");
        assert_eq!(probe_local_redis(&url).await, LocalRedis::NeedsAuth);
    }

    #[tokio::test]
    async fn probe_reports_nothing_listening() {
        // A bound-then-dropped listener guarantees a refused port.
        let port = std::net::TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let url = format!("redis://127.0.0.1:{port}");
        assert_eq!(probe_local_redis(&url).await, LocalRedis::NotFound);
    }

    #[test]
    fn a_placeholder_env_value_plans_a_pending_database() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            format!("REDIS_URL=\"{PLACEHOLDER_URL}\"\n"),
        )
        .unwrap();
        // Returns before any docker probe, so this stays hermetic.
        let action = plan_local_database(dir.path(), false).unwrap();
        assert!(matches!(action, DatabaseAction::Placeholder));
        assert_eq!(action.source(true), "placeholder - fill .env");
    }

    #[test]
    fn parse_container_info_reads_running_and_port() {
        let info = parse_container_info("true 6380\n").unwrap();
        assert!(info.running);
        assert_eq!(info.port, 6380);
    }

    #[test]
    fn parse_container_info_rejects_garbage() {
        assert!(parse_container_info("").is_none());
        assert!(parse_container_info("true").is_none());
        assert!(parse_container_info("true notaport").is_none());
    }

    #[test]
    fn container_name_slugs_the_directory() {
        assert_eq!(
            container_name(Path::new("/tmp/My Demo App")),
            "redis-init-my-demo-app"
        );
    }

    #[test]
    fn existing_env_restart_is_previewed() {
        let action = DatabaseAction::ExistingEnv {
            url: "redis://localhost:6379".into(),
            container: Some("redis-init-x".into()),
            restart: true,
        };
        let change = action.preview().unwrap();
        assert_eq!(change.status, Status::Planned);
        assert!(change.note.contains("would start"), "{}", change.note);

        let no_restart = DatabaseAction::ExistingEnv {
            url: "redis://h:1".into(),
            container: Some("redis-init-x".into()),
            restart: false,
        };
        assert!(no_restart.preview().is_none());
    }

    #[test]
    fn leftover_container_only_counts_for_a_local_url() {
        let stopped = || {
            Some(ContainerInfo {
                running: false,
                port: 6379,
            })
        };

        let remote = decide_existing_env(
            "rediss://default:s3cret@cloud.example:12000".into(),
            "redis-init-x".into(),
            stopped(),
        );
        assert_eq!(remote.container(), None);
        assert!(matches!(
            remote,
            DatabaseAction::ExistingEnv { restart: false, .. }
        ));

        let local = decide_existing_env(
            "redis://localhost:6379".into(),
            "redis-init-x".into(),
            stopped(),
        );
        assert_eq!(local.container(), Some("redis-init-x"));
        assert!(matches!(
            local,
            DatabaseAction::ExistingEnv { restart: true, .. }
        ));
    }

    #[test]
    fn leftover_container_is_ignored_when_env_points_elsewhere() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".env"),
            "REDIS_URL=\"rediss://default:x@cloud.example:12000\"\n",
        )
        .unwrap();
        // Whatever containers exist on this machine, a remote URL means none of
        // them belong to this database.
        let action = plan_local_database(dir.path(), false).unwrap();
        assert_eq!(action.container(), None);
        assert!(matches!(
            action,
            DatabaseAction::ExistingEnv { restart: false, .. }
        ));
    }

    #[test]
    fn new_container_preview_binds_loopback() {
        let action = DatabaseAction::RunNew {
            name: "redis-init-x".into(),
            image: "redis:8-alpine".into(),
            image_local: true,
            port: 6379,
            url: "redis://localhost:6379".into(),
        };
        let change = action.preview().unwrap();
        assert!(
            change.note.contains("-p 127.0.0.1:6379:6379"),
            "{}",
            change.note
        );
    }

    #[test]
    fn free_port_sees_a_dual_stack_ipv6_holder() {
        // Docker Desktop publishes ports on a dual-stack [::] listener; an
        // IPv4-only probe reads such a port as free and docker run then fails
        // with "port is already allocated".
        let listener = std::net::TcpListener::bind(("::", 0)).unwrap();
        let taken = listener.local_addr().unwrap().port();
        let free = free_port(taken).unwrap();
        assert!(free > taken, "picked the ipv6-held port {taken}");
    }

    #[test]
    fn free_port_finds_an_unused_port() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let taken = listener.local_addr().unwrap().port();
        let free = free_port(taken).unwrap();
        assert!(free > taken);
    }

    #[test]
    fn existing_env_url_wins_without_docker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "REDIS_URL=\"redis://h:1\"\n").unwrap();
        let action = plan_local_database(dir.path(), false).unwrap();
        assert_eq!(action.url(), "redis://h:1");
        assert_eq!(action.source(true), "existing .env");
        // No local container matches, so nothing restarts.
        assert!(matches!(
            action,
            DatabaseAction::ExistingEnv { restart: false, .. }
        ));
    }
}
