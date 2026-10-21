use crate::http::{exchange, Answer};
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default)]
pub struct Launcher {
    socket: Option<PathBuf>,
}

impl Launcher {
    pub fn new(socket: Option<PathBuf>) -> Self {
        Self { socket }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var_os("NAS_LAUNCHER_SOCKET")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        )
    }

    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Answer, String> {
        let socket = self
            .socket
            .as_ref()
            .ok_or("the launcher is not reachable")?;
        let body = body.map(Value::to_string).unwrap_or_default();
        exchange(socket, method, path, &[], &body, TIMEOUT)
            .await
            .map_err(|reason| format!("the launcher: {reason}"))
    }

    async fn send(&self, method: &str, path: &str, body: &Value) {
        if self.socket.is_none() {
            return;
        }
        match self.request(method, path, Some(body)).await {
            Ok(answer) if answer.status < 300 => {}
            Ok(answer) => eprintln!("<4>the launcher refused {method} {path}: {}", answer.body),
            Err(reason) => eprintln!("<4>{method} {path} was not sent: {reason}"),
        }
    }

    /// Declares whether the app needs to keep running in the background.
    pub async fn background(&self, needed: bool) {
        self.send(
            "PUT",
            "/api/apps/self/background",
            &json!({ "needed": needed }),
        )
        .await;
    }

    /// Declares whether a sync is active, so the launcher opens the sync ports.
    pub async fn sync_ports(&self, active: bool) {
        self.send("PUT", "/api/apps/self/sync", &json!({ "active": active }))
            .await;
    }
}

#[cfg(test)]
#[path = "../tests/unit/axum/launcher.rs"]
mod tests;
