//! What the application keeps in its database: the settings, the paired computers and their syncs.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    /// The upload limit, in KiB/s; 0 is none.
    pub max_send_kbps: u32,
    /// The download limit, in KiB/s; 0 is none.
    pub max_recv_kbps: u32,
    /// How often a whole folder is read again, besides the changes followed as they come.
    pub rescan_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_send_kbps: 0,
            max_recv_kbps: 0,
            rescan_minutes: 60,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_send_kbps > 10_000_000 || self.max_recv_kbps > 10_000_000 {
            return Err("A limit goes up to 10000000 KiB/s.".into());
        }
        if !(1..=1440).contains(&self.rescan_minutes) {
            return Err("The full rescan runs every 1 to 1440 minutes.".into());
        }
        Ok(())
    }
}

pub async fn settings(pool: &SqlitePool) -> Result<Settings, sqlx::Error> {
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT max_send_kbps, max_recv_kbps, rescan_minutes FROM settings WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(
        row.map_or_else(Settings::default, |(send, receive, rescan)| Settings {
            max_send_kbps: u32::try_from(send).unwrap_or_default(),
            max_recv_kbps: u32::try_from(receive).unwrap_or_default(),
            rescan_minutes: u32::try_from(rescan).unwrap_or(60),
        }),
    )
}

pub async fn save_settings(pool: &SqlitePool, settings: Settings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO settings (id, max_send_kbps, max_recv_kbps, rescan_minutes) \
         VALUES (1, ?, ?, ?)",
    )
    .bind(i64::from(settings.max_send_kbps))
    .bind(i64::from(settings.max_recv_kbps))
    .bind(i64::from(settings.rescan_minutes))
    .execute(pool)
    .await
    .map(drop)
}

/// A computer paired with the NAS.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    pub client_version: String,
    pub paired_at: i64,
}

pub async fn devices(pool: &SqlitePool) -> Result<Vec<Device>, sqlx::Error> {
    sqlx::query_as("SELECT id, name, client_version, paired_at FROM devices ORDER BY paired_at, id")
        .fetch_all(pool)
        .await
}

pub async fn device(pool: &SqlitePool, id: &str) -> Result<Option<Device>, sqlx::Error> {
    sqlx::query_as("SELECT id, name, client_version, paired_at FROM devices WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Pairs a computer, or updates its name and version when it pairs again.
pub async fn pair(pool: &SqlitePool, device: &Device) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO devices (id, name, client_version, paired_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, client_version = excluded.client_version",
    )
    .bind(&device.id)
    .bind(&device.name)
    .bind(&device.client_version)
    .bind(device.paired_at)
    .execute(pool)
    .await
    .map(drop)
}

/// Forgets a computer with all its syncs.
pub async fn unpair(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM syncs WHERE device = ?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    let gone = sqlx::query("DELETE FROM devices WHERE id = ?")
        .bind(id)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        > 0;
    transaction.commit().await?;
    Ok(gone)
}

/// A sync between a folder of a computer and a folder of the data disks.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Sync {
    pub id: String,
    pub device: String,
    pub name: String,
    /// The folder of the NAS, as the Explorer names it: `<disk>/<folder>`.
    pub nas_path: String,
    /// The folder of the computer, as the computer said it.
    pub pc_path: String,
    pub mode: String,
    pub paused: bool,
    pub revoked: bool,
    pub created_at: i64,
}

pub async fn syncs(pool: &SqlitePool) -> Result<Vec<Sync>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, device, name, nas_path, pc_path, mode, paused, revoked, created_at \
         FROM syncs ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await
}

pub async fn sync(pool: &SqlitePool, id: &str) -> Result<Option<Sync>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, device, name, nas_path, pc_path, mode, paused, revoked, created_at \
         FROM syncs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Writes a sync, new or changed.
pub async fn save_sync(pool: &SqlitePool, sync: &Sync) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO syncs \
         (id, device, name, nas_path, pc_path, mode, paused, revoked, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&sync.id)
    .bind(&sync.device)
    .bind(&sync.name)
    .bind(&sync.nas_path)
    .bind(&sync.pc_path)
    .bind(&sync.mode)
    .bind(sync.paused)
    .bind(sync.revoked)
    .bind(sync.created_at)
    .execute(pool)
    .await
    .map(drop)
}

pub async fn delete_sync(pool: &SqlitePool, id: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM syncs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

#[cfg(test)]
#[path = "../../tests/unit/axum/store.rs"]
mod tests;
