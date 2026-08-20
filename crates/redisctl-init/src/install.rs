//! Client SDK and redis-cli installation: what to install is decided read-only at
//! plan time; the package-manager commands run at apply time. An install failure is
//! reported as skipped, never as a run failure - the onboarding still stands.

use std::path::Path;

use crate::change::{Change, Status};
use crate::env::FileAction;
use crate::project::{Project, Runtime};
use crate::util::{ending_with_newline, exists, has_bin, read_if, sh, sh_in};
use crate::{Event, InitError};

const CLI_INSTALLER: &str = "https://packages.redis.io/redis-cli/install.sh";

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
    Append(FileAction),
    /// Fetch redis-cli via the official installer (statically linked,
    /// checksum-verified, no-sudo ~/.local/bin fallback).
    InstallCli,
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
            InstallAction::Append(action) => action.preview(),
            InstallAction::InstallCli => Change::new(
                "redis-cli",
                Status::Planned,
                format!("would install via {CLI_INSTALLER}"),
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
            InstallAction::Append(action) => action.perform(cwd),
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
            InstallAction::InstallCli => {
                on_event(Event::ProgressStart(
                    "installing redis-cli (official installer, packages.redis.io)".to_string(),
                ));
                let r = sh("sh", &["-c", &format!("curl -fsSL {CLI_INSTALLER} | sh")]);
                if r.status != 0 {
                    on_event(Event::ProgressDone(" failed".to_string()));
                    return Ok(Change::new(
                        "redis-cli",
                        Status::Skipped,
                        "installer failed - the project skill carries docker fallbacks",
                    ));
                }
                on_event(Event::ProgressDone(" done".to_string()));
                Ok(if has_bin("redis-cli") {
                    Change::new(
                        "redis-cli",
                        Status::Created,
                        "installed via the official packages.redis.io script",
                    )
                } else {
                    Change::new(
                        "redis-cli",
                        Status::Created,
                        "installed to ~/.local/bin - add it to PATH to use it",
                    )
                })
            }
        }
    }
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
            let has_dep = regex::Regex::new(r"(?m)^\s*redis([\[\s=<>~!]|$)")
                .expect("static regex")
                .is_match(requirements.as_deref().unwrap_or(""))
                || regex::Regex::new(r#""redis([\[">=<~]|")"#)
                    .expect("static regex")
                    .is_match(&read_if(cwd, "pyproject.toml").unwrap_or_default());
            if has_dep {
                return unchanged("redis client already a dependency");
            }
            if exists(cwd, "pyproject.toml") && (exists(cwd, "uv.lock") || has("uv")) {
                return command(
                    has,
                    "uv",
                    &["add", "redis"],
                    "pyproject.toml",
                    "redis-py installed via uv".to_string(),
                    "client package",
                );
            }
            if exists(cwd, "poetry.lock") {
                return command(
                    has,
                    "poetry",
                    &["add", "redis"],
                    "pyproject.toml",
                    "redis-py installed via poetry".to_string(),
                    "client package",
                );
            }
            if let Some(reqs) = requirements {
                return InstallAction::Append(FileAction::Write {
                    rel: "requirements.txt".to_string(),
                    content: format!("{}redis\n", ending_with_newline(&reqs)),
                    status: Status::Updated,
                    note: "redis-py added - run pip install -r requirements.txt".to_string(),
                });
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

/// The product SDK per runtime; everything else talks REST (or MCP) directly.
pub(crate) fn product_package(key: crate::ProductKey, runtime: Runtime) -> Option<&'static str> {
    match (key, runtime) {
        (crate::ProductKey::AgentMemory, Runtime::Node) => Some("@redis-iris/agent-memory"),
        (crate::ProductKey::LangCache, Runtime::Node) => Some("@redis-ai/langcache"),
        (crate::ProductKey::LangCache, Runtime::Python) => Some("langcache"),
        (crate::ProductKey::ContextRetriever, Runtime::Python) => Some("redis-context-retriever"),
        _ => None,
    }
}

pub(crate) fn plan_product_install(
    cwd: &Path,
    project: &Project,
    product: &crate::products::WiredProduct,
) -> InstallAction {
    decide_product(cwd, project, product, &has_bin)
}

fn decide_product(
    cwd: &Path,
    project: &Project,
    product: &crate::products::WiredProduct,
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
            let has_dep =
                regex::Regex::new(&format!(r"(?m)^\s*{}([\[\s=<>~!]|$)", regex::escape(pkg)))
                    .expect("escaped package regex")
                    .is_match(requirements.as_deref().unwrap_or(""))
                    || read_if(cwd, "pyproject.toml")
                        .unwrap_or_default()
                        .contains(&format!("\"{pkg}"));
            if has_dep {
                return InstallAction::Report(Change::new(
                    &label,
                    Status::Unchanged,
                    format!("{pkg} already a dependency"),
                ));
            }
            if exists(cwd, "pyproject.toml") && (exists(cwd, "uv.lock") || has("uv")) {
                return command(
                    has,
                    "uv",
                    &["add", pkg],
                    "pyproject.toml",
                    format!("{pkg} installed via uv"),
                    &label,
                );
            }
            if exists(cwd, "poetry.lock") {
                return command(
                    has,
                    "poetry",
                    &["add", pkg],
                    "pyproject.toml",
                    format!("{pkg} installed via poetry"),
                    &label,
                );
            }
            if let Some(reqs) = requirements {
                return InstallAction::Append(FileAction::Write {
                    rel: "requirements.txt".to_string(),
                    content: format!("{}{pkg}\n", ending_with_newline(&reqs)),
                    status: Status::Updated,
                    note: format!("{pkg} added - run pip install -r requirements.txt"),
                });
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

pub(crate) fn plan_redis_cli(install: bool) -> InstallAction {
    let local_bin = std::env::home_dir()
        .map(|home| home.join(".local/bin/redis-cli").exists())
        .unwrap_or(false);
    decide_redis_cli(install, &has_bin, local_bin)
}

fn decide_redis_cli(install: bool, has: &dyn Fn(&str) -> bool, local_bin: bool) -> InstallAction {
    // The official installer falls back to ~/.local/bin, which is often not on
    // PATH: without this check every run would re-run curl | sh.
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
    if !install {
        return InstallAction::Report(Change::new(
            "redis-cli",
            Status::Skipped,
            "--no-install-cli; the project skill carries docker fallbacks",
        ));
    }
    InstallAction::InstallCli
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::detect;

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
        match &action {
            InstallAction::Append(FileAction::Write { content, .. }) => {
                assert_eq!(content, "flask\nredis\n");
            }
            other => panic!("expected an append, got {other:?}"),
        }
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
        let change = decide_redis_cli(true, &|_| false, true).preview();
        assert_eq!(change.status, Status::Unchanged);
        assert!(change.note.contains("~/.local/bin"), "{}", change.note);
    }

    #[test]
    fn redis_cli_present_reads_unchanged() {
        let change = decide_redis_cli(true, &|_| true, false).preview();
        assert_eq!(change.status, Status::Unchanged);
        assert!(change.note.contains("already on PATH"), "{}", change.note);
    }

    #[test]
    fn redis_cli_opt_out_reads_skipped() {
        let change = decide_redis_cli(false, &|_| false, false).preview();
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.contains("--no-install-cli"), "{}", change.note);
    }

    #[test]
    fn redis_cli_missing_plans_the_installer() {
        let change = decide_redis_cli(true, &|_| false, false).preview();
        assert_eq!(change.status, Status::Planned);
        assert!(change.note.contains("packages.redis.io"), "{}", change.note);
    }
}
