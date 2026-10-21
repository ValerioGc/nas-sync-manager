//! External versioning for Syncthing. Files that the sync deletes or replaces on the NAS are moved
//! to the trash of their disk under `<disk>/.nas-trash/<path>`, the same trash used by
//! nas-file-manager. Syncthing calls `nas-sync-manager-axum --trash <folder> <file>` instead of
//! deleting the file. When `.nas-trash-off` exists at the disk root the file is deleted permanently.

use std::{
    fs,
    io::{self, ErrorKind},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub const TRASH: &str = ".nas-trash";
pub const TRASH_OFF: &str = ".nas-trash-off";

/// Moves a synced file to the trash of its disk, or deletes it when the trash is disabled.
/// `disks` holds the roots of the granted data disks, and any other location is rejected.
pub fn discard(folder: &Path, file: &Path, disks: &[PathBuf]) -> io::Result<()> {
    let relative = file
        .strip_prefix(folder)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map_or_else(|| file.to_path_buf(), Path::to_path_buf);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "the file is not inside its folder",
        ));
    }
    let folder = folder.canonicalize()?;
    let candidate = folder.join(&relative);
    let parent = candidate
        .parent()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "no parent"))?
        .canonicalize()?;
    if !parent.starts_with(&folder) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "the file leaves its folder",
        ));
    }
    let real = parent.join(
        candidate
            .file_name()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "no file"))?,
    );
    let disk = disks
        .iter()
        .filter(|disk| real.starts_with(disk))
        .max_by_key(|disk| disk.components().count())
        .cloned()
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::PermissionDenied,
                "the folder is not on a granted disk",
            )
        })?;
    let metadata = fs::symlink_metadata(&real)?;
    if disk.join(TRASH_OFF).exists() {
        return if metadata.is_dir() {
            fs::remove_dir_all(&real)
        } else {
            fs::remove_file(&real)
        };
    }
    let trash = disk.join(TRASH);
    if !trash.exists() {
        fs::create_dir(&trash)?;
        fs::set_permissions(&trash, fs::Permissions::from_mode(0o2770))?;
    }
    if fs::symlink_metadata(&trash)?.file_type().is_symlink() {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "the trash is a symbolic link",
        ));
    }
    let target = trash.join(real.strip_prefix(&disk).unwrap_or(&relative));
    prepare_trash_parent(&trash, &target)?;
    // Keep the version already in the trash under a numbered name.
    if fs::symlink_metadata(&target).is_ok() {
        fs::rename(&target, numbered(&target))?;
    }
    fs::rename(&real, &target)
}

/// Returns the first free sibling name, such as `report (1).pdf`.
pub fn numbered(taken: &Path) -> PathBuf {
    let name = taken
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (stem, extension) = match name.rfind('.') {
        Some(dot) if dot > 0 => name.split_at(dot),
        _ => (name.as_str(), ""),
    };
    (1..)
        .map(|number| taken.with_file_name(format!("{stem} ({number}){extension}")))
        .find(|candidate| fs::symlink_metadata(candidate).is_err())
        .unwrap_or_else(|| taken.to_path_buf())
}

fn prepare_trash_parent(trash: &Path, target: &Path) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        let mut directory = trash.to_path_buf();
        for component in parent
            .strip_prefix(trash)
            .map_err(io::Error::other)?
            .components()
        {
            directory.push(component);
            match fs::symlink_metadata(&directory) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                    return Err(io::Error::new(
                        ErrorKind::InvalidInput,
                        "the trash path is not a directory",
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    fs::create_dir(&directory)?;
                    fs::set_permissions(&directory, fs::Permissions::from_mode(0o2770))?;
                }
                Err(error) => return Err(error),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/axum/trash.rs"]
mod tests;
