//! Docker-free regressions for the actual fixture error wrapper.

#[path = "support/docker_fixture.rs"]
mod docker_fixture;

use docker_fixture::{FixtureError, FixtureStage, fixture_error};
use docker_wrapper::Error as DockerError;
use docker_wrapper::template::TemplateError;

fn command_error(stderr: &str) -> TemplateError {
    DockerError::command_failed(
        "docker run --env SECRET_ARG=synthetic-argument-secret",
        125,
        "synthetic-stdout-secret",
        stderr,
    )
    .into()
}

fn assert_safe(error: &anyhow::Error) {
    for rendered in [
        error.to_string(),
        format!("{error:#}"),
        format!("{error:?}"),
        format!("{error:#?}"),
    ] {
        for secret in [
            "synthetic-argument-secret",
            "synthetic-stdout-secret",
            "synthetic-stderr-secret",
            "synthetic-password",
            "SECRET_ARG",
            "redis://",
            "\u{1b}",
        ] {
            assert!(!rendered.contains(secret), "unsafe error rendering");
        }
    }
    // Anyhow's Debug may append a caller-controlled backtrace in CI. Bound the
    // fixture message and its own Debug, not framework backtrace scaffolding.
    assert!(error.to_string().len() < 300);
    let typed = error.downcast_ref::<FixtureError>().unwrap();
    assert!(format!("{typed:?}").len() < 300);
    assert_eq!(error.chain().count(), 1, "raw upstream errors stay private");
}

#[test]
fn command_failure_retains_typed_evidence_but_only_prints_fixed_diagnostics() {
    let stderr = "address already in use: redis://user:synthetic-password@host\nsynthetic-stderr-secret\u{1b}[31m";
    let original = command_error(stderr);
    let legacy = format!("Failed to start container: {original}");
    // Reproduce both bad alternatives: Display loses useful evidence and
    // exposes arguments; a blanket Debug dump would additionally expose stderr.
    assert!(!legacy.contains("125"));
    assert!(!legacy.contains("address already in use"));
    assert!(legacy.contains("synthetic-argument-secret"));
    assert!(format!("{original:?}").contains("synthetic-password"));
    let error = fixture_error(FixtureStage::RedisStart, original);
    let rendered = error.to_string();
    assert!(rendered.contains("Redis startup"));
    assert!(rendered.contains("docker exit 125"));
    assert!(rendered.contains("cause=port-in-use"));
    assert_safe(&error);

    let typed = error.downcast_ref::<FixtureError>().unwrap();
    let TemplateError::DockerError(DockerError::CommandFailed {
        exit_code,
        stderr: original_stderr,
        ..
    }) = typed.original_error()
    else {
        panic!("typed command error was lost");
    };
    assert_eq!(*exit_code, 125);
    assert_eq!(original_stderr, stderr);
}

#[test]
fn known_causes_have_distinct_fixed_labels() {
    for (stderr, label) in [
        ("PORT IS ALREADY ALLOCATED", "port-in-use"),
        (
            "container name 'private-name' is already in use",
            "container-name-in-use",
        ),
        (
            "Cannot connect to the Docker daemon",
            "docker-daemon-unreachable",
        ),
        ("Is the docker daemon running?", "docker-daemon-unreachable"),
        ("pull access denied for private/image", "image-unavailable"),
        ("manifest unknown", "image-unavailable"),
        ("no matching manifest", "image-unavailable"),
        ("repository does not exist", "image-unavailable"),
        ("permission denied", "permission-denied"),
        ("arbitrary synthetic-stderr-secret", "unclassified"),
    ] {
        let error = fixture_error(FixtureStage::StackStart, command_error(stderr));
        assert!(error.to_string().contains(&format!("cause={label};")));
        assert_safe(&error);
    }
}

#[test]
fn oversized_and_utf8_stderr_have_bounded_inspection_without_echoing_a_suffix() {
    let stderr = format!(
        "{}🔒permission denied synthetic-stderr-secret",
        "x".repeat(4095)
    );
    let error = fixture_error(FixtureStage::StackPort, command_error(&stderr));
    let rendered = error.to_string();
    assert!(rendered.contains("Stack port resolution"));
    assert!(rendered.contains("cause=unclassified"));
    assert!(rendered.contains("stderr inspection=4095/"));
    assert!(rendered.contains("truncated"));
    assert_safe(&error);

    let huge = format!(
        "permission denied {}",
        "synthetic-stderr-secret".repeat(50_000)
    );
    let error = fixture_error(FixtureStage::RedisPort, command_error(&huge));
    assert!(error.to_string().contains("stderr inspection=4096/"));
    assert!(error.to_string().contains("cause=permission-denied"));
    assert_safe(&error);
}

#[test]
fn non_command_errors_keep_stage_and_category_without_arbitrary_details() {
    for (original, label) in [
        (
            TemplateError::DockerError(DockerError::DockerNotFound),
            "docker-runtime-missing",
        ),
        (
            TemplateError::DockerError(DockerError::DaemonNotRunning),
            "docker-daemon-unavailable",
        ),
        (
            TemplateError::DockerError(DockerError::timeout(10)),
            "timeout",
        ),
        (
            TemplateError::Timeout("synthetic-stderr-secret".into()),
            "timeout",
        ),
        (
            TemplateError::InvalidConfig("redis://user:synthetic-password@host".into()),
            "invalid-fixture-configuration",
        ),
        (
            TemplateError::AlreadyRunning("synthetic-stderr-secret".into()),
            "fixture-already-running",
        ),
        (
            TemplateError::NotRunning("synthetic-stderr-secret".into()),
            "fixture-not-running",
        ),
        (
            TemplateError::DockerError(DockerError::custom("synthetic-stderr-secret")),
            "docker-runtime-error",
        ),
    ] {
        let error = fixture_error(FixtureStage::RedisPort, original);
        assert!(error.to_string().contains("Redis port resolution"));
        assert!(error.to_string().contains(&format!("cause={label};")));
        assert!(error.downcast_ref::<FixtureError>().is_some());
        assert_safe(&error);
    }
}

#[test]
fn empty_stderr_is_explicitly_unclassified() {
    let error = fixture_error(FixtureStage::RedisStart, command_error(""));
    assert!(error.to_string().contains("cause=unclassified"));
    assert!(error.to_string().contains("stderr inspection=0/0 bytes"));
    assert_safe(&error);
}
