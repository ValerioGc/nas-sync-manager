//! Syncthing on the NAS: a child process of the application, in its sandbox (its user, the data
//! disks of its grant, its state folder as home), restarted when it stops. Its API answers on a
//! unix socket in the state folder (the applications have no loopback) with a key the application
//! makes once; everything else the application writes through it: local network only, no relay,
//! no global discovery, no usage reports, no crash reports, no automatic upgrade.

use crate::http::exchange;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(30);
/// How long a stopped Syncthing waits before it starts again.
const RESTART: Duration = Duration::from_secs(5);
/// The port of the sync, which the base firewall opens to the local network while a sync is on.
pub const PORT: u16 = 22000;

/// Where Syncthing is, and how to speak to it.
#[derive(Debug, Clone)]
pub struct Syncthing {
    program: PathBuf,
    home: PathBuf,
    socket: PathBuf,
    key: String,
    running: Arc<AtomicBool>,
    /// How many times Syncthing started and answered: a new start is configured again.
    starts: Arc<AtomicU64>,
    /// Whether the last start failed: it did not run, or stopped before answering.
    failing: Arc<AtomicBool>,
}

/// A folder of Syncthing, as the application gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    pub kind: &'static str,
    pub device: String,
    pub paused: bool,
    pub rescan_seconds: u32,
    pub trash_command: String,
}

/// Where a folder is, as Syncthing says it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FolderStatus {
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub global_bytes: u64,
    #[serde(default)]
    pub global_files: u64,
    #[serde(default)]
    pub need_bytes: u64,
    #[serde(default)]
    pub need_files: u64,
    #[serde(default)]
    pub error: String,
}

/// What the peer still needs from a folder; NAS-only progress cannot describe an upload to a PC.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Completion {
    #[serde(default)]
    pub global_bytes: u64,
    #[serde(default)]
    pub need_bytes: u64,
    #[serde(default)]
    pub need_items: u64,
    #[serde(default)]
    pub need_deletes: u64,
    #[serde(default)]
    pub remote_state: String,
}

/// A key of 32 hexadecimal characters, from the random source of the kernel.
pub fn random_key() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("no random source: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The key of the API, made once and kept beside the home of Syncthing, readable only here.
pub fn key_in(home: &Path) -> Result<String, String> {
    fs::create_dir_all(home).map_err(|error| error.to_string())?;
    fs::set_permissions(home, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let file = home.join("api-key");
    if let Ok(key) = fs::read_to_string(&file) {
        let key = key.trim().to_owned();
        if key.len() >= 32 && key.chars().all(|c| c.is_ascii_hexdigit()) {
            fs::set_permissions(&file, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
            return Ok(key);
        }
    }
    fs::create_dir_all(home).map_err(|error| error.to_string())?;
    let key = random_key()?;
    fs::write(&file, &key).map_err(|error| error.to_string())?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    Ok(key)
}

/// The kind of a folder on the NAS for a mode of the sync.
pub fn folder_kind(mode: &str) -> &'static str {
    match mode {
        "pc-to-nas" => "receiveonly",
        "nas-to-pc" => "sendonly",
        _ => "sendreceive",
    }
}

impl Syncthing {
    pub fn new(program: PathBuf, home: PathBuf) -> Result<Self, String> {
        let key = key_in(&home)?;
        // The first process must already be local-only before the REST API becomes available.
        let config = home.join("config.xml");
        if !config.exists() {
            fs::write(&config, r#"<configuration version="37">
<gui enabled="true" tls="false"><address>127.0.0.1:8384</address></gui>
<options>
<listenAddress>tcp://:22000</listenAddress><listenAddress>quic://:22000</listenAddress>
<globalAnnounceEnabled>false</globalAnnounceEnabled><localAnnounceEnabled>true</localAnnounceEnabled>
<relaysEnabled>false</relaysEnabled><natEnabled>false</natEnabled>
<urAccepted>-1</urAccepted><crashReportingEnabled>false</crashReportingEnabled>
<autoUpgradeIntervalH>0</autoUpgradeIntervalH><startBrowser>false</startBrowser>
<limitBandwidthInLan>true</limitBandwidthInLan>
</options></configuration>"#).map_err(|error| error.to_string())?;
        }
        Ok(Self {
            socket: home.join("api.sock"),
            program,
            home,
            key,
            running: Arc::default(),
            starts: Arc::default(),
            failing: Arc::default(),
        })
    }

    /// `NAS_SYNCTHING` when set (a stand-in in the tests), otherwise the one of Debian.
    pub fn program_from_env() -> PathBuf {
        std::env::var_os("NAS_SYNCTHING")
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from("/usr/bin/syncthing"), PathBuf::from)
    }

    pub fn is_installed(&self) -> bool {
        self.program.is_file()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// How many times Syncthing started and answered.
    pub fn starts(&self) -> u64 {
        self.starts.load(Ordering::Relaxed)
    }

    /// Whether the last start failed, while the next one is waited for.
    pub fn is_failing(&self) -> bool {
        self.failing.load(Ordering::Relaxed)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .arg("serve")
            .arg(format!("--home={}", self.home.display()))
            .arg("--no-browser")
            .arg(format!("--gui-address=unix://{}", self.socket.display()))
            // The service of an application runs as a dynamic user without a home, and Syncthing
            // stops at once when $HOME is missing: its own folder stands in for it.
            .env("HOME", &self.home)
            .env("STGUIAPIKEY", &self.key)
            .env("STNODEFAULTFOLDER", "1")
            .env("STNOUPGRADE", "1")
            .env("STNORESTART", "1")
            // One process, without the monitor of Syncthing: the application restarts it, and
            // a kill leaves no child behind.
            .env("STMONITORED", "1")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        command
    }

    /// Runs Syncthing for as long as the application runs, starting it again when it stops.
    pub async fn supervise(self) {
        loop {
            if !self.is_installed() {
                tokio::time::sleep(RESTART * 12).await;
                continue;
            }
            let _ = fs::remove_file(&self.socket);
            match self.command().spawn() {
                Ok(mut child) => {
                    let ready = tokio::select! {
                        ready = self.wait_ready() => ready,
                        _ = child.wait() => false,
                    };
                    self.failing.store(!ready, Ordering::Relaxed);
                    if ready {
                        self.running.store(true, Ordering::Relaxed);
                        self.starts.fetch_add(1, Ordering::Relaxed);
                        let status = child.wait().await;
                        eprintln!("<4>syncthing stopped ({status:?}): it starts again");
                    } else {
                        let _ = child.kill().await;
                        eprintln!("<4>syncthing did not answer: it starts again");
                    }
                    self.running.store(false, Ordering::Relaxed);
                }
                Err(error) => {
                    self.failing.store(true, Ordering::Relaxed);
                    eprintln!("<3>syncthing did not start: {error}");
                }
            }
            tokio::time::sleep(RESTART).await;
        }
    }

    /// Whether the API answers within a minute of the start.
    async fn wait_ready(&self) -> bool {
        for _ in 0..120 {
            if self.get("/rest/system/ping").await.is_ok() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        false
    }

    async fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value, String> {
        let body = body.map(Value::to_string).unwrap_or_default();
        let answer = exchange(
            &self.socket,
            method,
            path,
            &[("X-API-Key", &self.key)],
            &body,
            TIMEOUT,
        )
        .await
        .map_err(|reason| format!("syncthing: {reason}"))?;
        if answer.status >= 300 {
            return Err(format!(
                "syncthing answered {}: {}",
                answer.status,
                answer.body.trim()
            ));
        }
        if answer.body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&answer.body).map_err(|error| error.to_string())
    }

    pub async fn get(&self, path: &str) -> Result<Value, String> {
        self.call("GET", path, None).await
    }

    /// The identity of the NAS in the sync.
    pub async fn device_id(&self) -> Result<String, String> {
        self.get("/rest/system/status")
            .await?
            .get("myID")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "syncthing did not say its identity".to_owned())
    }

    /// Local network only, nothing sent anywhere else, the limits of the settings.
    pub async fn configure(&self, send_kbps: u32, receive_kbps: u32) -> Result<(), String> {
        let options = json!({
            "globalAnnounceEnabled": false,
            "localAnnounceEnabled": true,
            "relaysEnabled": false,
            "natEnabled": false,
            "urAccepted": -1,
            "crashReportingEnabled": false,
            "autoUpgradeIntervalH": 0,
            "startBrowser": false,
            "listenAddresses": [format!("tcp://:{PORT}"), format!("quic://:{PORT}")],
            "maxSendKbps": send_kbps,
            "maxRecvKbps": receive_kbps,
            "limitBandwidthInLan": true,
        });
        self.call("PATCH", "/rest/config/options", Some(&options))
            .await
            .map(drop)
    }

    pub async fn add_device(&self, id: &str, name: &str) -> Result<(), String> {
        let device = json!({
            "deviceID": id,
            "name": name,
            "addresses": ["dynamic"],
            "autoAcceptFolders": false,
            "introducer": false,
        });
        self.call("PUT", &format!("/rest/config/devices/{id}"), Some(&device))
            .await
            .map(drop)
    }

    pub async fn remove_device(&self, id: &str) -> Result<(), String> {
        self.call("DELETE", &format!("/rest/config/devices/{id}"), None)
            .await
            .map(drop)
    }

    /// The devices Syncthing knows, by identity.
    pub async fn devices(&self) -> Result<Vec<String>, String> {
        Ok(self
            .get("/rest/config/devices")
            .await?
            .as_array()
            .map(|devices| {
                devices
                    .iter()
                    .filter_map(|device| device["deviceID"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn put_folder(&self, folder: &Folder) -> Result<(), String> {
        let mut body = json!({
            "id": folder.id,
            "label": folder.label,
            "path": folder.path,
            "type": folder.kind,
            "devices": [{ "deviceID": folder.device }],
            "rescanIntervalS": folder.rescan_seconds,
            "fsWatcherEnabled": true,
            // Install mandatory exclusions before any scan or file exchange can start.
            "paused": true,
            "versioning": {
                "type": "external",
                "params": { "command": folder.trash_command },
                "cleanupIntervalS": 0,
            },
        });
        self.call(
            "PUT",
            &format!("/rest/config/folders/{}", folder.id),
            Some(&body),
        )
        .await?;
        // A sync of a whole volume must never distribute its trash or NAS metadata.
        let answer = self
            .get(&format!("/rest/db/ignores?folder={}", folder.id))
            .await?;
        if let Some(error) = answer["error"].as_str().filter(|error| !error.is_empty()) {
            return Err(format!(
                "syncthing could not load its ignore patterns: {error}"
            ));
        }
        // Debian Syncthing returns null when a new folder has no .stignore yet.
        let mut ignores: Vec<String> = match answer.get("ignore") {
            Some(Value::Null) => Vec::new(),
            Some(Value::Array(lines)) => lines
                .iter()
                .map(|line| {
                    line.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "syncthing returned an invalid ignore pattern".to_owned())
                })
                .collect::<Result<_, _>>()?,
            _ => return Err("syncthing did not return its ignore patterns".to_owned()),
        };
        let mut changed = false;
        for pattern in ["/.nas-trash", "/.nas-trash-off"] {
            if !ignores.iter().any(|line| line == pattern) {
                ignores.insert(0, pattern.to_owned());
                changed = true;
            }
        }
        if changed {
            self.call(
                "POST",
                &format!("/rest/db/ignores?folder={}", folder.id),
                Some(&json!({ "ignore": ignores })),
            )
            .await?;
        }
        if !folder.paused {
            body["paused"] = json!(false);
            self.call(
                "PUT",
                &format!("/rest/config/folders/{}", folder.id),
                Some(&body),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn remove_folder(&self, id: &str) -> Result<(), String> {
        self.call("DELETE", &format!("/rest/config/folders/{id}"), None)
            .await
            .map(drop)
    }

    /// The folders Syncthing knows, by id.
    pub async fn folders(&self) -> Result<Vec<String>, String> {
        Ok(self
            .get("/rest/config/folders")
            .await?
            .as_array()
            .map(|folders| {
                folders
                    .iter()
                    .filter_map(|folder| folder["id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub async fn folder_status(&self, id: &str) -> Result<FolderStatus, String> {
        let status = self.get(&format!("/rest/db/status?folder={id}")).await?;
        serde_json::from_value(status).map_err(|error| error.to_string())
    }

    pub async fn completion(&self, folder: &str, device: &str) -> Result<Completion, String> {
        let value = self
            .get(&format!(
                "/rest/db/completion?folder={folder}&device={device}"
            ))
            .await?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    /// The devices connected now.
    pub async fn connected(&self) -> Result<Vec<String>, String> {
        let connections = self.get("/rest/system/connections").await?;
        Ok(connections["connections"]
            .as_object()
            .map(|all| {
                all.iter()
                    .filter(|(_, state)| state["connected"] == true)
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default())
    }

    /// When each device was last seen, and when each folder last changed, as Syncthing says it.
    pub async fn stats(&self) -> (Value, Value) {
        (
            self.get("/rest/stats/device").await.unwrap_or(Value::Null),
            self.get("/rest/stats/folder").await.unwrap_or(Value::Null),
        )
    }

    /// The last changes of a folder, the newest first: `{ time, action, path }`.
    pub async fn changes(&self, folder: &str) -> Vec<Value> {
        let events = self
            .get("/rest/events/disk?since=0&timeout=0&limit=200")
            .await
            .unwrap_or(Value::Null);
        let mut changes: Vec<Value> = events
            .as_array()
            .map(|events| {
                events
                    .iter()
                    .filter(|event| event["data"]["folder"] == folder)
                    .map(|event| {
                        json!({
                            "time": event["time"],
                            "action": event["data"]["action"],
                            "path": event["data"]["path"],
                            "remote": event["type"] == "RemoteChangeDetected",
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        changes.reverse();
        changes.truncate(20);
        changes
    }

    /// What the computer left out of a folder (its `.stignore`).
    pub async fn ignores(&self, folder: &str) -> Vec<String> {
        self.get(&format!("/rest/db/ignores?folder={folder}"))
            .await
            .ok()
            .and_then(|ignores| {
                ignores["ignore"].as_array().map(|lines| {
                    lines
                        .iter()
                        .filter_map(|line| line.as_str().map(str::to_owned))
                        .collect()
                })
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/axum/syncthing.rs"]
mod tests;
