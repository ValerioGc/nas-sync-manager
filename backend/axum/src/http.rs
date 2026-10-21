//! Minimal HTTP/1.1 client over a Unix socket, with one request per connection.

use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub body: String,
}

/// Sends a single request and reads the full response within `timeout`.
pub async fn exchange(
    socket: &Path,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
    timeout: Duration,
) -> Result<Answer, String> {
    let extra: String = headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    let exchange = async {
        let mut stream = UnixStream::connect(socket)
            .await
            .map_err(|error| format!("not reachable: {error}"))?;
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
             Content-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|error| format!("the request was not taken: {error}"))?;
        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .await
            .map_err(|error| format!("no answer: {error}"))?;
        parse(&raw)
    };
    tokio::time::timeout(timeout, exchange)
        .await
        .map_err(|_| "no answer in time".to_owned())?
}

/// Parses the status and body of a response, decoding chunked bodies.
pub fn parse(raw: &[u8]) -> Result<Answer, String> {
    let end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("the answer is not HTTP")?;
    let head = String::from_utf8_lossy(&raw[..end]);
    let body = &raw[end + 4..];
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .ok_or("the answer has no status")?;
    let chunked = head.lines().any(|line| {
        line.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    let body = if chunked {
        dechunk(body)
    } else {
        body.to_vec()
    };
    Ok(Answer {
        status,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(line) = body.windows(2).position(|window| window == b"\r\n") {
        let size = std::str::from_utf8(&body[..line])
            .ok()
            .and_then(|size| usize::from_str_radix(size.trim(), 16).ok());
        let rest = &body[line + 2..];
        match size {
            Some(size) if size > 0 && rest.len() >= size => {
                out.extend_from_slice(&rest[..size]);
                body = rest[size..].strip_prefix(b"\r\n").unwrap_or(&rest[size..]);
            }
            _ => break,
        }
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/axum/http.rs"]
mod tests;
