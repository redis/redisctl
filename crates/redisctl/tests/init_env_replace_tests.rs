//! Explicit URL replacement preserves credentials in the generated dotenv file.
#![cfg(unix)]

mod init_common;

#[test]
fn replacing_a_url_preserves_dollar_characters() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_common::skills_fixture();
    std::fs::write(
        dir.path().join(".env"),
        "REDIS_URL=\"redis://localhost:6379\"\n",
    )
    .unwrap();
    let url = "redis://default:abc$REDISCTL_TEST_UNSET@127.0.0.1:9";
    assert_cmd::Command::cargo_bin("redisctl")
        .unwrap()
        .current_dir(dir.path())
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
        .args([
            "init",
            "--url",
            url,
            "--no-install-cli",
            "--agent",
            "claude",
        ])
        .assert()
        .code(10);
    assert_eq!(
        redisctl_init::read_env_key(dir.path(), ".env", "REDIS_URL").as_deref(),
        Some(url)
    );
    let output = std::process::Command::new("sh")
        .args(["-c", ". ./.env; printf '%s' \"$REDIS_URL\""])
        .env_remove("REDISCTL_TEST_UNSET")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), url);
}
