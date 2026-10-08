//! The official Redis agent skills from redis/agent-skills.
//!
//! Primary path is the industry-standard skills CLI (`npx skills add`), which owns
//! `skills-lock.json` and its own layout. An explicitly named checkout (flag or env)
//! is copied directly instead - an explicit local source is an explicit choice, and
//! it works offline. With neither, the step is skipped with the remedy in the note;
//! skills are additive and the onboarding still stands. Outcomes are only knowable
//! after the installer runs, so previews carry the command and apply reports disk
//! truth.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::workflows::init::engine::change::{Change, Status};
use crate::workflows::init::engine::project::Agent;
use crate::workflows::init::engine::util::{has_bin, read_if, sh_in};
use crate::workflows::init::engine::{Event, InitError};

use crate::workflows::init::engine::SKILLS_DIR;

const LOCK_FILE: &str = "skills-lock.json";

/// The skills CLI's identifiers for the agents redisctl init supports; passing them
/// explicitly (instead of `--agent '*'`) keeps the layout to the directories the
/// chosen agents actually read.
fn cli_agent(agent: Agent) -> &'static str {
    match agent {
        Agent::Claude => "claude-code",
        Agent::Cursor => "cursor",
        Agent::Vscode => "github-copilot",
        Agent::Codex => "codex",
    }
}

fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

/// The skills CLI picks its own layout: usually a shared skills dir with per-agent
/// symlinks, but targeting claude-code alone copies straight into .claude/skills.
/// Probe both and report where a skill actually is, never assume.
fn target_dirs(cwd: &Path, global: bool) -> [PathBuf; 2] {
    if global {
        [home().join(".agents/skills"), home().join(".claude/skills")]
    } else {
        [cwd.join(SKILLS_DIR), cwd.join(".claude/skills")]
    }
}

fn installed_skill_path(cwd: &Path, global: bool, name: &str) -> Option<PathBuf> {
    target_dirs(cwd, global)
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.exists())
}

/// Where an install would land, named only when nothing was actually installed -
/// which directory the skills CLI uses is its own layout choice.
pub(crate) fn describe_target(global: bool) -> String {
    if global {
        format!(
            "{}/ or {}/",
            home().join(".agents/skills").display(),
            home().join(".claude/skills").display()
        )
    } else {
        format!("{SKILLS_DIR}/")
    }
}

/// The project dir a skills install lands in for this agent set.
pub fn project_dir(agents: &[Agent]) -> &'static str {
    if solo_claude(agents) {
        ".claude/skills"
    } else {
        SKILLS_DIR
    }
}

fn solo_claude(agents: &[Agent]) -> bool {
    agents == [Agent::Claude]
}

/// The offline fallback picks a real destination itself, mirroring the CLI's layout
/// choice: a lone claude-code target lands in that agent's own dir.
fn fallback_dir(cwd: &Path, global: bool, agents: &[Agent]) -> PathBuf {
    let dirs = target_dirs(cwd, global);
    if solo_claude(agents) {
        dirs[1].clone()
    } else {
        dirs[0].clone()
    }
}

/// skills-lock.json is written and owned by the skills CLI, version 1:
/// `{ "skills": { <name>: { ..., "computedHash": ... } } }`.
fn read_lock_hashes(cwd: &Path) -> BTreeMap<String, String> {
    let Some(lock) = read_if(cwd, LOCK_FILE)
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
    else {
        return BTreeMap::new();
    };
    lock["skills"]
        .as_object()
        .map(|skills| {
            skills
                .iter()
                .map(|(name, meta)| {
                    (
                        name.clone(),
                        meta["computedHash"]
                            .as_str()
                            .unwrap_or_default()
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The CLI's global lock: same idea, but its hash schema has already changed between
/// versions. The one thing stable across them is the `skills` key set, so read only
/// the names - it is what separates skills this tool manages from a user's own
/// personal ones sitting in the same tree.
fn read_global_lock_names() -> Option<BTreeSet<String>> {
    let content = std::fs::read_to_string(home().join(".agents/.skill-lock.json")).ok()?;
    let lock = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    Some(lock["skills"].as_object()?.keys().cloned().collect())
}

/// Which skill-bearing directories exist right now, for the global scope. A
/// directory counts as a skill when it holds SKILL.md.
fn global_skill_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for dir in target_dirs(Path::new(""), true) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().join("SKILL.md").exists()
                && let Some(name) = entry.file_name().to_str()
            {
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn skill_dirs(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().join("SKILL.md").exists())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect();
    names.sort();
    names
}

/// Skill dirs not tracked in the lock are the user's own; the standard installer
/// decides what happens to colliding names, so surface them and report disk truth.
/// Both layouts are scanned - a solo-Claude install lives in .claude/skills.
fn unmanaged_collisions(cwd: &Path) -> BTreeMap<String, String> {
    let managed = read_lock_hashes(cwd);
    let mut collisions = BTreeMap::new();
    for dir in target_dirs(cwd, false) {
        for name in skill_dirs(&dir) {
            if name == crate::workflows::init::engine::project_skill::NAME
                || managed.contains_key(&name)
            {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(dir.join(&name).join("SKILL.md")) {
                collisions.entry(name).or_insert(content);
            }
        }
    }
    collisions
}

fn walk_files(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, base, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_path_buf());
        }
    }
}

fn sorted_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files(dir, dir, &mut files);
    files.sort();
    files
}

fn dirs_equal(a: &Path, b: &Path) -> bool {
    if !a.exists() || !b.exists() {
        return false;
    }
    let files_a = sorted_files(a);
    if files_a != sorted_files(b) {
        return false;
    }
    files_a
        .iter()
        .all(|rel| std::fs::read(a.join(rel)).ok() == std::fs::read(b.join(rel)).ok())
}

/// A content signature for a skill directory, compared only within one run (before
/// vs after the installer), so a non-cryptographic hash is enough.
fn dir_signature(dir: &Path) -> Option<u64> {
    if !dir.exists() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for rel in sorted_files(dir) {
        rel.hash(&mut hasher);
        std::fs::read(dir.join(&rel)).ok()?.hash(&mut hasher);
    }
    Some(hasher.finish())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// What the install actually did, read back from disk truth.
pub(crate) struct SkillsOutcome {
    pub(crate) changes: Vec<Change>,
    pub(crate) installed: Vec<String>,
    pub(crate) installed_dir: Option<PathBuf>,
}

impl SkillsOutcome {
    fn skipped(change: Change) -> Self {
        Self {
            changes: vec![change],
            installed: Vec::new(),
            installed_dir: None,
        }
    }
}

/// The decided skills install, fixed at plan time; outcomes are disk truth read
/// back at apply time.
#[derive(Debug)]
pub(crate) struct SkillsAction {
    pub(crate) agents: Vec<Agent>,
    pub(crate) global: bool,
    pub(crate) repo: Option<PathBuf>,
}

impl SkillsAction {
    fn npx_args(&self) -> Vec<String> {
        // Pinning @latest keeps a stale npx cache from running an old skills CLI
        // that rejects newer flags.
        let mut args: Vec<String> = [
            "-y",
            "skills@latest",
            "add",
            "redis/agent-skills",
            "-s",
            "*",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        for agent in &self.agents {
            args.push("-a".to_string());
            args.push(cli_agent(*agent).to_string());
        }
        if self.global {
            args.push("-g".to_string());
        }
        args.push("-y".to_string());
        args
    }

    pub(crate) fn preview(&self, cwd: &Path) -> Vec<Change> {
        self.preview_with(cwd, &has_bin)
    }

    /// The npx check happens here too, so a dry run reports the same skip a real
    /// run would.
    fn preview_with(&self, cwd: &Path, has: &dyn Fn(&str) -> bool) -> Vec<Change> {
        match &self.repo {
            Some(repo) => match self.copy_steps(cwd, repo) {
                Ok(steps) => steps.into_iter().map(|step| step.change).collect(),
                Err(skipped) => vec![skipped],
            },
            None if !has("npx") => vec![self.npx_missing()],
            None => vec![Change::new(
                self.target(),
                Status::Planned,
                format!("would run: npx {}", self.npx_args().join(" ")),
            )],
        }
    }

    /// Where this agent set's skills land, for a line that names no installed skill.
    pub(crate) fn target(&self) -> String {
        if self.global {
            describe_target(true)
        } else {
            format!("{}/", project_dir(&self.agents))
        }
    }

    fn npx_missing(&self) -> Change {
        Change::new(
            self.target(),
            Status::Skipped,
            "npx not found - install Node, or pass --skills-repo <a redis/agent-skills checkout>",
        )
    }

    /// The skills a run is expected to install, for the plan-time project skill: a
    /// checkout's skills, the ones the lock already tracks, or none without npx.
    pub(crate) fn planned_installed(&self, cwd: &Path) -> Vec<String> {
        match &self.repo {
            Some(repo) => self
                .copy_steps(cwd, repo)
                .map(|steps| steps.into_iter().map(|step| step.name).collect())
                .unwrap_or_default(),
            None if !has_bin("npx") => Vec::new(),
            None => read_lock_hashes(cwd).into_keys().collect(),
        }
    }

    /// The checkout skills that need a `.claude/skills` link: a copy into the
    /// shared dir places none itself, while a solo-Claude copy already lands there.
    pub(crate) fn claude_links(&self, cwd: &Path) -> Vec<String> {
        let shared = fallback_dir(cwd, self.global, &self.agents) == cwd.join(SKILLS_DIR);
        match &self.repo {
            Some(repo) if shared => self
                .copy_steps(cwd, repo)
                .map(|steps| steps.into_iter().map(|step| step.name).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// What a checkout copy does per skill, decided the same way for the preview
    /// and the copy. `Err` is the one skipped line when there is nothing to copy.
    fn copy_steps(&self, cwd: &Path, repo: &Path) -> Result<Vec<CopyStep>, Change> {
        let skipped = |note: String| Change::new(self.target(), Status::Skipped, note);
        let source = repo.join("skills");
        if !source.exists() {
            return Err(skipped(format!(
                "{} has no skills/ directory",
                repo.display()
            )));
        }
        let destination = fallback_dir(cwd, self.global, &self.agents);
        let steps: Vec<CopyStep> = skill_dirs(&source)
            .into_iter()
            .map(|name| {
                let src = source.join(&name);
                let dst = destination.join(&name);
                let subject = if self.global {
                    format!("{}/", dst.display())
                } else {
                    format!("{}/", display_relative(cwd, &dst))
                };
                let change = if dirs_equal(&src, &dst) {
                    Change::new(subject, Status::Unchanged, "")
                } else {
                    Change::new(
                        subject,
                        if dst.exists() {
                            Status::Updated
                        } else {
                            Status::Created
                        },
                        "redis/agent-skills (local checkout; run npx skills add redis/agent-skills to adopt standard management)",
                    )
                };
                CopyStep {
                    name,
                    src,
                    dst,
                    change,
                }
            })
            .collect();
        if steps.is_empty() {
            return Err(skipped("no skills found in the checkout".to_string()));
        }
        Ok(steps)
    }

    pub(crate) fn perform(
        &self,
        cwd: &Path,
        on_event: &mut dyn FnMut(Event),
    ) -> Result<SkillsOutcome, InitError> {
        if let Some(repo) = &self.repo {
            return match self.copy_steps(cwd, repo) {
                Ok(steps) => self.copy_from(cwd, steps),
                Err(skipped) => Ok(SkillsOutcome::skipped(skipped)),
            };
        }

        if has_bin("npx") {
            let collisions = if self.global {
                BTreeMap::new()
            } else {
                unmanaged_collisions(cwd)
            };
            if !collisions.is_empty() {
                on_event(Event::Warning(format!(
                    "  note: existing skills not tracked in {LOCK_FILE}: {} - the standard installer decides whether to replace them (delete the dir to adopt the official copy)",
                    collisions.keys().cloned().collect::<Vec<_>>().join(", ")
                )));
            }
            let before_names = self.global.then(global_skill_names);
            let before_signatures: BTreeMap<String, Option<u64>> = before_names
                .iter()
                .flatten()
                .map(|name| {
                    (
                        name.clone(),
                        installed_skill_path(cwd, true, name).and_then(|p| dir_signature(&p)),
                    )
                })
                .collect();
            let before_lock = read_lock_hashes(cwd);

            on_event(Event::ProgressStart(
                "installing skills (npx skills add redis/agent-skills)".to_string(),
            ));
            let args = self.npx_args();
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let r = sh_in(cwd, "npx", &arg_refs);
            on_event(Event::ProgressDone(
                if r.status == 0 { " done" } else { " failed" }.to_string(),
            ));
            if r.status == 0 {
                return Ok(if self.global {
                    self.report_global(cwd, &before_names.unwrap_or_default(), &before_signatures)
                } else {
                    self.report_project(cwd, &before_lock, &collisions)
                });
            }
            return Ok(SkillsOutcome::skipped(Change::new(
                self.target(),
                Status::Skipped,
                failure_note(&r.stderr, &r.stdout, r.status),
            )));
        }

        Ok(SkillsOutcome::skipped(self.npx_missing()))
    }

    /// The npx project outcome, read from the lock the CLI maintains.
    fn report_project(
        &self,
        cwd: &Path,
        before: &BTreeMap<String, String>,
        collisions: &BTreeMap<String, String>,
    ) -> SkillsOutcome {
        let after = read_lock_hashes(cwd);
        let mut changes = Vec::new();
        let mut installed_dir = None;
        for (name, hash) in &after {
            let path = installed_skill_path(cwd, false, name);
            if installed_dir.is_none() {
                installed_dir = path
                    .as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf);
            }
            let subject = path
                .map(|p| format!("{}/", display_relative(cwd, &p)))
                .unwrap_or_else(|| name.clone());
            if let Some(previous_md) = collisions.get(name) {
                let now = installed_skill_path(cwd, false, name)
                    .and_then(|p| std::fs::read_to_string(p.join("SKILL.md")).ok());
                let (status, note) = if now.as_deref() == Some(previous_md.as_str()) {
                    (
                        Status::Kept,
                        "pre-existing skill left in place by the installer",
                    )
                } else {
                    (Status::Updated, "replaced by npx skills add")
                };
                changes.push(Change::new(subject, status, note));
                continue;
            }
            let status = match before.get(name) {
                None => Status::Created,
                Some(previous) if previous != hash => Status::Updated,
                Some(_) => Status::Unchanged,
            };
            changes.push(Change::new(subject, status, "npx skills add"));
        }
        let lock_status = if &after == before {
            Status::Unchanged
        } else if before.is_empty() {
            Status::Created
        } else {
            Status::Updated
        };
        changes.push(Change::new(
            LOCK_FILE,
            lock_status,
            "owned by the skills CLI",
        ));
        SkillsOutcome {
            changes,
            installed: after.into_keys().collect(),
            installed_dir,
        }
    }

    /// The npx global outcome: the global lock names what the CLI manages; a
    /// pre-existing personal skill in the same tree is never in it. Only when the
    /// lock cannot be read at all does this fall back to "appeared since before" -
    /// still never a name that was already there.
    fn report_global(
        &self,
        cwd: &Path,
        before_names: &BTreeSet<String>,
        before_signatures: &BTreeMap<String, Option<u64>>,
    ) -> SkillsOutcome {
        let after = global_skill_names();
        let managed: Vec<String> = read_global_lock_names()
            .unwrap_or_else(|| after.difference(before_names).cloned().collect())
            .into_iter()
            .filter(|name| after.contains(name))
            .collect();
        let mut installed_dir = None;
        let changes = managed
            .iter()
            .map(|name| {
                let path = installed_skill_path(cwd, true, name);
                if installed_dir.is_none() {
                    installed_dir = path
                        .as_deref()
                        .and_then(Path::parent)
                        .map(Path::to_path_buf);
                }
                let subject = path
                    .as_ref()
                    .map(|p| format!("{}/", p.display()))
                    .unwrap_or_else(|| name.clone());
                let status = if !before_names.contains(name) {
                    Status::Created
                } else if path.as_deref().and_then(dir_signature)
                    == before_signatures.get(name).copied().flatten()
                {
                    Status::Unchanged
                } else {
                    Status::Updated
                };
                Change::new(subject, status, "npx skills add")
            })
            .collect();
        SkillsOutcome {
            changes,
            installed: managed,
            installed_dir,
        }
    }

    /// Copy every skill from a checkout into the layout the real CLI would use.
    fn copy_from(&self, cwd: &Path, steps: Vec<CopyStep>) -> Result<SkillsOutcome, InitError> {
        let mut changes = Vec::new();
        let mut installed = Vec::new();
        for step in steps {
            if step.change.status != Status::Unchanged {
                let _ = std::fs::remove_dir_all(&step.dst);
                copy_dir(&step.src, &step.dst).map_err(|e| InitError::WriteFailed {
                    rel: step.dst.display().to_string(),
                    message: e.to_string(),
                })?;
            }
            changes.push(step.change);
            installed.push(step.name);
        }
        Ok(SkillsOutcome {
            changes,
            installed_dir: Some(fallback_dir(cwd, self.global, &self.agents)),
            installed,
        })
    }
}

struct CopyStep {
    name: String,
    src: PathBuf,
    dst: PathBuf,
    change: Change,
}

/// A failed install's note leads with the installer's own first error line - a
/// generic guess once sent a real failure down the wrong path. Notes render on one
/// line, so the line is capped.
fn failure_note(stderr: &str, stdout: &str, status: i32) -> String {
    let error = [stderr, stdout]
        .iter()
        .flat_map(|out| out.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("exit status {status}"));
    let error = if error.chars().count() > 160 {
        format!("{}...", error.chars().take(160).collect::<String>())
    } else {
        error
    };
    format!(
        "npx skills add failed: {error} - re-run it yourself, or pass --skills-repo <a redis/agent-skills checkout>"
    )
}

fn display_relative(cwd: &Path, path: &Path) -> String {
    path.strip_prefix(cwd).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_checkout(skills: &[&str]) -> tempfile::TempDir {
        let repo = tempfile::tempdir().unwrap();
        for name in skills {
            let dir = repo.path().join("skills").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), format!("# {name}\n")).unwrap();
        }
        repo
    }

    fn action(repo: &Path) -> SkillsAction {
        SkillsAction {
            agents: vec![Agent::Claude, Agent::Codex],
            global: false,
            repo: Some(repo.to_path_buf()),
        }
    }

    #[test]
    fn preview_carries_the_full_npx_command() {
        let a = SkillsAction {
            agents: vec![Agent::Claude, Agent::Vscode],
            global: true,
            repo: None,
        };
        let [change] = &a.preview_with(Path::new("/p"), &|_| true)[..] else {
            panic!("one npx line");
        };
        assert_eq!(change.status, Status::Planned);
        assert_eq!(
            change.note,
            "would run: npx -y skills@latest add redis/agent-skills -s * -a claude-code -a github-copilot -g -y"
        );
    }

    #[test]
    fn preview_without_npx_is_the_skip_a_real_run_reports() {
        let a = SkillsAction {
            agents: vec![Agent::Claude],
            global: false,
            repo: None,
        };
        let [change] = &a.preview_with(Path::new("/p"), &|_| false)[..] else {
            panic!("one skipped line");
        };
        assert_eq!(change.subject, ".claude/skills/");
        assert_eq!(change.status, Status::Skipped);
        assert!(change.note.starts_with("npx not found"), "{}", change.note);
        let shared = SkillsAction {
            agents: vec![Agent::Claude, Agent::Cursor],
            ..a
        };
        assert_eq!(
            shared.preview_with(Path::new("/p"), &|_| false)[0].subject,
            ".agents/skills/"
        );
    }

    #[test]
    fn failure_note_carries_the_installers_own_error() {
        let note = failure_note("Unknown agent: codex\nrun skills --help\n", "", 1);
        assert_eq!(
            note,
            "npx skills add failed: Unknown agent: codex - re-run it yourself, or pass --skills-repo <a redis/agent-skills checkout>"
        );
    }

    #[test]
    fn failure_note_falls_back_to_stdout_then_to_the_exit_status() {
        assert!(failure_note("", "only stdout spoke\n", 1).contains("only stdout spoke"));
        assert!(failure_note("", "", 127).contains("exit status 127"));
    }

    #[test]
    fn failure_note_truncates_a_rambling_error_line() {
        let long = "x".repeat(500);
        let note = failure_note(&long, "", 1);
        assert!(note.len() < 300, "{}", note.len());
        assert!(note.contains("..."));
    }

    #[test]
    fn checkout_preview_predicts_each_copy_including_drift() {
        let repo = fake_checkout(&["redis-basics", "redis-search"]);
        let project = tempfile::tempdir().unwrap();
        let lines = |changes: Vec<Change>| {
            changes
                .into_iter()
                .map(|c| (c.subject, c.status))
                .collect::<Vec<_>>()
        };
        let a = action(repo.path());
        let planned = lines(a.preview(project.path()));
        assert_eq!(
            planned,
            lines(a.perform(project.path(), &mut |_| {}).unwrap().changes)
        );
        std::fs::write(
            project.path().join(".agents/skills/redis-basics/SKILL.md"),
            "user edit\n",
        )
        .unwrap();
        let planned = lines(a.preview(project.path()));
        assert_eq!(
            planned,
            vec![
                (".agents/skills/redis-basics/".to_string(), Status::Updated),
                (
                    ".agents/skills/redis-search/".to_string(),
                    Status::Unchanged
                ),
            ]
        );
        assert_eq!(
            planned,
            lines(a.perform(project.path(), &mut |_| {}).unwrap().changes)
        );
    }

    #[test]
    fn checkout_links_only_the_shared_layout() {
        let repo = fake_checkout(&["redis-basics"]);
        let cwd = Path::new("/p");
        assert_eq!(action(repo.path()).claude_links(cwd), vec!["redis-basics"]);
        let solo = SkillsAction {
            agents: vec![Agent::Claude],
            global: false,
            repo: Some(repo.path().to_path_buf()),
        };
        assert!(solo.claude_links(cwd).is_empty());
        let global = SkillsAction {
            global: true,
            ..action(repo.path())
        };
        assert!(global.claude_links(cwd).is_empty());
    }

    #[test]
    fn solo_claude_checkout_reports_the_real_destination() {
        let repo = fake_checkout(&["redis-basics"]);
        let project = tempfile::tempdir().unwrap();
        let solo = SkillsAction {
            agents: vec![Agent::Claude],
            global: false,
            repo: Some(repo.path().to_path_buf()),
        };
        let changes = solo.perform(project.path(), &mut |_| {}).unwrap().changes;
        assert_eq!(changes[0].subject, ".claude/skills/redis-basics/");
        assert!(
            project
                .path()
                .join(".claude/skills/redis-basics/SKILL.md")
                .exists()
        );
        let rerun = solo.perform(project.path(), &mut |_| {}).unwrap();
        assert!(rerun.changes.iter().all(|c| c.status == Status::Unchanged));
    }

    #[test]
    fn unmanaged_collisions_see_the_claude_layout_too() {
        let project = tempfile::tempdir().unwrap();
        let dir = project.path().join(".claude/skills/mine");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "x").unwrap();
        let collisions = unmanaged_collisions(project.path());
        assert_eq!(collisions.keys().collect::<Vec<_>>(), vec!["mine"]);
    }

    #[test]
    fn explicit_checkout_copies_and_reruns_unchanged() {
        let repo = fake_checkout(&["redis-basics", "redis-search"]);
        let project = tempfile::tempdir().unwrap();
        let outcome = action(repo.path())
            .perform(project.path(), &mut |_| {})
            .unwrap();
        assert_eq!(outcome.installed, vec!["redis-basics", "redis-search"]);
        let summary: Vec<_> = outcome
            .changes
            .iter()
            .map(|c| (c.subject.as_str(), c.status))
            .collect();
        assert_eq!(
            summary,
            vec![
                (".agents/skills/redis-basics/", Status::Created),
                (".agents/skills/redis-search/", Status::Created),
            ]
        );
        assert!(
            project
                .path()
                .join(".agents/skills/redis-basics/SKILL.md")
                .exists()
        );

        let rerun = action(repo.path())
            .perform(project.path(), &mut |_| {})
            .unwrap();
        assert!(rerun.changes.iter().all(|c| c.status == Status::Unchanged));
        assert_eq!(rerun.installed.len(), 2);
    }

    #[test]
    fn a_drifted_skill_is_replaced_and_reported_updated() {
        let repo = fake_checkout(&["redis-basics"]);
        let project = tempfile::tempdir().unwrap();
        action(repo.path())
            .perform(project.path(), &mut |_| {})
            .unwrap();
        std::fs::write(
            project.path().join(".agents/skills/redis-basics/SKILL.md"),
            "user edit\n",
        )
        .unwrap();
        let changes = action(repo.path())
            .perform(project.path(), &mut |_| {})
            .unwrap()
            .changes;
        assert_eq!(changes[0].status, Status::Updated);
        assert_eq!(
            std::fs::read_to_string(project.path().join(".agents/skills/redis-basics/SKILL.md"))
                .unwrap(),
            "# redis-basics\n"
        );
    }

    #[test]
    fn a_checkout_without_skills_reads_skipped() {
        let repo = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let changes = action(repo.path())
            .perform(project.path(), &mut |_| {})
            .unwrap()
            .changes;
        assert_eq!(changes[0].status, Status::Skipped);
        assert!(changes[0].note.contains("no skills/ directory"));
    }

    #[test]
    fn unmanaged_collisions_ignore_the_lock_and_the_generated_skill() {
        let project = tempfile::tempdir().unwrap();
        for name in [
            "mine",
            "managed",
            crate::workflows::init::engine::project_skill::NAME,
        ] {
            let dir = project.path().join(SKILLS_DIR).join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), "x").unwrap();
        }
        std::fs::write(
            project.path().join(LOCK_FILE),
            r#"{"skills":{"managed":{"computedHash":"abc"}}}"#,
        )
        .unwrap();
        let collisions = unmanaged_collisions(project.path());
        assert_eq!(collisions.keys().collect::<Vec<_>>(), vec!["mine"]);
    }

    #[test]
    fn lock_hashes_read_the_v1_schema() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join(LOCK_FILE),
            r#"{"skills":{"a":{"computedHash":"h1"},"b":{"computedHash":"h2"}}}"#,
        )
        .unwrap();
        let hashes = read_lock_hashes(project.path());
        assert_eq!(hashes.get("a").map(String::as_str), Some("h1"));
        assert_eq!(hashes.len(), 2);
    }

    #[test]
    fn project_report_diffs_the_lock_and_reports_disk_truth_for_collisions() {
        let project = tempfile::tempdir().unwrap();
        // The installer left one collision untouched and updated the lock.
        let kept_dir = project.path().join(SKILLS_DIR).join("mine");
        std::fs::create_dir_all(&kept_dir).unwrap();
        std::fs::write(kept_dir.join("SKILL.md"), "original").unwrap();
        for name in ["fresh", "same"] {
            let dir = project.path().join(SKILLS_DIR).join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), "x").unwrap();
        }
        std::fs::write(
            project.path().join(LOCK_FILE),
            r#"{"skills":{"fresh":{"computedHash":"new"},"same":{"computedHash":"h"},"mine":{"computedHash":"m"}}}"#,
        )
        .unwrap();
        let before = BTreeMap::from([("same".to_string(), "h".to_string())]);
        let collisions = BTreeMap::from([("mine".to_string(), "original".to_string())]);
        let a = SkillsAction {
            agents: vec![Agent::Claude],
            global: false,
            repo: None,
        };
        let outcome = a.report_project(project.path(), &before, &collisions);
        assert_eq!(outcome.installed.len(), 3);
        let by_subject: BTreeMap<_, _> = outcome
            .changes
            .iter()
            .map(|c| (c.subject.clone(), c.status))
            .collect();
        assert_eq!(
            by_subject.get(".agents/skills/fresh/"),
            Some(&Status::Created)
        );
        assert_eq!(
            by_subject.get(".agents/skills/same/"),
            Some(&Status::Unchanged)
        );
        assert_eq!(by_subject.get(".agents/skills/mine/"), Some(&Status::Kept));
        assert_eq!(by_subject.get(LOCK_FILE), Some(&Status::Updated));
    }

    #[test]
    fn the_lock_line_reports_what_the_installer_did_to_it() {
        let project = tempfile::tempdir().unwrap();
        let dir = project.path().join(SKILLS_DIR).join("a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "x").unwrap();
        std::fs::write(
            project.path().join(LOCK_FILE),
            r#"{"skills":{"a":{"computedHash":"h"}}}"#,
        )
        .unwrap();
        let a = SkillsAction {
            agents: vec![Agent::Claude],
            global: false,
            repo: None,
        };
        let lock_status = |before: BTreeMap<String, String>| {
            a.report_project(project.path(), &before, &BTreeMap::new())
                .changes
                .into_iter()
                .find(|c| c.subject == LOCK_FILE)
                .map(|c| c.status)
        };
        assert_eq!(lock_status(BTreeMap::new()), Some(Status::Created));
        assert_eq!(
            lock_status(BTreeMap::from([("a".to_string(), "old".to_string())])),
            Some(Status::Updated)
        );
        assert_eq!(
            lock_status(BTreeMap::from([("a".to_string(), "h".to_string())])),
            Some(Status::Unchanged)
        );
    }

    #[test]
    fn solo_claude_fallback_lands_in_the_claude_dir() {
        let cwd = Path::new("/p");
        assert_eq!(
            fallback_dir(cwd, false, &[Agent::Claude]),
            cwd.join(".claude/skills")
        );
        assert_eq!(
            fallback_dir(cwd, false, &[Agent::Claude, Agent::Codex]),
            cwd.join(SKILLS_DIR)
        );
    }
}
