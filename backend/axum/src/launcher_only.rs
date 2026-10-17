//! The socket of the application answers the launcher only. Every application shares the group of
//! the folder of the sockets, so the permissions of the file would let any of them in: each
//! connection is checked against the user at its other end, which has to be the owner of that
//! folder (the launcher), the application itself or root. On a development copy the folder and the
//! socket are the developer's.

use axum::serve::Listener;
use std::{io, os::unix::fs::MetadataExt};
use tokio::net::{unix::SocketAddr, UnixListener, UnixStream};

pub struct LauncherOnly {
    listener: UnixListener,
    launcher: Option<u32>,
    own: Option<u32>,
}

impl LauncherOnly {
    pub fn new(listener: UnixListener) -> Self {
        let socket = listener
            .local_addr()
            .ok()
            .and_then(|address| address.as_pathname().map(ToOwned::to_owned));
        let owner =
            |path: &std::path::Path| std::fs::metadata(path).ok().map(|metadata| metadata.uid());
        let launcher = socket
            .as_deref()
            .map(folder_of)
            .and_then(|folder| owner(&folder));
        let own = socket.as_deref().and_then(owner);
        Self {
            listener,
            launcher,
            own,
        }
    }

    fn admits(&self, uid: u32) -> bool {
        uid == 0 || Some(uid) == self.launcher || Some(uid) == self.own
    }
}

/// The folder of a socket; a bare name is in the current folder.
fn folder_of(socket: &std::path::Path) -> std::path::PathBuf {
    match socket.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder.to_owned(),
        _ => std::path::PathBuf::from("."),
    }
}

impl Listener for LauncherOnly {
    type Io = UnixStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, address) = Listener::accept(&mut self.listener).await;
            match stream.peer_cred() {
                Ok(peer) if self.admits(peer.uid()) => return (stream, address),
                Ok(peer) => eprintln!(
                    "<4>refused a connection of the user {}: only the launcher speaks here",
                    peer.uid()
                ),
                Err(error) => eprintln!("<4>refused a connection without credentials: {error}"),
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/axum/launcher_only.rs"]
mod tests;
