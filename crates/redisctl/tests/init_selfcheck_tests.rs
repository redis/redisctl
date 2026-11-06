//! Database validation against disposable RESP fixtures, without Docker or credentials.
mod init_common;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy)]
enum Reply {
    Normal,
    RefuseSet(&'static str),
    Collision,
    RefuseGet,
    Mismatch,
    RefuseDel,
}

struct Fixture {
    url: String,
    commands: Arc<Mutex<Vec<Vec<String>>>>,
    values: Arc<Mutex<HashMap<String, String>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    fn new(reply: Reply) -> Self {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("redis://{}", listener.local_addr().unwrap());
        let commands = Arc::new(Mutex::new(Vec::new()));
        let values = Arc::new(Mutex::new(HashMap::from([(
            "redisctl:selfcheck".into(),
            "existing-value".into(),
        )])));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (seen, data, stopping) = (commands.clone(), values.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !stopping.load(std::sync::atomic::Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        break;
                    }
                    let count: usize = line.strip_prefix('*').unwrap().trim().parse().unwrap();
                    let mut args = Vec::new();
                    for _ in 0..count {
                        line.clear();
                        reader.read_line(&mut line).unwrap();
                        let len: usize = line.strip_prefix('$').unwrap().trim().parse().unwrap();
                        let mut bytes = vec![0; len + 2];
                        reader.read_exact(&mut bytes).unwrap();
                        args.push(String::from_utf8(bytes[..len].to_vec()).unwrap());
                    }
                    seen.lock().unwrap().push(args.clone());
                    let mut values = data.lock().unwrap();
                    let response = match args[0].as_str() {
                        "HELLO" => "-ERR unknown command 'HELLO'\r\n".into(),
                        "PING" => "+PONG\r\n".into(),
                        "SET" => match reply {
                            Reply::RefuseSet(code) => format!("-{code} fixture refusal\r\n"),
                            Reply::Collision => "$-1\r\n".into(),
                            _ => {
                                values.insert(args[1].clone(), args[2].clone());
                                "+OK\r\n".into()
                            }
                        },
                        "GET" => match reply {
                            Reply::RefuseGet => "-NOPERM fixture refusal\r\n".into(),
                            Reply::Mismatch => "$5\r\nother\r\n".into(),
                            _ => {
                                let value = &values[&args[1]];
                                format!("${}\r\n{value}\r\n", value.len())
                            }
                        },
                        "DEL" => match reply {
                            Reply::RefuseDel => "-NOPERM fixture refusal\r\n".into(),
                            _ => {
                                values.remove(&args[1]);
                                ":1\r\n".into()
                            }
                        },
                        _ => "+OK\r\n".into(),
                    };
                    drop(values);
                    if stream.write_all(response.as_bytes()).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            url,
            commands,
            values,
            stop,
            thread: Some(thread),
        }
    }

    fn init(&self) -> std::process::Output {
        let project = tempfile::tempdir().unwrap();
        let repo = init_common::skills_fixture();
        let mut cmd = init_common::hermetic_redisctl();
        let config = cmd.home().join("config.toml");
        cmd.current_dir(project.path())
            .env("REDISCTL_INIT_SKILLS_REPO", repo.path())
            .arg("--config-file")
            .arg(config)
            .args([
                "init",
                "--no-telemetry",
                "--no-install-cli",
                "--agent",
                "claude",
                "--url",
                &self.url,
            ]);
        cmd.output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn validation_uses_distinct_expiring_keys_and_cleans_up() {
    let fixture = Fixture::new(Reply::Normal);
    for _ in 0..2 {
        let output = fixture.init();
        assert!(
            output.status.success(),
            "{output:?}; commands: {:?}",
            fixture.commands.lock().unwrap()
        );
    }
    let commands = fixture.commands.lock().unwrap();
    let sets: Vec<_> = commands.iter().filter(|c| c[0] == "SET").collect();
    assert_eq!(sets.len(), 2);
    assert_ne!(sets[0][1], sets[1][1]);
    for set in sets {
        assert!(set[1].starts_with("redisctl:selfcheck:"));
        assert!(set[3..].windows(2).any(|p| p == ["EX", "60"]));
        assert!(set[3..].contains(&"NX".into()));
        let get = commands
            .iter()
            .position(|c| c[0] == "GET" && c[1] == set[1])
            .unwrap();
        let del = commands
            .iter()
            .position(|c| c[0] == "DEL" && c[1] == set[1])
            .unwrap();
        assert!(get < del);
    }
    assert_eq!(
        *fixture.values.lock().unwrap(),
        HashMap::from([("redisctl:selfcheck".into(), "existing-value".into())])
    );
}

#[test]
fn command_refusals_get_specific_remedies_without_stale_url_advice() {
    for (code, remedy) in [
        ("NOPERM", "ACL"),
        ("READONLY", "primary"),
        ("OOM", "memory"),
        ("MOVED", "Cluster"),
    ] {
        let fixture = Fixture::new(Reply::RefuseSet(code));
        let output = fixture.init();
        assert_eq!(output.status.code(), Some(10), "{output:?}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(text.contains(remedy), "{code}: {text}");
        assert!(!text.contains("URL is stale"), "{code}: {text}");
        assert!(!text.contains("could not talk to Redis"), "{code}: {text}");
        assert!(
            !fixture
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|c| c[0] == "DEL")
        );
    }
}

#[test]
fn failed_read_still_cleans_up_the_owned_key() {
    for reply in [Reply::RefuseGet, Reply::Mismatch] {
        let fixture = Fixture::new(reply);
        let output = fixture.init();
        assert_eq!(output.status.code(), Some(10), "{output:?}");
        assert!(
            fixture
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|c| c[0] == "DEL")
        );
        assert_eq!(fixture.values.lock().unwrap().len(), 1);
    }
}

#[test]
fn collision_does_not_read_or_delete_an_unowned_key() {
    let fixture = Fixture::new(Reply::Collision);
    let output = fixture.init();
    assert_eq!(output.status.code(), Some(10), "{output:?}");
    assert!(
        !fixture
            .commands
            .lock()
            .unwrap()
            .iter()
            .any(|c| c[0] == "GET" || c[0] == "DEL")
    );
}

#[test]
fn denied_cleanup_uses_ttl_fallback_without_failing_validation() {
    let fixture = Fixture::new(Reply::RefuseDel);
    let output = fixture.init();
    assert!(output.status.success(), "{output:?}");
    assert!(
        fixture
            .commands
            .lock()
            .unwrap()
            .iter()
            .any(|c| c[0] == "DEL")
    );
    assert_eq!(fixture.values.lock().unwrap().len(), 2);
}
