//! Where a folder of a rule really is. Explorer's picker hands out logical paths, `/<volume>/...`,
//! where the volume is the id of a data disk in the registry the launcher writes
//! (`NAS_STORAGE_REGISTRY`: id, name, path and kind, separated by tabs). A folder is resolved on the
//! disk it names and has to stay inside it, links followed; and a disk has to be mounted, because
//! the empty folder a disconnected disk leaves behind must not become a sync source.
//!
//! Without a registry (development and tests) the logical path is relative to one folder,
//! `NAS_STORAGE_ROOT`, taken as a disk that is always there.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub id: String,
    /// What the person calls the disk, as Explorer shows it.
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum Storage {
    /// The data disks of the NAS, which must be mounted.
    Volumes(Vec<Volume>),
    /// One folder standing for the disks, in development.
    Root(PathBuf),
}

/// A folder of a rule on the machine, and the disk it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub path: PathBuf,
    pub volume: PathBuf,
}

/// The volumes of the registry, or why it cannot be read.
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
    /// The disks as the launcher granted them, or the folder of development.
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

    /// The folder a logical path names, which has to exist when `create` is false. It is resolved
    /// on its disk, links followed, and refused when it leads out of it.
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

/// A disk as the page names it: the first segment of a logical path, and its name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VolumeName {
    pub id: String,
    pub name: String,
}

impl Storage {
    /// The names of the disks, for the page to show instead of their ids; none without a registry.
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

/// Whether a folder is a mount point: a disconnected disk leaves an empty folder behind.
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

/// A path of mountinfo, where a space, a tab, a newline and a backslash are written in octal.
fn unescape(field: &str) -> String {
    field
        .replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

/// Whether one folder is the other, or inside it.
pub fn nested(inner: &Path, outer: &Path) -> bool {
    inner.starts_with(outer)
}

/// A path relative to a folder, in the `/`-separated form the records keep.
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
#[path = "../../tests/unit/axum/storage.rs"]
mod tests;
