//! Stand-ins for `cloudflared` and `nocturne-tailnet`, for tests only (the `fake` feature).
//!
//! A stand-in accepts only the flags in the captured help fixtures, appends its arguments and
//! environment to `fake-record.jsonl` beside its own binary, and is steered by
//! `fake-control.json` in the same folder. The folder is a test's own copy of the binary, so a
//! test can also prove the binary was not changed by a run.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use crate::{HelperKind, allowed_flags_for};

pub const CONTROL_FILE: &str = "fake-control.json";
pub const RECORD_FILE: &str = "fake-record.jsonl";
pub const PUBLIC_PORT_FILE: &str = "fake-public-port";
pub const CHILD_PID_FILE: &str = "fake-child-pid";
pub const SPOOF_PORT_FILE: &str = "fake-spoof-port";

/// Folder of the running stand-in.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

pub fn control() -> Value {
    std::fs::read_to_string(exe_dir().join(CONTROL_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}))
}

/// Append this run to the record. Returns how many runs of `role` there have been, this one included.
pub fn record(role: &str, args: &[String]) -> usize {
    let path = exe_dir().join(RECORD_FILE);
    let before = std::fs::read_to_string(&path)
        .map(|t| {
            t.lines()
                .filter(|l| l.contains(&format!("\"role\":\"{role}\"")))
                .count()
        })
        .unwrap_or(0);
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let line = json!({
        "role": role,
        "argv": args,
        "env": env,
        "pid": std::process::id(),
        "at_ms": now_ms(),
    });
    append_line(&path, &line);
    before + 1
}

/// Appends `line` to the record in one write, so a reader rarely sees a run's line cut in two and
/// two runs' lines do not mix.
fn append_line(path: &std::path::Path, line: &Value) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(format!("{line}\n").as_bytes());
    }
}

/// Append one event of a run to the record: an input line it read, or how it ended.
pub fn note(role: &str, value: Value) {
    let line =
        json!({ "role": role, "value": value, "pid": std::process::id(), "at_ms": now_ms() });
    append_line(&exe_dir().join(RECORD_FILE), &line);
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Refuse a flag the pinned help does not list, the way the real program does.
pub fn check_flags(kind: HelperKind, command: &str, args: &[String]) -> Result<(), String> {
    let allowed = allowed_flags_for(kind, command);
    for arg in args {
        if arg.starts_with('-') && arg.len() > 1 {
            let name = arg.split('=').next().unwrap_or(arg);
            if !allowed.contains(&name) {
                return Err(format!("flag provided but not defined: {name}"));
            }
        }
    }
    Ok(())
}

/// The value after `--name` (as `--name value` or `--name=value`).
/// The socket of a `service: "unix:<path>"` ingress rule in a configuration Nocturne wrote.
pub fn ingress_unix(config: &str) -> Option<String> {
    let line = config
        .lines()
        .find_map(|l| l.trim().strip_prefix("- service: \"unix:"))?;
    let quoted = line.strip_suffix('"')?;
    Some(quoted.replace("\\\"", "\"").replace("\\\\", "\\"))
}

pub fn flag_value(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return Some(v.to_string());
        }
    }
    None
}

pub fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

/// What a run does after it has started, from the control file: crash, hang, leave a child
/// behind, or try to reach a public address (only the network check sets that).
pub fn behave(control: &Value, index: usize) {
    if control.get("edge_attempt").and_then(Value::as_bool) == Some(true) {
        let _ =
            std::net::UdpSocket::bind("0.0.0.0:0").map(|s| s.send_to(b"x", "198.51.100.1:7844"));
        let _ = TcpStream::connect_timeout(
            &"198.51.100.1:7844".parse().expect("address"),
            Duration::from_millis(300),
        );
    }
    if control.get("spawn_child").and_then(Value::as_bool) == Some(true)
        && let Ok(exe) = std::env::current_exe()
        && let Ok(child) = nocturne_lemonade::process::command(exe)
            .env("FAKE_ROLE", "sleeper")
            .spawn()
    {
        write_whole(&exe_dir().join(CHILD_PID_FILE), &child.id().to_string());
    }
    let runs = control
        .get("crash_runs")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    if control.get("mode").and_then(Value::as_str) == Some("crash") && (index as u64) <= runs {
        let after = control
            .get("crash_after_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        std::thread::sleep(Duration::from_millis(after));
        std::process::exit(1);
    }
}

/// What a quick tunnel prints when its connection drops, as `cloudflared` does.
pub const DROPPED_LINE: &str = "ERR Failed to dial a quic connection error=\"failed to dial to edge with quic: timeout: no recent network activity\" connIndex=0";
/// What it prints on every retry once the edge has deleted the tunnel.
pub const DELETED_LINE: &str = "ERR Register tunnel error from server side error=\"Unauthorized: Tunnel not found\" connIndex=0 event=0 ip=198.51.100.7";

/// The network going away under a run that is up, from the control file. With `drop_after_ms`
/// the connection drops; for runs up to `drop_runs` (every run when absent) the edge then deletes
/// the tunnel and every retry is refused, else the connection comes back on the same name. With
/// `still_ready` the refusals come while another connection stays up.
pub fn drop_connection(control: &Value, index: usize, ready: &AtomicBool) {
    let Some(after) = control.get("drop_after_ms").and_then(Value::as_u64) else {
        return;
    };
    let runs = control
        .get("drop_runs")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    let deleted = control.get("deleted").and_then(Value::as_bool) != Some(false);
    if index as u64 > runs {
        return;
    }
    std::thread::sleep(Duration::from_millis(after));
    let still_ready = control.get("still_ready").and_then(Value::as_bool) == Some(true);
    ready.store(still_ready, Ordering::SeqCst);
    eprintln!("{DROPPED_LINE}");
    if !deleted {
        std::thread::sleep(Duration::from_millis(200));
        eprintln!("INF Registered tunnel connection connIndex=0");
        ready.store(true, Ordering::SeqCst);
        return;
    }
    loop {
        std::thread::sleep(Duration::from_millis(100));
        eprintln!("{DELETED_LINE}");
    }
}

pub fn hangs(control: &Value) -> bool {
    control.get("mode").and_then(Value::as_str) == Some("hang")
}

/// Answer the metrics endpoints of a quick tunnel on `addr`, ready while `ready` holds. Returns
/// the port it answers on.
pub fn serve_metrics(addr: &str, hostname: String, ready: Arc<AtomicBool>) -> Option<u16> {
    let listener = TcpListener::bind(addr).ok()?;
    let port = listener.local_addr().ok()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap_or(0);
            let head = String::from_utf8_lossy(&buf[..n]);
            let path = head.split_whitespace().nth(1).unwrap_or("");
            let (status, body) = match path {
                "/quicktunnel" => ("200 OK", json!({ "hostname": hostname }).to_string()),
                "/ready" if ready.load(Ordering::SeqCst) => (
                    "200 OK",
                    json!({ "status": 200, "readyConnections": 1 }).to_string(),
                ),
                "/ready" => (
                    "503 Service Unavailable",
                    json!({ "status": 503, "readyConnections": 0 }).to_string(),
                ),
                _ => ("404 Not Found", String::new()),
            };
            let _ = write!(
                stream,
                "HTTP/1.0 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
        }
    });
    Some(port)
}

/// Writes `text` to `path` whole: to a name beside it first, then renamed, so a test that sees the
/// file never reads it empty or half written.
pub fn write_whole(path: &std::path::Path, text: &str) {
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    if std::fs::write(&part, text).is_ok() {
        let _ = std::fs::rename(&part, path);
    }
}

/// Another local program answering on `addr` as if it were the helper's metrics port, naming
/// `hostname`. Returns the port it answers on, once it does.
pub fn spawn_spoofer(addr: &str, hostname: &str) -> Option<u16> {
    let file = exe_dir().join(SPOOF_PORT_FILE);
    let _ = std::fs::remove_file(&file);
    nocturne_lemonade::process::command(std::env::current_exe().ok()?)
        .env("FAKE_ROLE", "spoofer")
        .env("FAKE_SPOOF_ADDR", addr)
        .env("FAKE_SPOOF_HOST", hostname)
        .spawn()
        .ok()?;
    for _ in 0..400 {
        if let Some(port) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| t.trim().parse().ok())
        {
            return Some(port);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    None
}

/// Run as the spoofer [`spawn_spoofer`] started.
pub fn spoofer_if_asked() {
    if std::env::var("FAKE_ROLE").as_deref() != Ok("spoofer") {
        return;
    }
    let addr = std::env::var("FAKE_SPOOF_ADDR").unwrap_or_default();
    let host = std::env::var("FAKE_SPOOF_HOST").unwrap_or_default();
    if let Some(port) = serve_metrics(&addr, host, Arc::new(AtomicBool::new(true))) {
        write_whole(&exe_dir().join(SPOOF_PORT_FILE), &port.to_string());
    }
    sleep_forever();
}

/// Where the stand-in forwards what arrives on its "public" port.
#[derive(Clone)]
pub enum Origin {
    Unix(PathBuf),
    Tcp(String),
}

trait Duplex: Read + Write + Send {
    fn pair(self: Box<Self>) -> std::io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)>;
}

impl Duplex for TcpStream {
    fn pair(self: Box<Self>) -> std::io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        let other = self.try_clone()?;
        Ok((Box::new(*self), Box::new(other)))
    }
}

#[cfg(unix)]
impl Duplex for std::os::unix::net::UnixStream {
    fn pair(self: Box<Self>) -> std::io::Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        let other = self.try_clone()?;
        Ok((Box::new(*self), Box::new(other)))
    }
}

fn connect(origin: &Origin) -> std::io::Result<Box<dyn Duplex>> {
    match origin {
        Origin::Tcp(addr) => Ok(Box::new(TcpStream::connect(addr)?)),
        #[cfg(unix)]
        Origin::Unix(path) => Ok(Box::new(std::os::unix::net::UnixStream::connect(path)?)),
        #[cfg(not(unix))]
        Origin::Unix(_) => Err(std::io::Error::other("no Unix sockets here")),
    }
}

fn tee(record: &Option<Arc<PathBuf>>, direction: &str, bytes: &[u8]) {
    if let Some(path) = record
        && let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
    {
        let _ = f.write_all(direction.as_bytes());
        let _ = f.write_all(bytes);
    }
}

fn pump(
    mut from: Box<dyn Read + Send>,
    mut to: Box<dyn Write + Send>,
    record: Option<Arc<PathBuf>>,
    direction: &'static str,
) {
    let mut buf = [0u8; 8192];
    while let Ok(n) = from.read(&mut buf) {
        if n == 0 || to.write_all(&buf[..n]).is_err() {
            break;
        }
        tee(&record, direction, &buf[..n]);
    }
}

/// Forward every connection to `origin`. Where `header` is given the first request carries it as
/// `(name, value)` in place of any header of that name the client sent: the Host, as cloudflared
/// does with `TUNNEL_HTTP_HOST_HEADER`, or the helper key header, as nocturne-tailnet does. With
/// `record` every byte is appended there, for the "relay sees no plaintext" proofs.
pub fn relay(
    listener: TcpListener,
    origin: Origin,
    header: Option<(String, String)>,
    record: Option<PathBuf>,
) {
    let record = record.map(Arc::new);
    for client in listener.incoming().flatten() {
        let (origin, header, record) = (origin.clone(), header.clone(), record.clone());
        std::thread::spawn(move || {
            let Ok(upstream) = connect(&origin) else {
                return;
            };
            let Ok((up_read, mut up_write)) = upstream.pair() else {
                return;
            };
            let mut client = client;
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while !head.ends_with(b"\r\n\r\n") && head.len() < 65536 {
                if client.read(&mut byte).unwrap_or(0) == 0 {
                    break;
                }
                head.push(byte[0]);
            }
            let head = match &header {
                Some((name, value)) => rewrite_header(&head, name, value),
                None => head,
            };
            tee(&record, ">", &head);
            if up_write.write_all(&head).is_err() {
                return;
            }
            let (Ok(back), Ok(closer)) = (client.try_clone(), client.try_clone()) else {
                return;
            };
            let down = std::thread::spawn({
                let record = record.clone();
                move || {
                    pump(up_read, Box::new(back), record, "<");
                    let _ = closer.shutdown(std::net::Shutdown::Write);
                }
            });
            pump(Box::new(client), up_write, record, ">");
            let _ = down.join();
        });
    }
}

fn rewrite_header(head: &[u8], name: &str, value: &str) -> Vec<u8> {
    let text = String::from_utf8_lossy(head);
    let wanted = name.to_ascii_lowercase().replace('_', "-");
    let mut out = String::new();
    for (i, line) in text.split_inclusive("\r\n").enumerate() {
        let field = line
            .split(':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .replace('_', "-");
        if i > 0 && line.contains(':') && field.trim() == wanted {
            continue;
        }
        out.push_str(line);
        if i == 0 {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
    }
    out.into_bytes()
}

/// Stay as a quiet child of the stand-in: used to prove a stop ends the whole group.
pub fn sleeper_if_asked() {
    if std::env::var("FAKE_ROLE").as_deref() == Ok("sleeper") {
        sleep_forever();
    }
}
