use super::*;
use crate::test_support::Scratch;

#[test]
fn what_the_sync_deletes_goes_to_the_trash_of_its_disk_one_version_each() {
    let scratch = Scratch::new();
    let disk = scratch.path().join("disk");
    let folder = disk.join("Foto");
    let first = scratch.write("disk/Foto/2026/a.jpg", "first");
    discard(&folder, &first, std::slice::from_ref(&disk)).unwrap();
    assert!(!first.exists());
    assert_eq!(
        fs::read_to_string(disk.join(".nas-trash/Foto/2026/a.jpg")).unwrap(),
        "first"
    );
    assert_eq!(
        fs::metadata(disk.join(".nas-trash"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o2770
    );
    for directory in [".nas-trash/Foto", ".nas-trash/Foto/2026"] {
        assert_eq!(
            fs::metadata(disk.join(directory))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            0o2770
        );
    }
    // A path relative to the folder, as Syncthing may give it, and a version that replaces the old.
    scratch.write("disk/Foto/2026/a.jpg", "second");
    discard(
        &folder,
        Path::new("2026/a.jpg"),
        std::slice::from_ref(&disk),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(disk.join(".nas-trash/Foto/2026/a.jpg")).unwrap(),
        "second"
    );
}

#[test]
fn with_the_trash_off_it_deletes_for_good_and_never_leaves_its_folder() {
    let scratch = Scratch::new();
    let disk = scratch.path().join("disk");
    let file = scratch.write("disk/Foto/a.jpg", "x");
    scratch.write("disk/.nas-trash-off", "");
    discard(&disk.join("Foto"), &file, std::slice::from_ref(&disk)).unwrap();
    assert!(!file.exists());
    assert!(!disk.join(".nas-trash").exists());
    fs::create_dir_all(disk.join("Foto/old")).unwrap();
    discard(
        &disk.join("Foto"),
        Path::new("old"),
        std::slice::from_ref(&disk),
    )
    .unwrap();
    assert!(!disk.join("Foto/old").exists());

    for wrong in ["../escape.jpg", "/etc/passwd"] {
        let refused = discard(
            &disk.join("Foto"),
            Path::new(wrong),
            std::slice::from_ref(&disk),
        );
        assert!(refused.is_err(), "{wrong}");
    }
    assert!(discard(&disk.join("Foto"), Path::new("missing.jpg"), &[]).is_err());
}

#[test]
fn rejects_ungranted_folders_and_symlinks_in_source_or_trash_parents() {
    let scratch = Scratch::new();
    scratch.write("disk/Foto/a.txt", "keep");
    scratch.write("outside/a.txt", "outside");
    let disk = scratch.path().join("disk");
    let folder = disk.join("Foto");
    let outside = scratch.path().join("outside");
    std::os::unix::fs::symlink(&outside, folder.join("escape")).unwrap();
    assert!(discard(
        &folder,
        Path::new("escape/a.txt"),
        std::slice::from_ref(&disk)
    )
    .is_err());
    assert!(discard(&folder, Path::new("a.txt"), &[]).is_err());
    std::os::unix::fs::symlink(&outside, disk.join(TRASH)).unwrap();
    assert!(discard(&folder, Path::new("a.txt"), std::slice::from_ref(&disk)).is_err());
    fs::remove_file(disk.join(TRASH)).unwrap();
    fs::create_dir(disk.join(TRASH)).unwrap();
    std::os::unix::fs::symlink(&outside, disk.join(TRASH).join("Foto")).unwrap();
    assert!(discard(&folder, Path::new("a.txt"), std::slice::from_ref(&disk)).is_err());
    assert_eq!(
        fs::read_to_string(outside.join("a.txt")).unwrap(),
        "outside"
    );
    assert_eq!(fs::read_to_string(folder.join("a.txt")).unwrap(), "keep");
}
