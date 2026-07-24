//! Credential delivery to a dotenv file (PRD §4.4/§5.3).
//!
//! The `quick-database` workflow never prints the connection string to stdout/stderr; it
//! writes it to a file so it can't leak into terminal scrollback, CI logs, or an agent's
//! captured output. This module owns that file I/O and the git-hygiene follow-up, with no
//! dependency on the API layer so it can be unit-tested in isolation.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeliveryError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl DeliveryError {
    fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            source,
        }
    }
}

/// What `deliver`/`deliver_vars` did, for reporting and tests.
#[derive(Debug, Clone)]
pub struct DeliveryOutcome {
    /// The file that was written.
    pub path: PathBuf,
    /// The primary variable name written into it (the first of the set).
    pub variable: String,
    /// True when the target file did not exist and was created fresh.
    pub created: bool,
    /// True when at least one existing `VAR=` line was replaced (re-run / reuse path).
    pub replaced: bool,
}

/// Append (or replace) a single `variable=value` in the dotenv file at `path`.
/// Convenience wrapper over [`deliver_vars`].
pub fn deliver(path: &Path, variable: &str, value: &str) -> Result<DeliveryOutcome, DeliveryError> {
    deliver_vars(path, &[(variable, value)])
}

/// Write a set of `KEY=value` pairs into the dotenv file at `path` in one pass.
///
/// - target exists → each existing `KEY=` line is replaced in place rather than duplicated
///   (idempotent re-run), all other lines are preserved byte-for-byte, and any genuinely new
///   keys are appended under a single `# --- added by redisctl … ---` marker. The file is
///   only rewritten when the content actually changes.
/// - target missing → created with mode `0o600` (unix), a marker, and all pairs.
///
/// No backups are made: unrelated lines are never touched, so the only thing an overwrite
/// changes is the managed keys — exactly what the caller asked for.
///
/// `vars` must be non-empty; the first pair is treated as the primary variable for reporting.
pub fn deliver_vars(path: &Path, vars: &[(&str, &str)]) -> Result<DeliveryOutcome, DeliveryError> {
    let primary = vars.first().map(|(k, _)| k.to_string()).unwrap_or_default();

    if path.exists() {
        let original = fs::read_to_string(path).map_err(|e| DeliveryError::io(path, e))?;
        let (rewritten, replaced) = upsert_vars(&original, vars);
        if rewritten != original {
            fs::write(path, &rewritten).map_err(|e| DeliveryError::io(path, e))?;
        }
        // The file holds credentials now — ensure it isn't group/world-readable, even if it
        // pre-existed with looser permissions (a fresh file is created 0600 below, but an
        // existing 0644 .env would otherwise stay readable after we write the password).
        restrict_permissions(path)?;
        Ok(DeliveryOutcome {
            path: path.to_path_buf(),
            variable: primary,
            created: false,
            replaced,
        })
    } else {
        let mut contents = format!("{}\n", marker_comment());
        for (k, v) in vars {
            contents.push_str(&format!("{k}={v}\n"));
        }
        write_new_private(path, &contents)?;
        Ok(DeliveryOutcome {
            path: path.to_path_buf(),
            variable: primary,
            created: true,
            replaced: false,
        })
    }
}

/// If `path` sits inside a git working tree and isn't already ignored, append it to the
/// repo-root `.gitignore`. Outside a git repo this is a silent no-op (PRD §7.2). Returns
/// `true` iff `.gitignore` was modified.
pub fn ensure_gitignored(path: &Path) -> Result<bool, DeliveryError> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match std::env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(_) => return Ok(false),
        }
    };

    let Some(repo_root) = find_repo_root(&abs) else {
        return Ok(false); // not a git repo → skip
    };

    // Entry to add: path relative to the repo root when possible, else the bare file name.
    let entry = abs
        .strip_prefix(&repo_root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .or_else(|| abs.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    if entry.is_empty() {
        return Ok(false);
    }
    let file_name = abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let gitignore = repo_root.join(".gitignore");
    let existing = fs::read_to_string(&gitignore).unwrap_or_default();
    if already_ignored(&existing, &entry, &file_name) {
        return Ok(false);
    }

    let mut contents = existing;
    if !contents.is_empty() && !contents.ends_with('\n') {
        contents.push('\n');
    }
    contents.push_str(&entry);
    contents.push('\n');
    fs::write(&gitignore, &contents).map_err(|e| DeliveryError::io(&gitignore, e))?;
    Ok(true)
}

fn marker_comment() -> String {
    format!(
        "# --- added by redisctl on {} ---",
        chrono::Local::now().format("%Y-%m-%d")
    )
}

/// Replace each existing `KEY=…` line in place; append the remaining (new) keys under a
/// single marker. Returns the new content and whether at least one line was replaced. All
/// non-matching lines are preserved byte-for-byte.
fn upsert_vars(original: &str, vars: &[(&str, &str)]) -> (String, bool) {
    let mut remaining: Vec<(&str, &str)> = vars.to_vec();
    let mut replaced_any = false;
    let mut out_lines: Vec<String> = Vec::new();

    for line in original.lines() {
        let trimmed = line.trim_start();
        if let Some(pos) = remaining
            .iter()
            .position(|(k, _)| trimmed.starts_with(&format!("{k}=")))
        {
            let (k, v) = remaining.remove(pos);
            out_lines.push(format!("{k}={v}"));
            replaced_any = true;
        } else {
            out_lines.push(line.to_string());
        }
    }

    let mut out = out_lines.join("\n");
    // Preserve a trailing newline if the original had one (byte-for-byte for the kept lines).
    if original.ends_with('\n') {
        out.push('\n');
    }

    // Append any keys that had no existing line, under one marker.
    if !remaining.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&marker_comment());
        out.push('\n');
        for (k, v) in &remaining {
            out.push_str(&format!("{k}={v}\n"));
        }
    }

    (out, replaced_any)
}

#[cfg(unix)]
fn write_new_private(path: &Path, contents: &str) -> Result<(), DeliveryError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| DeliveryError::io(path, e))?;
    file.write_all(contents.as_bytes())
        .map_err(|e| DeliveryError::io(path, e))
}

// Windows has no POSIX mode bits; create the file normally and rely on the user profile's
// ACLs. Documented limitation (PRD §5.3).
#[cfg(not(unix))]
fn write_new_private(path: &Path, contents: &str) -> Result<(), DeliveryError> {
    fs::write(path, contents).map_err(|e| DeliveryError::io(path, e))
}

/// Tighten an existing credentials file to owner-only (`0o600`) on unix. No-op elsewhere.
#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), DeliveryError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| DeliveryError::io(path, e))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), DeliveryError> {
    Ok(())
}

fn find_repo_root(start: &Path) -> Option<PathBuf> {
    // `start` is a file path; begin at its directory.
    let mut dir = start.parent();
    while let Some(d) = dir {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

fn already_ignored(gitignore: &str, entry: &str, file_name: &str) -> bool {
    gitignore.lines().any(|line| {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            return false;
        }
        let l = l.trim_start_matches("./");
        l == entry || l == file_name
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn creates_fresh_file_with_marker() {
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        let out = deliver(&env, "REDIS_URL", "rediss://x@h:1").unwrap();
        assert!(out.created);
        assert!(!out.replaced);
        let body = fs::read_to_string(&env).unwrap();
        assert!(body.contains("REDIS_URL=rediss://x@h:1"));
        assert!(body.contains("added by redisctl"));
    }

    #[cfg(unix)]
    #[test]
    fn existing_loose_file_is_tightened_to_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        fs::write(&env, "EXISTING=1\n").unwrap();
        fs::set_permissions(&env, fs::Permissions::from_mode(0o644)).unwrap();
        // Writing secrets into a pre-existing world-readable file must tighten it.
        deliver(&env, "REDIS_URL", "rediss://x@h:1").unwrap();
        let mode = fs::metadata(&env).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    #[cfg(unix)]
    #[test]
    fn fresh_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        deliver(&env, "REDIS_URL", "rediss://x@h:1").unwrap();
        let mode = fs::metadata(&env).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    #[test]
    fn existing_file_is_appended_preserving_others() {
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        fs::write(&env, "EXISTING=1\nOTHER=two\n").unwrap();
        let out = deliver(&env, "REDIS_URL", "rediss://x@h:1").unwrap();
        assert!(!out.created);
        assert!(!out.replaced);
        let body = fs::read_to_string(&env).unwrap();
        // Unrelated lines preserved byte-for-byte at the head; no backup file created.
        assert!(body.starts_with("EXISTING=1\nOTHER=two\n"));
        assert!(body.contains("REDIS_URL=rediss://x@h:1"));
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "only .env exists, no backup"
        );
    }

    #[test]
    fn writes_multiple_vars_and_reruns_cleanly() {
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        let vars = [
            ("REDIS_URL", "rediss://default:p@h:1"),
            ("REDIS_HOST", "h"),
            ("REDIS_PORT", "1"),
        ];
        let out = deliver_vars(&env, &vars).unwrap();
        assert!(out.created);
        assert_eq!(out.variable, "REDIS_URL");
        let body = fs::read_to_string(&env).unwrap();
        for (k, v) in vars {
            assert!(body.contains(&format!("{k}={v}")), "missing {k}");
        }

        // Re-run with new values: every key is replaced in place, none duplicated.
        let vars2 = [
            ("REDIS_URL", "rediss://default:p2@h2:2"),
            ("REDIS_HOST", "h2"),
            ("REDIS_PORT", "2"),
        ];
        let out2 = deliver_vars(&env, &vars2).unwrap();
        assert!(out2.replaced);
        let body2 = fs::read_to_string(&env).unwrap();
        assert_eq!(body2.matches("REDIS_URL=").count(), 1);
        assert_eq!(body2.matches("REDIS_HOST=").count(), 1);
        assert_eq!(body2.matches("REDIS_PORT=").count(), 1);
        assert!(body2.contains("REDIS_HOST=h2"));
        assert!(!body2.contains("REDIS_HOST=h\n"));
    }

    #[test]
    fn unchanged_rerun_is_noop() {
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        deliver(&env, "REDIS_URL", "rediss://x@h:1").unwrap();
        let before = fs::read_to_string(&env).unwrap();

        // Identical re-run: content stable, no extra files.
        let out = deliver(&env, "REDIS_URL", "rediss://x@h:1").unwrap();
        assert!(!out.created);
        assert_eq!(fs::read_to_string(&env).unwrap(), before);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn rerun_replaces_not_duplicates() {
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        deliver(&env, "REDIS_URL", "rediss://old@h:1").unwrap();
        let out = deliver(&env, "REDIS_URL", "rediss://new@h:2").unwrap();
        assert!(out.replaced);
        let body = fs::read_to_string(&env).unwrap();
        assert_eq!(body.matches("REDIS_URL=").count(), 1, "no duplicate var");
        assert!(body.contains("REDIS_URL=rediss://new@h:2"));
        assert!(!body.contains("rediss://old@h:1"));
    }

    #[test]
    fn gitignore_appended_in_repo() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        let env = dir.path().join(".env");
        fs::write(&env, "A=1\n").unwrap();
        let changed = ensure_gitignored(&env).unwrap();
        assert!(changed);
        let gi = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gi.lines().any(|l| l.trim() == ".env"));
    }

    #[test]
    fn gitignore_skipped_outside_repo() {
        let dir = tempdir().unwrap();
        let env = dir.path().join(".env");
        fs::write(&env, "A=1\n").unwrap();
        assert!(!ensure_gitignored(&env).unwrap());
        assert!(!dir.path().join(".gitignore").exists());
    }

    #[test]
    fn gitignore_not_duplicated_when_already_ignored() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".gitignore"), "node_modules\n.env\n").unwrap();
        let env = dir.path().join(".env");
        fs::write(&env, "A=1\n").unwrap();
        let changed = ensure_gitignored(&env).unwrap();
        assert!(!changed);
        let gi = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(gi.matches(".env").count(), 1);
    }
}
