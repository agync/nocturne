//! The engine's log, read from its standard output and error and written to `lemond.log` with
//! provider bodies and the key withheld. Some of the engine's lines carry an upstream body: cut to
//! about 200 bytes in the streaming error and the discovery warning, and whole in the
//! non-streaming error dump. A cut body can hold part of a key, so replacing the key is not
//! enough: everything after a body marker is withheld up to the next line that starts with the
//! engine's log prefix (`YYYY-MM-DD HH:MM:SS.mmm [Severity] (tag)`,
//! `include/lemon/runtime_config.h:115`). A line longer than [`MAX_BUFFERED`] is never held
//! whole: past that, the rest of it is withheld.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::online::OnlineKey;

/// Text after any of these is a body from upstream: `(Cloud) Provider returned status N, body:`
/// (`cloud_server.cpp:860-861`), the discovery warning's `Body:` (`:1068-1071`) and the
/// non-streaming error dump (`routes/openai/text/chat_completions.cpp:313`).
pub const BODY_MARKERS: &[&str] = &["Backend returned error response:", "body:", "Body:"];
pub const WITHHELD: &str = "[withheld]";
pub const KEY_MARK: &str = "[key]";
/// Longest line written, in bytes.
pub const MAX_LINE: usize = 1024;
/// Most of one line held in memory while reading; the rest of a longer line is dropped.
pub const MAX_BUFFERED: usize = 64 * 1024;
/// A log file is rotated at this size, and this many files are kept in all.
pub const ROTATE_AT: u64 = 8 * 1024 * 1024;
pub const FILES_KEPT: usize = 3;
/// Lines waiting for the file. When the file falls behind, lines are dropped rather than the
/// engine being made to wait on its own output.
const QUEUE: usize = 1024;

/// Takes bodies and the keys out of the engine's log lines.
#[derive(Default)]
pub struct Redactor {
    key: Mutex<Option<OnlineKey>>,
    launch_keys: Mutex<Vec<Zeroizing<String>>>,
    withholding: Mutex<bool>,
}

impl Redactor {
    pub fn new() -> Self {
        Self::default()
    }

    /// The key the engine is given at this launch, if any.
    pub fn set_key(&self, key: Option<&OnlineKey>) {
        *self.key.lock().unwrap_or_else(|e| e.into_inner()) = key.cloned();
    }

    /// The keys this launch of the engine checks requests against, hidden as the route's is.
    pub fn set_launch_keys(&self, keys: &[&str]) {
        *self.launch_keys.lock().unwrap_or_else(|e| e.into_inner()) =
            keys.iter().map(|k| Zeroizing::new(k.to_string())).collect();
    }

    /// The line as it may be written, or `None` when all of it is withheld. Lines are judged in
    /// order: a body marker withholds the rest of its line and every line after it until one
    /// starts with the engine's prefix.
    pub fn line(&self, s: &str) -> Option<String> {
        let mut withholding = self.withholding.lock().unwrap_or_else(|e| e.into_inner());
        if has_prefix(s) {
            *withholding = false;
        } else if *withholding {
            return None;
        }
        let out = match BODY_MARKERS
            .iter()
            .filter_map(|m| s.find(m).map(|at| at + m.len()))
            .min()
        {
            Some(end) => {
                *withholding = true;
                format!("{} {WITHHELD}", &s[..end])
            }
            None => s.to_string(),
        };
        let route = self.key.lock().unwrap_or_else(|e| e.into_inner());
        let launch = self.launch_keys.lock().unwrap_or_else(|e| e.into_inner());
        let keys: Vec<&str> = route
            .iter()
            .map(|k| k.expose())
            .chain(launch.iter().map(|k| k.as_str()))
            .collect();
        let out = hide_keys(&out, &keys);
        Some(cap(out))
    }
}

/// Each key whole, then each key's first and last eight characters, wherever they appear. The
/// whole keys go first so that one key's ends cannot break another's match.
fn hide_keys(line: &str, keys: &[&str]) -> String {
    let mut out = line.to_string();
    for key in keys.iter().filter(|k| !k.is_empty()) {
        out = out.replace(key, KEY_MARK);
    }
    for key in keys {
        let chars: Vec<char> = key.chars().collect();
        if chars.len() >= 8 {
            let head: String = chars[..8].iter().collect();
            let tail: String = chars[chars.len() - 8..].iter().collect();
            out = out.replace(&head, KEY_MARK).replace(&tail, KEY_MARK);
        }
    }
    out
}

fn cap(mut s: String) -> String {
    if s.len() > MAX_LINE {
        let mut end = MAX_LINE;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

/// Whether a line starts with `YYYY-MM-DD HH:MM:SS.mmm [`.
fn has_prefix(s: &str) -> bool {
    let b = s.as_bytes();
    const SHAPE: &[u8] = b"0000-00-00 00:00:00.000 [";
    b.len() >= SHAPE.len()
        && SHAPE.iter().zip(b).all(|(want, got)| match want {
            b'0' => got.is_ascii_digit(),
            other => other == got,
        })
}

/// Reads the engine's output line by line on its own task, redacts each line and hands it to a
/// writer thread through a bounded queue. Never blocks the engine: a full queue drops the line.
pub fn spawn<S>(stdout: S, file: PathBuf, redact: Arc<Redactor>) -> JoinHandle<()>
where
    S: AsyncRead + Unpin + Send + 'static,
{
    spawn_rotating_at(stdout, file, redact, ROTATE_AT)
}

/// [`spawn`] with another rotation size, for tests.
#[doc(hidden)]
pub fn spawn_rotating_at<S>(
    stdout: S,
    file: PathBuf,
    redact: Arc<Redactor>,
    rotate_at: u64,
) -> JoinHandle<()>
where
    S: AsyncRead + Unpin + Send + 'static,
{
    pump(stdout, writer(file, rotate_at), redact)
}

/// [`spawn`] for both of the engine's outputs, into one file. Each is redacted on its own, so a
/// body that starts on one is never ended by a line on the other. Standard error has no line
/// that starts with the engine's prefix, so a body marker there withholds the rest of it for that
/// launch. That is meant: it holds only errors from before the engine's log starts and what its
/// backends print, and a body there could run to any length.
pub fn spawn_both<S, E>(
    stdout: S,
    stderr: E,
    file: PathBuf,
    redact: Arc<Redactor>,
    errors: Arc<Redactor>,
) -> JoinHandle<()>
where
    S: AsyncRead + Unpin + Send + 'static,
    E: AsyncRead + Unpin + Send + 'static,
{
    let tx = writer(file, ROTATE_AT);
    let out = pump(stdout, tx.clone(), redact);
    let err = pump(stderr, tx, errors);
    tokio::spawn(async move {
        let _ = out.await;
        let _ = err.await;
    })
}

/// The queue to a thread that writes lines to `file`.
fn writer(file: PathBuf, rotate_at: u64) -> mpsc::Sender<String> {
    let (tx, rx) = mpsc::channel::<String>(QUEUE);
    if let Err(e) = std::thread::Builder::new()
        .name("lemond-log".into())
        .spawn(move || write_lines(rx, &file, rotate_at))
    {
        tracing::warn!("the engine's log has no writer: {e}");
    }
    tx
}

/// Reads one output line by line, redacts each line and queues it for the writer.
fn pump<S>(stdout: S, tx: mpsc::Sender<String>, redact: Arc<Redactor>) -> JoinHandle<()>
where
    S: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut stdout = stdout;
        let mut chunk = vec![0u8; 8192];
        let mut line: Vec<u8> = Vec::new();
        // Past `MAX_BUFFERED`, the rest of the line is skipped until its end.
        let mut skipping = false;
        let mut dropped = 0u64;
        let mut emit = |raw: &[u8]| {
            let text = String::from_utf8_lossy(raw);
            if let Some(out) = redact.line(text.trim_end_matches('\r'))
                && tx.try_send(out).is_err()
            {
                dropped += 1;
            }
        };
        loop {
            let n = match stdout.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            for &b in &chunk[..n] {
                if b == b'\n' {
                    if !skipping {
                        emit(&line);
                    }
                    line.clear();
                    skipping = false;
                } else if !skipping {
                    line.push(b);
                    if line.len() >= MAX_BUFFERED {
                        emit(&line);
                        line.clear();
                        skipping = true;
                    }
                }
            }
        }
        if !line.is_empty() && !skipping {
            emit(&line);
        }
        if dropped > 0 {
            tracing::warn!("{dropped} engine log lines were dropped");
        }
    })
}

fn write_lines(mut rx: mpsc::Receiver<String>, path: &Path, rotate_at: u64) {
    let mut file = open(path);
    let mut size = file
        .as_ref()
        .and_then(|f| f.metadata().ok())
        .map(|m| m.len())
        .unwrap_or(0);
    while let Some(line) = rx.blocking_recv() {
        if size >= rotate_at {
            drop(file.take());
            rotate(path);
            file = open(path);
            size = 0;
        }
        if let Some(f) = file.as_mut()
            && writeln!(f, "{line}").is_ok()
        {
            size += line.len() as u64 + 1;
        }
    }
}

/// `lemond.log` becomes `lemond.log.1`, which becomes `lemond.log.2`; the oldest is removed.
fn rotate(path: &Path) {
    let name = |n: usize| {
        let mut p = path.as_os_str().to_owned();
        p.push(format!(".{n}"));
        PathBuf::from(p)
    };
    let _ = std::fs::remove_file(name(FILES_KEPT - 1));
    for n in (1..FILES_KEPT - 1).rev() {
        let _ = std::fs::rename(name(n), name(n + 1));
    }
    let _ = std::fs::rename(path, name(1));
}

/// The log, readable and writable by its owner only.
fn open(path: &Path) -> Option<File> {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut opts = OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    match opts.open(path) {
        Ok(f) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
            }
            Some(f)
        }
        Err(e) => {
            tracing::warn!("the engine's log could not be opened: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launch_keys_are_hidden_whole_and_in_part() {
        let r = Redactor::new();
        let (api, admin) = ("a1b2c3d4e5f6a7b8c9d0e1f2", "f0e1d2c3b4a5f6e7d8c9b0a1");
        r.set_launch_keys(&[api, admin]);
        let line = format!(
            "keys {api} {admin} head {} tail {}",
            &api[..8],
            &admin[admin.len() - 8..]
        );
        let out = r.line(&line).unwrap();
        assert_eq!(out, "keys [key] [key] head [key] tail [key]");
    }

    #[test]
    fn one_keys_end_does_not_break_anothers_match() {
        let r = Redactor::new();
        let (api, admin) = ("a1b2c3d4e5f6a7b8", "00a1b2c3d4ffffff");
        r.set_launch_keys(&[api, admin]);
        assert_eq!(r.line(&format!("{admin} {api}")).unwrap(), "[key] [key]");
    }

    #[test]
    fn the_prefix_is_recognised() {
        assert!(has_prefix(
            "2026-11-13 22:06:22.734 [Error] (Server) Backend returned"
        ));
        assert!(!has_prefix("authorization was Bearer sk"));
        assert!(!has_prefix("2026-11-13 22:06:22 [Error]"));
        assert!(!has_prefix(""));
    }

    #[test]
    fn a_long_line_is_cut_on_a_character() {
        let out = cap("é".repeat(MAX_LINE));
        assert!(out.len() <= MAX_LINE);
        assert!(out.is_char_boundary(out.len()));
    }
}
