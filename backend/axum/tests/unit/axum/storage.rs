use super::*;
use crate::test_support::Scratch;

#[test]
fn reads_the_disks_of_the_registry() {
    let volumes = parse_registry("abc\tPhotos\t/srv/storage/abc\tdevice\nnet\tShare\t/mnt/net\n\n")
        .expect("registry");
    assert_eq!(
        volumes,
        [
            Volume {
                id: "abc".to_owned(),
                name: "Photos".to_owned(),
                path: PathBuf::from("/srv/storage/abc")
            },
            Volume {
                id: "net".to_owned(),
                name: "Share".to_owned(),
                path: PathBuf::from("/mnt/net")
            },
        ]
    );
    assert!(parse_registry("only\ttwo").is_err());
    assert!(parse_registry("a\t\t/path").is_err());
}

#[test]
fn resolves_a_folder_on_its_disk_and_never_out_of_it() {
    let disk = Scratch::new();
    disk.write("Photos/2026/a.jpg", "a");
    let outside = Scratch::new();
    std::os::unix::fs::symlink(outside.path(), disk.path().join("escape")).expect("link");
    let storage = Storage::Root(disk.path().to_path_buf());

    let photos = storage.resolve("/Photos/2026", false).expect("resolved");
    assert_eq!(photos.path, disk.path().join("Photos/2026"));
    assert_eq!(photos.volume, disk.path());
    assert!(storage.resolve("/Missing", false).is_err());
    let made = storage.resolve("/Backup/new", true).expect("created");
    assert!(made.path.is_dir());
    for refused in ["/Photos/../..", "/escape", "/Photos/2026/a.jpg"] {
        assert!(storage.resolve(refused, false).is_err(), "{refused}");
    }
}

#[test]
fn takes_only_a_disk_of_the_registry_that_is_mounted() {
    let unmounted = Scratch::new();
    let storage = Storage::Volumes(vec![
        Volume {
            id: "system".to_owned(),
            name: "System".to_owned(),
            path: PathBuf::from("/"),
        },
        Volume {
            id: "gone".to_owned(),
            name: "Gone".to_owned(),
            path: unmounted.path().to_path_buf(),
        },
    ]);

    let folder = storage
        .resolve("/system/tmp", false)
        .expect("on a mounted disk");
    assert_eq!(folder.volume, PathBuf::from("/"));
    let refused = storage.resolve("/gone", false).expect_err("not mounted");
    assert!(refused.contains("not mounted"), "{refused}");
    assert!(storage
        .resolve("/other/x", false)
        .expect_err("unknown")
        .contains("not connected"));
    assert!(storage
        .resolve("/", false)
        .expect_err("no disk")
        .contains("names no disk"));
}

#[test]
fn reads_the_paths_of_the_mount_table_and_relative_paths() {
    assert_eq!(unescape("/srv/my\\040disk\\134x"), "/srv/my disk\\x");
    assert!(is_mounted(Path::new("/")));
    assert!(nested(Path::new("/a/b"), Path::new("/a")));
    assert!(!nested(Path::new("/ab"), Path::new("/a")));
    assert_eq!(relative(Path::new("a/b/c.txt")), "a/b/c.txt");
}

#[test]
fn takes_the_disks_from_the_environment() {
    let registry = Scratch::new();
    let file = registry.write("volumes.tsv", "abc\tPhotos\t/srv/abc\n");
    unsafe { std::env::set_var("NAS_STORAGE_REGISTRY", &file) };
    let granted = Storage::from_env();
    unsafe { std::env::remove_var("NAS_STORAGE_REGISTRY") };
    assert!(matches!(granted, Ok(Storage::Volumes(volumes)) if volumes.len() == 1));
    assert!(matches!(Storage::from_env(), Ok(Storage::Root(_))));
}

#[test]
fn names_the_disks_for_the_page() {
    let storage = Storage::Volumes(
        parse_registry("abc\tPhotos\t/srv/storage/abc\tdevice\n").expect("registry"),
    );
    assert_eq!(
        storage.names(),
        [VolumeName {
            id: "abc".to_owned(),
            name: "Photos".to_owned()
        }]
    );
    assert!(Storage::Root(PathBuf::from("/tmp")).names().is_empty());
}
