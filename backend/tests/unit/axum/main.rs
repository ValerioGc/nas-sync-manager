use super::*;
use crate::test_support::{database, FakeLauncher, FakeSyncthing, Scratch, NAS_ID, PC_ID};

struct Server {
    _scratch: Scratch,
    socket: PathBuf,
    fake: FakeSyncthing,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn new() -> Self {
        let scratch = Scratch::new();
        scratch.write("ui/index.html", "<html>Sync</html>");
        scratch.write("ui/assets/app.js", "console.log('sync')");
        scratch.write("disk/Foto/a.txt", "a");
        let fake = FakeSyncthing::start(&scratch).await;
        let launcher = FakeLauncher::start(&scratch);
        let service = Arc::new(Service::new(
            database().await,
            launcher.launcher(),
            fake.syncthing.clone(),
            Ok(storage::Storage::Root(scratch.path().join("disk"))),
            Path::new("/opt/app"),
        ));
        let socket = scratch.path().join("app.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let app = router(AppState {
            ui_dir: scratch.path().join("ui"),
            service,
        });
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            _scratch: scratch,
            socket,
            fake,
            task,
        }
    }
    async fn call(
        &self,
        method: &str,
        path: &str,
        device: Option<&str>,
        body: Value,
    ) -> (u16, Value) {
        let headers: Vec<_> = device.map(|id| (CLIENT, id)).into_iter().collect();
        let answer = http::exchange(
            &self.socket,
            method,
            path,
            &headers,
            &if body.is_null() {
                String::new()
            } else {
                body.to_string()
            },
            Duration::from_secs(2),
        )
        .await
        .unwrap();
        (
            answer.status,
            serde_json::from_str(&answer.body).unwrap_or(Value::Null),
        )
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[tokio::test]
async fn routes_pair_a_computer_and_manage_its_sync_until_revocation() {
    let server = Server::new().await;
    assert_eq!(
        server.call("GET", "/health", None, Value::Null).await.1["status"],
        "ok"
    );
    assert_eq!(
        server.call("GET", "/api/status", None, Value::Null).await.1["deviceId"],
        NAS_ID
    );
    for path in ["/api/client/folders", "/api/client/syncs"] {
        assert_eq!(server.call("GET", path, None, Value::Null).await.0, 401);
        assert_eq!(
            server.call("GET", path, Some(PC_ID), Value::Null).await.0,
            403
        );
    }
    assert_eq!(
        server
            .call(
                "POST",
                "/api/client/pair",
                None,
                json!({ "deviceId": PC_ID, "name": "PC", "version": "1" })
            )
            .await
            .0,
        401
    );
    assert_eq!(
        server
            .call(
                "POST",
                "/api/client/pair",
                Some(PC_ID),
                json!({ "deviceId": NAS_ID, "name": "PC", "version": "1" })
            )
            .await
            .0,
        403
    );
    assert_eq!(
        server
            .call(
                "POST",
                "/api/client/pair",
                Some("bad"),
                json!({ "deviceId": "bad", "name": "PC", "version": "1" })
            )
            .await
            .0,
        400
    );
    assert_eq!(
        server
            .call(
                "POST",
                "/api/client/pair",
                Some(PC_ID),
                json!({ "deviceId": PC_ID, "name": "Portatile", "version": "1" })
            )
            .await
            .0,
        200
    );
    assert_eq!(
        server
            .call(
                "GET",
                "/api/client/folders?path=Foto",
                Some(PC_ID),
                Value::Null
            )
            .await
            .0,
        200
    );
    assert_eq!(
        server
            .call("GET", "/api/client/folders", Some(PC_ID), Value::Null)
            .await
            .1[0]["name"],
        "Foto"
    );
    let (code, sync) = server
        .call(
            "POST",
            "/api/client/syncs",
            Some(PC_ID),
            json!({ "name": "Foto", "nasPath": "Foto", "pcPath": "/home/Foto", "mode": "two-way" }),
        )
        .await;
    assert_eq!(code, 200);
    let id = sync["id"].as_str().unwrap();
    assert_eq!(
        server
            .call("GET", "/api/client/syncs", Some(PC_ID), Value::Null)
            .await
            .1[0]["id"],
        id
    );
    assert_eq!(
        server.call("GET", "/api/syncs", None, Value::Null).await.1[0]["name"],
        "Foto"
    );
    let detail = format!("/api/syncs/{id}");
    assert_eq!(
        server.call("GET", &detail, None, Value::Null).await.1["sync"]["id"],
        id
    );
    for action in ["pause", "resume"] {
        assert_eq!(
            server
                .call("POST", &format!("{detail}/{action}"), None, Value::Null)
                .await
                .0,
            200
        );
    }
    assert_eq!(
        server
            .call(
                "PUT",
                &format!("/api/client/syncs/{id}"),
                Some(PC_ID),
                json!({ "name": "Nuovo", "paused": true })
            )
            .await
            .1["paused"],
        true
    );
    server._scratch.write(
        "disk/Foto/a.sync-conflict-20261010-120000-ABCDEFG.txt",
        "copy",
    );
    assert_eq!(
        server
            .call(
                "POST",
                &format!("{detail}/conflicts"),
                None,
                json!({ "path": "a.sync-conflict-20261010-120000-ABCDEFG.txt", "keep": "this" })
            )
            .await
            .0,
        204
    );
    assert_eq!(
        server
            .call("GET", "/api/volumes", None, Value::Null)
            .await
            .1,
        json!([])
    );
    assert_eq!(
        server
            .call("GET", "/api/devices", None, Value::Null)
            .await
            .1[0]["name"],
        "Portatile"
    );
    assert_eq!(
        server
            .call("GET", "/api/settings", None, Value::Null)
            .await
            .1["rescanMinutes"],
        60
    );
    assert_eq!(
        server
            .call(
                "PUT",
                "/api/settings",
                None,
                json!({ "maxSendKbps": 10, "maxRecvKbps": 20, "rescanMinutes": 30 })
            )
            .await
            .0,
        200
    );
    assert_eq!(
        server.fake.known.lock().unwrap().options["limitBandwidthInLan"],
        true
    );
    assert_eq!(
        server
            .call(
                "PUT",
                "/api/settings",
                None,
                json!({ "maxSendKbps": 0, "maxRecvKbps": 0, "rescanMinutes": 0 })
            )
            .await
            .0,
        400
    );
    assert_eq!(
        server
            .call("POST", &format!("{detail}/revoke"), None, Value::Null)
            .await
            .0,
        204
    );
    assert_eq!(
        server.call("GET", &detail, None, Value::Null).await.1["sync"]["state"],
        "revoked"
    );
    assert_eq!(
        server
            .call(
                "DELETE",
                &format!("/api/client/syncs/{id}"),
                Some(PC_ID),
                Value::Null
            )
            .await
            .0,
        204
    );
    assert_eq!(server.call("GET", &detail, None, Value::Null).await.0, 404);
    assert_eq!(
        server
            .call(
                "DELETE",
                &format!("/api/devices/{PC_ID}"),
                None,
                Value::Null
            )
            .await
            .0,
        204
    );
    assert_eq!(
        server
            .call("GET", "/api/client/syncs", Some(PC_ID), Value::Null)
            .await
            .0,
        403
    );
    assert!(server._scratch.path().join("disk/Foto/a.txt").exists());
}

#[tokio::test]
async fn serves_only_existing_assets_and_never_html_for_an_unknown_api() {
    let server = Server::new().await;
    for (path, status, mime, cache) in [
        ("/", 200, "text/html", "no-cache"),
        ("/assets/app.js", 200, "text/javascript", "immutable"),
        ("/gone", 404, "", ""),
        ("/api/unknown", 404, "", ""),
        ("/api", 404, "", ""),
    ] {
        let answer = http::exchange(&server.socket, "GET", path, &[], "", Duration::from_secs(2))
            .await
            .unwrap();
        assert_eq!(answer.status, status, "{path}");
        let mut stream = UnixStream::connect(&server.socket).await.unwrap();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        assert!(raw.contains(mime) && raw.contains(cache), "{raw}");
    }
    for (extension, mime) in [
        ("css", "text/css"),
        ("svg", "image/svg+xml"),
        ("png", "image/png"),
        ("webp", "image/webp"),
        ("ico", "image/x-icon"),
        ("woff2", "font/woff2"),
        ("dat", "application/octet-stream"),
    ] {
        server
            ._scratch
            .write(&format!("ui/file.{extension}"), "asset");
        let state = AppState {
            ui_dir: server._scratch.path().join("ui"),
            service: Arc::new(Service::new(
                database().await,
                launcher::Launcher::new(None),
                server.fake.syncthing.clone(),
                Err("none".into()),
                Path::new("/bin/app"),
            )),
        };
        let response =
            static_file(State(state), format!("/file.{extension}").parse().unwrap()).await;
        assert!(response.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with(mime));
    }
}

#[test]
fn validates_runtime_paths_and_trash_arguments() {
    assert_eq!(
        database_path(Some("data.db".into()), None).unwrap(),
        PathBuf::from("data.db")
    );
    assert_eq!(
        database_path(None, Some("/state:/other".into())).unwrap(),
        PathBuf::from("/state/sync-manager.sqlite3")
    );
    assert!(database_path(None, None)
        .unwrap()
        .ends_with("sync-manager.sqlite3"));
    assert!(socket_path(vec![], None).is_err());
    assert!(socket_path(vec!["--listen".into()], None).is_err());
    assert!(socket_path(vec!["--wrong".into()], None).is_err());
    assert!(socket_path(vec![], Some("".into())).is_err());
    assert_eq!(
        socket_path(
            vec!["--listen".into(), "/tmp/app.sock".into()],
            Some("ignored".into())
        )
        .unwrap(),
        PathBuf::from("/tmp/app.sock")
    );
    assert!(trash_command(&[]).is_none());
    assert!(trash_command(&["--trash".into()]).unwrap().is_err());
    let scratch = Scratch::new();
    assert_eq!(
        ui_dir_for(Some(scratch.path().into()), Err("none".into())).unwrap(),
        scratch.path()
    );
    assert!(ui_dir_for(None, Err("no executable".into())).is_err());
    assert!(ui_dir_for(None, Ok(PathBuf::from("/"))).is_err());
    scratch.write("dist/index.html", "ui");
    assert_eq!(
        ui_dir_for(None, Ok(scratch.path().join("bin/app"))).unwrap(),
        scratch.path().join("dist")
    );
    assert!(ui_dir_for(None, Ok(scratch.path().join("else/bin/app")))
        .unwrap()
        .ends_with("dist/axum"));
}

#[tokio::test]
async fn protects_an_occupied_socket_and_cleans_up_after_shutdown() {
    let scratch = Scratch::new();
    let path = scratch.path().join("app.sock");
    prepare_socket(&path).await.unwrap();
    let occupied = UnixListener::bind(&path).unwrap();
    assert!(prepare_socket(&path)
        .await
        .unwrap_err()
        .contains("already listening"));
    drop(occupied);
    // Other test threads may be forking their stand-in executable as this listener closes.
    tokio::time::timeout(Duration::from_secs(2), async {
        while prepare_socket(&path).await.is_err() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(!path.exists());
    scratch.write("app.sock", "keep");
    assert!(prepare_socket(&path)
        .await
        .unwrap_err()
        .contains("occupied"));
    std::fs::remove_file(&path).unwrap();
    let database = scratch.path().join("state/app.db");
    let missing = run(
        vec![],
        Some(path.clone().into_os_string()),
        Some(scratch.path().join("missing")),
        database.clone(),
        std::future::ready(()),
    )
    .await;
    assert!(missing
        .unwrap_err()
        .to_string()
        .contains("Frontend not found"));
    scratch.write("ui/index.html", "ui");
    run(
        vec![],
        Some(path.clone().into_os_string()),
        Some(scratch.path().join("ui")),
        database.clone(),
        std::future::ready(()),
    )
    .await
    .unwrap();
    assert!(!path.exists());
    assert!(database.exists());
}
