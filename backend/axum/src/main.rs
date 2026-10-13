mod http;
mod launcher;
mod service;
mod storage;
mod store;
mod syncthing;
mod trash;

use axum::{
    extract::{Path as Param, Query, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use service::{Failure, NewSync, Pairing, Service, SyncChange};
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions},
};
use std::{
    env,
    ffi::OsString,
    os::unix::fs::FileTypeExt,
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// The header that names the computer a client request comes from. The gateway sets it for a
/// computer it recognized by its certificate (and drops one a browser sends); the page never does.
const CLIENT: &str = "x-nas-client";

#[derive(Clone)]
struct AppState {
    ui_dir: PathBuf,
    service: Arc<Service>,
}

type Answer<T> = Result<Json<T>, Failure>;

async fn health() -> impl IntoResponse {
    Json(json!({"status":"ok","version":env!("CARGO_PKG_VERSION")}))
}

async fn status(State(state): State<AppState>) -> Json<Value> {
    Json(state.service.status().await)
}

async fn syncs(State(state): State<AppState>) -> Answer<Vec<service::Listed>> {
    Ok(Json(state.service.list(None).await?))
}

async fn sync_detail(State(state): State<AppState>, Param(id): Param<String>) -> Answer<Value> {
    Ok(Json(state.service.detail(&id).await?))
}

async fn pause(State(state): State<AppState>, Param(id): Param<String>) -> Answer<store::Sync> {
    Ok(Json(state.service.pause(&id, true).await?))
}

async fn resume(State(state): State<AppState>, Param(id): Param<String>) -> Answer<store::Sync> {
    Ok(Json(state.service.pause(&id, false).await?))
}

async fn revoke(
    State(state): State<AppState>,
    Param(id): Param<String>,
) -> Result<StatusCode, Failure> {
    state.service.revoke(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Resolve {
    path: String,
    keep: String,
}

async fn resolve(
    State(state): State<AppState>,
    Param(id): Param<String>,
    Json(input): Json<Resolve>,
) -> Result<StatusCode, Failure> {
    state.service.resolve(&id, &input.path, &input.keep).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn volumes(State(state): State<AppState>) -> Json<Vec<storage::VolumeName>> {
    Json(state.service.volumes())
}

async fn devices(State(state): State<AppState>) -> Answer<Vec<Value>> {
    Ok(Json(state.service.devices().await?))
}

async fn revoke_device(
    State(state): State<AppState>,
    Param(id): Param<String>,
) -> Result<StatusCode, Failure> {
    state.service.revoke_device(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn settings(State(state): State<AppState>) -> Answer<store::Settings> {
    Ok(Json(store::settings(state.service.pool()).await?))
}

async fn save_settings(
    State(state): State<AppState>,
    Json(settings): Json<store::Settings>,
) -> Answer<store::Settings> {
    Ok(Json(state.service.save_settings(settings).await?))
}

/// The computer a client request speaks for.
fn client_of(headers: &HeaderMap) -> Option<&str> {
    headers.get(CLIENT).and_then(|value| value.to_str().ok())
}

async fn pair(State(state): State<AppState>, Json(pairing): Json<Pairing>) -> Answer<Value> {
    Ok(Json(state.service.pair(pairing).await?))
}

#[derive(Debug, Deserialize)]
struct PathQuery {
    path: Option<String>,
}

async fn choices(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PathQuery>,
) -> Answer<Vec<service::Choice>> {
    state.service.client(client_of(&headers)).await?;
    Ok(Json(
        state
            .service
            .choices(query.path.as_deref().unwrap_or_default())
            .await?,
    ))
}

async fn client_syncs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Answer<Vec<service::Listed>> {
    let device = state.service.client(client_of(&headers)).await?;
    Ok(Json(state.service.list(Some(&device.id)).await?))
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<NewSync>,
) -> Answer<service::Listed> {
    let device = state.service.client(client_of(&headers)).await?;
    Ok(Json(state.service.create(&device, input).await?))
}

async fn change(
    State(state): State<AppState>,
    headers: HeaderMap,
    Param(id): Param<String>,
    Json(input): Json<SyncChange>,
) -> Answer<service::Listed> {
    let device = state.service.client(client_of(&headers)).await?;
    Ok(Json(state.service.change(&device, &id, input).await?))
}

async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Param(id): Param<String>,
) -> Result<StatusCode, Failure> {
    let device = state.service.client(client_of(&headers)).await?;
    state.service.remove(&device, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// What stops the application before it serves.
type Fatal = Box<dyn std::error::Error + Send + Sync>;

/// `NAS_DB_PATH` when set, otherwise the database in the first state directory of the unit.
fn database_path(
    configured: Option<OsString>,
    state_directory: Option<OsString>,
) -> Result<PathBuf, String> {
    if let Some(path) = configured.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    if let Some(directory) = state_directory
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.split(':').next().map(str::to_owned))
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(directory).join("sync-manager.sqlite3"));
    }
    if cfg!(debug_assertions) {
        return Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/sync-manager.sqlite3"));
    }
    Err("Set NAS_DB_PATH or run under systemd with STATE_DIRECTORY.".to_owned())
}

async fn open_database(path: &Path) -> Result<SqlitePool, Fatal> {
    if let Some(directory) = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(directory).await?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// `NAS_UI_DIR` when set, the `dist` of the package beside the binary, or the build of the repository.
fn ui_dir_for(
    configured: Option<PathBuf>,
    executable: Result<PathBuf, String>,
) -> Result<PathBuf, String> {
    if let Some(path) = configured {
        return Ok(path);
    }
    let executable = executable?;
    let binary_dir = executable.parent().ok_or("Invalid executable path")?;
    let packaged = binary_dir
        .parent()
        .ok_or("Invalid executable path")?
        .join("dist");
    if packaged.join("index.html").is_file() {
        Ok(packaged)
    } else {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dist/axum"))
    }
}

async fn static_file(State(state): State<AppState>, uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") || path == "/api" {
        return StatusCode::NOT_FOUND.into_response();
    }
    let requested = if path == "/" {
        "index.html"
    } else {
        &path[1..]
    };
    let relative = Path::new(requested);
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let file = state.ui_dir.join(relative);
    let mime = match file
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    };
    // The files of `assets/` carry a hash of their content in the name: the browser keeps them for
    // good. The page that names them is asked for again every time.
    let cache = if requested.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    match tokio::fs::read(file).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn prepare_socket(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = tokio::fs::symlink_metadata(path).await {
        if !metadata.file_type().is_socket() {
            return Err(format!("Socket path is occupied: {}", path.display()));
        }
        match UnixStream::connect(path).await {
            Ok(_) => return Err(format!("Socket is already listening: {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {}
            Err(error) => return Err(format!("Unable to check socket: {error}")),
        }
        tokio::fs::remove_file(path)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// The socket of `--listen`, or the one the launcher configured.
fn socket_path(args: Vec<OsString>, configured: Option<OsString>) -> Result<PathBuf, String> {
    let mut listen = configured;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--listen" {
            listen = Some(args.next().ok_or("--listen requires a socket path")?);
        } else {
            return Err(format!("Unrecognized argument: {}", arg.to_string_lossy()));
        }
    }
    let path = PathBuf::from(listen.ok_or("Provide --listen or NAS_APP_SOCKET")?);
    if path.as_os_str().is_empty() {
        return Err("Empty socket path".into());
    }
    Ok(path)
}

/// Serves the sync and the interface on the socket until `shutdown_signal` completes.
async fn run(
    args: Vec<OsString>,
    configured_socket: Option<OsString>,
    configured_ui: Option<PathBuf>,
    database: PathBuf,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) -> Result<(), Fatal> {
    let path = socket_path(args, configured_socket)?;
    let ui_dir = ui_dir_for(
        configured_ui,
        env::current_exe().map_err(|error| error.to_string()),
    )?;
    if !ui_dir.join("index.html").is_file() {
        return Err(format!("Frontend not found: {}", ui_dir.display()).into());
    }
    let home = database
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        .join("syncthing");
    let pool = open_database(&database).await?;
    let syncthing = syncthing::Syncthing::new(syncthing::Syncthing::program_from_env(), home)?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let service = Arc::new(Service::new(
        pool,
        launcher::Launcher::from_env(),
        syncthing.clone(),
        storage::Storage::from_env(),
        &executable,
    ));
    prepare_socket(&path).await?;
    let listener = UnixListener::bind(&path)?;
    let app = router(AppState {
        ui_dir,
        service: Arc::clone(&service),
    });
    println!("nas-sync-manager listening on {}", path.display());
    // What runs without the window: Syncthing, given everything whenever it starts, and the
    // launcher told whether a sync is on.
    let supervisor = tokio::spawn(syncthing.supervise());
    let follower = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.follow().await }
    });
    let declarations = tokio::spawn({
        let service = Arc::clone(&service);
        async move {
            loop {
                service.declare().await;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
        }
    });
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await;
    supervisor.abort();
    follower.abort();
    declarations.abort();
    if let Err(error) = tokio::fs::remove_file(&path).await {
        eprintln!("Unable to remove socket: {error}");
    }
    result?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/status", get(status))
        .route("/api/syncs", get(syncs))
        .route("/api/syncs/{id}", get(sync_detail))
        .route("/api/syncs/{id}/pause", post(pause))
        .route("/api/syncs/{id}/resume", post(resume))
        .route("/api/syncs/{id}/revoke", post(revoke))
        .route("/api/syncs/{id}/conflicts", post(resolve))
        .route("/api/volumes", get(volumes))
        .route("/api/devices", get(devices))
        .route("/api/devices/{id}", delete(revoke_device))
        .route("/api/settings", get(settings).put(save_settings))
        .route("/api/client/pair", post(pair))
        .route("/api/client/folders", get(choices))
        .route("/api/client/syncs", get(client_syncs).post(create))
        .route("/api/client/syncs/{id}", put(change).delete(remove))
        .fallback(get(static_file))
        .with_state(state)
}

/// `--trash <folder> <file>`: the versioning command Syncthing runs for what it deletes or
/// replaces, which goes to the trash of its disk.
fn trash_command(args: &[OsString]) -> Option<Result<(), Fatal>> {
    if args.first().is_none_or(|first| first != "--trash") {
        return None;
    }
    let [_, folder, file] = args else {
        return Some(Err("usage: --trash <folder> <file>".into()));
    };
    let disks = match storage::Storage::from_env() {
        Ok(storage::Storage::Volumes(volumes)) => {
            volumes.into_iter().map(|volume| volume.path).collect()
        }
        Ok(storage::Storage::Root(root)) => vec![root],
        Err(_) => Vec::new(),
    };
    Some(
        trash::discard(Path::new(folder), Path::new(file), &disks)
            .map_err(|error| error.to_string().into()),
    )
}

#[tokio::main]
async fn main() -> Result<(), Fatal> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    if let Some(result) = trash_command(&args) {
        return result;
    }
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    run(
        args,
        env::var_os("NAS_APP_SOCKET"),
        env::var_os("NAS_UI_DIR").map(PathBuf::from),
        database_path(env::var_os("NAS_DB_PATH"), env::var_os("STATE_DIRECTORY"))?,
        async move {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
        },
    )
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/axum/main.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/axum/support.rs"]
mod test_support;
