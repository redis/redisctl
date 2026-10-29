//! Synthetic, offline compatibility checks for the module ZIP dependency.
//! No module is loaded, uploaded or extracted. Each child has an isolated config.
#![cfg(feature = "enterprise")]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

fn command(directory: &TempDir) -> Command {
    let mut command = Command::cargo_bin("redisctl").unwrap();
    command
        .env_clear()
        .env("HOME", directory.path())
        .env("USERPROFILE", directory.path())
        .env("XDG_CONFIG_HOME", directory.path())
        .current_dir(directory.path())
        .arg("--config-file")
        .arg(directory.path().join("config.toml"))
        .args(["--output", "json", "enterprise", "module"])
        .timeout(Duration::from_secs(5));
    command
}

fn inspect(directory: &TempDir, path: &Path) -> Value {
    let assertion = command(directory)
        .arg("inspect")
        .arg(path)
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    serde_json::from_slice(&assertion.get_output().stdout).unwrap()
}

fn write_archive(path: &Path, entries: &[(&str, &[u8])], compression: CompressionMethod) {
    let mut writer = ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(compression),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

#[test]
fn package_preserves_entries_bytes_and_inspection_contract() {
    let directory = TempDir::new().unwrap();
    let module = directory.path().join("fixture.so");
    let metadata = directory.path().join("input.json");
    let archive = directory.path().join("nested/package.zip");
    let module_bytes: Vec<u8> = (0..=255).collect();
    let metadata_bytes = serde_json::to_vec_pretty(&json!({
        "module_name":"fixture", "display_name":"Synthetic 世界", "version":100,
        "semantic_version":"1.0.0", "min_redis_version":"8.0.0",
        "compatible_redis_version":"8.0.0", "capabilities":["no_multi"],
        "commands":[{"command_name":"FIXTURE.GET", "flags":["readonly"]}]
    }))
    .unwrap();
    fs::write(&module, &module_bytes).unwrap();
    fs::write(&metadata, &metadata_bytes).unwrap();
    let assertion = command(&directory)
        .arg("package")
        .arg("--module")
        .arg(&module)
        .arg("--metadata")
        .arg(&metadata)
        .arg("--out")
        .arg(&archive)
        .arg("--validate")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let report: Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(
        report,
        json!({"status":"success", "output":archive,
        "size":fs::metadata(&archive).unwrap().len(), "files":["module.json","fixture.so"]})
    );
    let mut reader = ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
    assert_eq!(reader.len(), 2);
    for (index, (name, expected)) in [
        ("module.json", metadata_bytes.as_slice()),
        ("fixture.so", module_bytes.as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        let mut entry = reader.by_index(index).unwrap();
        assert_eq!(entry.name().unwrap(), name);
        assert_eq!(entry.compression(), CompressionMethod::Deflated);
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
    assert_eq!(
        inspect(&directory, &archive),
        json!({
            "file":archive, "valid":true, "files":[
                {"name":"module.json","size":metadata_bytes.len()},
                {"name":"fixture.so","size":module_bytes.len()}],
            "has_module_json":true,"has_so_file":true,"so_file":"fixture.so",
            "has_subdirectories":false,"metadata":{
                "module_name":"fixture","display_name":"Synthetic 世界","version":100,
                "semantic_version":"1.0.0","min_redis_version":"8.0.0",
                "compatible_redis_version":"8.0.0","commands_count":1,"capabilities":["no_multi"]}
        })
    );
    command(&directory)
        .arg("inspect")
        .arg(&archive)
        .args(["--full", "--output", "table"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty())
        .stdout(
            predicate::str::contains("FIXTURE.GET")
                .and(predicate::str::contains("Package structure is valid")),
        );
    command(&directory)
        .arg("validate")
        .arg(&metadata)
        .arg("--strict")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    assert_eq!(fs::read(module).unwrap(), module_bytes);
    assert_eq!(fs::read(metadata).unwrap(), metadata_bytes);
    assert!(!directory.path().join("config.toml").exists());
}

#[test]
fn inspect_reads_an_independent_stored_archive() {
    // Python stdlib zipfile, ZIP_STORED, fixed 2020-01-01 timestamp, mode 0644.
    // Kept as bytes so upgrading our writer cannot silently rewrite the input fixture.
    const HEX: &str = concat!(
        "504b03041400000000000000215015cd305c25000000250000000b0000006d6f64756c652e6a736f6e7b226d6f64756c",
        "655f6e616d65223a2266697874757265222c2276657273696f6e223a317d504b030414000000000000002150cfd94382",
        "0b0000000b0000000a000000666978747572652e736f0073796e746865746963ff504b01021403140000000000000021",
        "5015cd305c25000000250000000b0000000000000000000000a481000000006d6f64756c652e6a736f6e504b01021403",
        "14000000000000002150cfd943820b0000000b0000000a0000000000000000000000a4814e000000666978747572652e",
        "736f504b0506000000000200020071000000810000000000",
    );
    let bytes: Vec<u8> = (0..HEX.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&HEX[offset..offset + 2], 16).unwrap())
        .collect();
    let directory = TempDir::new().unwrap();
    let archive = directory.path().join("external.zip");
    fs::write(&archive, &bytes).unwrap();
    let report = inspect(&directory, &archive);
    assert_eq!(report["valid"], true);
    assert_eq!(report["metadata"]["version"], 1);
    assert_eq!(
        report["files"],
        json!([
        {"name":"module.json","size":37},{"name":"fixture.so","size":11}])
    );
    assert_eq!(fs::read(archive).unwrap(), bytes);
    assert!(!directory.path().join("module.json").exists());
    assert!(!directory.path().join("fixture.so").exists());
}

#[test]
fn inspect_preserves_existing_compression_support() {
    let directory = TempDir::new().unwrap();
    for compression in [
        CompressionMethod::Stored,
        CompressionMethod::Deflated,
        CompressionMethod::Bzip2,
        CompressionMethod::Zstd,
        CompressionMethod::Xz,
    ] {
        let archive = directory.path().join("compressed.zip");
        write_archive(
            &archive,
            &[
                ("module.json", br#"{"module_name":"fixture"}"#),
                ("fixture.so", b"synthetic, never loaded"),
            ],
            compression,
        );
        let before = fs::read(&archive).unwrap();
        let report = inspect(&directory, &archive);
        assert_eq!(report["valid"], true, "{compression:?}");
        assert_eq!(
            report["metadata"]["module_name"], "fixture",
            "{compression:?}"
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
    }
}

#[test]
fn inspect_reads_utf8_and_unambiguous_legacy_cp437_names() {
    let directory = TempDir::new().unwrap();
    let archive = directory.path().join("names.zip");
    write_archive(
        &archive,
        &[
            ("module.json", br#"{"module_name":"fixture"}"#),
            ("á.so", b"synthetic"),
        ],
        CompressionMethod::Stored,
    );
    assert_eq!(inspect(&directory, &archive)["so_file"], "á.so");

    write_archive(
        &archive,
        &[
            ("module.json", br#"{"module_name":"fixture"}"#),
            ("x.so", b"synthetic"),
        ],
        CompressionMethod::Stored,
    );
    let mut bytes = fs::read(&archive).unwrap();
    let positions: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(index, part)| (part == b"x.so").then_some(index))
        .collect();
    assert_eq!(positions.len(), 2); // local and central names; ASCII writer left UTF-8 flag clear
    for index in positions {
        bytes[index] = 0x82; // CP437 é: not a complete UTF-8 sequence
    }
    fs::write(&archive, &bytes).unwrap();
    let report = inspect(&directory, &archive);
    assert_eq!(report["so_file"], "é.so");
    assert_eq!(report["valid"], true);
    assert_eq!(fs::read(archive).unwrap(), bytes);
    assert!(!directory.path().join("é.so").exists());
}

#[test]
fn inspect_prefers_valid_utf8_for_ambiguous_legacy_names() {
    let directory = TempDir::new().unwrap();
    let archive = directory.path().join("ambiguous-name.zip");
    write_archive(
        &archive,
        &[
            ("module.json", br#"{"module_name":"fixture"}"#),
            ("xx.so", b"synthetic"),
        ],
        CompressionMethod::Stored,
    );
    let mut bytes = fs::read(&archive).unwrap();
    let positions: Vec<_> = bytes
        .windows(5)
        .enumerate()
        .filter_map(|(index, part)| (part == b"xx.so").then_some(index))
        .collect();
    assert_eq!(positions.len(), 2);
    // Confirm that neither header marks the ASCII placeholder as UTF-8.
    for (index, name_offset, flags_offset, signature) in [
        (positions[0], 30, 6, b"PK\x03\x04"),
        (positions[1], 46, 8, b"PK\x01\x02"),
    ] {
        let header = index - name_offset;
        assert_eq!(&bytes[header..header + 4], signature);
        let flags = u16::from_le_bytes([
            bytes[header + flags_offset],
            bytes[header + flags_offset + 1],
        ]);
        assert_eq!(flags & (1 << 11), 0);
        // Valid UTF-8 á, but CP437 ├í: ZIP 9 deliberately prefers UTF-8 here.
        bytes[index..index + 2].copy_from_slice(&[0xc3, 0xa1]);
    }
    fs::write(&archive, &bytes).unwrap();
    let report = inspect(&directory, &archive);
    assert_eq!(report["so_file"], "á.so");
    assert_eq!(report["files"][1], json!({"name":"á.so","size":9}));
    assert_eq!(report["valid"], true);
    assert_eq!(fs::read(archive).unwrap(), bytes);
    assert!(!directory.path().join("á.so").exists());
    assert!(!directory.path().join("├í.so").exists());
    assert!(!directory.path().join("module.json").exists());
}

#[test]
fn corrupt_metadata_crc_is_a_controlled_failure_without_extraction() {
    let directory = TempDir::new().unwrap();
    let archive = directory.path().join("crc.zip");
    let metadata = br#"{"module_name":"fixture"}"#;
    write_archive(
        &archive,
        &[("module.json", metadata), ("fixture.so", b"synthetic")],
        CompressionMethod::Stored,
    );
    let mut bytes = fs::read(&archive).unwrap();
    let offset = bytes
        .windows(metadata.len())
        .position(|part| part == metadata)
        .unwrap();
    bytes[offset + 1] = b'!'; // leave both recorded CRCs unchanged
    fs::write(&archive, &bytes).unwrap();
    let assertion = command(&directory)
        .arg("inspect")
        .arg(&archive)
        .assert()
        .failure()
        .stdout(predicate::str::is_empty());
    let diagnostic: Value = serde_json::from_slice(&assertion.get_output().stderr).unwrap();
    assert!(diagnostic["error"]["message"].is_string());
    assert!(!directory.path().join("module.json").exists());
    assert!(!directory.path().join("fixture.so").exists());
    assert_eq!(fs::read(archive).unwrap(), bytes);
}

#[test]
fn inspect_reports_missing_and_nested_entries_without_extracting() {
    let directory = TempDir::new().unwrap();
    let archive = directory.path().join("incomplete.zip");
    write_archive(
        &archive,
        &[("fixture.so", b"synthetic")],
        CompressionMethod::Stored,
    );
    let report = inspect(&directory, &archive);
    assert_eq!(report["valid"], false);
    assert_eq!(report["has_module_json"], false);
    write_archive(
        &archive,
        &[
            ("module.json", br#"{"module_name":"fixture"}"#),
            ("nested/fixture.so", b"synthetic"),
        ],
        CompressionMethod::Deflated,
    );
    let report = inspect(&directory, &archive);
    assert_eq!(report["valid"], false);
    assert_eq!(report["has_subdirectories"], true);
    assert!(!directory.path().join("nested").exists());
    assert!(!directory.path().join("module.json").exists());
    assert!(!directory.path().join("config.toml").exists());
}

#[test]
fn invalid_archive_stays_a_structured_stderr_failure() {
    let directory = TempDir::new().unwrap();
    let archive = directory.path().join("invalid.zip");
    fs::write(&archive, b"not a zip archive").unwrap();
    let assertion = command(&directory)
        .arg("inspect")
        .arg(&archive)
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty());
    let error: Value = serde_json::from_slice(&assertion.get_output().stderr).unwrap();
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Failed to read zip archive")
    );
    assert_eq!(fs::read(archive).unwrap(), b"not a zip archive");
}

#[test]
fn invalid_metadata_is_rejected_before_package_creation() {
    let directory = TempDir::new().unwrap();
    let module = directory.path().join("fixture.so");
    let metadata = directory.path().join("invalid.json");
    let archive = directory.path().join("must-not-exist/package.zip");
    fs::write(&module, b"synthetic").unwrap();
    fs::write(&metadata, br#"{"module_name":""}"#).unwrap();
    command(&directory)
        .arg("package")
        .arg("--module")
        .arg(&module)
        .arg("--metadata")
        .arg(&metadata)
        .arg("--out")
        .arg(&archive)
        .arg("--validate")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("Module name is required"))
        .stderr(predicate::str::contains(
            "Module metadata validation failed",
        ));
    assert!(!archive.parent().unwrap().exists());
    assert_eq!(fs::read(module).unwrap(), b"synthetic");
    assert_eq!(fs::read(metadata).unwrap(), br#"{"module_name":""}"#);
}
