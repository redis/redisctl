//! The `.env` / `.gitignore` contract: mutations are decided read-only at plan time
//! and performed at apply time, so a dry run renders exactly what a real run does.

use std::path::Path;

use crate::InitError;
use crate::change::{Change, Status};
use crate::util::{ending_with_newline, mask_url, read_if};

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

/// Read one key out of a dotenv-style file: `KEY=value`, optional `export`, optional
/// quotes.
pub fn read_env_key(dir: &Path, rel: &str, key: &str) -> Option<String> {
    let content = read_if(dir, rel)?;
    let re = regex::Regex::new(&format!(
        r"^\s*(?:export\s+)?{}\s*=\s*(.*)$",
        regex::escape(key)
    ))
    .expect("escaped key regex");
    for line in content.lines() {
        if let Some(captures) = re.captures(line) {
            return Some(strip_edge_quotes(captures[1].trim()).to_string());
        }
    }
    None
}

fn strip_edge_quotes(value: &str) -> &str {
    let value = value
        .strip_prefix('"')
        .or_else(|| value.strip_prefix('\''))
        .unwrap_or(value);
    value
        .strip_suffix('"')
        .or_else(|| value.strip_suffix('\''))
        .unwrap_or(value)
}

/// Set a key in a dotenv-style file. Appends with a provenance comment; an existing
/// key is never clobbered - same value reads as unchanged, a different one is kept.
pub(crate) fn plan_env_set(
    dir: &Path,
    rel: &str,
    key: &str,
    value: &str,
) -> Result<FileAction, InitError> {
    // Quoted so the .env stays shell-sourceable.
    let line = format!("{key}=\"{value}\"");
    let Some(content) = read_for_planning(dir, rel)? else {
        return Ok(FileAction::Write {
            rel: rel.to_string(),
            content: format!("# Added by redisctl init\n{line}\n"),
            status: Status::Created,
            note: String::new(),
        });
    };
    match read_env_key(dir, rel, key) {
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
            content: format!(
                "{}\n# Added by redisctl init\n{line}\n",
                ending_with_newline(&content)
            ),
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
    let existing = read_env_key(dir, rel, key);
    match existing {
        Some(old) if old != value => {
            let content = read_for_planning(dir, rel)?.unwrap_or_default();
            let re = regex::Regex::new(&format!(
                r"(?m)^\s*(?:export\s+)?{}\s*=.*$",
                regex::escape(key)
            ))
            .expect("escaped key regex");
            let replaced = re
                .replace(&content, format!("{key}=\"{value}\""))
                .into_owned();
            Ok(FileAction::Write {
                rel: rel.to_string(),
                content: replaced,
                status: Status::Updated,
                note: format!("{key} replaced (was {})", mask_url(&old)),
            })
        }
        _ => plan_env_set(dir, rel, key, value),
    }
}

/// Set several keys as one provenance block. Never-clobber per key: absent keys are
/// added, present-and-identical ignored, present-and-different kept (named, never
/// echoed). `base` is the file content this block plans against - callers planning
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
    let existing_value = |key: &str| {
        content.as_deref().and_then(|text| {
            text.lines().find_map(|line| {
                let line = line.trim_start();
                let line = line.strip_prefix("export ").unwrap_or(line);
                let (k, v) = line.split_once('=')?;
                (k.trim() == key).then(|| strip_edge_quotes(v.trim()).to_string())
            })
        })
    };
    let mut added = Vec::new();
    let mut kept = Vec::new();
    let mut lines = Vec::new();
    for (key, value) in entries {
        match existing_value(key) {
            Some(existing) if existing == *value => {}
            Some(_) => kept.push(key.as_str()),
            None => {
                added.push(key.as_str());
                lines.push(format!("{key}=\"{value}\""));
            }
        }
    }
    if !added.is_empty() {
        let block = format!("# Added by redisctl init\n{}\n", lines.join("\n"));
        let (status, new_content) = match &content {
            None => (Status::Created, block),
            Some(existing) => (
                Status::Updated,
                format!("{}\n{block}", ending_with_newline(existing).trim_end()),
            ),
        };
        return Ok((
            FileAction::Write {
                rel: rel.to_string(),
                content: new_content.clone(),
                status,
                note: added.join(", "),
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
    let base = content.map(|c| ending_with_newline(&c)).unwrap_or_default();
    Ok(FileAction::Write {
        rel: ".gitignore".to_string(),
        content: format!("{base}\n# Added by redisctl init - never commit credentials\n.env\n"),
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
