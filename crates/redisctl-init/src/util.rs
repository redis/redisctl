//! Small shared helpers: credential masking, binary lookup, project file reads.

use std::path::Path;
use std::sync::OnceLock;

/// Mask the password in URL-shaped text - deliberately broader than valid redis://
/// URLs, because rejected input gets echoed in error messages.
pub fn mask_url(url: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r#"([A-Za-z][A-Za-z0-9+.-]*://[^:@/\s"']*|[^:@\s/"']+):[^@\s"']+@"#)
            .expect("static regex")
    });
    re.replace_all(url, "$1:****@").into_owned()
}

/// Whether a binary is on PATH.
pub fn has_bin(bin: &str) -> bool {
    which::which(bin).is_ok()
}

pub(crate) struct ShOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Run a command and capture its output; a spawn failure reads as status -1.
pub(crate) fn sh(cmd: &str, args: &[&str]) -> ShOutput {
    match std::process::Command::new(cmd).args(args).output() {
        Ok(out) => ShOutput {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => ShOutput {
            status: -1,
            stdout: String::new(),
            stderr: e.to_string(),
        },
    }
}

/// Lowercase, runs of anything non-alphanumeric collapsed to one dash, edges trimmed.
pub(crate) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "project".to_string()
    } else {
        out.to_string()
    }
}

/// Appending to a file whose last line has no newline would splice onto it.
pub(crate) fn ending_with_newline(content: &str) -> String {
    if content.is_empty() || content.ends_with('\n') {
        content.to_string()
    } else {
        format!("{content}\n")
    }
}

/// Whether `rel` exists under `dir`.
pub fn exists(dir: &Path, rel: &str) -> bool {
    dir.join(rel).exists()
}

/// The content of `rel` under `dir`, or `None` when it does not exist or cannot be read.
pub fn read_if(dir: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(rel)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_url_hides_the_password() {
        assert_eq!(
            mask_url("redis://default:s3cret@host.example:12000"),
            "redis://default:****@host.example:12000"
        );
    }

    #[test]
    fn mask_url_handles_empty_username() {
        assert_eq!(mask_url("rediss://:pw@h:1"), "rediss://:****@h:1");
    }

    #[test]
    fn mask_url_leaves_passwordless_urls_alone() {
        assert_eq!(mask_url("redis://localhost:6379"), "redis://localhost:6379");
    }

    #[test]
    fn mask_url_masks_inside_larger_text() {
        assert_eq!(
            mask_url("connect via redis://u:pw@h:1 please"),
            "connect via redis://u:****@h:1 please"
        );
    }

    #[test]
    fn mask_url_masks_unknown_schemes() {
        assert_eq!(
            mask_url("redisx://default:secret@host:6379"),
            "redisx://default:****@host:6379"
        );
        assert_eq!(
            mask_url("https://user:token@example.com/path"),
            "https://user:****@example.com/path"
        );
    }

    #[test]
    fn mask_url_masks_bare_userinfo_without_a_scheme() {
        assert_eq!(
            mask_url("default:secret@host:6379"),
            "default:****@host:6379"
        );
    }

    #[test]
    fn slug_collapses_and_trims() {
        assert_eq!(slug("My App (v2)"), "my-app-v2");
        assert_eq!(slug("--hello--"), "hello");
        assert_eq!(slug("!!!"), "project");
    }

    #[test]
    fn ending_with_newline_appends_only_when_missing() {
        assert_eq!(ending_with_newline("a\n"), "a\n");
        assert_eq!(ending_with_newline("a"), "a\n");
        assert_eq!(ending_with_newline(""), "");
    }
}
