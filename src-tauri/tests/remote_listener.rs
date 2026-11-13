//! The listener for paired phones: local only, keyed on Windows, strict headers, exact-origin
//! CORS, limits before reading, and the same frames over WebSocket and long-poll.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use nocturne_app_lib::remote::listener::{self, Bind, ListenerConfig, ListenerHandle};
use nocturne_app_lib::remote::protocol::*;
use nocturne_app_lib::remote::static_files::DirFiles;
use nocturne_app_lib::remote::testing::client::{
    Addr, End, Mode, Opened, PhoneClient, Wire, Ws, WsMsg, http,
};
use nocturne_app_lib::remote::testing::{self, Bench, TEST_HOST};
use nocturne_contract::{RemoteAnswer, RemoteCall};
use nocturne_helpers::{KeyCarrier, Origin};

const KEY: &str = "k7f3a9c2e1b8d4f6a0c3e5b7d9f1a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6";

struct Running {
    _dir: tempfile::TempDir,
    bench: Bench,
    handle: ListenerHandle,
    addr: Addr,
}

fn phone_files(dir: &std::path::Path) -> std::path::PathBuf {
    let files = dir.join("phone");
    std::fs::create_dir_all(files.join("assets")).unwrap();
    std::fs::write(
        files.join("index.html"),
        b"<!doctype html><title>Nocturne</title>",
    )
    .unwrap();
    std::fs::write(
        files.join("sw.js"),
        b"self.addEventListener('fetch', () => {});",
    )
    .unwrap();
    std::fs::write(files.join("manifest.webmanifest"), b"{}").unwrap();
    std::fs::write(files.join("assets/app.js"), b"console.log(1)").unwrap();
    files
}

async fn start(bind: impl FnOnce(&std::path::Path) -> Bind) -> Running {
    let dir = tempfile::tempdir().unwrap();
    let bench = testing::bench(dir.path());
    let files = phone_files(dir.path());
    let handle = listener::serve(
        ListenerConfig {
            bind: bind(dir.path()),
            files: Arc::new(DirFiles(files)),
        },
        bench.core.clone(),
    )
    .await
    .unwrap();
    let addr = match &handle.origin {
        Origin::Unix(p) => Addr::Unix(p.clone()),
        Origin::Tcp { port, .. } => Addr::Tcp(*port),
    };
    Running {
        _dir: dir,
        bench,
        handle,
        addr,
    }
}

fn unix(dir: &std::path::Path) -> Bind {
    Bind::Unix(dir.join("remote").join("run").join("listener.sock"))
}

fn tcp(carrier: KeyCarrier) -> impl FnOnce(&std::path::Path) -> Bind {
    move |_| Bind::Tcp {
        key: KEY.into(),
        carrier,
    }
}

/// The local form everywhere a Unix socket exists; the Windows form otherwise.
async fn local() -> Running {
    if cfg!(unix) {
        start(unix).await
    } else {
        start(tcp(KeyCarrier::HostHeader)).await
    }
}

/// Headers that carry the key when the listener needs one.
fn keyed(r: &Running) -> Vec<(&'static str, &'static str)> {
    match r.addr {
        Addr::Tcp(_) => vec![("host", KEY)],
        Addr::Unix(_) => vec![],
    }
}

async fn get(r: &Running, path: &str, extra: &[(&str, &str)]) -> testing::client::Http {
    let mut h = keyed(r);
    h.extend_from_slice(extra);
    http(&r.addr, "GET", path, &h, &[]).await.unwrap()
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn unix_socket_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let r = start(unix).await;
    let Origin::Unix(path) = r.handle.origin.clone() else {
        panic!("a socket")
    };
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);
    assert_eq!(r.handle.describe(), "unix socket, owner only");
    assert_eq!(get(&r, "/p", &[]).await.status, 200);
    r.handle.stop().await;
    assert!(!path.exists(), "the socket goes with the listener");
}

#[tokio::test(flavor = "multi_thread")]
async fn tcp_binds_loopback_only() {
    let r = start(tcp(KeyCarrier::HostHeader)).await;
    let Origin::Tcp { port, key } = &r.handle.origin else {
        panic!("tcp")
    };
    assert_eq!(key, KEY);
    assert!(r.handle.describe().starts_with("127.0.0.1:"));
    assert!(!r.handle.describe().contains(KEY));
    // Reachable on loopback; nothing listens on another address of this computer.
    assert!(std::net::TcpStream::connect(("127.0.0.1", *port)).is_ok());
    for ip in local_addresses() {
        assert!(
            std::net::TcpStream::connect_timeout(
                &std::net::SocketAddr::new(ip, *port),
                Duration::from_millis(300)
            )
            .is_err(),
            "{ip}"
        );
    }
}

fn local_addresses() -> Vec<std::net::IpAddr> {
    // The address the system would use to reach the outside, without sending anything.
    let probe = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut out = Vec::new();
    if probe.connect("192.0.2.1:9").is_ok()
        && let Ok(a) = probe.local_addr()
        && !a.ip().is_loopback()
        && !a.ip().is_unspecified()
    {
        out.push(a.ip());
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_without_helper_key() {
    let host = start(tcp(KeyCarrier::HostHeader)).await;
    let res = http(&host.addr, "GET", "/p", &[("host", "phone.test")], &[])
        .await
        .unwrap();
    assert_eq!(res.status, 404);
    assert!(res.body.is_empty());
    let res = http(&host.addr, "GET", "/p", &[("host", KEY)], &[])
        .await
        .unwrap();
    assert_eq!(res.status, 200);
    let res = http(&host.addr, "GET", &format!("/{KEY}/p"), &[], &[])
        .await
        .unwrap();
    assert_eq!(
        res.status, 404,
        "the path does not carry the key for this helper"
    );

    let path = start(tcp(KeyCarrier::PathPrefix)).await;
    assert_eq!(
        http(&path.addr, "GET", "/p", &[], &[])
            .await
            .unwrap()
            .status,
        404
    );
    assert_eq!(
        http(&path.addr, "GET", &format!("/{KEY}/p"), &[], &[])
            .await
            .unwrap()
            .status,
        200
    );
    assert_eq!(
        http(&path.addr, "GET", &format!("/{}/p", &KEY[..40]), &[], &[])
            .await
            .unwrap()
            .status,
        404
    );
    let ws = Ws::connect(&path.addr, PATH_SESSION, &[]).await;
    assert_eq!(ws.err().map(|h| h.status), Some(404));
}

#[tokio::test(flavor = "multi_thread")]
async fn key_never_echoed_or_redirected() {
    for carrier in [KeyCarrier::HostHeader, KeyCarrier::PathPrefix] {
        let r = start(tcp(carrier)).await;
        let paths = ["/", "/p", "/s", "/nope", "/assets/app.js", "/../etc"];
        for p in paths {
            for (path, host) in [
                (p.to_string(), KEY.to_string()),
                (format!("/{KEY}{p}"), "phone.test".to_string()),
                (p.to_string(), "phone.test".to_string()),
            ] {
                for method in ["GET", "POST", "OPTIONS"] {
                    let res = http(&r.addr, method, &path, &[("host", &host)], &[])
                        .await
                        .unwrap();
                    let raw = String::from_utf8_lossy(&res.raw);
                    assert!(!raw.contains(KEY), "{method} {path}: {raw}");
                    assert!(!(300..400).contains(&res.status), "{method} {path}");
                    assert!(res.header("location").is_none());
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn origin_must_be_current_or_a_home_origin() {
    let r = local().await;
    let keys = r.bench.paired("Pixel");
    let _ = keys;
    let current = format!("https://{TEST_HOST}");
    for (origin, ok) in [
        (current.as_str(), true),
        ("https://evil.example", false),
        ("null", false),
        ("http://amber-otter-lamp-quiet.trycloudflare.com", false),
    ] {
        let mut h = keyed(&r);
        h.push(("origin", origin));
        let res = http(&r.addr, "OPTIONS", PATH_SESSION, &h, &[])
            .await
            .unwrap();
        assert_eq!(res.status == 204, ok, "{origin}");
        assert_eq!(
            Ws::connect(&r.addr, PATH_SESSION, &h).await.is_ok(),
            ok,
            "{origin}"
        );
    }
    // A phone paired at an earlier address keeps its home origin.
    r.bench
        .core
        .set_host(Some("new-address.trycloudflare.com".into()));
    let mut h = keyed(&r);
    h.push(("origin", &current));
    assert_eq!(
        http(&r.addr, "OPTIONS", PATH_SESSION, &h, &[])
            .await
            .unwrap()
            .status,
        204
    );
    r.bench.shell.engine.revoke_all_devices().unwrap();
    assert_eq!(
        http(&r.addr, "OPTIONS", PATH_SESSION, &h, &[])
            .await
            .unwrap()
            .status,
        403
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cors_echoes_exact_origin_with_vary() {
    let r = local().await;
    let origin = format!("https://{TEST_HOST}");
    let mut h = keyed(&r);
    h.push(("origin", &origin));
    let res = http(&r.addr, "OPTIONS", PATH_SESSION, &h, &[])
        .await
        .unwrap();
    assert_eq!(
        res.header("access-control-allow-origin"),
        Some(origin.as_str())
    );
    assert_eq!(res.header("vary"), Some("Origin"));
    let res = http(
        &r.addr,
        "GET",
        "/s?sid=00000000000000000000000000000000",
        &h,
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        res.header("access-control-allow-origin"),
        Some(origin.as_str())
    );
    assert_eq!(res.header("vary"), Some("Origin"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cors_never_wildcard_or_credentials() {
    let r = local().await;
    let origin = format!("https://{TEST_HOST}");
    for origin in [Some(origin.as_str()), Some("https://evil.example"), None] {
        for (method, path) in [
            ("OPTIONS", "/s"),
            ("GET", "/s?sid=00000000000000000000000000000000"),
            ("GET", "/p"),
            ("GET", "/sw.js"),
        ] {
            let mut h = keyed(&r);
            if let Some(o) = origin {
                h.push(("origin", o));
            }
            let res = http(&r.addr, method, path, &h, &[]).await.unwrap();
            for v in res.headers_named("access-control-allow-origin") {
                assert_ne!(v, "*");
                assert_eq!(Some(v), origin);
            }
            assert!(res.header("access-control-allow-credentials").is_none());
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_paths_are_404() {
    let r = local().await;
    for p in [
        "/api/v1/models",
        "/s/extra",
        "/.git/config",
        "/assets/../index.html",
        "/index.html.bak",
        "/sw.js/",
        "/remote/computer.key",
        "/missing.js",
    ] {
        let res = get(&r, p, &[]).await;
        assert_eq!(res.status, 404, "{p}");
        assert!(res.body.is_empty());
    }
    let mut h = keyed(&r);
    h.push(("content-type", "application/json"));
    assert_eq!(
        http(&r.addr, "POST", "/p", &h, b"{}").await.unwrap().status,
        404
    );
    assert_eq!(
        http(&r.addr, "PUT", "/s", &keyed(&r), b"")
            .await
            .unwrap()
            .status,
        404
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn lemond_never_reachable() {
    let lemond = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = lemond.local_addr().unwrap().port();
    let hits = Arc::new(AtomicUsize::new(0));
    let seen = hits.clone();
    // Counts what arrives asking for the paths below; a stray connection from another test
    // that happens to reuse this port asks for something else.
    tokio::spawn(async move {
        while let Ok((mut s, _)) = lemond.accept().await {
            let seen = seen.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut buf = vec![0u8; 4096];
                let n = tokio::time::timeout(Duration::from_secs(1), s.read(&mut buf))
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(0);
                let got = String::from_utf8_lossy(&buf[..n]);
                if got.contains("models") || got.contains("completions") {
                    seen.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    for r in [local().await, start(tcp(KeyCarrier::PathPrefix)).await] {
        if let Origin::Tcp { port: mine, .. } = r.handle.origin {
            assert_ne!(mine, port);
        }
        let target = format!("127.0.0.1:{port}");
        for p in [
            format!("http://{target}/api/v1/models"),
            "/api/v1/chat/completions".to_string(),
            "/v1/models".to_string(),
        ] {
            let mut h = keyed(&r);
            h.push(("x-forwarded-host", &target));
            let res = http(&r.addr, "GET", &p, &h, &[]).await.unwrap();
            assert_eq!(res.status, 404);
        }
    }
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn phone_headers_are_strict() {
    let r = local().await;
    for p in [
        "/",
        "/p",
        "/r",
        "/sw.js",
        "/manifest.webmanifest",
        "/assets/app.js",
    ] {
        let res = get(&r, p, &[]).await;
        assert_eq!(res.status, 200, "{p}");
        let csp = res.header("content-security-policy").unwrap();
        for part in [
            "default-src 'none'",
            "script-src 'self'",
            "frame-ancestors 'none'",
            "connect-src 'self' https://*.trycloudflare.com wss://*.trycloudflare.com",
            "base-uri 'none'",
            "form-action 'none'",
        ] {
            assert!(csp.contains(part), "{p}: {part}");
        }
        assert!(!csp.contains("unsafe"));
        assert_eq!(res.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(res.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(
            res.header("cross-origin-opener-policy"),
            Some("same-origin")
        );
        assert_eq!(
            res.header("permissions-policy"),
            Some("camera=(self), microphone=(self), geolocation=()")
        );
    }
    assert!(
        get(&r, "/sw.js", &[])
            .await
            .header("content-type")
            .unwrap()
            .starts_with("text/javascript")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn no_key_bucket_20_a_minute() {
    let r = start(tcp(KeyCarrier::HostHeader)).await;
    for _ in 0..LIMIT_NO_KEY + 5 {
        let res = http(&r.addr, "GET", "/p", &[("host", "nope")], &[])
            .await
            .unwrap();
        assert_eq!(res.status, 404, "always the same bare 404");
    }
    assert!(
        !r.bench
            .core
            .limits
            .has(nocturne_app_lib::remote::limits::Bucket::NoKey, "")
    );
    // The keyed path is not limited by the no-key bucket.
    assert_eq!(get(&r, "/p", &[]).await.status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn static_300_a_minute() {
    let r = local().await;
    let mut ok = 0;
    for _ in 0..LIMIT_STATIC {
        ok += usize::from(get(&r, "/sw.js", &[]).await.status == 200);
    }
    assert_eq!(ok, LIMIT_STATIC as usize);
    assert_eq!(get(&r, "/sw.js", &[]).await.status, 429);
    r.bench.mono.advance(Duration::from_secs(60));
    assert_eq!(get(&r, "/sw.js", &[]).await.status, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn oversized_frame_refused_before_read() {
    let r = local().await;
    // A body declared too large is refused from its headers alone.
    let limit = MAX_LONG_POLL_BATCH * (4 + MAX_FRAME);
    let big = (limit + 1).to_string();
    let mut h = keyed(&r);
    h.push(("content-length", &big));
    let res = http(&r.addr, "POST", "/s", &h, b"").await.unwrap();
    assert_eq!(res.status, 413);
    // One frame over MAX_FRAME in a long-poll batch.
    let mut body = ((MAX_FRAME + 1) as u32).to_be_bytes().to_vec();
    body.resize(4 + MAX_FRAME + 1, 0);
    let res = http(&r.addr, "POST", "/s", &keyed(&r), &body)
        .await
        .unwrap();
    assert_eq!(res.status, 413);
    // A WebSocket message over MAX_FRAME ends the socket.
    let mut ws = Ws::connect(&r.addr, PATH_SESSION, &keyed(&r))
        .await
        .unwrap();
    let _ = ws.send(&vec![0u8; MAX_FRAME + 1]).await;
    assert!(matches!(ws.recv().await, WsMsg::Close(_) | WsMsg::Eof));
}

#[tokio::test(flavor = "multi_thread")]
async fn long_poll_one_outstanding_per_direction() {
    let r = local().await;
    let keys = r.bench.paired("Pixel");
    let wire = Wire::open_with(&r.addr, Mode::LongPoll, &keyed(&r)).await;
    let Opened::Session(mut phone, _) = PhoneClient::open_on(wire, &keys).await else {
        panic!("a session")
    };
    let hex: String = phone.sid().iter().map(|b| format!("{b:02x}")).collect();
    let path = format!("/s?sid={hex}");
    let h: Vec<(String, String)> = keyed(&r)
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    let get = |path: String| {
        let (addr, h) = (r.addr.clone(), h.clone());
        async move {
            let hh: Vec<(&str, &str)> = h.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
            http(&addr, "GET", &path, &hh, &[]).await.unwrap()
        }
    };
    let first = tokio::spawn(get(path.clone()));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!first.is_finished(), "the first poll is held");
    assert_eq!(
        get(path.clone()).await.status,
        409,
        "one held poll per session"
    );
    // Up is free while down is held; the answer comes on the held poll.
    phone.request(RemoteCall::GetStatus).await.unwrap();
    let answered = tokio::time::timeout(Duration::from_secs(5), first)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answered.status, 200);
    let held = tokio::spawn(get(path.clone()));
    tokio::time::sleep(Duration::from_millis(200)).await;
    r.bench.core.close_all();
    let ended = tokio::time::timeout(Duration::from_secs(1), held).await;
    assert_eq!(
        ended.expect("the held poll ends at once").unwrap().status,
        410
    );
    assert_eq!(get(path).await.status, 410);
}

#[tokio::test(flavor = "multi_thread")]
async fn long_poll_batch_cap() {
    let r = local().await;
    let frame = [1u8; 30];
    let mut body = Vec::new();
    for _ in 0..MAX_LONG_POLL_BATCH + 1 {
        body.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        body.extend_from_slice(&frame);
    }
    let res = http(&r.addr, "POST", "/s", &keyed(&r), &body)
        .await
        .unwrap();
    assert_eq!(res.status, 413);
    assert_eq!(
        r.bench.core.conv_count(),
        0,
        "nothing was read into a conversation"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_cap() {
    let r = local().await;
    let mut held = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        held.push(
            Ws::connect(&r.addr, PATH_SESSION, &keyed(&r))
                .await
                .unwrap(),
        );
    }
    let res = get(&r, "/p", &[]).await;
    assert_eq!(res.status, 409, "full is not off");
    assert!(
        Ws::connect(&r.addr, PATH_SESSION, &keyed(&r))
            .await
            .is_err()
    );
    drop(held);
    let mut ok = false;
    for _ in 0..50 {
        if get(&r, "/p", &[]).await.status == 200 {
            ok = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(ok, "the cap frees as connections close");
}

#[tokio::test(flavor = "multi_thread")]
async fn handshake_timeout() {
    let r = local().await;
    let mut ws = Ws::connect(&r.addr, PATH_SESSION, &keyed(&r))
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let end = tokio::time::timeout(Duration::from_secs(HANDSHAKE_TIMEOUT_SECS + 10), ws.recv())
        .await
        .expect("closed by the listener");
    assert!(matches!(end, WsMsg::Close(_) | WsMsg::Eof));
    assert!(started.elapsed() >= Duration::from_secs(HANDSHAKE_TIMEOUT_SECS - 1));
}

#[tokio::test(flavor = "multi_thread")]
async fn long_poll_and_websocket_carry_the_same_frames() {
    let r = local().await;
    let keys = r.bench.paired("Pixel");
    let mut layouts = Vec::new();
    for mode in [Mode::WebSocket, Mode::LongPoll] {
        let wire = Wire::open_with(&r.addr, mode, &keyed(&r)).await;
        let Opened::Session(mut phone, hello) = PhoneClient::open_on(wire, &keys).await else {
            panic!("{mode:?}: a session")
        };
        assert_eq!(hello.device_name, "Pixel");
        let answer = phone.call(RemoteCall::GetStatus).await.unwrap();
        assert!(matches!(answer, RemoteAnswer::Status { .. }), "{mode:?}");
        let lens = |v: &Vec<Vec<u8>>| v.iter().map(|f| (f[1], f.len())).collect::<Vec<_>>();
        layouts.push((lens(&phone.wire.sent), lens(&phone.wire.received)));
    }
    assert_eq!(
        layouts[0], layouts[1],
        "the same frames, kinds and sizes, either way"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sent_bytes_counted_once() {
    let r = local().await;
    let engine = r.bench.shell.engine.clone();
    let page = get(&r, "/p", &[]).await;
    let keys = r.bench.paired("Pixel");
    let Opened::Session(mut phone, _) = PhoneClient::open_on(
        Wire::open_with(&r.addr, Mode::WebSocket, &keyed(&r)).await,
        &keys,
    )
    .await
    else {
        panic!("a session")
    };
    phone.call(RemoteCall::GetStatus).await.unwrap();
    let frames: usize = phone.wire.received.iter().map(Vec::len).sum();
    r.bench.core.flush_sent();
    r.bench.core.flush_sent();
    let sent = engine.remote_state().unwrap().sent_bytes as usize;
    assert_eq!(sent, page.body.len() + frames);
    let _ = End::Closed;
}

#[tokio::test(flavor = "multi_thread")]
async fn off_answers_503() {
    let r = local().await;
    r.bench.core.set_on(None);
    for (method, path) in [
        ("GET", "/p"),
        ("POST", "/s"),
        ("GET", "/s?sid=00000000000000000000000000000000"),
    ] {
        let res = http(&r.addr, method, path, &keyed(&r), &[]).await.unwrap();
        assert_eq!(res.status, 503, "{method} {path}");
    }
}
