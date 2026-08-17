//! Helpers shared by the hermetic init suites (not a test target itself).

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
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
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
