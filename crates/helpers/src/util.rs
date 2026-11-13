//! Small helpers shared by the two transports: a loopback HTTP probe that never uses a proxy,
//! and running a helper's command line tool with a scrubbed environment.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::HelperError;

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

fn command(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    c.args(args)
        .env_clear()
        .envs(env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    c
}

/// Run to the end and return what it printed, or kill after `within`.
pub(crate) async fn run_cli(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
    within: Duration,
) -> Result<String, HelperError> {
    let child = command(program, args, env)
        .spawn()
        .map_err(|e| HelperError::Spawn(e.to_string()))?;
    match tokio::time::timeout(within, child.wait_with_output()).await {
        Ok(Ok(out)) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(Err(e)) => Err(HelperError::Spawn(e.to_string())),
        Err(_) => Err(HelperError::Exited("the command took too long".into())),
    }
}

/// Run to the end, handing every output line (both streams) to `on_line` as it arrives.
/// Returns the exit code. Cancelling kills the command.
pub(crate) async fn run_cli_watch(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
    cancel: &CancellationToken,
    mut on_line: impl FnMut(&str),
) -> Result<Option<i32>, HelperError> {
    let mut child = command(program, args, env)
        .spawn()
        .map_err(|e| HelperError::Spawn(e.to_string()))?;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(l);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(l);
            }
        });
    }
    drop(tx);
    let status = loop {
        tokio::select! {
            Some(line) = rx.recv() => on_line(&line),
            s = child.wait() => break s.map_err(|e| HelperError::Exited(e.to_string()))?,
            () = cancel.cancelled() => {
                let _ = child.start_kill();
                return Err(HelperError::Stopped);
            }
        }
    };
    let drain = async {
        while let Some(line) = rx.recv().await {
            on_line(&line);
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(2), drain).await;
    Ok(status.code())
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
