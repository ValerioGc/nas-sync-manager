use super::*;
use crate::test_support::{FakeSyncthing, Scratch, NAS_ID, PC_ID};

#[test]
fn the_key_of_the_api_is_made_once_and_kept_private() {
    let scratch = Scratch::new();
    let home = scratch.path().join("home");
    let key = key_in(&home).expect("key");
    assert_eq!(key.len(), 32);
    assert_eq!(key_in(&home).expect("same key"), key);
    assert_eq!(
        fs::metadata(home.join("api-key"))
            .expect("file")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    fs::write(home.join("api-key"), "short").expect("broken");
    assert_ne!(key_in(&home).expect("new key"), "short");
    assert_ne!(random_key().expect("one"), random_key().expect("another"));
    let engine = Syncthing::new(PathBuf::from("/usr/bin/syncthing"), home).unwrap();
    let command = engine.command();
    let command = command.as_std();
    assert!(command
        .get_args()
        .all(|arg| !arg.to_string_lossy().contains(&engine.key)));
    assert!(command
        .get_envs()
        .any(|(name, value)| name == "STGUIAPIKEY"
            && value == Some(std::ffi::OsStr::new(&engine.key))));
    assert!(command
        .get_envs()
        .any(|(name, value)| name == "HOME" && value == Some(engine.home.as_os_str())));
}

#[test]
fn each_mode_is_a_kind_of_folder_on_the_nas() {
    assert_eq!(folder_kind("two-way"), "sendreceive");
    assert_eq!(folder_kind("pc-to-nas"), "receiveonly");
    assert_eq!(folder_kind("nas-to-pc"), "sendonly");
}

#[tokio::test]
async fn runs_syncthing_and_speaks_to_it_with_its_key() {
    let scratch = Scratch::new();
    let fake = FakeSyncthing::start(&scratch).await;
    let syncthing = &fake.syncthing;
    assert!(syncthing.is_installed());
    assert!(syncthing.starts() >= 1);
    assert_eq!(syncthing.device_id().await.expect("id"), NAS_ID);

    syncthing.configure(100, 200).await.expect("configured");
    let options = fake.known.lock().unwrap().options.clone();
    assert_eq!(options["globalAnnounceEnabled"], false);
    assert_eq!(options["relaysEnabled"], false);
    assert_eq!(options["urAccepted"], -1);
    assert_eq!(options["maxSendKbps"], 100);
    assert_eq!(options["limitBandwidthInLan"], true);
    assert_eq!(
        options["listenAddresses"],
        json!(["tcp://:22000", "quic://:22000"])
    );

    syncthing
        .add_device(PC_ID, "Portatile")
        .await
        .expect("device");
    assert!(syncthing
        .devices()
        .await
        .expect("devices")
        .contains(&PC_ID.to_owned()));
    let folder = Folder {
        id: "nas-abc".into(),
        label: "Foto".into(),
        path: scratch.path().join("Foto"),
        kind: "receiveonly",
        device: PC_ID.into(),
        paused: false,
        rescan_seconds: 3600,
        trash_command: "trash %FOLDER_PATH% %FILE_PATH%".into(),
    };
    syncthing.put_folder(&folder).await.expect("folder");
    let given = fake.known.lock().unwrap().folders["nas-abc"].clone();
    assert_eq!(given["type"], "receiveonly");
    assert_eq!(given["versioning"]["type"], "external");
    assert!(fake
        .known
        .lock()
        .unwrap()
        .ignores
        .contains(&"/.nas-trash".to_owned()));
    assert!(fake
        .known
        .lock()
        .unwrap()
        .ignores
        .contains(&".nas-backup-*".to_owned()));
    assert_eq!(syncthing.folders().await.expect("folders"), ["nas-abc"]);
    assert_eq!(
        syncthing
            .folder_status("nas-abc")
            .await
            .expect("status")
            .state,
        "idle"
    );

    {
        let mut known = fake.known.lock().unwrap();
        known.connected = vec![PC_ID.to_owned()];
        known.ignores = vec!["*.tmp".to_owned()];
        known.events = json!([
            { "type": "LocalChangeDetected", "time": "t1",
              "data": { "folder": "nas-abc", "action": "modified", "path": "a.jpg" } },
            { "type": "RemoteChangeDetected", "time": "t2",
              "data": { "folder": "nas-abc", "action": "deleted", "path": "b.jpg" } },
            { "type": "RemoteChangeDetected", "time": "t3",
              "data": { "folder": "other", "action": "added", "path": "c.jpg" } }
        ]);
    }
    assert_eq!(syncthing.connected().await.expect("connected"), [PC_ID]);
    assert_eq!(syncthing.ignores("nas-abc").await, ["*.tmp"]);
    let changes = syncthing.changes("nas-abc").await;
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["path"], "b.jpg");
    let (devices, folders) = syncthing.stats().await;
    assert!(devices.is_null() && folders.is_null());

    syncthing.remove_folder("nas-abc").await.expect("removed");
    assert!(syncthing.remove_folder("nas-abc").await.is_err());
    syncthing.remove_device(PC_ID).await.expect("removed");
    assert!(syncthing.folder_status("missing").await.is_err());
    assert!(fake.asked().contains(&"GET /rest/system/ping".to_owned()));
}

#[tokio::test]
async fn a_syncthing_that_stops_before_answering_is_failing_until_it_runs() {
    let scratch = Scratch::new();
    let syncthing =
        Syncthing::new(PathBuf::from("/bin/false"), scratch.path().join("home")).expect("new");
    assert!(syncthing.is_installed());
    assert!(!syncthing.is_failing());
    let supervising = tokio::spawn(syncthing.clone().supervise());
    for _ in 0..100 {
        if syncthing.is_failing() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(syncthing.is_failing());
    assert!(!syncthing.is_running());
    supervising.abort();
}

#[tokio::test]
async fn a_syncthing_that_is_not_installed_never_runs() {
    let scratch = Scratch::new();
    let syncthing =
        Syncthing::new(scratch.path().join("missing"), scratch.path().join("home")).expect("new");
    assert!(!syncthing.is_installed());
    assert!(!syncthing.is_running());
    assert!(syncthing.get("/rest/system/ping").await.is_err());
    let supervising = tokio::spawn(syncthing.clone().supervise());
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!syncthing.is_running());
    supervising.abort();
    assert_eq!(
        Syncthing::program_from_env(),
        PathBuf::from("/usr/bin/syncthing")
    );
}

#[tokio::test]
async fn folders_start_only_after_exclusions_are_installed_and_stay_paused_on_failure() {
    let scratch = Scratch::new();
    let fake = FakeSyncthing::start(&scratch).await;
    let folder = Folder {
        id: "nas-protected".into(),
        label: "Disk".into(),
        path: scratch.path().to_path_buf(),
        kind: "sendreceive",
        device: PC_ID.into(),
        paused: false,
        rescan_seconds: 3600,
        trash_command: "trash %FOLDER_PATH% %FILE_PATH%".into(),
    };
    fake.syncthing.put_folder(&folder).await.unwrap();
    let requests = fake.requests.lock().unwrap().clone();
    let configurations: Vec<_> = requests
        .iter()
        .filter(|(line, _)| line == "PUT /rest/config/folders/nas-protected")
        .collect();
    assert_eq!(configurations.len(), 2);
    assert_eq!(configurations[0].1["paused"], true);
    assert_eq!(configurations[1].1["paused"], false);
    let ignores = requests
        .iter()
        .position(|(line, _)| line == "POST /rest/db/ignores?folder=nas-protected")
        .unwrap();
    let resume = requests
        .iter()
        .rposition(|(line, _)| line == "PUT /rest/config/folders/nas-protected")
        .unwrap();
    assert!(ignores < resume);
    {
        let mut known = fake.known.lock().unwrap();
        known.ignores.clear();
        known.refuse = vec!["POST /rest/db/ignores?folder=nas-protected".into()];
    }
    assert!(fake.syncthing.put_folder(&folder).await.is_err());
    assert_eq!(
        fake.known.lock().unwrap().folders[&folder.id]["paused"],
        true
    );
    {
        let mut known = fake.known.lock().unwrap();
        known.refuse.clear();
        known.ignores_error = "cannot read .stignore".into();
    }
    assert!(fake.syncthing.put_folder(&folder).await.is_err());
    assert_eq!(
        fake.known.lock().unwrap().folders[&folder.id]["paused"],
        true
    );
}
