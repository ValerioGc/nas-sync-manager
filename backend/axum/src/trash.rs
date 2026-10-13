//! What the sync deletes or replaces on the NAS goes to the trash of its disk (nas-file-manager):
//! Syncthing runs `nas-sync-manager-axum --trash <folder> <file>` instead of removing the file
//! (external versioning). The file goes to `<disk>/.nas-trash/<its path on the disk>`, in place of
//! what had that path there; while the trash of the disk is off (`.nas-trash-off` at its root) it
//! is removed for good.

use std::{
    fs,
    io::{self, ErrorKind},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

pub const TRASH: &str = ".nas-trash";
pub const TRASH_OFF: &str = ".nas-trash-off";

/// Moves a file of a synced folder to the trash of its disk, or removes it while the trash is off.
/// `disks` are the roots of the granted data disks; an ungranted folder is refused.
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
    if let Some(parent) = target.parent() {
        let mut directory = trash.clone();
        for component in parent
            .strip_prefix(&trash)
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
    match fs::symlink_metadata(&target) {
        Ok(existing) if existing.is_dir() => fs::remove_dir_all(&target)?,
        Ok(_) => fs::remove_file(&target)?,
        Err(_) => {}
    }
    fs::rename(&real, &target)
}

#[cfg(test)]
#[path = "../../tests/unit/axum/trash.rs"]
mod tests;
