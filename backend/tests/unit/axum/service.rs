use super::*;
use crate::test_support::{database, FakeLauncher, FakeSyncthing, Scratch, NAS_ID, PC_ID};

const OTHER_ID: &str = "OTHEROT-AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG";

struct Bench {
    scratch: Scratch,
    launcher: FakeLauncher,
    syncthing: FakeSyncthing,
    service: Service,
}

impl Bench {
    async fn new() -> Self {
        let scratch = Scratch::new();
        std::fs::create_dir_all(scratch.path().join("disk/Foto")).unwrap();
        std::fs::create_dir_all(scratch.path().join("disk/Documenti/Lavoro")).unwrap();
        let launcher = FakeLauncher::start(&scratch);
        let syncthing = FakeSyncthing::start(&scratch).await;
        let service = Service::new(
            database().await,
            launcher.launcher(),
            syncthing.syncthing.clone(),
            Ok(Storage::Root(scratch.path().join("disk"))),
            Path::new("/opt/sync/bin/nas-sync-manager-axum"),
        );
        Self {
            scratch,
            launcher,
            syncthing,
            service,
        }
    }

    async fn paired(&self) -> Device {
        self.service
            .pair(Pairing {
                device_id: PC_ID.to_owned(),
                name: "Portatile".to_owned(),
                version: "0.1.0".to_owned(),
            })
            .await
            .expect("paired");
        self.service.client(Some(PC_ID)).await.expect("client")
    }

    async fn created(&self, device: &Device, nas_path: &str, mode: &str) -> Listed {
        self.service
            .create(
                device,
                NewSync {
                    name: "Foto".to_owned(),
                    nas_path: nas_path.to_owned(),
                    pc_path: "/home/me/Foto".to_owned(),
                    mode: mode.to_owned(),
                },
            )
            .await
            .expect("created")
    }
}

#[test]
fn reads_identities_and_conflict_copies() {
    assert!(valid_device_id(PC_ID));
    for wrong in [
        "",
        "short",
        "pcpcpcp-AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG",
    ] {
        assert!(!valid_device_id(wrong), "{wrong}");
    }
    assert_eq!(
        original_of("a.sync-conflict-20261010-120000-ABCDEFG.txt").as_deref(),
        Some("a.txt")
    );
    assert_eq!(
        original_of("Makefile.sync-conflict-20261010-120000-ABCDEFG").as_deref(),
        Some("Makefile")
    );
    for wrong in [
        "a.txt",
        ".sync-conflict-20261010-120000-ABCDEFG.txt",
        "a.sync-conflict-2026101X-120000-ABCDEFG.txt",
        "a.sync-conflict-20261010",
    ] {
        assert_eq!(original_of(wrong), None, "{wrong}");
    }
    assert!(new_folder_id().unwrap().starts_with("nas-"));
}

#[test]
fn the_state_of_a_sync_says_what_holds_it() {
    let sync = Sync {
        id: "nas-a".into(),
        device: PC_ID.into(),
        name: "Foto".into(),
        nas_path: "Foto".into(),
        pc_path: "/home".into(),
        mode: "two-way".into(),
        paused: false,
        revoked: false,
        created_at: 1,
    };
    let status = |state: &str, global: u64, need: u64, error: &str| syncthing::FolderStatus {
        state: state.into(),
        global_bytes: global,
        need_bytes: need,
        error: error.into(),
        ..Default::default()
    };
    let idle = status("idle", 100, 0, "");
    assert_eq!(
        state_of(&sync, Some(&idle), true),
        ("synced".into(), Some(100.0))
    );
    assert_eq!(
        state_of(&sync, Some(&status("syncing", 200, 50, "")), true),
        ("syncing".into(), Some(75.0))
    );
    assert_eq!(state_of(&sync, Some(&idle), false).0, "offline");
    assert_eq!(
        state_of(&sync, Some(&status("idle", 1, 0, "disk full")), true).0,
        "error"
    );
    assert_eq!(state_of(&sync, None, true).0, "unknown");
    let paused = Sync {
        paused: true,
        ..sync.clone()
    };
    assert_eq!(state_of(&paused, Some(&idle), true).0, "paused");
    let revoked = Sync {
        revoked: true,
        ..sync
    };
    assert_eq!(state_of(&revoked, Some(&idle), true).0, "revoked");
}

#[tokio::test]
async fn a_computer_pairs_creates_its_syncs_and_sees_only_its_own() {
    let bench = Bench::new().await;
    let refused = bench.service.client(Some(PC_ID)).await;
    assert_eq!(refused.unwrap_err().status, StatusCode::FORBIDDEN);
    assert_eq!(
        bench.service.client(None).await.unwrap_err().status,
        StatusCode::UNAUTHORIZED
    );
    let wrong = bench
        .service
        .pair(Pairing {
            device_id: "nope".into(),
            name: "x".into(),
            version: "1".into(),
        })
        .await;
    assert_eq!(wrong.unwrap_err().status, StatusCode::BAD_REQUEST);
    let answer = bench
        .service
        .pair(Pairing {
            device_id: PC_ID.into(),
            name: "Portatile".into(),
            version: "0.1.0".into(),
        })
        .await
        .unwrap();
    assert_eq!(answer, json!({ "deviceId": NAS_ID, "port": 22000 }));
    let device = bench.service.client(Some(PC_ID)).await.unwrap();

    let choices = bench.service.choices("").await.unwrap();
    assert_eq!(
        choices
            .iter()
            .map(|choice| choice.path.as_str())
            .collect::<Vec<_>>(),
        ["Documenti", "Foto"]
    );
    assert_eq!(
        bench.service.choices("Documenti").await.unwrap()[0].path,
        "Documenti/Lavoro"
    );
    assert!(bench.service.choices("../etc").await.is_err());

    let listed = bench.created(&device, "/Foto/", "pc-to-nas").await;
    assert_eq!(listed.sync.nas_path, "Foto");
    assert_eq!(listed.device_name, "Portatile");
    let folder = bench.syncthing.known.lock().unwrap().folders[&listed.sync.id].clone();
    assert_eq!(folder["type"], "receiveonly");
    assert_eq!(folder["devices"][0]["deviceID"], PC_ID);
    assert_eq!(
        folder["versioning"]["params"]["command"],
        "\"/opt/sync/bin/nas-sync-manager-axum\" --trash %FOLDER_PATH% %FILE_PATH%"
    );
    assert_eq!(folder["rescanIntervalS"], 3600);
    // On: in background, and the ports open.
    assert_eq!(
        bench
            .launcher
            .bodies("PUT /api/apps/self/sync")
            .last()
            .unwrap(),
        &json!({ "active": true })
    );

    // A folder inside one already synced, or holding one, is refused.
    for nested in ["Foto", "Foto/2026", ""] {
        std::fs::create_dir_all(bench.scratch.path().join("disk/Foto/2026")).unwrap();
        let refused = bench
            .service
            .create(
                &device,
                NewSync {
                    name: "Ancora".into(),
                    nas_path: nested.into(),
                    pc_path: "/home/me".into(),
                    mode: "two-way".into(),
                },
            )
            .await;
        assert_eq!(
            refused.unwrap_err().status,
            StatusCode::CONFLICT,
            "{nested}"
        );
    }
    for (name, mode) in [("", "two-way"), ("Foto", "sideways")] {
        let refused = bench
            .service
            .create(
                &device,
                NewSync {
                    name: name.into(),
                    nas_path: "Documenti".into(),
                    pc_path: "/home/me".into(),
                    mode: mode.into(),
                },
            )
            .await;
        assert_eq!(refused.unwrap_err().status, StatusCode::BAD_REQUEST);
    }

    // Another computer sees none of them.
    bench
        .service
        .pair(Pairing {
            device_id: OTHER_ID.into(),
            name: "Fisso".into(),
            version: "0.1.0".into(),
        })
        .await
        .unwrap();
    let other = bench.service.client(Some(OTHER_ID)).await.unwrap();
    assert!(bench
        .service
        .list(Some(&other.id))
        .await
        .unwrap()
        .is_empty());
    assert!(bench
        .service
        .change(&other, &listed.sync.id, SyncChange::default())
        .await
        .is_err());
    assert_eq!(bench.service.list(Some(PC_ID)).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_sync_is_changed_paused_revoked_and_removed() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let listed = bench.created(&device, "Foto", "two-way").await;
    let id = listed.sync.id.clone();
    bench.syncthing.known.lock().unwrap().connected = vec![PC_ID.to_owned()];
    assert_eq!(bench.service.list(None).await.unwrap()[0].state, "synced");

    let changed = bench
        .service
        .change(
            &device,
            &id,
            SyncChange {
                name: Some("Foto di casa".into()),
                pc_path: Some("/home/me/Immagini".into()),
                mode: Some("nas-to-pc".into()),
                paused: Some(false),
            },
        )
        .await
        .unwrap();
    assert_eq!(changed.sync.name, "Foto di casa");
    assert_eq!(
        bench.syncthing.known.lock().unwrap().folders[&id]["type"],
        "sendonly"
    );
    for wrong in [
        SyncChange {
            name: Some(" ".into()),
            ..Default::default()
        },
        SyncChange {
            pc_path: Some("\n".into()),
            ..Default::default()
        },
        SyncChange {
            mode: Some("both".into()),
            ..Default::default()
        },
    ] {
        assert!(bench.service.change(&device, &id, wrong).await.is_err());
    }

    let paused = bench.service.pause(&id, true).await.unwrap();
    assert!(paused.paused);
    assert_eq!(
        bench.syncthing.known.lock().unwrap().folders[&id]["paused"],
        true
    );
    assert_eq!(bench.service.list(None).await.unwrap()[0].state, "paused");
    assert_eq!(
        bench
            .launcher
            .bodies("PUT /api/apps/self/background")
            .last()
            .unwrap(),
        &json!({ "needed": false })
    );
    bench.service.pause(&id, false).await.unwrap();

    bench.service.revoke(&id).await.unwrap();
    assert!(!bench
        .syncthing
        .known
        .lock()
        .unwrap()
        .folders
        .contains_key(&id));
    let revoked = bench.service.list(Some(PC_ID)).await.unwrap();
    assert_eq!(revoked[0].state, "revoked");
    assert_eq!(
        bench.service.pause(&id, true).await.unwrap_err().status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        bench
            .service
            .change(&device, &id, SyncChange::default())
            .await
            .unwrap_err()
            .status,
        StatusCode::CONFLICT
    );
    bench.service.remove(&device, &id).await.unwrap();
    assert!(bench.service.list(None).await.unwrap().is_empty());
    assert_eq!(
        bench.service.revoke(&id).await.unwrap_err().status,
        StatusCode::NOT_FOUND
    );
    // The folder of the NAS stays.
    assert!(bench.scratch.path().join("disk/Foto").is_dir());
}

#[tokio::test]
async fn the_detail_shows_changes_conflicts_and_what_the_computer_leaves_out() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "two-way").await.sync.id;
    bench.scratch.write("disk/Foto/2026/a.jpg", "mine");
    bench.scratch.write(
        "disk/Foto/2026/a.sync-conflict-20261010-120000-ABCDEFG.jpg",
        "theirs",
    );
    bench.scratch.write(
        "disk/Foto/.stfolder/x.sync-conflict-20261010-120000-ABCDEFG.jpg",
        "no",
    );
    bench.scratch.write(
        "disk/Foto/b.sync-conflict-20261010-120000-ABCDEFG.txt",
        "copy",
    );
    bench.syncthing.known.lock().unwrap().ignores = vec!["*.tmp".into()];

    let detail = bench.service.detail(&id).await.unwrap();
    assert_eq!(detail["sync"]["id"], json!(id));
    assert_eq!(detail["ignores"], json!(["*.tmp"]));
    let conflicts = detail["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 2);
    assert_eq!(conflicts[0]["original"], "2026/a.jpg");
    assert_eq!(conflicts[1]["original"], "b.txt");

    // Keep the other: the copy takes the place of the file, which goes to the trash.
    bench
        .service
        .resolve(
            &id,
            "2026/a.sync-conflict-20261010-120000-ABCDEFG.jpg",
            "other",
        )
        .await
        .unwrap();
    let disk = bench.scratch.path().join("disk");
    assert_eq!(
        std::fs::read_to_string(disk.join("Foto/2026/a.jpg")).unwrap(),
        "theirs"
    );
    assert_eq!(
        std::fs::read_to_string(disk.join(".nas-trash/Foto/2026/a.jpg")).unwrap(),
        "mine"
    );
    // Keep this: the copy goes to the trash.
    bench
        .service
        .resolve(&id, "b.sync-conflict-20261010-120000-ABCDEFG.txt", "this")
        .await
        .unwrap();
    assert!(disk
        .join(".nas-trash/Foto/b.sync-conflict-20261010-120000-ABCDEFG.txt")
        .exists());
    for (path, keep, status) in [
        (
            "../x.sync-conflict-20261010-120000-ABCDEFG.txt",
            "this",
            StatusCode::BAD_REQUEST,
        ),
        ("plain.txt", "this", StatusCode::BAD_REQUEST),
        (
            "gone.sync-conflict-20261010-120000-ABCDEFG.txt",
            "this",
            StatusCode::NOT_FOUND,
        ),
    ] {
        let refused = bench.service.resolve(&id, path, keep).await;
        assert_eq!(refused.unwrap_err().status, status, "{path}");
    }
    bench.scratch.write(
        "disk/Foto/c.sync-conflict-20261010-120000-ABCDEFG.txt",
        "copy",
    );
    let refused = bench
        .service
        .resolve(&id, "c.sync-conflict-20261010-120000-ABCDEFG.txt", "both")
        .await;
    assert_eq!(refused.unwrap_err().status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_revoked_computer_loses_its_syncs_and_syncthing_gets_everything_again() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "two-way").await.sync.id;
    {
        let mut known = bench.syncthing.known.lock().unwrap();
        known.stats_device = json!({ PC_ID: { "lastSeen": "2026-10-11T10:00:00Z" } });
        known.stats_folder =
            json!({ id.clone(): { "lastFile": { "at": "2026-10-11T10:01:00Z" } } });
        // What Syncthing knows besides the database goes at the next start.
        known
            .folders
            .insert("stray".into(), json!({ "id": "stray" }));
        known
            .devices
            .insert(OTHER_ID.into(), json!({ "deviceID": OTHER_ID }));
        known.folders.remove(&id);
    }
    let devices = bench.service.devices().await.unwrap();
    assert_eq!(devices[0]["lastSeen"], "2026-10-11T10:00:00Z");
    assert_eq!(devices[0]["syncs"], 1);
    assert_eq!(
        bench.service.list(None).await.unwrap()[0]
            .last_sync
            .as_deref(),
        Some("2026-10-11T10:01:00Z")
    );
    assert!(!bench.service.list(None).await.unwrap()[0].trash_off);
    std::fs::write(bench.scratch.path().join("disk/.nas-trash-off"), "").unwrap();
    assert!(bench.service.list(None).await.unwrap()[0].trash_off);
    std::fs::remove_file(bench.scratch.path().join("disk/.nas-trash-off")).unwrap();
    // What never happened, as Syncthing writes it, is no date.
    bench.syncthing.known.lock().unwrap().stats_folder =
        json!({ id.clone(): { "lastFile": { "at": "0001-01-01T00:00:00Z" } } });
    assert_eq!(bench.service.list(None).await.unwrap()[0].last_sync, None);

    let saved = bench
        .service
        .save_settings(Settings {
            max_send_kbps: 300,
            max_recv_kbps: 0,
            rescan_minutes: 10,
        })
        .await
        .unwrap();
    assert_eq!(saved.rescan_minutes, 10);
    {
        let known = bench.syncthing.known.lock().unwrap();
        assert_eq!(known.options["maxSendKbps"], 300);
        assert!(!known.folders.contains_key("stray"));
        assert!(!known.devices.contains_key(OTHER_ID));
        assert_eq!(known.folders[&id]["rescanIntervalS"], 600);
    }
    assert!(bench
        .service
        .save_settings(Settings {
            max_send_kbps: 0,
            max_recv_kbps: 0,
            rescan_minutes: 0,
        })
        .await
        .is_err());

    bench.service.revoke_device(PC_ID).await.unwrap();
    assert!(bench.service.list(None).await.unwrap().is_empty());
    assert!(!bench
        .syncthing
        .known
        .lock()
        .unwrap()
        .devices
        .contains_key(PC_ID));
    assert_eq!(
        bench.service.revoke_device(PC_ID).await.unwrap_err().status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(bench.service.status().await["deviceId"], NAS_ID);
    assert_eq!(bench.service.status().await["failing"], false);
}

#[tokio::test]
async fn without_syncthing_or_the_disks_nothing_is_created() {
    let scratch = Scratch::new();
    let syncthing =
        Syncthing::new(scratch.path().join("missing"), scratch.path().join("home")).unwrap();
    let service = Service::new(
        database().await,
        Launcher::new(None),
        syncthing,
        Err("the data disks are not granted".into()),
        Path::new("/bin/app"),
    );
    let pairing = service
        .pair(Pairing {
            device_id: PC_ID.into(),
            name: "Portatile".into(),
            version: "0.1.0".into(),
        })
        .await;
    assert_eq!(pairing.unwrap_err().message, "Syncthing is not installed");
    assert!(service.choices("").await.is_err());
    assert!(service.volumes().is_empty());
    assert!(service.disks().is_empty());
    assert_eq!(service.status().await["installed"], false);
    assert!(service.devices().await.unwrap().is_empty());
    let failure = Failure::from(sqlx::Error::RowNotFound);
    assert_eq!(failure.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        Failure::missing("x").into_response().status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn conflict_resolution_rejects_escape_links_even_without_an_original() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "two-way").await.sync.id;
    let name = "a.sync-conflict-20261010-120000-ABCDEFG.txt";
    bench.scratch.write(&format!("outside/{name}"), "outside");
    let outside = bench.scratch.path().join("outside");
    std::os::unix::fs::symlink(&outside, bench.scratch.path().join("disk/Foto/escape")).unwrap();
    assert_eq!(
        bench
            .service
            .resolve(&id, &format!("escape/{name}"), "other")
            .await
            .unwrap_err()
            .status,
        StatusCode::BAD_REQUEST
    );
    assert!(outside.join(name).exists());
    assert!(!outside.join("a.txt").exists());
}

#[tokio::test]
async fn a_revoked_folder_can_be_paired_again_and_missing_folders_are_never_recreated() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let first = bench.created(&device, "Foto", "two-way").await;
    bench.service.revoke(&first.sync.id).await.unwrap();
    let second = bench.created(&device, "Foto", "two-way").await;
    assert_ne!(first.sync.id, second.sync.id);
    let folder = bench.scratch.path().join("disk/Foto");
    std::fs::remove_dir(&folder).unwrap();
    bench
        .syncthing
        .known
        .lock()
        .unwrap()
        .folders
        .remove(&second.sync.id);
    assert!(bench.service.reconcile().await.is_err());
    assert!(!folder.exists());
    assert!(!bench
        .syncthing
        .known
        .lock()
        .unwrap()
        .folders
        .contains_key(&second.sync.id));
}

#[tokio::test]
async fn simultaneous_requests_cannot_create_overlapping_syncs() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let input = || NewSync {
        name: "Foto".into(),
        nas_path: "Foto".into(),
        pc_path: "/home/me".into(),
        mode: "two-way".into(),
    };
    let (one, two) = tokio::join!(
        bench.service.create(&device, input()),
        bench.service.create(&device, input())
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    assert_eq!(
        one.err().or_else(|| two.err()).unwrap().status,
        StatusCode::CONFLICT
    );
    bench.service.revoke_device(&device.id).await.unwrap();
    assert_eq!(
        bench
            .service
            .create(&device, input())
            .await
            .unwrap_err()
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn a_restarted_engine_gets_the_saved_configuration_and_folders_again() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "two-way").await.sync.id;
    let service = std::sync::Arc::new(bench.service);
    let follower = tokio::spawn({
        let service = service.clone();
        async move { service.follow().await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    {
        let mut known = bench.syncthing.known.lock().unwrap();
        known.folders.clear();
        known.devices.clear();
        known.options = Value::Null;
    }
    bench.syncthing.restart(&bench.scratch).await;
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if bench
                .syncthing
                .known
                .lock()
                .unwrap()
                .folders
                .contains_key(&id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(bench
        .syncthing
        .known
        .lock()
        .unwrap()
        .devices
        .contains_key(PC_ID));
    assert_eq!(
        bench.syncthing.known.lock().unwrap().options["relaysEnabled"],
        false
    );
    follower.abort();
}

#[tokio::test]
async fn a_failed_engine_mutation_never_reports_a_pause_or_revocation_as_applied() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "two-way").await.sync.id;
    bench.syncthing.known.lock().unwrap().refuse = vec![format!("PUT /rest/config/folders/{id}")];
    assert_eq!(
        bench.service.pause(&id, true).await.unwrap_err().status,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert!(
        !store::sync(bench.service.pool(), &id)
            .await
            .unwrap()
            .unwrap()
            .paused
    );
    bench.syncthing.known.lock().unwrap().refuse =
        vec![format!("DELETE /rest/config/folders/{id}")];
    for result in [
        bench.service.revoke(&id).await,
        bench.service.remove(&device, &id).await,
        bench.service.revoke_device(&device.id).await,
    ] {
        assert_eq!(result.unwrap_err().status, StatusCode::SERVICE_UNAVAILABLE);
    }
    assert!(
        !store::sync(bench.service.pool(), &id)
            .await
            .unwrap()
            .unwrap()
            .revoked
    );
    assert!(bench.service.client(Some(&device.id)).await.is_ok());
    bench.syncthing.known.lock().unwrap().refuse =
        vec![format!("DELETE /rest/config/devices/{}", device.id)];
    assert!(bench.service.revoke_device(&device.id).await.is_err());
    assert!(bench.service.client(Some(&device.id)).await.is_ok());
    bench.syncthing.known.lock().unwrap().refuse.clear();
    bench.service.revoke_device(&device.id).await.unwrap();
    assert!(bench.service.client(Some(&device.id)).await.is_err());
}

#[tokio::test]
async fn an_upload_is_not_synced_until_the_peer_has_received_the_folder() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "nas-to-pc").await.sync.id;
    {
        let mut known = bench.syncthing.known.lock().unwrap();
        known.connected = vec![PC_ID.to_owned()];
        known.completion.insert(id.clone(), json!({ "remoteState": "valid", "globalBytes": 1000, "needBytes": 250, "needItems": 1 }));
    }
    let listed = bench.service.list(None).await.unwrap();
    assert_eq!(listed[0].state, "syncing");
    assert_eq!(listed[0].progress, Some(75.0));
    assert_eq!(listed[0].need_bytes, 250);
    for (remote_state, state) in [
        ("notSharing", "unknown"),
        ("paused", "paused"),
        ("valid", "synced"),
    ] {
        bench
            .syncthing
            .known
            .lock()
            .unwrap()
            .completion
            .insert(id.clone(), json!({ "remoteState": remote_state }));
        assert_eq!(bench.service.list(None).await.unwrap()[0].state, state);
    }
}

#[tokio::test]
async fn a_failed_folder_reconciliation_is_retried_without_restarting_the_engine() {
    let bench = Bench::new().await;
    let device = bench.paired().await;
    let id = bench.created(&device, "Foto", "two-way").await.sync.id;
    {
        let mut known = bench.syncthing.known.lock().unwrap();
        known.folders.clear();
        known.refuse = vec![format!("PUT /rest/config/folders/{id}")];
    }
    assert!(bench.service.reconcile().await.is_err());
    let service = std::sync::Arc::new(bench.service);
    let follower = tokio::spawn({
        let service = service.clone();
        async move { service.follow().await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    bench.syncthing.known.lock().unwrap().refuse.clear();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if bench
                .syncthing
                .known
                .lock()
                .unwrap()
                .folders
                .contains_key(&id)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    follower.abort();
}

#[tokio::test]
async fn the_nas_identity_cannot_be_registered_as_a_computer() {
    let bench = Bench::new().await;
    assert_eq!(
        bench
            .service
            .pair(Pairing {
                device_id: NAS_ID.into(),
                name: "NAS".into(),
                version: "1".into(),
            })
            .await
            .unwrap_err()
            .status,
        StatusCode::BAD_REQUEST
    );
    assert!(store::devices(bench.service.pool())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn failed_saved_settings_are_retried_after_the_engine_was_already_configured() {
    let bench = Bench::new().await;
    let service = std::sync::Arc::new(bench.service);
    let follower = tokio::spawn({
        let service = service.clone();
        async move { service.follow().await }
    });
    tokio::time::timeout(Duration::from_secs(4), async {
        while service.reconcile_needed.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    bench.syncthing.known.lock().unwrap().refuse = vec!["PATCH /rest/config/options".into()];
    assert!(service
        .save_settings(Settings {
            max_send_kbps: 123,
            max_recv_kbps: 456,
            rescan_minutes: 60,
        })
        .await
        .is_err());
    assert!(service.reconcile_needed.load(Ordering::Relaxed));
    bench.syncthing.known.lock().unwrap().refuse.clear();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if bench.syncthing.known.lock().unwrap().options["maxSendKbps"] == 123 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    follower.abort();
}
