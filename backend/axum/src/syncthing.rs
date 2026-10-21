//! Syncthing runs as a child process inside the app sandbox, with the app state folder as its home,
//! and is restarted whenever it stops. Its REST API listens on a private Unix socket, since apps
//! have no loopback, and is protected by a persistent key. The app limits it to the local network
//! and turns off relays, global discovery, usage reports, crash reports and automatic upgrades.

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

const RESTART: Duration = Duration::from_secs(5);

pub const PORT: u16 = 22000;

/// Supervised Syncthing process and its API client.
#[derive(Debug, Clone)]
pub struct Syncthing {
    program: PathBuf,
    home: PathBuf,
    socket: PathBuf,
    key: String,
    running: Arc<AtomicBool>,
    starts: Arc<AtomicU64>,
    failing: Arc<AtomicBool>,
}

/// Folder configuration sent to Syncthing.
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

/// Folder status reported by Syncthing.
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

/// Remote completion of a folder. The local status alone cannot show an upload to the computer.
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

/// Returns 32 hexadecimal characters read from `/dev/urandom`.
pub fn random_key() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(&mut bytes))
        .map_err(|error| format!("no random source: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Returns the API key stored in the Syncthing home, created on first use with owner-only access.
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
    let key = random_key()?;
    fs::write(&file, &key).map_err(|error| error.to_string())?;
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600))
        .map_err(|error| error.to_string())?;
    Ok(key)
}

/// Syncthing folder type on the NAS for a sync mode.
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
        // Write a LAN-only configuration before the first start, so the API never runs unrestricted.
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

    /// `NAS_SYNCTHING` when set, as in tests, otherwise the Debian binary.
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

    /// Number of successful starts.
    pub fn starts(&self) -> u64 {
        self.starts.load(Ordering::Relaxed)
    }

    /// Whether the last start failed and a retry is pending.
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
            // The app service runs as a dynamic user without a home, and Syncthing exits at once
            // when $HOME is missing, so its own folder is used instead.
            .env("HOME", &self.home)
            .env("STGUIAPIKEY", &self.key)
            .env("STNODEFAULTFOLDER", "1")
            .env("STNOUPGRADE", "1")
            .env("STNORESTART", "1")
            // Single process without the Syncthing monitor, so the app handles restarts and a
            // kill leaves no orphan child.
            .env("STMONITORED", "1")
            .stdin(Stdio::null())
            .kill_on_drop(true);
        command
    }

    /// Runs Syncthing for the whole life of the app and restarts it whenever it stops.
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

    /// Whether the API answers within one minute of the start.
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

    /// Device ID of the NAS.
    pub async fn device_id(&self) -> Result<String, String> {
        self.get("/rest/system/status")
            .await?
            .get("myID")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| "syncthing did not say its identity".to_owned())
    }

    /// Applies the LAN-only options and the bandwidth limits.
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

    /// Device IDs known to Syncthing.
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
        // Never sync the trash or the NAS metadata, even when a whole volume is shared.
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
        for pattern in ["/.nas-trash", "/.nas-trash-off", ".nas-backup-*"] {
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

    /// Folder IDs known to Syncthing.
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

    /// IDs of the devices connected right now.
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

    /// Device and folder statistics, with the last contact and the last change.
    pub async fn stats(&self) -> (Value, Value) {
        (
            self.get("/rest/stats/device").await.unwrap_or(Value::Null),
            self.get("/rest/stats/folder").await.unwrap_or(Value::Null),
        )
    }

    /// The 20 most recent disk changes of a folder, newest first.
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
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        changes.reverse();
        changes.truncate(20);
        changes
    }

    /// Ignore patterns of a folder, read from its `.stignore`.
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
#[path = "../tests/unit/axum/syncthing.rs"]
mod tests;
