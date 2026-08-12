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
}
