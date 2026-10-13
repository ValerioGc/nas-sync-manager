//! What the tests of the backend share: a database in memory, folders of their own, a launcher and
//! a Syncthing on sockets of their own, which answer as the real ones and keep what they were asked.

use crate::{launcher::Launcher, syncthing::Syncthing, MIGRATOR};
use serde_json::{json, Map, Value};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixListener,
};

/// The identity of the Syncthing of the NAS, and of a computer.
pub const NAS_ID: &str = "NASNASN-AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG";
pub const PC_ID: &str = "PCPCPCP-AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG";

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A folder of the test, removed when it is dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nas-sync-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("scratch");
        Self(path.canonicalize().expect("scratch path"))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes a file, its folders included, and returns its path.
    pub fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().expect("a folder")).expect("folders");
        std::fs::write(&path, content).expect("file");
        path
    }

    /// An executable written aside and copied in place, so no fork of another test keeps it open.
    pub fn program(&self, name: &str, body: &str) -> PathBuf {
        let path = self.0.join(name);
        let draft = self.0.join(format!(".{name}.draft"));
        std::fs::write(&draft, format!("#!/bin/sh\n{body}\n")).expect("draft");
        let copied = std::process::Command::new("cp")
            .arg(&draft)
            .arg(&path)
            .status()
            .expect("cp");
        assert!(copied.success());
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("mode");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub async fn database() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("database");
    MIGRATOR.run(&pool).await.expect("migrations");
    pool
}

type Requests = Arc<Mutex<Vec<(String, Value)>>>;

/// Answers `request` on `socket`, for as long as the test runs, keeping every request.
fn serve(
    socket: &Path,
    requests: Requests,
    answer: impl Fn(&str, &Value, &str) -> (u16, Value) + Send + Sync + 'static,
) {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket).expect("socket");
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let (requests, answer) = (Arc::clone(&requests), Arc::clone(&answer));
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut buffer = [0; 8192];
                let (head, length) = loop {
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        return;
                    }
                    raw.extend_from_slice(&buffer[..read]);
                    if let Some(end) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&raw[..end]).into_owned();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("Content-Length: "))
                            .and_then(|length| length.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        raw.drain(..end + 4);
                        break (head, length);
                    }
                };
                while raw.len() < length {
                    let read = stream.read(&mut buffer).await.unwrap_or(0);
                    if read == 0 {
                        break;
                    }
                    raw.extend_from_slice(&buffer[..read]);
                }
                let line = head
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches(" HTTP/1.1")
                    .to_owned();
                let key = head
                    .lines()
                    .find_map(|line| line.strip_prefix("X-API-Key: "))
                    .unwrap_or_default()
                    .to_owned();
                let body: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
                let (status, reply) = answer(&line, &body, &key);
                requests.lock().expect("requests").push((line, body));
                let reply = if reply.is_null() {
                    String::new()
                } else {
                    reply.to_string()
                };
                let _ = stream
                    .write_all(
                        format!(
                            "HTTP/1.1 {status} X\r\nContent-Length: {}\r\n\r\n{reply}",
                            reply.len()
                        )
                        .as_bytes(),
                    )
                    .await;
            });
        }
    });
}

/// A launcher that accepts everything and keeps what it was asked.
pub struct FakeLauncher {
    pub socket: PathBuf,
    pub requests: Requests,
}

impl FakeLauncher {
    pub fn start(scratch: &Scratch) -> Self {
        let socket = scratch.0.join("launcher.sock");
        let requests = Requests::default();
        serve(&socket, Arc::clone(&requests), |_, _, _| (204, Value::Null));
        Self { socket, requests }
    }

    pub fn launcher(&self) -> Launcher {
        Launcher::new(Some(self.socket.clone()))
    }

    pub fn bodies(&self, line: &str) -> Vec<Value> {
        self.requests
            .lock()
            .expect("requests")
            .iter()
            .filter(|(asked, _)| asked == line)
            .map(|(_, body)| body.clone())
            .collect()
    }
}

/// What the fake Syncthing knows: its configuration and what the test says of its folders.
#[derive(Default)]
pub struct Known {
    pub options: Value,
    pub devices: Map<String, Value>,
    pub folders: Map<String, Value>,
    pub status: Map<String, Value>,
    pub completion: Map<String, Value>,
    pub connected: Vec<String>,
    pub stats_device: Value,
    pub stats_folder: Value,
    pub events: Value,
    pub ignores: Vec<String>,
    pub ignores_error: String,
    pub refuse: Vec<String>,
}

/// A Syncthing on the socket of its home, answering the API the application uses.
pub struct FakeSyncthing {
    pub known: Arc<Mutex<Known>>,
    pub requests: Requests,
    pub syncthing: Syncthing,
    supervisor: tokio::task::JoinHandle<()>,
    home: PathBuf,
}

fn fake_syncthing_answer(known: &Mutex<Known>, line: &str, body: &Value) -> (u16, Value) {
    let mut known = known.lock().expect("known");
    if known.refuse.iter().any(|refused| refused == line) {
        return (503, json!("unavailable"));
    }
    let (method, path) = line.split_once(' ').unwrap_or_default();
    let (path, query) = path.split_once('?').unwrap_or((path, ""));
    let folder = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("folder="))
        .unwrap_or_default()
        .to_owned();
    let named = |prefix: &str| path.strip_prefix(prefix).map(str::to_owned);
    match (method, path) {
        ("GET", "/rest/system/ping") => (200, json!({ "ping": "pong" })),
        ("GET", "/rest/system/status") => (200, json!({ "myID": NAS_ID })),
        ("PATCH", "/rest/config/options") => {
            known.options = body.clone();
            (200, Value::Null)
        }
        ("GET", "/rest/config/devices") => {
            let mut devices: Vec<Value> = known.devices.values().cloned().collect();
            devices.push(json!({ "deviceID": NAS_ID }));
            (200, Value::Array(devices))
        }
        ("GET", "/rest/config/folders") => {
            (200, Value::Array(known.folders.values().cloned().collect()))
        }
        ("GET", "/rest/db/status") => match known.status.get(&folder) {
            Some(status) => (200, status.clone()),
            None if known.folders.contains_key(&folder) => (
                200,
                json!({ "state": "idle", "globalBytes": 0, "needBytes": 0 }),
            ),
            None => (404, json!("no such folder")),
        },
        ("GET", "/rest/db/completion") => (
            200,
            known
                .completion
                .get(&folder)
                .cloned()
                .unwrap_or_else(|| json!({ "remoteState": "valid" })),
        ),
        ("GET", "/rest/system/connections") => {
            let connections: Map<String, Value> = known
                .connected
                .iter()
                .map(|id| (id.clone(), json!({ "connected": true })))
                .collect();
            (200, json!({ "connections": connections }))
        }
        ("GET", "/rest/stats/device") => (200, known.stats_device.clone()),
        ("GET", "/rest/stats/folder") => (200, known.stats_folder.clone()),
        ("GET", "/rest/events/disk") => (200, known.events.clone()),
        ("GET", "/rest/db/ignores") => (
            200,
            json!({ "ignore": if known.ignores.is_empty() { Value::Null } else { json!(known.ignores) }, "error": known.ignores_error }),
        ),
        ("POST", "/rest/db/ignores") => {
            known.ignores = body["ignore"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect();
            (200, Value::Null)
        }
        ("PUT", _) if path.starts_with("/rest/config/devices/") => {
            let id = named("/rest/config/devices/").unwrap_or_default();
            known.devices.insert(id, body.clone());
            (200, Value::Null)
        }
        ("DELETE", _) if path.starts_with("/rest/config/devices/") => {
            let id = named("/rest/config/devices/").unwrap_or_default();
            if known.devices.remove(&id).is_some() {
                (200, Value::Null)
            } else {
                (404, json!("no such device"))
            }
        }
        ("PUT", _) if path.starts_with("/rest/config/folders/") => {
            let id = named("/rest/config/folders/").unwrap_or_default();
            known.folders.insert(id, body.clone());
            (200, Value::Null)
        }
        ("DELETE", _) if path.starts_with("/rest/config/folders/") => {
            let id = named("/rest/config/folders/").unwrap_or_default();
            if known.folders.remove(&id).is_some() {
                (200, Value::Null)
            } else {
                (404, json!("no such folder"))
            }
        }
        _ => (404, json!("unknown")),
    }
}

impl FakeSyncthing {
    /// A Syncthing that runs: a program that only waits, and this API on its socket.
    pub async fn start(scratch: &Scratch) -> Self {
        let program = scratch.program(
            "syncthing",
            &format!(
                "echo $$ > '{}'\nexec sleep 600",
                scratch.path().join("pid").display()
            ),
        );
        let home = scratch.0.join("home");
        let syncthing = Syncthing::new(program, home.clone()).expect("syncthing");
        let supervisor = tokio::spawn(syncthing.clone().supervise());
        // A marker from the process proves the supervisor already cleared the socket.
        tokio::time::timeout(Duration::from_secs(2), async {
            while !scratch.path().join("pid").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("child started");
        let known = Arc::new(Mutex::new(Known::default()));
        let requests = Requests::default();
        let key = std::fs::read_to_string(home.join("api-key")).expect("key");
        let answering = Arc::clone(&known);
        serve(
            &home.join("api.sock"),
            Arc::clone(&requests),
            move |line, body, sent| {
                if sent != key.trim() {
                    return (403, json!("CSRF Error"));
                }
                fake_syncthing_answer(&answering, line, body)
            },
        );
        for _ in 0..100 {
            if syncthing.is_running() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(syncthing.is_running(), "the fake syncthing answers");
        Self {
            known,
            requests,
            syncthing,
            supervisor,
            home,
        }
    }

    pub async fn restart(&self, scratch: &Scratch) {
        let pidfile = scratch.path().join("pid");
        let previous = std::fs::read_to_string(&pidfile).unwrap();
        let started = self.syncthing.starts();
        let status = std::process::Command::new("kill")
            .args(["-TERM", previous.trim()])
            .status()
            .unwrap();
        assert!(status.success());
        tokio::time::timeout(Duration::from_secs(10), async {
            while std::fs::read_to_string(&pidfile).unwrap_or_default() == previous
                || self.home.join("api.sock").exists()
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("child restarted");
        let known = Arc::clone(&self.known);
        let key = std::fs::read_to_string(self.home.join("api-key")).unwrap();
        serve(
            &self.home.join("api.sock"),
            Arc::clone(&self.requests),
            move |line, body, sent| {
                if sent != key.trim() {
                    return (403, json!("CSRF Error"));
                }
                fake_syncthing_answer(&known, line, body)
            },
        );
        tokio::time::timeout(Duration::from_secs(3), async {
            while self.syncthing.starts() == started {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("API answered after restart");
    }

    pub fn asked(&self) -> Vec<String> {
        self.requests
            .lock()
            .expect("requests")
            .iter()
            .map(|(line, _)| line.clone())
            .collect()
    }
}

impl Drop for FakeSyncthing {
    fn drop(&mut self) {
        self.supervisor.abort();
    }
}
