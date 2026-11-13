//! Regression coverage for the rustix/xattr size-query contract.
//!
//! Run explicitly on a Unix filesystem that supports extended-attribute values
//! and name lists larger than 4 KiB. Linux CI uses /dev/shm; a caller can select
//! another filesystem with REDISCTL_XATTR_TEST_DIR. Unsupported filesystems fail.

#![cfg(unix)]

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

fn fixture() -> io::Result<(tempfile::TempDir, PathBuf)> {
    let directory = match std::env::var_os("REDISCTL_XATTR_TEST_DIR") {
        Some(root) => tempfile::tempdir_in(root)?,
        None => tempfile::tempdir()?,
    };
    let path = directory.path().join("attributes");
    std::fs::write(&path, [])?;
    Ok((directory, path))
}

#[test]
#[ignore = "requires a Unix filesystem supporting extended attributes larger than 4 KiB"]
fn large_value_roundtrips_exact_bytes() -> io::Result<()> {
    let (_directory, path) = fixture()?;
    let expected: Vec<u8> = (0..8192).map(|index| (index % 251) as u8).collect();
    xattr::set(&path, "user.redisctl-large-value", &expected)?;

    let actual = xattr::get(&path, "user.redisctl-large-value")?;
    assert_eq!(actual.as_ref().map(Vec::len), Some(expected.len()));
    assert_eq!(actual.as_deref(), Some(expected.as_slice()));
    Ok(())
}

#[test]
#[ignore = "requires a Unix filesystem supporting extended-attribute name lists larger than 4 KiB"]
fn large_name_list_preserves_every_name() -> io::Result<()> {
    let (_directory, path) = fixture()?;
    let mut expected: BTreeSet<OsString> = xattr::list(&path)?.collect();
    for index in 0..128 {
        let name = format!("user.redisctl-{index:03}-{}", "n".repeat(48));
        xattr::set(&path, &name, b"value")?;
        expected.insert(name.into());
    }
    let serialized_length: usize = expected
        .iter()
        .map(|name| name.as_os_str().as_bytes().len() + 1)
        .sum();
    assert!(serialized_length > 4096);

    let actual: BTreeSet<OsString> = xattr::list(&path)?.collect();
    assert_eq!(actual.len(), expected.len());
    assert_eq!(actual, expected);
    Ok(())
}

#[test]
#[ignore = "requires a Unix filesystem supporting extended attributes"]
fn empty_attribute_is_distinct_from_missing() -> io::Result<()> {
    let (_directory, path) = fixture()?;
    assert_eq!(xattr::get(&path, "user.redisctl-missing")?, None);
    xattr::set(&path, "user.redisctl-empty", b"")?;
    assert_eq!(xattr::get(&path, "user.redisctl-empty")?, Some(Vec::new()));
    assert_eq!(xattr::get(&path, "user.redisctl-missing")?, None);
    Ok(())
}
