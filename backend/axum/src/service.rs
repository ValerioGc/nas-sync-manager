//! The syncs of the NAS: what the database keeps, given to Syncthing and read back from it.
//!
//! A sync is created and changed only by the client of its computer (`/api/client/...`); the page
//! of the NAS shows them, pauses and resumes them and revokes them. A revoked sync leaves
//! Syncthing (its folder on the NAS stays as it is) and stays in the database until its computer
//! removes it, so the client can say it was revoked; a revoked computer loses all its syncs.
//!
//! Whenever Syncthing starts it is given everything again (`reconcile`): the options, the paired
//! computers and the folders, and whatever it knows besides them goes.

use crate::{
    launcher::Launcher,
    storage::{self, Storage},
    store::{self, Device, Settings, Sync},
    syncthing::{self, Folder, Syncthing},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MODES: [&str; 3] = ["two-way", "pc-to-nas", "nas-to-pc"];
/// The prefix of the folders this application gives Syncthing.
const FOLDER_PREFIX: &str = "nas-";
const MARKER: &str = ".sync-conflict-";
const MAX_CONFLICTS: usize = 200;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| {
            i64::try_from(time.as_millis()).unwrap_or(i64::MAX)
        })
}

/// A refusal, with the status the page reads it by.
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub status: StatusCode,
    pub message: String,
}

impl Failure {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn missing(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(StatusCode::SERVICE_UNAVAILABLE, message)
    }
}

impl From<sqlx::Error> for Failure {
    fn from(error: sqlx::Error) -> Self {
        eprintln!("Sync database error: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to access the sync database",
        )
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

/// Whether an identity of Syncthing is one: eight groups of seven letters and digits of base 32.
pub fn valid_device_id(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    groups.len() == 8
        && groups.iter().all(|group| {
            group.len() == 7
                && group
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
        })
}

fn valid_text(text: &str, longest: usize) -> bool {
    let text = text.trim();
    !text.is_empty() && text.chars().count() <= longest && !text.chars().any(char::is_control)
}

/// The id of a new folder: `nas-` and ten letters.
pub fn new_folder_id() -> Result<String, String> {
    let key = syncthing::random_key()?;
    let letters: String = key
        .bytes()
        .take(10)
        .map(|byte| char::from(b'a' + byte % 26))
        .collect();
    Ok(format!("{FOLDER_PREFIX}{letters}"))
}

/// The name of the file a conflict copy stands for: `a.sync-conflict-20260101-120000-ABCDEFG.txt`
/// is a copy of `a.txt`.
pub fn original_of(name: &str) -> Option<String> {
    let (base, rest) = name.rsplit_once(MARKER)?;
    // A date, a time and the short identity of the device that made it: 8 + 1 + 6 + 1 + 7.
    let extension = rest.get(23..)?;
    let stamp = rest.get(..23)?;
    let valid = stamp
        .as_bytes()
        .iter()
        .enumerate()
        .all(|(index, byte)| match index {
            8 | 15 => *byte == b'-',
            0..=14 => byte.is_ascii_digit(),
            _ => byte.is_ascii_alphanumeric(),
        });
    (valid && !base.is_empty() && (extension.is_empty() || extension.starts_with('.')))
        .then(|| format!("{base}{extension}"))
}

/// Where a sync is, for the page and the client.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    #[serde(flatten)]
    pub sync: Sync,
    pub device_name: String,
    /// `synced`, `syncing`, `paused`, `offline`, `error`, `revoked` or `unknown`.
    pub state: String,
    pub progress: Option<f64>,
    pub global_bytes: u64,
    pub global_files: u64,
    pub need_bytes: u64,
    pub need_files: u64,
    pub last_sync: Option<String>,
    pub error: Option<String>,
}

/// The state of a sync from what Syncthing says of it and of its computer.
pub fn state_of(
    sync: &Sync,
    status: Option<&syncthing::FolderStatus>,
    connected: bool,
) -> (String, Option<f64>) {
    if sync.revoked {
        return ("revoked".into(), None);
    }
    if sync.paused {
        return ("paused".into(), None);
    }
    let Some(status) = status else {
        return ("unknown".into(), None);
    };
    if !status.error.is_empty() || status.state == "error" {
        return ("error".into(), None);
    }
    if !connected {
        return ("offline".into(), None);
    }
    if matches!(status.state.as_str(), "paused" | "unknown") {
        return (status.state.clone(), None);
    }
    if status.need_bytes == 0 && status.need_files == 0 && status.state == "idle" {
        return ("synced".into(), Some(100.0));
    }
    let progress = (status.global_bytes > 0).then(|| {
        (status.global_bytes.saturating_sub(status.need_bytes)) as f64 * 100.0
            / status.global_bytes as f64
    });
    (
        if status.state.starts_with("scanning") {
            "scanning"
        } else {
            "syncing"
        }
        .into(),
        progress,
    )
}

/// A copy Syncthing kept when both sides changed a file.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// The copy, inside the folder of the sync.
    pub path: String,
    /// The file it is a copy of.
    pub original: String,
    pub size: u64,
}

fn conflict_of(entry: &fs::DirEntry, folder: &Path, name: &str) -> Option<Conflict> {
    let original = original_of(name)?;
    let relative = entry
        .path()
        .strip_prefix(folder)
        .map(storage::relative)
        .unwrap_or_default();
    let parent = Path::new(&relative)
        .parent()
        .map(storage::relative)
        .unwrap_or_default();
    Some(Conflict {
        original: if parent.is_empty() {
            original
        } else {
            format!("{parent}/{original}")
        },
        path: relative,
        size: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
    })
}

/// The conflict copies under a folder, the first ones.
pub fn conflicts_in(folder: &Path) -> Vec<Conflict> {
    let mut found = Vec::new();
    let mut folders = vec![folder.to_path_buf()];
    while let Some(current) = folders.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !name.starts_with(".st") && !name.starts_with(".nas-") {
                    folders.push(entry.path());
                }
                continue;
            }
            let Some(conflict) = conflict_of(&entry, folder, &name) else {
                continue;
            };
            found.push(conflict);
            if found.len() >= MAX_CONFLICTS {
                return found;
            }
        }
    }
    found.sort_by(|left, right| left.path.cmp(&right.path));
    found
}

/// What a client sends to pair its computer.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pairing {
    pub device_id: String,
    pub name: String,
    pub version: String,
}

/// A new sync, as the client of a computer asks for it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NewSync {
    pub name: String,
    pub nas_path: String,
    pub pc_path: String,
    pub mode: String,
}

/// A change of a sync from its client: what is left out stays.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SyncChange {
    pub name: Option<String>,
    pub pc_path: Option<String>,
    pub mode: Option<String>,
    pub paused: Option<bool>,
}

/// A folder of the data disks the client may pick.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub path: String,
}

pub struct Service {
    pool: SqlitePool,
    launcher: Launcher,
    syncthing: Syncthing,
    storage: Result<Storage, String>,
    trash_command: String,
    mutations: tokio::sync::Mutex<()>,
    reconcile_needed: AtomicBool,
}

impl Service {
    pub fn new(
        pool: SqlitePool,
        launcher: Launcher,
        syncthing: Syncthing,
        storage: Result<Storage, String>,
        executable: &Path,
    ) -> Self {
        Self {
            mutations: tokio::sync::Mutex::new(()),
            reconcile_needed: AtomicBool::new(true),
            pool,
            launcher,
            syncthing,
            storage,
            trash_command: format!(
                "\"{}\" --trash %FOLDER_PATH% %FILE_PATH%",
                executable.display()
            ),
        }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    fn storage(&self) -> Result<&Storage, Failure> {
        self.storage
            .as_ref()
            .map_err(|reason| Failure::unavailable(reason.clone()))
    }

    fn running(&self) -> Result<(), Failure> {
        if self.syncthing.is_running() {
            Ok(())
        } else if self.syncthing.is_installed() {
            Err(Failure::unavailable("Syncthing is starting"))
        } else {
            Err(Failure::unavailable("Syncthing is not installed"))
        }
    }

    /// The folder of the NAS of a sync, on its disk.
    async fn folder_path(&self, sync: &Sync, create: bool) -> Result<PathBuf, Failure> {
        resolve_storage(self.storage()?, &sync.nas_path, create)
            .await
            .map(|folder| folder.path)
    }

    /// Gives a sync to Syncthing as it is now: a revoked one leaves it.
    async fn apply(&self, sync: &Sync, settings: Settings) -> Result<(), Failure> {
        if sync.revoked {
            return removed(self.syncthing.remove_folder(&sync.id).await);
        }
        let folder = Folder {
            id: sync.id.clone(),
            label: sync.name.clone(),
            // A vanished data folder must not be recreated as an empty sync source.
            path: self.folder_path(sync, false).await?,
            kind: syncthing::folder_kind(&sync.mode),
            device: sync.device.clone(),
            paused: sync.paused,
            rescan_seconds: settings.rescan_minutes.saturating_mul(60),
            trash_command: self.trash_command.clone(),
        };
        self.syncthing
            .put_folder(&folder)
            .await
            .map_err(Failure::unavailable)
    }

    /// Tells the launcher whether a sync is on: the application in background, the ports open.
    pub async fn declare(&self) {
        let active = store::syncs(&self.pool)
            .await
            .map(|syncs| syncs.iter().any(|sync| !sync.paused && !sync.revoked))
            .unwrap_or(false);
        self.launcher.background(active).await;
        self.launcher.sync_ports(active).await;
    }

    /// Gives Syncthing everything again, and takes from it what it knows besides.
    pub async fn reconcile(&self) -> Result<(), Failure> {
        let _guard = self.mutations.lock().await;
        self.reconcile_locked().await
    }

    async fn reconcile_locked(&self) -> Result<(), Failure> {
        self.reconcile_needed.store(true, Ordering::Relaxed);
        let settings = store::settings(&self.pool).await?;
        self.syncthing
            .configure(settings.max_send_kbps, settings.max_recv_kbps)
            .await
            .map_err(Failure::unavailable)?;
        let devices = store::devices(&self.pool).await?;
        let syncs = store::syncs(&self.pool).await?;
        let own = self
            .syncthing
            .device_id()
            .await
            .map_err(Failure::unavailable)?;
        for known in self
            .syncthing
            .devices()
            .await
            .map_err(Failure::unavailable)?
        {
            if known != own && !devices.iter().any(|device| device.id == known) {
                removed(self.syncthing.remove_device(&known).await)?;
            }
        }
        for device in &devices {
            self.syncthing
                .add_device(&device.id, &device.name)
                .await
                .map_err(Failure::unavailable)?;
        }
        for known in self
            .syncthing
            .folders()
            .await
            .map_err(Failure::unavailable)?
        {
            if !syncs.iter().any(|sync| sync.id == known && !sync.revoked) {
                removed(self.syncthing.remove_folder(&known).await)?;
            }
        }
        let mut failed = None;
        for sync in syncs.iter().filter(|sync| !sync.revoked) {
            if let Err(failure) = self.apply(sync, settings).await {
                eprintln!(
                    "<4>the sync {} was not given: {}",
                    sync.name, failure.message
                );
                failed = Some(failure);
            }
        }
        if failed.is_none() {
            self.reconcile_needed.store(false, Ordering::Relaxed);
        }
        failed.map_or(Ok(()), Err)
    }

    /// Gives Syncthing everything whenever it starts, for as long as the application runs.
    pub async fn follow(&self) {
        let mut configured = 0;
        loop {
            let starts = self.syncthing.starts();
            if (starts != configured || self.reconcile_needed.load(Ordering::Relaxed))
                && self.syncthing.is_running()
            {
                match self.reconcile().await {
                    Ok(()) => configured = starts,
                    Err(failure) => {
                        eprintln!("<4>syncthing was not configured: {}", failure.message)
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Whether Syncthing is there and runs, and the identity of the NAS.
    pub async fn status(&self) -> Value {
        let device_id = if self.syncthing.is_running() {
            self.syncthing.device_id().await.ok()
        } else {
            None
        };
        json!({
            "installed": self.syncthing.is_installed(),
            "running": self.syncthing.is_running(),
            "failing": self.syncthing.is_failing(),
            "deviceId": device_id,
        })
    }

    /// Every sync with where it is; `device` keeps only those of one computer.
    pub async fn list(&self, device: Option<&str>) -> Result<Vec<Listed>, Failure> {
        let devices = store::devices(&self.pool).await?;
        let syncs = store::syncs(&self.pool).await?;
        let running = self.syncthing.is_running();
        let connected = if running {
            self.syncthing.connected().await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let (_, folder_stats) = if running {
            self.syncthing.stats().await
        } else {
            (Value::Null, Value::Null)
        };
        let mut listed = Vec::new();
        for sync in syncs
            .into_iter()
            .filter(|sync| device.is_none_or(|device| sync.device == device))
        {
            let mut status = if running && !sync.revoked {
                self.syncthing.folder_status(&sync.id).await.ok()
            } else {
                None
            };
            let online = connected.contains(&sync.device);
            self.include_peer_completion(&sync, status.as_mut(), online)
                .await;
            let (state, progress) = state_of(&sync, status.as_ref(), online);
            let status = status.unwrap_or_default();
            let last_sync = folder_stats[&sync.id]["lastFile"]["at"]
                .as_str()
                .or_else(|| folder_stats[&sync.id]["lastScan"].as_str())
                .map(str::to_owned)
                .filter(|at| !at.starts_with("1970"));
            listed.push(Listed {
                device_name: devices
                    .iter()
                    .find(|known| known.id == sync.device)
                    .map(|known| known.name.clone())
                    .unwrap_or_default(),
                state,
                progress,
                global_bytes: status.global_bytes,
                global_files: status.global_files,
                need_bytes: status.need_bytes,
                need_files: status.need_files,
                last_sync,
                error: (!status.error.is_empty()).then_some(status.error),
                sync,
            });
        }
        Ok(listed)
    }

    async fn include_peer_completion(
        &self,
        sync: &Sync,
        status: Option<&mut syncthing::FolderStatus>,
        online: bool,
    ) {
        if !online || sync.paused || sync.revoked || sync.mode == "pc-to-nas" {
            return;
        }
        let Some(status) = status else {
            return;
        };
        match self.syncthing.completion(&sync.id, &sync.device).await {
            Ok(peer) => {
                status.global_bytes = status.global_bytes.max(peer.global_bytes);
                status.need_bytes = status.need_bytes.max(peer.need_bytes);
                status.need_files = status
                    .need_files
                    .max(peer.need_items.max(peer.need_deletes));
                if peer.remote_state == "paused" {
                    status.state = "paused".into();
                } else if peer.remote_state != "valid" {
                    status.state = "unknown".into();
                }
            }
            Err(_) => status.state = "unknown".into(),
        }
    }

    async fn known(&self, id: &str) -> Result<Sync, Failure> {
        store::sync(&self.pool, id)
            .await?
            .ok_or_else(|| Failure::missing("No such sync."))
    }

    /// One sync in detail: its last changes, its conflicts and what its computer leaves out.
    pub async fn detail(&self, id: &str) -> Result<Value, Failure> {
        let sync = self.known(id).await?;
        let listed = self
            .list(None)
            .await?
            .into_iter()
            .find(|listed| listed.sync.id == sync.id)
            .ok_or_else(|| Failure::missing("No such sync."))?;
        let running = self.syncthing.is_running() && !sync.revoked;
        let conflicts = match self.folder_path(&sync, false).await {
            Ok(path) => tokio::task::spawn_blocking(move || conflicts_in(&path))
                .await
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        Ok(json!({
            "sync": listed,
            "changes": if running { self.syncthing.changes(&sync.id).await } else { Vec::new() },
            "ignores": if running { self.syncthing.ignores(&sync.id).await } else { Vec::new() },
            "conflicts": conflicts,
        }))
    }

    /// Pauses or resumes a sync, from the NAS or from its client.
    pub async fn pause(&self, id: &str, paused: bool) -> Result<Sync, Failure> {
        let _guard = self.mutations.lock().await;
        let mut sync = self.known(id).await?;
        if sync.revoked {
            return Err(Failure::new(StatusCode::CONFLICT, "The sync was revoked."));
        }
        sync.paused = paused;
        if self.syncthing.is_running() {
            let settings = store::settings(&self.pool).await?;
            self.apply(&sync, settings).await?;
        }
        store::save_sync(&self.pool, &sync).await?;
        self.declare().await;
        Ok(sync)
    }

    /// Takes a sync away from Syncthing; the folder of the NAS stays as it is.
    pub async fn revoke(&self, id: &str) -> Result<(), Failure> {
        let _guard = self.mutations.lock().await;
        let mut sync = self.known(id).await?;
        sync.revoked = true;
        if self.syncthing.is_running() {
            let settings = store::settings(&self.pool).await?;
            self.apply(&sync, settings).await?;
        }
        store::save_sync(&self.pool, &sync).await?;
        self.declare().await;
        Ok(())
    }

    /// The paired computers, online or not, with when they were last seen.
    pub async fn devices(&self) -> Result<Vec<Value>, Failure> {
        let devices = store::devices(&self.pool).await?;
        let syncs = store::syncs(&self.pool).await?;
        let running = self.syncthing.is_running();
        let connected = if running {
            self.syncthing.connected().await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let (device_stats, _) = if running {
            self.syncthing.stats().await
        } else {
            (Value::Null, Value::Null)
        };
        Ok(devices
            .into_iter()
            .map(|device| {
                let last_seen = device_stats[&device.id]["lastSeen"]
                    .as_str()
                    .filter(|at| !at.starts_with("1970"))
                    .map(str::to_owned);
                json!({
                    "online": connected.contains(&device.id),
                    "lastSeen": last_seen,
                    "syncs": syncs.iter().filter(|sync| sync.device == device.id).count(),
                    "id": device.id,
                    "name": device.name,
                    "clientVersion": device.client_version,
                    "pairedAt": device.paired_at,
                })
            })
            .collect())
    }

    /// Revokes a computer: it leaves Syncthing with all its syncs.
    pub async fn revoke_device(&self, id: &str) -> Result<(), Failure> {
        let _guard = self.mutations.lock().await;
        let syncs = store::syncs(&self.pool).await?;
        if store::device(&self.pool, id).await?.is_none() {
            return Err(Failure::missing("No such computer."));
        }
        if self.syncthing.is_running() {
            for sync in syncs.iter().filter(|sync| sync.device == id) {
                removed(self.syncthing.remove_folder(&sync.id).await)?;
            }
            removed(self.syncthing.remove_device(id).await)?;
        }
        store::unpair(&self.pool, id).await?;
        self.declare().await;
        Ok(())
    }

    pub async fn save_settings(&self, settings: Settings) -> Result<Settings, Failure> {
        settings.validate().map_err(Failure::bad)?;
        let _guard = self.mutations.lock().await;
        store::save_settings(&self.pool, settings).await?;
        if self.syncthing.is_running() {
            self.reconcile_locked().await?;
        }
        Ok(settings)
    }

    /// Keeps one side of a conflict: `this`, the file as it is (the copy goes to the trash), or
    /// `other`, the copy, which takes the place of the file (the file goes to the trash).
    pub async fn resolve(&self, id: &str, path: &str, keep: &str) -> Result<(), Failure> {
        let _guard = self.mutations.lock().await;
        let sync = self.known(id).await?;
        let folder = self.folder_path(&sync, false).await?;
        if sync.revoked {
            return Err(Failure::new(StatusCode::CONFLICT, "The sync was revoked."));
        }
        let disks = self.disks();
        let (path, keep) = (path.to_owned(), keep.to_owned());
        tokio::task::spawn_blocking(move || resolve_conflict(&folder, &path, &keep, &disks))
            .await
            .map_err(|error| Failure::unavailable(error.to_string()))?
    }

    /// The names of the disks, for the page to show instead of their ids.
    pub fn volumes(&self) -> Vec<storage::VolumeName> {
        self.storage
            .as_ref()
            .map(Storage::names)
            .unwrap_or_default()
    }

    /// The roots of the data disks, where their trash is.
    pub fn disks(&self) -> Vec<PathBuf> {
        match &self.storage {
            Ok(Storage::Volumes(volumes)) => {
                volumes.iter().map(|volume| volume.path.clone()).collect()
            }
            Ok(Storage::Root(root)) => vec![root.clone()],
            Err(_) => Vec::new(),
        }
    }

    // The client of a computer.

    /// A computer that is paired: the client speaks for it.
    pub async fn client(&self, device: Option<&str>) -> Result<Device, Failure> {
        let device = device.ok_or_else(|| {
            Failure::new(StatusCode::UNAUTHORIZED, "The computer is not identified.")
        })?;
        store::device(&self.pool, device).await?.ok_or_else(|| {
            Failure::new(
                StatusCode::FORBIDDEN,
                "The computer is not paired, or was revoked.",
            )
        })
    }

    /// Pairs a computer: it is given the identity of the NAS and the port of the sync.
    pub async fn pair(&self, pairing: Pairing) -> Result<Value, Failure> {
        let _guard = self.mutations.lock().await;
        if !valid_device_id(&pairing.device_id) {
            return Err(Failure::bad("Not an identity of Syncthing."));
        }
        if !valid_text(&pairing.name, 60) || !valid_text(&pairing.version, 40) {
            return Err(Failure::bad("The name or the version is not valid."));
        }
        self.running()?;
        let own = self
            .syncthing
            .device_id()
            .await
            .map_err(Failure::unavailable)?;
        if pairing.device_id == own {
            return Err(Failure::bad(
                "The NAS cannot be paired as its own computer.",
            ));
        }
        let device = Device {
            id: pairing.device_id,
            name: pairing.name.trim().to_owned(),
            client_version: pairing.version.trim().to_owned(),
            paired_at: now_ms(),
        };
        self.syncthing
            .add_device(&device.id, &device.name)
            .await
            .map_err(Failure::unavailable)?;
        store::pair(&self.pool, &device).await?;
        Ok(json!({
            "deviceId": own,
            "port": syncthing::PORT,
        }))
    }

    /// The folders a client may pick on the data disks: the disks, then what each folder holds.
    pub async fn choices(&self, path: &str) -> Result<Vec<Choice>, Failure> {
        let storage = self.storage()?.clone();
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || choices_in(&storage, &path))
            .await
            .map_err(|error| Failure::unavailable(error.to_string()))?
    }

    /// Creates a sync of a computer, on a folder no other sync holds or is held by.
    pub async fn create(&self, device: &Device, input: NewSync) -> Result<Listed, Failure> {
        let _guard = self.mutations.lock().await;
        self.client(Some(&device.id)).await?;
        if !valid_text(&input.name, 80) || !valid_text(&input.pc_path, 4096) {
            return Err(Failure::bad(
                "The name or the folder of the computer is not valid.",
            ));
        }
        if !MODES.contains(&input.mode.as_str()) {
            return Err(Failure::bad("The mode is two-way, pc-to-nas or nas-to-pc."));
        }
        self.running()?;
        let storage = self.storage()?;
        if input.nas_path.split('/').any(|part| {
            part.starts_with(".nas-") || [".stfolder", ".stversions", ".stignore"].contains(&part)
        }) {
            return Err(Failure::bad("The folder is reserved for NAS metadata."));
        }
        let folder = resolve_storage(storage, &input.nas_path, false).await?;
        for existing in store::syncs(&self.pool)
            .await?
            .into_iter()
            .filter(|sync| !sync.revoked)
        {
            if let Ok(other) = resolve_storage(storage, &existing.nas_path, false).await {
                if storage::nested(&folder.path, &other.path)
                    || storage::nested(&other.path, &folder.path)
                {
                    return Err(Failure::new(
                        StatusCode::CONFLICT,
                        format!("The folder is already synced by {}.", existing.name),
                    ));
                }
            }
        }
        let sync = Sync {
            id: new_folder_id().map_err(Failure::unavailable)?,
            device: device.id.clone(),
            name: input.name.trim().to_owned(),
            nas_path: input.nas_path.trim_matches('/').to_owned(),
            pc_path: input.pc_path.trim().to_owned(),
            mode: input.mode,
            paused: false,
            revoked: false,
            created_at: now_ms(),
        };
        let settings = store::settings(&self.pool).await?;
        self.apply(&sync, settings).await?;
        store::save_sync(&self.pool, &sync).await?;
        self.declare().await;
        self.listed(&sync.id).await
    }

    async fn listed(&self, id: &str) -> Result<Listed, Failure> {
        self.list(None)
            .await?
            .into_iter()
            .find(|listed| listed.sync.id == id)
            .ok_or_else(|| Failure::missing("No such sync."))
    }

    /// The sync of a computer, never another's.
    async fn owned(&self, device: &Device, id: &str) -> Result<Sync, Failure> {
        let sync = self.known(id).await?;
        if sync.device == device.id {
            Ok(sync)
        } else {
            Err(Failure::missing("No such sync."))
        }
    }

    pub async fn change(
        &self,
        device: &Device,
        id: &str,
        change: SyncChange,
    ) -> Result<Listed, Failure> {
        let _guard = self.mutations.lock().await;
        self.client(Some(&device.id)).await?;
        let mut sync = self.owned(device, id).await?;
        if sync.revoked {
            return Err(Failure::new(StatusCode::CONFLICT, "The sync was revoked."));
        }
        if let Some(name) = change.name {
            if !valid_text(&name, 80) {
                return Err(Failure::bad("The name is not valid."));
            }
            sync.name = name.trim().to_owned();
        }
        if let Some(pc_path) = change.pc_path {
            if !valid_text(&pc_path, 4096) {
                return Err(Failure::bad("The folder of the computer is not valid."));
            }
            sync.pc_path = pc_path.trim().to_owned();
        }
        if let Some(mode) = change.mode {
            if !MODES.contains(&mode.as_str()) {
                return Err(Failure::bad("The mode is two-way, pc-to-nas or nas-to-pc."));
            }
            sync.mode = mode;
        }
        if let Some(paused) = change.paused {
            sync.paused = paused;
        }
        if self.syncthing.is_running() {
            let settings = store::settings(&self.pool).await?;
            self.apply(&sync, settings).await?;
        }
        store::save_sync(&self.pool, &sync).await?;
        self.declare().await;
        self.listed(&sync.id).await
    }

    /// Removes a sync of a computer, revoked or not; the folder of the NAS stays.
    pub async fn remove(&self, device: &Device, id: &str) -> Result<(), Failure> {
        let _guard = self.mutations.lock().await;
        let sync = self.owned(device, id).await?;
        if !sync.revoked && self.syncthing.is_running() {
            removed(self.syncthing.remove_folder(&sync.id).await)?;
        }
        store::delete_sync(&self.pool, &sync.id).await?;
        self.declare().await;
        Ok(())
    }
}

fn removed(result: Result<(), String>) -> Result<(), Failure> {
    result.or_else(|reason| {
        if reason.contains("answered 404:") {
            Ok(())
        } else {
            Err(Failure::unavailable(reason))
        }
    })
}

async fn resolve_storage(
    storage: &Storage,
    path: &str,
    create: bool,
) -> Result<storage::Folder, Failure> {
    let (storage, path) = (storage.clone(), path.to_owned());
    tokio::task::spawn_blocking(move || storage.resolve(&path, create).map_err(Failure::bad))
        .await
        .map_err(|error| Failure::unavailable(error.to_string()))?
}

fn choices_in(storage: &Storage, path: &str) -> Result<Vec<Choice>, Failure> {
    if path.trim_matches('/').is_empty() {
        if let Storage::Volumes(volumes) = storage {
            return Ok(volumes
                .iter()
                .map(|volume| Choice {
                    name: volume.name.clone(),
                    path: volume.id.clone(),
                })
                .collect());
        }
    }
    let folder = storage.resolve(path, false).map_err(Failure::bad)?;
    let base = path.trim_matches('/').to_owned();
    let mut choices: Vec<Choice> = fs::read_dir(&folder.path)
        .map_err(|error| Failure::bad(error.to_string()))?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with(".nas-") && !name.starts_with(".st"))
        .map(|name| Choice {
            path: if base.is_empty() {
                name.clone()
            } else {
                format!("{base}/{name}")
            },
            name,
        })
        .collect();
    choices.sort_by_key(|choice| choice.name.to_lowercase());
    Ok(choices)
}

fn resolve_conflict(
    folder: &Path,
    path: &str,
    keep: &str,
    disks: &[PathBuf],
) -> Result<(), Failure> {
    let relative = Path::new(path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(Failure::bad("Not a file of the sync."));
    }
    let name = relative
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Failure::bad("Not a file of the sync."))?;
    let original = original_of(name).ok_or_else(|| Failure::bad("Not a conflict copy."))?;
    let copy = folder.join(relative);
    let parent = copy
        .parent()
        .ok_or_else(|| Failure::bad("No parent folder."))?
        .canonicalize()
        .map_err(|_| Failure::missing("The conflict folder is gone."))?;
    if !parent.starts_with(folder) {
        return Err(Failure::bad("The conflict copy leaves its folder."));
    }
    let target = copy.with_file_name(&original);
    if !fs::symlink_metadata(&copy).is_ok_and(|metadata| metadata.is_file()) {
        return Err(Failure::missing("The conflict copy is gone."));
    }
    let result = match keep {
        "this" => crate::trash::discard(folder, &copy, disks),
        "other" => {
            if fs::symlink_metadata(&target).is_ok() {
                crate::trash::discard(folder, &target, disks)
                    .map_err(|error| Failure::unavailable(error.to_string()))?;
            }
            fs::rename(&copy, &target)
        }
        _ => return Err(Failure::bad("Keep this or the other.")),
    };
    result.map_err(|error| Failure::unavailable(error.to_string()))
}

#[cfg(test)]
#[path = "../../tests/unit/axum/service.rs"]
mod tests;
