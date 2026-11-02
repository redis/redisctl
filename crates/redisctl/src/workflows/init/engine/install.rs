//! Client SDK and redis-cli installation: what to install is decided read-only at
//! plan time; the package-manager commands run at apply time. An install failure is
//! reported as skipped, never as a run failure - the onboarding still stands.

use std::io::Write;
use std::path::Path;

use crate::workflows::init::engine::change::{Change, Status};
use crate::workflows::init::engine::env::{FileAction, read_for_planning};
use crate::workflows::init::engine::pins::{
    REDIS_CLI_INSTALLER as CLI_INSTALLER, REDIS_CLI_VERSION,
};
use crate::workflows::init::engine::project::{Project, Runtime};
use crate::workflows::init::engine::util::{
    ending_with_newline, exists, first_error_line, has_bin, read_if, sh, sh_env, sh_in,
};
use crate::workflows::init::engine::{Event, InitError};

/// Whether init may run the redis-cli installer when redis-cli is missing. It runs a
/// remote script, so only a person at a terminal or an explicit flag says yes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliInstall {
    /// `--no-install-cli`.
    Never,
    /// No flag and nothing can prompt (piped, CI, `--defaults`): name the command.
    Suggest,
    /// No flag at a terminal: plan the install and ask before it runs.
    Ask,
    /// `--install-cli`.
    Always,
}

impl CliInstall {
    pub fn from_flags(no_install: bool, install: bool, can_prompt: bool) -> Self {
        match (no_install, install, can_prompt) {
            (true, _, _) => CliInstall::Never,
            (false, true, _) => CliInstall::Always,
            (false, false, true) => CliInstall::Ask,
            (false, false, false) => CliInstall::Suggest,
        }
    }
}

/// The version the installer is told to fetch: the user's own `REDIS_CLI_VERSION`
/// when it looks like a version (or `stable`/`latest`), the pin otherwise. It is
/// printed in a copy-paste command, so nothing shell-special gets through.
pub fn cli_version() -> String {
    pick_cli_version(std::env::var("REDIS_CLI_VERSION").ok())
}

fn pick_cli_version(user: Option<String>) -> String {
    user.map(|v| v.trim().to_string())
        .filter(|v| {
            !v.is_empty()
                && v.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        })
        .unwrap_or_else(|| REDIS_CLI_VERSION.to_string())
}

/// The same install as a command the user can run.
fn manual_cli_install() -> String {
    format!(
        "curl --proto '=https' --tlsv1.2 -fsSL {CLI_INSTALLER} | REDIS_CLI_VERSION={} sh",
        cli_version()
    )
}

/// A declined prompt is a skip, with the command for later.
pub(crate) fn declined_cli_install() -> InstallAction {
    InstallAction::Report(Change::new(
        "redis-cli",
        Status::Skipped,
        format!("not installed; to install later: {}", manual_cli_install()),
    ))
}

/// One decided install, fixed at plan time.
#[derive(Debug)]
pub(crate) enum InstallAction {
    /// Nothing to run: the decision (unchanged/kept/skipped) is the report.
    Report(Change),
    /// Run a package-manager command that installs a package.
    Command {
        cmd: String,
        args: Vec<String>,
        file: &'static str,
        note: String,
        /// The ledger subject when the command itself is the story (planned, failed).
        label: String,
    },
    /// Append to a requirements-style manifest.
    Append { package: &'static str, note: String },
    /// Fetch redis-cli via the official installer (statically linked, no-sudo
    /// ~/.local/bin fallback). The script checks the binary against a SHA-256 file
    /// from the same server, and skips the check when that file is missing.
    InstallCli {
        /// Ask at the terminal before running it (`CliInstall::Ask`).
        ask: bool,
    },
}

impl InstallAction {
    pub(crate) fn preview(&self) -> Change {
        match self {
            InstallAction::Report(change) => change.clone(),
            InstallAction::Command {
                cmd, args, label, ..
            } => Change::new(
                label.clone(),
                Status::Planned,
                format!("would run: {cmd} {}", args.join(" ")),
            ),
            InstallAction::Append { note, .. } => {
                Change::new("requirements.txt", Status::Updated, note.clone())
            }
            InstallAction::InstallCli { ask } => Change::new(
                "redis-cli",
                Status::Planned,
                format!(
                    "would {}install redis-cli {} via {CLI_INSTALLER}",
                    if *ask { "ask, then " } else { "" },
                    cli_version()
                ),
            ),
        }
    }

    pub(crate) fn perform(
        &self,
        cwd: &Path,
        on_event: &mut dyn FnMut(Event),
    ) -> Result<Change, InitError> {
        match self {
            InstallAction::Report(change) => Ok(change.clone()),
            InstallAction::Append { package, note } => {
                let content = read_for_planning(cwd, "requirements.txt")?.unwrap_or_default();
                if content.lines().any(|line| line.trim() == *package) {
                    return Ok(Change::new("requirements.txt", Status::Unchanged, ""));
                }
                FileAction::Write {
                    rel: "requirements.txt".to_string(),
                    content: format!("{}{package}\n", ending_with_newline(&content)),
                    status: Status::Updated,
                    note: note.clone(),
                }
                .perform(cwd)
            }
            InstallAction::Command {
                cmd,
                args,
                file,
                note,
                label,
            } => {
                let shown = format!("{cmd} {}", args.join(" "));
                on_event(Event::ProgressStart(format!(
                    "installing {label} ({shown})"
                )));
                let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
                let r = sh_in(cwd, cmd, &arg_refs);
                if r.status != 0 {
                    on_event(Event::ProgressDone(" failed".to_string()));
                    return Ok(Change::new(
                        label.clone(),
                        Status::Skipped,
                        format!("{shown} failed - install manually"),
                    ));
                }
                on_event(Event::ProgressDone(" done".to_string()));
                Ok(Change::new(*file, Status::Updated, note.clone()))
            }
            InstallAction::InstallCli { .. } => {
                on_event(Event::ProgressStart(format!(
                    "installing redis-cli {} (official installer, packages.redis.io)",
                    cli_version()
                )));
                let change = install_cli();
                on_event(Event::ProgressDone(
                    if change.status == Status::Created {
                        " done"
                    } else {
                        " failed"
                    }
                    .to_string(),
                ));
                Ok(change)
            }
        }
    }
}

/// Download, run, then look: the line says installed only when a redis-cli is there.
fn install_cli() -> Change {
    let skipped = |why: String| {
        Change::new(
            "redis-cli",
            Status::Skipped,
            format!("{why} - the project skill carries docker fallbacks"),
        )
    };
    // packages.redis.io also answers on plain http: refuse anything but https, on
    // redirects too.
    let download = sh(
        "curl",
        &["--proto", "=https", "--tlsv1.2", "-fsSL", CLI_INSTALLER],
    );
    if download.status != 0 {
        return skipped(format!(
            "could not download {CLI_INSTALLER}: {}",
            first_error_line(&download.stderr, &download.stdout, download.status)
        ));
    }
    if download.stdout.trim().is_empty() {
        return skipped(format!("{CLI_INSTALLER} served an empty script"));
    }
    let script =
        std::env::temp_dir().join(format!("redisctl-redis-cli-{}.sh", uuid::Uuid::new_v4()));
    let saved = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&script)
        .and_then(|mut file| file.write_all(download.stdout.as_bytes()));
    if let Err(e) = saved {
        return skipped(format!("could not save the installer: {e}"));
    }
    // An inherited REDIS_CLI_BASE_URL would point the script's own downloads away
    // from the packages.redis.io the prompt named. Only init's fetch of the script
    // is https-only; the script's downloads and its same-origin checksum are its own.
    let run = sh_env(
        "sh",
        &[&script.to_string_lossy()],
        &[("REDIS_CLI_VERSION", &cli_version())],
        &["REDIS_CLI_BASE_URL"],
    );
    let _ = std::fs::remove_file(&script);
    if run.status != 0 {
        return skipped(format!(
            "the installer failed: {}",
            first_error_line(&run.stderr, &run.stdout, run.status)
        ));
    }
    if has_bin("redis-cli") {
        Change::new(
            "redis-cli",
            Status::Created,
            "installed via the official packages.redis.io script",
        )
    } else if local_bin_cli() {
        Change::new(
            "redis-cli",
            Status::Created,
            "installed to ~/.local/bin - add it to PATH to use it",
        )
    } else {
        skipped("the installer left no runnable redis-cli on PATH or in ~/.local/bin".to_string())
    }
}

/// The official installer's no-sudo fallback, often not on PATH.
fn local_bin_cli() -> bool {
    std::env::home_dir().is_some_and(|home| which::which(home.join(".local/bin/redis-cli")).is_ok())
}

pub(crate) fn plan_client_install(cwd: &Path, project: &Project) -> InstallAction {
    decide_client(cwd, project, &has_bin)
}

/// Plan one install command, whichever package manager owns the manifest. The
/// missing-binary check happens here so a dry run reports the same skip a real
/// run would.
fn command(
    has: &dyn Fn(&str) -> bool,
    cmd: &str,
    args: &[&str],
    file: &'static str,
    note: String,
    label: &str,
) -> InstallAction {
    if !has(cmd) {
        return InstallAction::Report(Change::new(
            label,
            Status::Skipped,
            format!("{cmd} not found - install {label} manually"),
        ));
    }
    InstallAction::Command {
        cmd: cmd.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        file,
        note,
        label: label.to_string(),
    }
}

fn decide_client(cwd: &Path, project: &Project, has: &dyn Fn(&str) -> bool) -> InstallAction {
    let unchanged =
        |note: &str| InstallAction::Report(Change::new("client package", Status::Unchanged, note));
    let skipped =
        |note: &str| InstallAction::Report(Change::new("client package", Status::Skipped, note));

    match project.runtime {
        Runtime::Node => {
            let Ok(pkg) = serde_json::from_str::<serde_json::Value>(
                &read_if(cwd, "package.json").unwrap_or_default(),
            ) else {
                return InstallAction::Report(Change::new(
                    "package.json",
                    Status::Kept,
                    "unparseable; skipping client install",
                ));
            };
            if !pkg["dependencies"]["redis"].is_null() || !pkg["devDependencies"]["redis"].is_null()
            {
                return InstallAction::Report(Change::new(
                    "package.json",
                    Status::Unchanged,
                    "redis client already a dependency",
                ));
            }
            let pm = project.pm.unwrap_or("npm");
            let args: &[&str] = if pm == "npm" {
                &["install", "redis"]
            } else {
                &["add", "redis"]
            };
            command(
                has,
                pm,
                args,
                "package.json",
                format!("redis client installed via {pm}"),
                "client package",
            )
        }
        Runtime::Python => {
            let requirements = read_if(cwd, "requirements.txt");
            let pyproject = read_if(cwd, "pyproject.toml").unwrap_or_default();
            let has_dep = regex::Regex::new(r"(?m)^\s*redis([\[\s=<>~!]|$)")
                .expect("static regex")
                .is_match(requirements.as_deref().unwrap_or(""))
                || regex::Regex::new(r#""redis([\[">=<~]|")"#)
                    .expect("static regex")
                    .is_match(&pyproject)
                || poetry_declares(&pyproject, "redis");
            if has_dep {
                return unchanged("redis client already a dependency");
            }
            if let Some(pm) = python_pm(cwd, has) {
                return command(
                    has,
                    pm,
                    &["add", "redis"],
                    "pyproject.toml",
                    format!("redis-py installed via {pm}"),
                    "client package",
                );
            }
            if requirements.is_some() {
                return InstallAction::Append {
                    package: "redis",
                    note: "redis-py added - run pip install -r requirements.txt".to_string(),
                };
            }
            skipped("add redis-py manually (pip install redis)")
        }
        Runtime::Go => {
            if read_if(cwd, "go.mod")
                .unwrap_or_default()
                .contains("github.com/redis/go-redis")
            {
                return unchanged("go-redis already a dependency");
            }
            command(
                has,
                "go",
                &["get", "github.com/redis/go-redis/v9"],
                "go.mod",
                "go-redis installed".to_string(),
                "client package",
            )
        }
        Runtime::Rust => {
            if regex::Regex::new(r"(?m)^\s*redis\s*=")
                .expect("static regex")
                .is_match(&read_if(cwd, "Cargo.toml").unwrap_or_default())
            {
                return unchanged("redis crate already a dependency");
            }
            command(
                has,
                "cargo",
                &["add", "redis"],
                "Cargo.toml",
                "redis crate installed".to_string(),
                "client package",
            )
        }
        Runtime::Java => {
            // Maven/Gradle have no standard add-dependency command, and regex surgery
            // on pom.xml is clobber-prone. The generated skill carries the exact
            // snippet; the user's agent applies it to the build file.
            let build = format!(
                "{}{}{}",
                read_if(cwd, "pom.xml").unwrap_or_default(),
                read_if(cwd, "build.gradle").unwrap_or_default(),
                read_if(cwd, "build.gradle.kts").unwrap_or_default()
            );
            if build.contains("redis.clients") || build.contains("io.lettuce") {
                return unchanged("a Redis client is already a dependency");
            }
            skipped(
                "add Jedis (redis.clients:jedis) - exact snippet is in the redis-project-setup skill",
            )
        }
        Runtime::Unknown => {
            skipped("no package manifest detected - install the Redis client for your language")
        }
    }
}

/// Who installs into a Python project. A lockfile names its manager, then a
/// `[tool.poetry]` table does, and only a bare `pyproject.toml` goes to uv on PATH:
/// uv being installed says nothing about how this project is managed.
fn python_pm(cwd: &Path, has: &dyn Fn(&str) -> bool) -> Option<&'static str> {
    let pyproject = exists(cwd, "pyproject.toml");
    let poetry_table = regex::Regex::new(r"(?m)^\s*\[\s*tool\.poetry\s*[\].]")
        .expect("static regex")
        .is_match(&read_if(cwd, "pyproject.toml").unwrap_or_default());
    if pyproject && exists(cwd, "uv.lock") {
        Some("uv")
    } else if exists(cwd, "poetry.lock") || poetry_table {
        Some("poetry")
    } else if pyproject && has("uv") {
        Some("uv")
    } else {
        None
    }
}

/// Whether a Poetry dependency table in `pyproject` has `pkg` as a key
/// (`redis = "^5.0"`). Names compare normalized, as pip compares them.
fn poetry_declares(pyproject: &str, pkg: &str) -> bool {
    let normalize = |name: &str| name.to_ascii_lowercase().replace(['_', '.'], "-");
    let table = regex::Regex::new(
        r"^\[\s*tool\.poetry\.(dependencies|dev-dependencies|group\.[^\]]+\.dependencies)\s*\]",
    )
    .expect("static regex");
    let key = regex::Regex::new(r#"^["']?([A-Za-z0-9._-]+)["']?\s*="#).expect("static regex");
    let mut in_dependencies = false;
    pyproject.lines().map(str::trim).any(|line| {
        if line.starts_with('[') {
            in_dependencies = table.is_match(line);
            return false;
        }
        in_dependencies
            && key
                .captures(line)
                .is_some_and(|captures| normalize(&captures[1]) == normalize(pkg))
    })
}

/// The product SDK per runtime; everything else talks REST (or MCP) directly.
pub(crate) fn product_package(
    key: crate::workflows::init::engine::ProductKey,
    runtime: Runtime,
) -> Option<&'static str> {
    match (key, runtime) {
        (crate::workflows::init::engine::ProductKey::AgentMemory, Runtime::Node) => {
            Some("@redis-iris/agent-memory")
        }
        (crate::workflows::init::engine::ProductKey::LangCache, Runtime::Node) => {
            Some("@redis-ai/langcache")
        }
        (crate::workflows::init::engine::ProductKey::LangCache, Runtime::Python) => {
            Some("langcache")
        }
        (crate::workflows::init::engine::ProductKey::ContextRetriever, Runtime::Python) => {
            Some("redis-context-retriever")
        }
        _ => None,
    }
}

pub(crate) fn plan_product_install(
    cwd: &Path,
    project: &Project,
    product: &crate::workflows::init::engine::products::WiredProduct,
) -> InstallAction {
    decide_product(cwd, project, product, &has_bin)
}

fn decide_product(
    cwd: &Path,
    project: &Project,
    product: &crate::workflows::init::engine::products::WiredProduct,
    has: &dyn Fn(&str) -> bool,
) -> InstallAction {
    let label = format!("{} SDK", product.label());
    let no_sdk = |note: String| InstallAction::Report(Change::new(&label, Status::Skipped, note));
    let pkg = match product_package(product.spec.key, project.runtime) {
        Some(pkg) => pkg,
        None if product.spec.mcp => {
            return no_sdk("no SDK needed - the coding agent uses its MCP tools".to_string());
        }
        None => {
            return no_sdk(format!(
                "no {} SDK - call the REST API at {}",
                project.runtime.as_str(),
                product.url()
            ));
        }
    };
    match project.runtime {
        Runtime::Node => {
            let Ok(manifest) = serde_json::from_str::<serde_json::Value>(
                &read_if(cwd, "package.json").unwrap_or_default(),
            ) else {
                return InstallAction::Report(Change::new(
                    &label,
                    Status::Kept,
                    "package.json is unparseable; install it yourself",
                ));
            };
            if !manifest["dependencies"][pkg].is_null()
                || !manifest["devDependencies"][pkg].is_null()
            {
                return InstallAction::Report(Change::new(
                    &label,
                    Status::Unchanged,
                    format!("{pkg} already a dependency"),
                ));
            }
            let pm = project.pm.unwrap_or("npm");
            let args: &[&str] = if pm == "npm" {
                &["install", pkg]
            } else {
                &["add", pkg]
            };
            command(
                has,
                pm,
                args,
                "package.json",
                format!("{pkg} installed via {pm}"),
                &label,
            )
        }
        Runtime::Python => {
            let requirements = read_if(cwd, "requirements.txt");
            let pyproject = read_if(cwd, "pyproject.toml").unwrap_or_default();
            let has_dep =
                regex::Regex::new(&format!(r"(?m)^\s*{}([\[\s=<>~!]|$)", regex::escape(pkg)))
                    .expect("escaped package regex")
                    .is_match(requirements.as_deref().unwrap_or(""))
                    || pyproject.contains(&format!("\"{pkg}"))
                    || poetry_declares(&pyproject, pkg);
            if has_dep {
                return InstallAction::Report(Change::new(
                    &label,
                    Status::Unchanged,
                    format!("{pkg} already a dependency"),
                ));
            }
            if let Some(pm) = python_pm(cwd, has) {
                return command(
                    has,
                    pm,
                    &["add", pkg],
                    "pyproject.toml",
                    format!("{pkg} installed via {pm}"),
                    &label,
                );
            }
            if requirements.is_some() {
                return InstallAction::Append {
                    package: pkg,
                    note: format!("{pkg} added - run pip install -r requirements.txt"),
                };
            }
            no_sdk(format!(
                "install {pkg} for your runtime, or call the REST API directly"
            ))
        }
        // product_package answers None for every other runtime, which the no-SDK
        // arms above already turned into a report.
        _ => no_sdk(format!(
            "install {pkg} for your runtime, or call the REST API directly"
        )),
    }
}

pub(crate) fn plan_redis_cli(mode: CliInstall) -> InstallAction {
    decide_redis_cli(mode, &has_bin, local_bin_cli())
}

fn decide_redis_cli(
    mode: CliInstall,
    has: &dyn Fn(&str) -> bool,
    local_bin: bool,
) -> InstallAction {
    // The official installer falls back to ~/.local/bin, which is often not on
    // PATH: without this check every run would run the installer again.
    if local_bin && !has("redis-cli") {
        return InstallAction::Report(Change::new(
            "redis-cli",
            Status::Unchanged,
            "already installed to ~/.local/bin - add it to PATH to use it",
        ));
    }
    if has("redis-cli") {
        let version = sh("redis-cli", &["--version"])
            .stdout
            .split_whitespace()
            .find_map(|word| {
                word.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
                    .then(|| word.to_string())
            });
        return InstallAction::Report(Change::new(
            "redis-cli",
            Status::Unchanged,
            match version {
                Some(v) => format!("already on PATH ({v})"),
                None => "already on PATH".to_string(),
            },
        ));
    }
    if mode == CliInstall::Never {
        return InstallAction::Report(Change::new(
            "redis-cli",
            Status::Skipped,
            "--no-install-cli; the project skill carries docker fallbacks",
        ));
    }
    // Checked before suggesting or asking: neither should offer a command that
    // cannot run here (native Windows has no sh).
    for tool in ["curl", "sh"] {
        if !has(tool) {
            return InstallAction::Report(Change::new(
                "redis-cli",
                Status::Skipped,
                format!("{tool} not found - the project skill carries docker fallbacks"),
            ));
        }
    }
    if mode == CliInstall::Suggest {
        return InstallAction::Report(Change::new(
            "redis-cli",
            Status::Skipped,
            format!(
                "not installed: nothing to ask (no terminal, or --defaults); pass --install-cli, or run: {}",
                manual_cli_install()
            ),
        ));
    }
    InstallAction::InstallCli {
        ask: mode == CliInstall::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::init::engine::project::detect;

    fn project_in(dir: &Path) -> Project {
        detect(dir)
    }

    fn dir_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in files {
            std::fs::write(dir.path().join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn node_with_redis_already_present_is_unchanged() {
        let dir = dir_with(&[("package.json", r#"{"dependencies":{"redis":"^4"}}"#)]);
        let action = decide_client(dir.path(), &project_in(dir.path()), &|_| true);
        let change = action.preview();
        assert_eq!(change.status, Status::Unchanged);
        assert_eq!(change.subject, "package.json");
    }

    #[test]
    fn node_unparseable_manifest_is_kept() {
        let dir = dir_with(&[("package.json", "not json")]);
        let change = decide_client(dir.path(), &project_in(dir.path()), &|_| true).preview();
        assert_eq!(change.status, Status::Kept);
        assert!(change.note.contains("unparseable"), "{}", change.note);
    }

    #[test]
    fn node_uses_the_detected_package_manager() {
        let dir = dir_with(&[("package.json", r#"{"name":"x"}"#), ("pnpm-lock.yaml", "")]);
        let action = decide_client(dir.path(), &project_in(dir.path()), &|_| true);
        match action {
            InstallAction::Command { cmd, args, .. } => {
                assert_eq!(cmd, "pnpm");
                assert_eq!(args, vec!["add", "redis"]);
            }
            other => panic!("expected a command, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_package_manager_reads_as_skipped() {
        let dir = dir_with(&[("package.json", r#"{"name":"x"}"#)]);
        let change = decide_client(dir.path(), &project_in(dir.path()), &|_| false).preview();
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.contains("npm not found"), "{}", change.note);
    }

    #[test]
    fn python_requirements_get_redis_appended() {
        let dir = dir_with(&[("requirements.txt", "flask\n")]);
        let action = decide_client(dir.path(), &project_in(dir.path()), &|_| false);
        action.perform(dir.path(), &mut |_| {}).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("requirements.txt")).unwrap(),
            "flask\nredis\n"
        );
    }

    #[test]
    fn python_existing_requirement_is_unchanged() {
        let dir = dir_with(&[("requirements.txt", "redis>=5\n")]);
        let change = decide_client(dir.path(), &project_in(dir.path()), &|_| true).preview();
        assert_eq!(change.status, Status::Unchanged);
    }

    #[test]
    fn python_pyproject_with_uv_uses_uv_add() {
        let dir = dir_with(&[("pyproject.toml", "[project]\nname = \"x\"\n")]);
        let action = decide_client(dir.path(), &project_in(dir.path()), &|bin| bin == "uv");
        match action {
            InstallAction::Command { cmd, .. } => assert_eq!(cmd, "uv"),
            other => panic!("expected a command, got {other:?}"),
        }
    }

    fn planned_cmd(action: InstallAction) -> String {
        match action {
            InstallAction::Command { cmd, .. } => cmd,
            other => panic!("expected a command, got {other:?}"),
        }
    }

    fn langcache() -> crate::workflows::init::engine::products::WiredProduct {
        crate::workflows::init::engine::products::WiredProduct {
            spec: crate::workflows::init::engine::products::SPECS
                .iter()
                .find(|spec| spec.key == crate::workflows::init::engine::ProductKey::LangCache)
                .unwrap(),
            url: "https://l".to_string(),
            id: Some("c1".to_string()),
            key: "k".to_string(),
            ready: true,
        }
    }

    #[test]
    fn python_picks_the_manager_the_project_uses_over_uv_on_path() {
        let everything = |_: &str| true;
        let pyproject = ("pyproject.toml", "[project]\nname = \"x\"\n");
        for (files, pm) in [
            (vec![pyproject, ("poetry.lock", "")], "poetry"),
            (
                vec![("pyproject.toml", "[tool.poetry]\nname = \"x\"\n")],
                "poetry",
            ),
            (
                vec![(
                    "pyproject.toml",
                    "[tool.poetry.dependencies]\npython = \"^3.11\"\n",
                )],
                "poetry",
            ),
            (vec![pyproject, ("uv.lock", ""), ("poetry.lock", "")], "uv"),
            (vec![pyproject], "uv"),
            // A plugin's table is not Poetry's own.
            (
                vec![(
                    "pyproject.toml",
                    "[tool.poetry-dynamic-versioning]\nenable = true\n",
                )],
                "uv",
            ),
        ] {
            let dir = dir_with(&files);
            let project = project_in(dir.path());
            assert_eq!(
                planned_cmd(decide_client(dir.path(), &project, &everything)),
                pm,
                "{files:?}"
            );
            assert_eq!(
                planned_cmd(decide_product(
                    dir.path(),
                    &project,
                    &langcache(),
                    &everything
                )),
                pm,
                "{files:?}"
            );
        }
    }

    #[test]
    fn python_poetry_dependency_keys_read_as_unchanged() {
        let dir = dir_with(&[
            (
                "pyproject.toml",
                "[tool.poetry]\nname = \"x\"\n\n[tool.poetry.dependencies]\npython = \"^3.11\"\nredis = \"^5.0\"\n\n[tool.poetry.group.dev.dependencies]\nLangCache = { version = \"^0.1\" }\n",
            ),
            ("poetry.lock", ""),
        ]);
        let project = project_in(dir.path());
        let client = decide_client(dir.path(), &project, &|_| true).preview();
        assert_eq!(client.status, Status::Unchanged, "{}", client.note);
        let sdk = decide_product(dir.path(), &project, &langcache(), &|_| true).preview();
        assert_eq!(sdk.status, Status::Unchanged, "{}", sdk.note);

        // The same key outside a dependency table is not a dependency.
        let other = dir_with(&[
            ("pyproject.toml", "[tool.myapp]\nredis = \"redis://h:1\"\n"),
            ("poetry.lock", ""),
        ]);
        assert_eq!(
            planned_cmd(decide_client(
                other.path(),
                &project_in(other.path()),
                &|_| true
            )),
            "poetry"
        );
    }

    #[test]
    fn go_and_rust_get_their_native_add_commands() {
        let go = dir_with(&[("go.mod", "module demo\n")]);
        match decide_client(go.path(), &project_in(go.path()), &|_| true) {
            InstallAction::Command { cmd, args, .. } => {
                assert_eq!(cmd, "go");
                assert_eq!(args, vec!["get", "github.com/redis/go-redis/v9"]);
            }
            other => panic!("expected a command, got {other:?}"),
        }
        let rust = dir_with(&[("Cargo.toml", "[package]\nname = \"x\"\n")]);
        match decide_client(rust.path(), &project_in(rust.path()), &|_| true) {
            InstallAction::Command { cmd, args, .. } => {
                assert_eq!(cmd, "cargo");
                assert_eq!(args, vec!["add", "redis"]);
            }
            other => panic!("expected a command, got {other:?}"),
        }
    }

    #[test]
    fn rust_existing_dependency_is_unchanged() {
        let dir = dir_with(&[("Cargo.toml", "[dependencies]\nredis = \"0.27\"\n")]);
        let change = decide_client(dir.path(), &project_in(dir.path()), &|_| true).preview();
        assert_eq!(change.status, Status::Unchanged);
    }

    #[test]
    fn java_is_never_mutated_only_advised() {
        let dir = dir_with(&[("pom.xml", "<project/>")]);
        let change = decide_client(dir.path(), &project_in(dir.path()), &|_| true).preview();
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.contains("Jedis"), "{}", change.note);

        let with_client = dir_with(&[("pom.xml", "<dep>redis.clients</dep>")]);
        let change = decide_client(with_client.path(), &project_in(with_client.path()), &|_| {
            true
        })
        .preview();
        assert_eq!(change.status, Status::Unchanged);
    }

    #[test]
    fn unknown_runtime_is_skipped_with_a_hint() {
        let dir = tempfile::tempdir().unwrap();
        let change = decide_client(dir.path(), &project_in(dir.path()), &|_| true).preview();
        assert_eq!(change.status, Status::Skipped);
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_install_command_reads_as_skipped_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let action = InstallAction::Command {
            cmd: "false".to_string(),
            args: vec![],
            file: "package.json",
            note: "never used".to_string(),
            label: "client package".to_string(),
        };
        let change = action.perform(dir.path(), &mut |_| {}).unwrap();
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.contains("install manually"), "{}", change.note);
    }

    #[cfg(unix)]
    #[test]
    fn a_successful_install_command_reports_the_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let action = InstallAction::Command {
            cmd: "true".to_string(),
            args: vec![],
            file: "package.json",
            note: "redis client installed via true".to_string(),
            label: "client package".to_string(),
        };
        let change = action.perform(dir.path(), &mut |_| {}).unwrap();
        assert_eq!(change.status, Status::Updated);
        assert_eq!(change.subject, "package.json");
    }

    #[cfg(unix)]
    #[test]
    fn install_commands_run_in_the_plans_directory_not_the_process_cwd() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let action = InstallAction::Command {
            cmd: "touch".to_string(),
            args: vec!["sub/marker".to_string()],
            file: "package.json",
            note: "n".to_string(),
            label: "client package".to_string(),
        };
        // `sub/` exists only inside the plan's cwd: the command succeeds there and
        // nowhere else.
        let change = action.perform(dir.path(), &mut |_| {}).unwrap();
        assert_eq!(change.status, Status::Updated, "{}", change.note);
        assert!(dir.path().join("sub/marker").exists());
    }

    #[test]
    fn local_bin_redis_cli_counts_as_installed() {
        let change = decide_redis_cli(CliInstall::Always, &|_| false, true).preview();
        assert_eq!(change.status, Status::Unchanged);
        assert!(change.note.contains("~/.local/bin"), "{}", change.note);
    }

    #[test]
    fn redis_cli_present_reads_unchanged() {
        let change = decide_redis_cli(CliInstall::Always, &|_| true, false).preview();
        assert_eq!(change.status, Status::Unchanged);
        assert!(change.note.contains("already on PATH"), "{}", change.note);
    }

    #[test]
    fn redis_cli_opt_out_reads_skipped() {
        let change = decide_redis_cli(CliInstall::Never, &|_| false, false).preview();
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.contains("--no-install-cli"), "{}", change.note);
    }

    #[test]
    fn redis_cli_missing_plans_the_installer() {
        let change = decide_redis_cli(
            CliInstall::Always,
            &|bin| bin == "curl" || bin == "sh",
            false,
        )
        .preview();
        assert_eq!(change.status, Status::Planned);
        assert!(change.note.contains("packages.redis.io"), "{}", change.note);
        assert!(!change.note.contains("ask"), "{}", change.note);
    }

    #[test]
    fn a_terminal_run_plans_the_installer_behind_a_question() {
        let action = decide_redis_cli(CliInstall::Ask, &|bin| bin == "curl" || bin == "sh", false);
        assert!(matches!(action, InstallAction::InstallCli { ask: true }));
        assert!(
            action
                .preview()
                .note
                .starts_with("would ask, then install redis-cli")
        );
    }

    #[test]
    fn nothing_to_ask_names_the_command_instead_of_running_it() {
        let change = decide_redis_cli(CliInstall::Suggest, &|_| true, false).preview();
        assert_eq!(
            change.status,
            Status::Unchanged,
            "present wins over the mode"
        );
        let change = decide_redis_cli(
            CliInstall::Suggest,
            &|bin| bin == "curl" || bin == "sh",
            false,
        )
        .preview();
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.contains("--install-cli"), "{}", change.note);
        assert!(
            change
                .note
                .contains("curl --proto '=https' --tlsv1.2 -fsSL"),
            "{}",
            change.note
        );
        assert!(
            change.note.contains("REDIS_CLI_VERSION="),
            "{}",
            change.note
        );
    }

    #[test]
    fn a_missing_curl_or_sh_is_named_before_suggesting_or_asking() {
        for mode in [CliInstall::Suggest, CliInstall::Ask, CliInstall::Always] {
            let change = decide_redis_cli(mode, &|bin| bin == "sh", false).preview();
            assert_eq!(
                change.note,
                "curl not found - the project skill carries docker fallbacks"
            );
            let change = decide_redis_cli(mode, &|bin| bin == "curl", false).preview();
            assert_eq!(
                change.note,
                "sh not found - the project skill carries docker fallbacks"
            );
        }
    }

    #[test]
    fn a_declined_prompt_is_a_skip_with_the_command() {
        let change = declined_cli_install().preview();
        assert_eq!(change.status, Status::Skipped);
        assert!(
            change
                .note
                .starts_with("not installed; to install later: curl --proto '=https'"),
            "{}",
            change.note
        );
    }

    #[test]
    fn only_a_plain_version_from_the_environment_is_used() {
        assert_eq!(pick_cli_version(None), REDIS_CLI_VERSION);
        assert_eq!(pick_cli_version(Some(" 7.4.0 ".into())), "7.4.0");
        assert_eq!(pick_cli_version(Some("stable".into())), "stable");
        assert_eq!(pick_cli_version(Some("".into())), REDIS_CLI_VERSION);
        assert_eq!(
            pick_cli_version(Some("8; rm -rf ~".into())),
            REDIS_CLI_VERSION
        );
    }

    #[test]
    fn the_flags_and_the_terminal_pick_the_mode() {
        assert_eq!(CliInstall::from_flags(true, false, true), CliInstall::Never);
        assert_eq!(
            CliInstall::from_flags(false, true, false),
            CliInstall::Always
        );
        assert_eq!(CliInstall::from_flags(false, false, true), CliInstall::Ask);
        assert_eq!(
            CliInstall::from_flags(false, false, false),
            CliInstall::Suggest
        );
    }
}
