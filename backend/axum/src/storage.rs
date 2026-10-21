//! Resolves the logical paths of the Explorer picker, `/<volume>/...`, to real folders. Each volume
//! is a data disk listed in the launcher registry `NAS_STORAGE_REGISTRY`, with tab separated id,
//! name, path and kind. A resolved folder must stay inside its disk after following symbolic links.
//! The disk must be mounted, so the empty mount point of a disconnected disk never becomes a sync
//! source.
//!
//! Without a registry, in development and tests, paths are relative to `NAS_STORAGE_ROOT`.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub id: String,
    /// Display name of the disk in Explorer.
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Storage {
    /// Data disks of the NAS, which must be mounted.
    Volumes(Vec<Volume>),
    /// Single folder that stands in for the disks in development.
    Root(PathBuf),
}

/// A resolved folder and the root of its disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub path: PathBuf,
    pub volume: PathBuf,
}

/// Parses the disk registry written by the launcher.
pub fn parse_registry(content: &str) -> Result<Vec<Volume>, String> {
    let mut volumes = Vec::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        if !matches!(fields.len(), 3 | 4) || fields.iter().any(|field| field.is_empty()) {
            return Err("the registry of the disks is not readable".to_owned());
        }
        volumes.push(Volume {
            id: fields[0].to_owned(),
            name: fields[1].to_owned(),
            path: PathBuf::from(fields[2]),
        });
    }
    Ok(volumes)
}

impl Storage {
    /// Loads the disks granted by the launcher, or the development folder.
    pub fn from_env() -> Result<Self, String> {
        let setting = |name: &str| std::env::var_os(name).filter(|value| !value.is_empty());
        if let Some(registry) = setting("NAS_STORAGE_REGISTRY") {
            let content = fs::read_to_string(&registry)
                .map_err(|error| format!("the registry of the disks is not readable: {error}"))?;
            return parse_registry(&content).map(Self::Volumes);
        }
        if let Some(root) = setting("NAS_STORAGE_ROOT") {
            return Ok(Self::Root(PathBuf::from(root)));
        }
        if cfg!(debug_assertions) {
            return Ok(Self::Root(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/storage"),
            ));
        }
        Err("The data disks are not available to this application.".to_owned())
    }

    /// Resolves a logical path to a real folder inside its disk, after following symbolic links.
    /// The folder must already exist unless `create` is true.
    pub fn resolve(&self, logical: &str, create: bool) -> Result<Folder, String> {
        let trimmed = logical.trim_matches('/');
        let mut segments = trimmed.split('/').filter(|segment| !segment.is_empty());
        if segments
            .clone()
            .any(|segment| segment == "." || segment == "..")
        {
            return Err(format!("{logical} is not a folder of the disks"));
        }
        let (volume, rest): (PathBuf, Vec<&str>) = match self {
            Self::Volumes(volumes) => {
                let id = segments
                    .next()
                    .ok_or_else(|| format!("{logical} names no disk"))?;
                let volume = volumes
                    .iter()
                    .find(|volume| volume.id == id)
                    .ok_or_else(|| format!("the disk of {logical} is not connected"))?;
                (volume.path.clone(), segments.collect())
            }
            Self::Root(root) => (root.clone(), segments.collect()),
        };
        let volume = volume
            .canonicalize()
            .map_err(|error| format!("the disk of {logical} is not readable: {error}"))?;
        if matches!(self, Self::Volumes(_)) && !is_mounted(&volume) {
            return Err(format!("the disk of {logical} is not mounted"));
        }
        let wanted = rest
            .iter()
            .fold(volume.clone(), |path, segment| path.join(segment));
        if create {
            fs::create_dir_all(&wanted)
                .map_err(|error| format!("{logical} cannot be created: {error}"))?;
        }
        let path = wanted
            .canonicalize()
            .map_err(|error| format!("{logical} is not reachable: {error}"))?;
        if !path.starts_with(&volume) || !path.is_dir() {
            return Err(format!("{logical} is not a folder of its disk"));
        }
        Ok(Folder { path, volume })
    }
}

/// Disk id and display name, shown by the UI in place of the id.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VolumeName {
    pub id: String,
    pub name: String,
}

impl Storage {
    pub fn names(&self) -> Vec<VolumeName> {
        match self {
            Self::Volumes(volumes) => volumes
                .iter()
                .map(|volume| VolumeName {
                    id: volume.id.clone(),
                    name: volume.name.clone(),
                })
                .collect(),
            Self::Root(_) => Vec::new(),
        }
    }
}

/// Whether a folder is a mount point. A disconnected disk leaves an empty folder behind.
pub fn is_mounted(path: &Path) -> bool {
    let Ok(mounts) = fs::read_to_string("/proc/self/mountinfo") else {
        return false;
    };
    mounts.lines().any(|line| {
        line.split_whitespace()
            .nth(4)
            .is_some_and(|mount| Path::new(&unescape(mount)) == path)
    })
}

/// Decodes the octal escapes that mountinfo uses for space, tab, newline and backslash.
fn unescape(field: &str) -> String {
    field
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// Whether `inner` is `outer` itself or a folder inside it.
pub fn nested(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

/// Joins the normal components of a path with `/`.
pub fn relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
#[path = "../tests/unit/axum/storage.rs"]
mod tests;
