//! Small helpers for the transports: a loopback HTTP probe that never uses a proxy, and finding
//! an address in a helper's output.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// `GET path` on `127.0.0.1:port`: the status and the body, or `None` when nothing answers.
pub(crate) async fn local_get(port: u16, path: &str) -> Option<(u16, String)> {
    let work = async {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.ok()?;
        let request = format!("GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n");
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut raw = Vec::new();
        stream.take(1 << 20).read_to_end(&mut raw).await.ok()?;
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n")?;
        let status = head.split_whitespace().nth(1)?.parse().ok()?;
        Some((status, body.to_string()))
    };
    tokio::time::timeout(Duration::from_secs(2), work)
        .await
        .ok()
        .flatten()
}

/// The last `https://<name><suffix>` address printed in `text`.
pub(crate) fn last_https_host(text: &str, suffix: &str) -> Option<String> {
    let mut found = None;
    for word in text.split(|c: char| c.is_whitespace() || c == '|' || c == '"') {
        if let Some(rest) = word.strip_prefix("https://") {
            let host = rest.trim_end_matches(['/', ',', '.']);
            if host.ends_with(suffix) && host.len() > suffix.len() + 1 {
                found = Some(format!("https://{host}"));
            }
        }
    }
    found
}
