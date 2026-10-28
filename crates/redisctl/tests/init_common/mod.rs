//! Helpers shared by the hermetic init suites (not a test target itself).
//! Each suite compiles this module separately, so a helper one suite skips is
//! not dead code.
#![allow(dead_code)]

use tempfile::TempDir;

/// A fake redis/agent-skills checkout keeps the skills step offline.
pub fn skills_fixture() -> TempDir {
    let repo = tempfile::tempdir().unwrap();
    let skill = repo.path().join("skills/redis-basics");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), "# basics\n").unwrap();
    repo
}

/// Just enough RESP to pass init's validation (AUTH, PING, SET, GET, DEL).
/// Returns the loopback port it serves on.
pub fn fake_redis() -> u16 {
    fake_redis_on(std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap())
}

/// [`fake_redis`] on a listener the caller bound.
pub fn fake_redis_on(listener: std::net::TcpListener) -> u16 {
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut stream = stream;
            let mut stored = String::new();
            let mut line = String::new();
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if !line.starts_with('*') {
                    continue;
                }
                let argc: usize = line[1..].trim().parse().unwrap_or(0);
                let mut args = Vec::with_capacity(argc);
                for _ in 0..argc {
                    let mut len = String::new();
                    let mut arg = String::new();
                    if reader.read_line(&mut len).unwrap_or(0) == 0
                        || reader.read_line(&mut arg).unwrap_or(0) == 0
                    {
                        break;
                    }
                    args.push(arg.trim_end().to_string());
                }
                let reply = match args.first().map(|c| c.to_ascii_uppercase()) {
                    Some(cmd) if cmd == "PING" => "+PONG\r\n".to_string(),
                    Some(cmd) if cmd == "SET" => {
                        stored = args.get(2).cloned().unwrap_or_default();
                        "+OK\r\n".to_string()
                    }
                    Some(cmd) if cmd == "GET" => {
                        format!("${}\r\n{}\r\n", stored.len(), stored)
                    }
                    Some(cmd) if cmd == "DEL" => ":1\r\n".to_string(),
                    _ => "+OK\r\n".to_string(),
                };
                if stream.write_all(reply.as_bytes()).is_err() {
                    break;
                }
            }
        }
    });
    port
}

/// `redisctl` with a throwaway HOME and failing `curl`/`wget` fakes first on PATH,
/// so no run reaches the network or the developer's home. A run that calls a fake
/// fails the test when this drops. Only `--install-cli` reaches the redis-cli
/// installer from a test (nothing can answer its prompt); with it, put your own fakes
/// on PATH.
pub struct Hermetic {
    cmd: assert_cmd::Command,
    home: TempDir,
}

pub fn hermetic_redisctl() -> Hermetic {
    let home = tempfile::tempdir().unwrap();
    let bin = home.path().join("downloaders");
    std::fs::create_dir(&bin).unwrap();
    #[cfg(unix)]
    for name in ["curl", "wget"] {
        use std::os::unix::fs::PermissionsExt;
        let path = bin.join(name);
        let log = home.path().join("downloads.log");
        std::fs::write(
            &path,
            format!("#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 1\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&path))).unwrap();
    let mut cmd = assert_cmd::Command::cargo_bin("redisctl").unwrap();
    cmd.env("HOME", home.path())
        .env("PATH", path)
        // Explicitly off: a key in the developer's (or CI's) environment must not
        // make test runs send telemetry.
        .env("REDISCTL_INIT_AMPLITUDE_KEY", "")
        // The redis-cli installer's own variables: tests set them when they mean to.
        .env_remove("REDIS_CLI_VERSION")
        .env_remove("REDIS_CLI_BASE_URL");
    Hermetic { cmd, home }
}

impl Hermetic {
    /// The fakes' directory, for a test that sets its own PATH.
    pub fn downloaders(&self) -> std::path::PathBuf {
        self.home.path().join("downloaders")
    }
}

impl std::ops::Deref for Hermetic {
    type Target = assert_cmd::Command;
    fn deref(&self) -> &Self::Target {
        &self.cmd
    }
}

impl std::ops::DerefMut for Hermetic {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cmd
    }
}

impl Drop for Hermetic {
    fn drop(&mut self) {
        if let Ok(calls) = std::fs::read_to_string(self.home.path().join("downloads.log"))
            && !std::thread::panicking()
        {
            panic!("the run reached a downloader:\n{calls}");
        }
    }
}
