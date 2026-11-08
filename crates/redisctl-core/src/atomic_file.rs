//! Atomic replacement for project files, preserving existing permissions.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct Temporary(PathBuf);

impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Write through a fresh sibling, sync it, then rename it over the destination.
/// Existing symlinks are followed so the link itself survives. Callers remain responsible
/// for deciding which symlinks are allowed. `private` gives new files mode 0600 on Unix;
/// existing files retain their permissions.
pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    write_with(path, private, |file| file.write_all(bytes))
}

fn write_with(
    path: &Path,
    private: bool,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let path = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path)?,
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(error),
    };
    let permissions = match fs::metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(io::Error::other("destination is not a regular file"));
            }
            Some(metadata.permissions())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let (temporary, mut file) = loop {
        let candidate = parent.join(format!(
            ".redisctl-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // Never expose a partial copy of an existing private file.
            options.mode(if private || permissions.is_some() {
                0o600
            } else {
                0o666
            });
        }
        #[cfg(not(unix))]
        let _ = private;
        match options.open(&candidate) {
            Ok(file) => break (Temporary(candidate), file),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    write(&mut file)?;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary.0, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_write_preserves_original_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        fs::write(&path, "SECRET=original\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        }
        assert!(
            write_with(&path, true, |file| {
                file.write_all(b"partial")?;
                Err(io::Error::other("injected disk failure"))
            })
            .is_err()
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "SECRET=original\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn preserves_mode_and_symlink_and_creates_private_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        write_atomic(&path, b"first", true).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.path().join("link");
        symlink(".env", &link).unwrap();
        write_atomic(&link, b"second", true).unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn rename_failure_removes_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target");
        assert!(
            write_with(&path, false, |_| {
                fs::create_dir(&path)?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
