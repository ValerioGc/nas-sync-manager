//! The door to the launcher, `NAS_LAUNCHER_SOCKET`: the launcher knows this application by its
//! cgroup and lets it write only as itself. Through it the application says whether it has work to
//! do without its window and whether a sync is on, which opens the ports of the sync to the local
//! network.
//!
//! Without the socket (in development) nothing is sent.

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

    /// A request whose answer matters only to the journal: nothing without the socket.
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

    /// Says whether this application has work to do without its window.
    pub async fn background(&self, needed: bool) {
        self.send(
            "PUT",
            "/api/apps/self/background",
            &json!({ "needed": needed }),
        )
        .await;
    }

    /// Says whether a sync is on: the launcher opens the ports of the sync meanwhile.
    pub async fn sync_ports(&self, active: bool) {
        self.send("PUT", "/api/apps/self/sync", &json!({ "active": active }))
            .await;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/axum/launcher.rs"]
mod tests;
