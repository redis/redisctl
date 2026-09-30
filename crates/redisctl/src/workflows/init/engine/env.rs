//! The `.env` / `.gitignore` contract: mutations are decided read-only at plan time
//! and performed at apply time, so a dry run renders exactly what a real run does.

use std::path::Path;

use crate::workflows::init::engine::InitError;
use crate::workflows::init::engine::change::{Change, Status};
use crate::workflows::init::engine::products::is_configured;
use crate::workflows::init::engine::util::{mask_url, read_if};

const PROVENANCE: &str = "# Added by redisctl init";

/// One decided file mutation. The decision (and its change report) is fixed at plan
/// time; only the write happens at apply time.
#[derive(Debug)]
pub(crate) enum FileAction {
    Write {
        rel: String,
        content: String,
        status: Status,
        note: String,
    },
    Unchanged {
        rel: String,
    },
    Kept {
        rel: String,
        note: String,
    },
}

impl FileAction {
    pub(crate) fn preview(&self) -> Change {
        match self {
            FileAction::Write {
                rel, status, note, ..
            } => Change::new(rel.clone(), *status, note.clone()),
            FileAction::Unchanged { rel } => Change::new(rel.clone(), Status::Unchanged, ""),
            FileAction::Kept { rel, note } => Change::new(rel.clone(), Status::Kept, note.clone()),
        }
    }

    pub(crate) fn perform(&self, dir: &Path) -> Result<Change, InitError> {
        if let FileAction::Write { rel, content, .. } = self {
            let path = dir.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| InitError::WriteFailed {
                    rel: rel.clone(),
                    message: e.to_string(),
                })?;
            }
            std::fs::write(&path, content).map_err(|e| InitError::WriteFailed {
                rel: rel.clone(),
                message: e.to_string(),
            })?;
        }
        Ok(self.preview())
    }
}

/// Read the file for mutation planning. A file that exists but cannot be read
/// (permissions, non-UTF-8) must not be mistaken for a missing one: overwriting it
/// would destroy user content.
pub(crate) fn read_for_planning(dir: &Path, rel: &str) -> Result<Option<String>, InitError> {
    match read_if(dir, rel) {
        Some(content) => Ok(Some(content)),
        None if dir.join(rel).exists() => Err(InitError::UnreadableFile {
            rel: rel.to_string(),
        }),
        None => Ok(None),
    }
}

/// The first `KEY=value` line for `key`: group 1 is the indent and optional
/// `export`, group 2 the raw value up to the line ending.
fn key_line(key: &str) -> regex::Regex {
    regex::Regex::new(&format!(
        r"(?m)^([ \t]*(?:export[ \t]+)?){}[ \t]*=([^\r\n]*)",
        regex::escape(key)
    ))
    .expect("escaped key regex")
}

/// Read one key out of a dotenv-style file: `KEY=value`, optional `export`, optional
/// quotes, optional trailing ` # comment`.
pub fn read_env_key(dir: &Path, rel: &str, key: &str) -> Option<String> {
    value_in(&read_if(dir, rel)?, key)
}

fn value_in(content: &str, key: &str) -> Option<String> {
    let captures = key_line(key).captures(content)?;
    Some(parse_value(&captures[2]).to_string())
}

fn parse_value(raw: &str) -> &str {
    let raw = raw.trim();
    for quote in ['"', '\''] {
        if let Some(quoted) = raw
            .strip_prefix(quote)
            .and_then(|rest| rest.find(quote).map(|end| &rest[..end]))
        {
            return quoted;
        }
    }
    let comment = raw
        .char_indices()
        .find(|&(i, c)| c == '#' && raw[..i].ends_with([' ', '\t']))
        .map_or(raw.len(), |(i, _)| i);
    raw[..comment].trim_end()
}

/// `lines` appended after `content` with one blank line between, in the file's own
/// line ending.
fn append_block(content: &str, lines: &[String]) -> String {
    let nl = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut out = content.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(nl);
    }
    out.push_str(nl);
    for line in lines {
        out.push_str(line);
        out.push_str(nl);
    }
    out
}

/// Swap `key`'s line for `assignment`, keeping its indent, `export` and line ending.
fn replace_value(content: &str, key: &str, assignment: &str) -> String {
    key_line(key)
        .replace(content, |captures: &regex::Captures| {
            format!("{}{assignment}", &captures[1])
        })
        .into_owned()
}

fn env_assignment(key: &str, value: &str) -> Result<String, InitError> {
    // Literal quoting keeps dotenv readers and the MCP shell in agreement.
    let quote = if value.contains(['$', '`', '\\', '"']) {
        '\''
    } else {
        '"'
    };
    if value.contains(quote) || value.contains(['\n', '\r']) {
        return Err(InitError::InvalidEnvValue {
            key: key.to_string(),
        });
    }
    Ok(format!("{key}={quote}{value}{quote}"))
}

/// Set a key in a dotenv-style file. Appends with a provenance comment; an existing
/// key is never clobbered - same value reads as unchanged, a different one is kept.
pub(crate) fn plan_env_set(
    dir: &Path,
    rel: &str,
    key: &str,
    value: &str,
) -> Result<FileAction, InitError> {
    let line = env_assignment(key, value)?;
    let Some(content) = read_for_planning(dir, rel)? else {
        return Ok(FileAction::Write {
            rel: rel.to_string(),
            content: format!("{PROVENANCE}\n{line}\n"),
            status: Status::Created,
            note: String::new(),
        });
    };
    match value_in(&content, key) {
        Some(existing) if existing == value => Ok(FileAction::Unchanged {
            rel: rel.to_string(),
        }),
        Some(_) => Ok(FileAction::Kept {
            rel: rel.to_string(),
            note: format!(
                "existing {key} left untouched (ours would be {})",
                mask_url(value)
            ),
        }),
        None => Ok(FileAction::Write {
            rel: rel.to_string(),
            content: append_block(&content, &[PROVENANCE.to_string(), line]),
            status: Status::Updated,
            note: String::new(),
        }),
    }
}

/// Like [`plan_env_set`], but with explicit consent to supersede: a different
/// existing value is replaced in place and the note names the old one (masked).
/// Used only when the user explicitly chose a database source.
pub(crate) fn plan_env_replace(
    dir: &Path,
    rel: &str,
    key: &str,
    value: &str,
) -> Result<FileAction, InitError> {
    let content = read_for_planning(dir, rel)?.unwrap_or_default();
    match value_in(&content, key) {
        Some(old) if old != value => {
            let assignment = env_assignment(key, value)?;
            Ok(FileAction::Write {
                rel: rel.to_string(),
                content: replace_value(&content, key, &assignment),
                status: Status::Updated,
                note: format!("{key} replaced (was {})", mask_url(&old)),
            })
        }
        _ => plan_env_set(dir, rel, key, value),
    }
}

/// Set several keys as one provenance block. Never-clobber per key: absent keys are
/// added, present-and-identical ignored, a placeholder filled in place, anything
/// else different kept (named, never echoed). `base` is the file content this block
/// plans against - callers planning
/// several blocks into one file thread each Write's content into the next call, so
/// later blocks see earlier ones instead of a stale disk read.
pub(crate) fn plan_env_set_block(
    dir: &Path,
    rel: &str,
    base: Option<String>,
    entries: &[(String, String)],
) -> Result<(FileAction, Option<String>), InitError> {
    let content = match base {
        Some(content) => Some(content),
        None => read_for_planning(dir, rel)?,
    };
    let mut current = content.clone().unwrap_or_default();
    let mut added = Vec::new();
    let mut filled = Vec::new();
    let mut kept = Vec::new();
    let mut lines = vec![PROVENANCE.to_string()];
    for (key, value) in entries {
        match value_in(&current, key) {
            Some(existing) if existing == *value => {}
            Some(existing) if !is_configured(&existing) && is_configured(value) => {
                current = replace_value(&current, key, &env_assignment(key, value)?);
                filled.push(key.as_str());
            }
            Some(_) => kept.push(key.as_str()),
            None => {
                added.push(key.as_str());
                lines.push(env_assignment(key, value)?);
            }
        }
    }
    if !added.is_empty() || !filled.is_empty() {
        let new_content = match &content {
            None => format!("{}\n", lines.join("\n")),
            Some(_) if added.is_empty() => current,
            Some(_) => append_block(&current, &lines),
        };
        let mut note = Vec::new();
        if !added.is_empty() {
            note.push(added.join(", "));
        }
        if !filled.is_empty() {
            note.push(format!("{} filled in", filled.join(", ")));
        }
        return Ok((
            FileAction::Write {
                rel: rel.to_string(),
                content: new_content.clone(),
                status: if content.is_none() {
                    Status::Created
                } else {
                    Status::Updated
                },
                note: note.join("; "),
            },
            Some(new_content),
        ));
    }
    if !kept.is_empty() {
        return Ok((
            FileAction::Kept {
                rel: rel.to_string(),
                note: format!("existing {} left untouched", kept.join(", ")),
            },
            content,
        ));
    }
    Ok((
        FileAction::Unchanged {
            rel: rel.to_string(),
        },
        content,
    ))
}

/// Make sure `.gitignore` covers `.env` before credentials land in it.
pub(crate) fn plan_gitignore_env(dir: &Path) -> Result<FileAction, InitError> {
    let content = read_for_planning(dir, ".gitignore")?;
    let covered = content
        .as_deref()
        .unwrap_or("")
        .lines()
        .any(|line| matches!(line.trim(), ".env" | ".env*" | "*.env"));
    if covered {
        return Ok(FileAction::Unchanged {
            rel: ".gitignore".to_string(),
        });
    }
    let status = if content.is_none() {
        Status::Created
    } else {
        Status::Updated
    };
    Ok(FileAction::Write {
        rel: ".gitignore".to_string(),
        content: append_block(
            content.as_deref().unwrap_or_default(),
            &[
                format!("{PROVENANCE} - never commit credentials"),
                ".env".to_string(),
            ],
        ),
        status,
        note: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn read_env_key_handles_export_spaces_and_quotes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".env"),
            "# comment\nexport REDIS_URL = \"redis://localhost:6379\"\nOTHER='x'\nBARE=y\n",
        )
        .unwrap();
        let read = |key| read_env_key(dir.path(), ".env", key);
        assert_eq!(read("REDIS_URL").as_deref(), Some("redis://localhost:6379"));
        assert_eq!(read("OTHER").as_deref(), Some("x"));
        assert_eq!(read("BARE").as_deref(), Some("y"));
        assert_eq!(read("MISSING"), None);
    }

    #[test]
    fn env_set_creates_the_file_with_a_provenance_comment() {
        let dir = tempfile::tempdir().unwrap();
        let action =
            plan_env_set(dir.path(), ".env", "REDIS_URL", "redis://localhost:6379").unwrap();
        let change = action.perform(dir.path()).unwrap();
        assert_eq!(change.status, Status::Created);
        assert_eq!(
            fs::read_to_string(dir.path().join(".env")).unwrap(),
            "# Added by redisctl init\nREDIS_URL=\"redis://localhost:6379\"\n"
        );
    }

    #[test]
    fn env_set_appends_without_touching_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), "EXISTING=1").unwrap();
        plan_env_set(dir.path(), ".env", "REDIS_URL", "redis://localhost:6379")
            .unwrap()
            .perform(dir.path())
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(".env")).unwrap(),
            "EXISTING=1\n\n# Added by redisctl init\nREDIS_URL=\"redis://localhost:6379\"\n"
        );
    }

    #[test]
    fn env_set_same_value_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".env"),
            "REDIS_URL=\"redis://localhost:6379\"\n",
        )
        .unwrap();
        let action =
            plan_env_set(dir.path(), ".env", "REDIS_URL", "redis://localhost:6379").unwrap();
        assert_eq!(action.preview().status, Status::Unchanged);
    }

    #[test]
    fn env_replace_swaps_the_value_and_masks_the_old_one_in_the_note() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".env"),
            "A=1\nREDIS_URL=\"redis://old:secret@h:1\"\nB=2\n",
        )
        .unwrap();
        let action = plan_env_replace(dir.path(), ".env", "REDIS_URL", "redis://new:6379").unwrap();
        let FileAction::Write {
            content,
            status,
            note,
            ..
        } = action
        else {
            panic!("expected a write");
        };
        assert_eq!(status, Status::Updated);
        assert!(
            content.contains("REDIS_URL=\"redis://new:6379\""),
            "{content}"
        );
        assert!(!content.contains("secret"), "{content}");
        assert!(
            content.contains("A=1") && content.contains("B=2"),
            "{content}"
        );
        assert!(note.contains("redis://old:****@h:1"), "{note}");
        assert!(!note.contains("secret"), "{note}");
    }

    #[test]
    fn env_replace_reads_unchanged_on_same_value_and_appends_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), "REDIS_URL=\"redis://h:1\"\n").unwrap();
        assert!(matches!(
            plan_env_replace(dir.path(), ".env", "REDIS_URL", "redis://h:1").unwrap(),
            FileAction::Unchanged { .. }
        ));
        let fresh = tempfile::tempdir().unwrap();
        assert!(matches!(
            plan_env_replace(fresh.path(), ".env", "REDIS_URL", "redis://h:1").unwrap(),
            FileAction::Write {
                status: Status::Created,
                ..
            }
        ));
    }

    #[test]
    fn env_set_never_clobbers_a_different_value_and_masks_ours_in_the_note() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), "REDIS_URL=\"redis://keep-me:1\"\n").unwrap();
        let action = plan_env_set(
            dir.path(),
            ".env",
            "REDIS_URL",
            "redis://default:secret@h:2",
        )
        .unwrap();
        let change = action.perform(dir.path()).unwrap();
        assert_eq!(change.status, Status::Kept);
        assert!(
            change.note.contains("redis://default:****@h:2"),
            "{}",
            change.note
        );
        assert!(!change.note.contains("secret"));
        assert_eq!(
            fs::read_to_string(dir.path().join(".env")).unwrap(),
            "REDIS_URL=\"redis://keep-me:1\"\n"
        );
    }

    #[test]
    fn an_existing_but_unreadable_file_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), [0xff, 0xfe, 0x00]).unwrap();
        let err = plan_env_set(dir.path(), ".env", "REDIS_URL", "redis://h:1").unwrap_err();
        assert!(err.to_string().contains("refusing to overwrite"), "{err}");
        assert_eq!(
            fs::read(dir.path().join(".env")).unwrap(),
            [0xff, 0xfe, 0x00]
        );
    }

    #[test]
    fn gitignore_gains_env_once_and_respects_globs() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "node_modules\n").unwrap();
        plan_gitignore_env(dir.path())
            .unwrap()
            .perform(dir.path())
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "node_modules\n\n# Added by redisctl init - never commit credentials\n.env\n"
        );

        let glob_dir = tempfile::tempdir().unwrap();
        fs::write(glob_dir.path().join(".gitignore"), "*.env\n").unwrap();
        let action = plan_gitignore_env(glob_dir.path()).unwrap();
        assert_eq!(action.preview().status, Status::Unchanged);
    }

    #[test]
    fn missing_gitignore_is_created() {
        let dir = tempfile::tempdir().unwrap();
        plan_gitignore_env(dir.path())
            .unwrap()
            .perform(dir.path())
            .unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join(".gitignore")).unwrap(),
            "\n# Added by redisctl init - never commit credentials\n.env\n"
        );
    }
}

#[cfg(test)]
mod block_tests {
    use super::*;

    fn write_block(
        dir: &Path,
        base: Option<String>,
        entries: &[(&str, &str)],
    ) -> (FileAction, Option<String>) {
        let owned: Vec<(String, String)> = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        plan_env_set_block(dir, ".env", base, &owned).unwrap()
    }

    #[test]
    fn blocks_thread_content_so_a_second_product_sees_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let (first, carried) = write_block(
            dir.path(),
            None,
            &[
                ("AGENT_MEMORY_URL", "https://m"),
                ("AGENT_MEMORY_API_KEY", "<paste-from-redis-cloud>"),
            ],
        );
        let FileAction::Write { note, .. } = &first else {
            panic!("expected write");
        };
        assert_eq!(note, "AGENT_MEMORY_URL, AGENT_MEMORY_API_KEY");
        let (second, carried) = write_block(
            dir.path(),
            carried,
            &[("LANGCACHE_URL", "https://l"), ("LANGCACHE_API_KEY", "k")],
        );
        let FileAction::Write { content, .. } = &second else {
            panic!("expected write");
        };
        // One provenance block per product, both surviving in the final content.
        assert_eq!(content.matches("# Added by redisctl init").count(), 2);
        assert!(
            content.contains("AGENT_MEMORY_URL=\"https://m\""),
            "{content}"
        );
        assert!(content.contains("LANGCACHE_API_KEY=\"k\""), "{content}");
        assert!(carried.is_some());
    }

    #[test]
    fn existing_keys_are_kept_by_name_and_never_echoed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "LANGCACHE_API_KEY=\"s3cret\"\n").unwrap();
        let (action, _) = write_block(dir.path(), None, &[("LANGCACHE_API_KEY", "other")]);
        let FileAction::Kept { note, .. } = &action else {
            panic!("expected kept, got {action:?}");
        };
        assert_eq!(note, "existing LANGCACHE_API_KEY left untouched");
        assert!(!note.contains("s3cret") && !note.contains("other"));
    }

    #[test]
    fn identical_values_read_as_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "A=\"1\"\n").unwrap();
        let (action, _) = write_block(dir.path(), None, &[("A", "1")]);
        assert!(matches!(action, FileAction::Unchanged { .. }), "{action:?}");
    }
}

#[cfg(test)]
mod line_tests {
    use super::*;
    use std::fs;

    fn read_all(content: &str, key: &str) -> Option<String> {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), content).unwrap();
        read_env_key(dir.path(), ".env", key)
    }

    #[test]
    fn read_env_key_follows_dotenv_quoting_and_comments() {
        for (content, expected) in [
            ("K=\"a#b\"\n", "a#b"),
            ("K='a#b c'\n", "a#b c"),
            ("K=\"quoted\" # note\n", "quoted"),
            ("K='single' # note\n", "single"),
            ("K=bare # note\n", "bare"),
            ("K=pa#ss\n", "pa#ss"),
            ("K=value   \n", "value"),
            ("K=\"value\"\r\n", "value"),
            ("K=bare\r\nL=2\r\n", "bare"),
            ("export\tK=exported\n", "exported"),
            ("  export K = spaced\n", "spaced"),
            ("# K=commented\nK=real\n", "real"),
        ] {
            assert_eq!(
                read_all(content, "K").as_deref(),
                Some(expected),
                "{content:?}"
            );
        }
        assert_eq!(read_all("KEY=1\nOTHER_K=2\n", "K"), None);
    }

    fn replaced(content: &str) -> String {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), content).unwrap();
        let FileAction::Write { content, .. } =
            plan_env_replace(dir.path(), ".env", "REDIS_URL", "redis://new:1").unwrap()
        else {
            panic!("expected a write");
        };
        content
    }

    #[test]
    fn env_replace_changes_only_the_keys_line() {
        for (before, after) in [
            (
                "A=1\n\nREDIS_URL=redis://old\nB=2\n",
                "A=1\n\nREDIS_URL=\"redis://new:1\"\nB=2\n",
            ),
            (
                "A=1\r\n\r\nREDIS_URL=redis://old\r\nB=2\r\n",
                "A=1\r\n\r\nREDIS_URL=\"redis://new:1\"\r\nB=2\r\n",
            ),
            (
                "A=1\n  export REDIS_URL=\"redis://old\"\nB=2",
                "A=1\n  export REDIS_URL=\"redis://new:1\"\nB=2",
            ),
            (
                "REDIS_URL=redis://old\nREDIS_URL=redis://second\n",
                "REDIS_URL=\"redis://new:1\"\nREDIS_URL=redis://second\n",
            ),
        ] {
            assert_eq!(replaced(before), after, "{before:?}");
        }
    }

    #[test]
    fn appends_keep_the_files_line_ending() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), "A=1\r\n").unwrap();
        let FileAction::Write { content, .. } =
            plan_env_set(dir.path(), ".env", "REDIS_URL", "redis://h:1").unwrap()
        else {
            panic!("expected a write");
        };
        assert_eq!(
            content,
            "A=1\r\n\r\n# Added by redisctl init\r\nREDIS_URL=\"redis://h:1\"\r\n"
        );
        let entries = [("LANGCACHE_URL".to_string(), "https://l".to_string())];
        let (FileAction::Write { content, .. }, _) =
            plan_env_set_block(dir.path(), ".env", None, &entries).unwrap()
        else {
            panic!("expected a write");
        };
        assert_eq!(
            content,
            "A=1\r\n\r\n# Added by redisctl init\r\nLANGCACHE_URL=\"https://l\"\r\n"
        );
    }

    #[test]
    fn a_block_fills_a_placeholder_in_place_and_names_it() {
        let dir = tempfile::tempdir().unwrap();
        let before = "A=1\r\n\r\nexport LANGCACHE_API_KEY=\"<paste-from-redis-cloud>\"\r\nB=2\r\n";
        fs::write(dir.path().join(".env"), before).unwrap();
        let entries = [("LANGCACHE_API_KEY".to_string(), "S3cretKey".to_string())];
        let (FileAction::Write { content, note, .. }, _) =
            plan_env_set_block(dir.path(), ".env", None, &entries).unwrap()
        else {
            panic!("expected a write");
        };
        assert_eq!(
            content,
            "A=1\r\n\r\nexport LANGCACHE_API_KEY=\"S3cretKey\"\r\nB=2\r\n"
        );
        assert_eq!(note, "LANGCACHE_API_KEY filled in");
    }
}
