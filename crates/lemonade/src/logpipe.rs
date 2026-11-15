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
use std::time::{Duration, Instant};

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

/// How long a failed rotation or open waits before the next try, so a permanent fault costs one
/// round of tries, not one per line.
const RETRY_AFTER: Duration = Duration::from_secs(30);
/// Lines waiting for the file. When the file falls behind, lines are dropped rather than the
/// engine being made to wait on its own output.
const QUEUE: usize = 1024;

/// Takes bodies and the keys out of the engine's log lines.
pub struct Redactor {
    keys: Vec<Zeroizing<String>>,
    withholding: Mutex<bool>,
}

impl Redactor {
    /// The redactor for one launch of the engine, which hides the route's key, when there is one,
    /// and the keys the launch checks requests against. It is the only way to make one, so none
    /// can leave a key out.
    pub fn for_launch(route: Option<&OnlineKey>, launch: &[&str]) -> Self {
        let keys = route
            .map(OnlineKey::expose)
            .into_iter()
            .chain(launch.iter().copied())
            .map(|k| Zeroizing::new(k.to_string()))
            .collect();
        Redactor {
            keys,
            withholding: Mutex::new(false),
        }
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
        let keys: Vec<&str> = self.keys.iter().map(|k| k.as_str()).collect();
        let out = hide_keys(&out, &keys);
        Some(cap(out))
    }
}

/// The shortest run of a key's characters that is hidden on its own. A key wrapped over two
/// lines shows at most one character fewer than this on either side. A caller that cuts a line
/// hides the keys first and then drops the last `KEY_PART - 1` characters of the piece.
pub const KEY_PART: usize = 8;

/// Every key whole, however short, and every run of [`KEY_PART`] or more bytes that also appears
/// in one, becomes [`KEY_MARK`]. The keys are compared as they are, byte by byte, and a run that
/// ends inside a character stops short of it. The longest run starting at each place wins, so one key's part cannot break
/// the match of another.
pub fn hide_keys(line: &str, keys: &[&str]) -> String {
    let bytes = line.as_bytes();
    let keys: Vec<&[u8]> = keys
        .iter()
        .filter(|k| !k.is_empty())
        .map(|k| k.as_bytes())
        .collect();
    if keys.is_empty() {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    let mut kept = 0;
    while at < bytes.len() {
        let run = keys
            .iter()
            .flat_map(|key| (0..key.len()).map(move |from| (*key, from)))
            .map(|(key, from)| {
                let mut run = key[from..]
                    .iter()
                    .zip(&bytes[at..])
                    .take_while(|(a, b)| a == b)
                    .count();
                while run > 0 && !line.is_char_boundary(at + run) {
                    run -= 1;
                }
                if line.is_char_boundary(at) && run >= KEY_PART.min(key.len()) {
                    run
                } else {
                    0
                }
            })
            .max()
            .unwrap_or(0);
        if run > 0 {
            out.push_str(&line[kept..at]);
            out.push_str(KEY_MARK);
            at += run;
            kept = at;
        } else {
            at += 1;
        }
    }
    out.push_str(&line[kept..]);
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
    let (tx, writer) = writer(file, rotate_at);
    let pumped = pump(stdout, tx, redact);
    tokio::spawn(async move {
        let _ = pumped.await;
        finish(writer).await;
    })
}

/// Waits for the writer thread to write what is queued and exit. Every sender is gone by then,
/// because each output's task owns its own and has ended.
async fn finish(writer: Option<std::thread::JoinHandle<()>>) {
    if let Some(writer) = writer {
        let _ = tokio::task::spawn_blocking(move || writer.join()).await;
    }
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
    let (tx, writer) = writer(file, ROTATE_AT);
    let out = pump(stdout, tx.clone(), redact);
    let err = pump(stderr, tx, errors);
    tokio::spawn(async move {
        let _ = out.await;
        let _ = err.await;
        finish(writer).await;
    })
}

/// The queue to a thread that writes lines to `file`, and the thread.
fn writer(
    file: PathBuf,
    rotate_at: u64,
) -> (mpsc::Sender<String>, Option<std::thread::JoinHandle<()>>) {
    let (tx, rx) = mpsc::channel::<String>(QUEUE);
    let thread = std::thread::Builder::new()
        .name("lemond-log".into())
        .spawn(move || write_lines(rx, &file, rotate_at, RETRY_AFTER, open))
        .map_err(|e| tracing::warn!("the engine's log has no writer: {e}"))
        .ok();
    (tx, thread)
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

/// Where the log is written: a file, or a stand-in in tests.
trait Sink: Write {
    fn len(&self) -> std::io::Result<u64>;
}

impl Sink for File {
    fn len(&self) -> std::io::Result<u64> {
        self.metadata().map(|m| m.len())
    }
}

fn write_lines<W: Sink>(
    mut rx: mpsc::Receiver<String>,
    path: &Path,
    rotate_at: u64,
    retry_after: Duration,
    mut open: impl FnMut(&Path) -> std::io::Result<W>,
) {
    let mut open_warned = false;
    let mut open = |path: &Path| match open(path) {
        Ok(f) => {
            open_warned = false;
            Some(f)
        }
        Err(e) => {
            if !std::mem::replace(&mut open_warned, true) {
                tracing::warn!("the engine's log could not be opened: {e}");
            }
            None
        }
    };
    // A size that cannot be read is taken as full, so the next line rotates the file and the
    // size is known again, and a file that has grown is never mistaken for an empty one.
    let length = |file: Option<&W>| file.map_or(0, |f| f.len().unwrap_or(rotate_at));
    let mut file = open(path);
    let mut size = length(file.as_ref());
    let mut warned = false;
    let mut write_warned = false;
    let mut retry_at = Instant::now();
    let mut reopen_at = Instant::now() + retry_after;
    let mut opened_at = Instant::now();
    while let Some(line) = rx.blocking_recv() {
        if file.is_none() && Instant::now() >= reopen_at {
            opened_at = Instant::now();
            file = open(path);
            size = length(file.as_ref());
            if file.is_none() {
                reopen_at = Instant::now() + retry_after;
            }
        }
        if file.is_some() && size >= rotate_at && Instant::now() >= retry_at {
            // Closed first, so the reopened file is a new live file and not the renamed one.
            drop(file.take());
            let rotated = rotate(path);
            match &rotated {
                Ok(()) => warned = false,
                Err(e) => {
                    retry_at = Instant::now() + retry_after;
                    if !std::mem::replace(&mut warned, true) {
                        tracing::warn!("the engine's log could not be rotated: {e}");
                    }
                }
            }
            opened_at = Instant::now();
            file = open(path);
            if file.is_none() {
                reopen_at = Instant::now() + retry_after;
            }
            // A failed rotation leaves the old file in place; it is tried again later.
            size = if rotated.is_ok() {
                0
            } else {
                length(file.as_ref())
            };
        }
        if let Some(f) = file.as_mut() {
            match f.write_all(format!("{line}\n").as_bytes()) {
                Ok(()) => {
                    size += line.len() as u64 + 1;
                    write_warned = false;
                }
                Err(e) => {
                    // The handle may be dead. It is opened again on a later line, but at most
                    // once per interval counted from the last open, so a young handle that fails
                    // waits out the rest of the interval.
                    if !std::mem::replace(&mut write_warned, true) {
                        tracing::warn!("the engine's log could not be written: {e}");
                    }
                    file = None;
                    reopen_at = opened_at + retry_after;
                }
            }
        }
    }
}

/// `lemond.log` becomes `lemond.log.1`, which becomes `lemond.log.2`; the oldest is removed. A
/// file that is not there is skipped. The first other error stops the rotation and is returned,
/// so no older generation is overwritten by one that could not move on.
fn rotate(path: &Path) -> std::io::Result<()> {
    let name = |n: usize| {
        let mut p = path.as_os_str().to_owned();
        p.push(format!(".{n}"));
        PathBuf::from(p)
    };
    let step = |r: std::io::Result<()>| match r {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    };
    step(retry(|| std::fs::remove_file(name(FILES_KEPT - 1))))?;
    for n in (1..FILES_KEPT - 1).rev() {
        step(retry(|| std::fs::rename(name(n), name(n + 1))))?;
    }
    step(retry(|| std::fs::rename(path, name(1))))
}

/// Another process (a scanner, an indexer) can hold a file for a moment. Windows then refuses the
/// move with a sharing or lock violation, which `std` reports as an uncategorized error, or with
/// access denied. Elsewhere a refusal is permanent and is returned at once. Used for moves, removals
/// and opening the log.
fn retry<T>(mut op: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut tries = 0;
    loop {
        match op() {
            Err(e) if tries < 5 && is_busy(&e) => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20 * tries));
            }
            other => return other,
        }
    }
}

fn is_busy(e: &std::io::Error) -> bool {
    cfg!(windows) && is_busy_on_windows(e)
}

/// Access denied, a sharing violation (32), a lock violation (33), or a delete that is pending
/// (303): a file marked for deletion still has an open handle, usually another process's scanner,
/// and the name is free once that handle closes, so creating or moving onto it works shortly after.
/// `ERROR_CANNOT_MAKE` (82) is left out: it reports a directory entry that cannot be made, which
/// does not clear by waiting a moment, and the writer's own retry interval already covers it
/// without holding the thread for the short retries.
fn is_busy_on_windows(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied
        || matches!(e.raw_os_error(), Some(32 | 33 | 303))
}

/// The log, readable and writable by its owner only.
fn open(path: &Path) -> std::io::Result<File> {
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
    let f = retry(|| opts.open(path))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = f.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_with_wide_characters_is_hidden_on_char_boundaries() {
        let key = "k3y-\u{e9}\u{e9}\u{e9}-9f8e7d6c";
        assert_eq!(
            hide_keys(&format!("a\u{e9}{key}z"), &[key]),
            "a\u{e9}[key]z"
        );
        let near = "k3y-\u{e9}\u{e9}\u{e9}-\u{e3}";
        assert_eq!(hide_keys(near, &[key]), "[key]\u{e3}");
        assert_eq!(hide_keys("k3y-\u{e9}", &[key]), "k3y-\u{e9}");
    }

    #[test]
    fn the_launch_keys_are_hidden_whole_and_in_part() {
        let (api, admin) = ("a1b2c3d4e5f6a7b8c9d0e1f2", "f0e1d2c3b4a5f6e7d8c9b0a1");
        let r = Redactor::for_launch(None, &[api, admin]);
        let line = format!(
            "keys {api} {admin} head {} tail {}",
            &api[..8],
            &admin[admin.len() - 8..]
        );
        let out = r.line(&line).unwrap();
        assert_eq!(out, "keys [key] [key] head [key] tail [key]");
    }

    #[test]
    fn a_key_wrapped_over_two_lines_is_hidden_down_to_its_last_few_characters() {
        let key = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6";
        let r = Redactor::for_launch(None, &[key]);
        // Wrapped over two lines: each part is hidden whole.
        assert_eq!(
            r.line(&format!("auth {}", &key[..13])).unwrap(),
            "auth [key]"
        );
        assert_eq!(r.line(&format!("{} ok", &key[13..])).unwrap(), "[key] ok");
        // A part shorter than KEY_PART is not, and is too short to matter.
        assert_eq!(r.line(&key[..7]).unwrap(), &key[..7]);
        // A short key is still hidden whole.
        let short = Redactor::for_launch(None, &["k3y"]);
        assert_eq!(short.line("the k3y").unwrap(), "the [key]");
    }

    #[test]
    fn one_keys_end_does_not_break_anothers_match() {
        let (api, admin) = ("a1b2c3d4e5f6a7b8", "00a1b2c3d4ffffff");
        let r = Redactor::for_launch(None, &[api, admin]);
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

    #[test]
    fn rotation_shifts_every_generation_and_skips_missing_ones() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("lemond.log");
        std::fs::write(&live, "c").unwrap();
        rotate(&live).unwrap();
        assert!(!live.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("lemond.log.1")).unwrap(),
            "c"
        );
        std::fs::write(&live, "d").unwrap();
        rotate(&live).unwrap();
        std::fs::write(&live, "e").unwrap();
        rotate(&live).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("lemond.log.1")).unwrap(),
            "e"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("lemond.log.2")).unwrap(),
            "d"
        );
    }

    #[test]
    fn a_failed_step_stops_the_rotation_and_keeps_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let live = dir.path().join("lemond.log");
        std::fs::write(&live, "live").unwrap();
        std::fs::write(dir.path().join("lemond.log.1"), "one").unwrap();
        // A directory in the way makes removing `.2` fail on every platform.
        let two = dir.path().join("lemond.log.2");
        std::fs::create_dir(&two).unwrap();
        std::fs::write(two.join("keep"), "x").unwrap();
        assert!(rotate(&live).is_err());
        assert_eq!(std::fs::read_to_string(&live).unwrap(), "live");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("lemond.log.1")).unwrap(),
            "one"
        );
    }

    #[test]
    fn a_log_that_could_not_be_opened_is_opened_again() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lemond.log");
        let (tx, rx) = mpsc::channel::<String>(8);
        for l in ["one", "two", "three", "four"] {
            tx.blocking_send(l.into()).unwrap();
        }
        drop(tx);
        let mut calls = 0;
        write_lines(rx, &path, ROTATE_AT, Duration::ZERO, |p| {
            calls += 1;
            if calls <= 2 {
                Err(std::io::Error::other("busy"))
            } else {
                open(p)
            }
        });
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "two\nthree\nfour\n"
        );
    }

    #[test]
    fn a_log_that_cannot_be_opened_is_tried_once_per_period() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lemond.log");
        let (tx, rx) = mpsc::channel::<String>(8);
        for l in ["one", "two", "three"] {
            tx.blocking_send(l.into()).unwrap();
        }
        drop(tx);
        let mut calls = 0;
        write_lines(rx, &path, ROTATE_AT, Duration::from_secs(60), |_| {
            calls += 1;
            Err::<File, _>(std::io::Error::other("busy"))
        });
        assert_eq!(calls, 1, "one try at the start, none per line");
    }

    #[tokio::test]
    async fn both_outputs_are_written_when_the_task_ends() {
        use tokio::io::AsyncWriteExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lemond.log");
        let (mut out_w, out_r) = tokio::io::duplex(1024);
        let (mut err_w, err_r) = tokio::io::duplex(1024);
        let task = spawn_both(
            out_r,
            err_r,
            path.clone(),
            Arc::new(Redactor::for_launch(None, &[])),
            Arc::new(Redactor::for_launch(None, &[])),
        );
        out_w.write_all(b"from out\n").await.unwrap();
        err_w.write_all(b"from err\n").await.unwrap();
        drop(out_w);
        drop(err_w);
        task.await.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("from out") && text.contains("from err"),
            "{text}"
        );
    }

    struct Flaky {
        len_fails: bool,
        lines: Arc<Mutex<Vec<String>>>,
        fail_from: Option<usize>,
        written: usize,
    }

    impl Write for Flaky {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.fail_from.is_some_and(|n| self.written >= n) {
                return Err(std::io::Error::other("gone"));
            }
            self.written += 1;
            self.lines
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(buf).trim_end().to_string());
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Sink for Flaky {
        fn len(&self) -> std::io::Result<u64> {
            if self.len_fails {
                Err(std::io::Error::other("no size"))
            } else {
                Ok(0)
            }
        }
    }

    #[test]
    fn a_log_that_fails_to_write_is_opened_again() {
        let (tx, rx) = mpsc::channel::<String>(8);
        for l in ["a", "b", "c", "d"] {
            tx.blocking_send(l.into()).unwrap();
        }
        drop(tx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut opens = 0;
        write_lines(rx, Path::new("unused"), ROTATE_AT, Duration::ZERO, |_| {
            opens += 1;
            Ok(Flaky {
                len_fails: false,
                lines: seen.clone(),
                fail_from: (opens == 1).then_some(1),
                written: 0,
            })
        });
        assert_eq!(opens, 2);
        assert_eq!(*seen.lock().unwrap(), ["a", "c", "d"]);
    }

    #[test]
    fn a_log_that_keeps_failing_is_opened_once_per_period() {
        let (tx, rx) = mpsc::channel::<String>(8);
        for l in ["a", "b", "c", "d"] {
            tx.blocking_send(l.into()).unwrap();
        }
        drop(tx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut opens = 0;
        write_lines(
            rx,
            Path::new("unused"),
            ROTATE_AT,
            Duration::from_secs(60),
            |_| {
                opens += 1;
                Ok(Flaky {
                    len_fails: false,
                    lines: seen.clone(),
                    fail_from: Some(1),
                    written: 0,
                })
            },
        );
        assert_eq!(opens, 1);
        assert_eq!(*seen.lock().unwrap(), ["a"]);
    }

    #[test]
    fn a_size_that_cannot_be_read_counts_as_full() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lemond.log");
        let (tx, rx) = mpsc::channel::<String>(8);
        tx.blocking_send("a".into()).unwrap();
        drop(tx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut opens = 0;
        write_lines(rx, &path, ROTATE_AT, Duration::ZERO, |_| {
            opens += 1;
            Ok(Flaky {
                len_fails: opens == 1,
                lines: seen.clone(),
                fail_from: None,
                written: 0,
            })
        });
        assert_eq!(opens, 2, "opened again after rotating");
        assert_eq!(*seen.lock().unwrap(), ["a"]);
    }

    #[test]
    fn a_size_that_stays_unreadable_rotates_once_per_period() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("lemond.log");
        std::fs::create_dir(tmp.path().join("lemond.log.2")).unwrap();
        let (tx, rx) = mpsc::channel::<String>(8);
        for line in ["a", "b", "c"] {
            tx.blocking_send(line.into()).unwrap();
        }
        drop(tx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut opens = 0;
        write_lines(rx, &path, ROTATE_AT, Duration::from_secs(60), |_| {
            opens += 1;
            Ok(Flaky {
                len_fails: true,
                lines: seen.clone(),
                fail_from: None,
                written: 0,
            })
        });
        assert_eq!(opens, 2, "one failed rotation, then none per line");
        assert_eq!(*seen.lock().unwrap(), ["a", "b", "c"]);
    }

    #[test]
    fn only_transient_windows_errors_are_retried() {
        let code = std::io::Error::from_raw_os_error;
        assert!(is_busy_on_windows(
            &std::io::ErrorKind::PermissionDenied.into()
        ));
        for busy in [32, 33, 303] {
            assert!(is_busy_on_windows(&code(busy)), "{busy}");
        }
        for not in [2, 3, 82, 112] {
            assert!(!is_busy_on_windows(&code(not)), "{not}");
        }
        assert_eq!(is_busy(&code(32)), cfg!(windows));
        assert_eq!(is_busy(&code(5)), cfg!(windows));
    }

    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
        type Writer = Captured;

        fn make_writer(&'a self) -> Captured {
            self.clone()
        }
    }

    #[test]
    fn a_log_that_stays_closed_warns_once_per_outage() {
        // write_lines runs on this thread and spawns none, so the scoped default sees every event.
        let captured = Captured::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_ansi(false)
            .finish();
        let (tx, rx) = mpsc::channel::<String>(8);
        for l in ["a", "b", "c", "d", "e", "f"] {
            tx.blocking_send(l.into()).unwrap();
        }
        drop(tx);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut opens = 0;
        // Opens 1 to 3 fail, 4 works and its file then fails on the second line, 5 and 6 fail.
        tracing::subscriber::with_default(subscriber, || {
            write_lines(rx, Path::new("unused"), ROTATE_AT, Duration::ZERO, |_| {
                opens += 1;
                if opens == 4 {
                    Ok(Flaky {
                        len_fails: false,
                        lines: seen.clone(),
                        fail_from: Some(1),
                        written: 0,
                    })
                } else {
                    Err(std::io::Error::other("busy"))
                }
            });
        });
        let text = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
        assert_eq!(text.matches("could not be opened").count(), 2, "{text}");
        assert_eq!(text.matches("could not be written").count(), 1, "{text}");
    }
}
