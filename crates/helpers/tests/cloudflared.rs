//! The Cloudflare transport against the stand-in `cloudflared`.
#![cfg(feature = "fake")]

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use common::{Sandbox, argv, env_keys, until, wait_state};
use nocturne_helpers::{
    CloudflaredOptions, CloudflaredTransport, HelperError, HelperKind, LaunchSpec, Origin,
    Transport, TransportState,
};
use serde_json::json;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-cloudflared");

fn options() -> CloudflaredOptions {
    CloudflaredOptions {
        poll: Duration::from_millis(50),
        ready_within: Duration::from_secs(20),
        backoff: Some(vec![Duration::from_millis(50)]),
        grace: Some(Duration::from_secs(2)),
    }
}

fn transport(sb: &Sandbox) -> CloudflaredTransport {
    let installed = sb.install("c", FAKE, "cloudflared", HelperKind::Cloudflared);
    CloudflaredTransport::with_options(&sb.dirs(), &installed, options())
}

fn spec(origin: Origin) -> LaunchSpec {
    LaunchSpec {
        origin,
        machine_name: "nocturne-ab12".into(),
    }
}

fn is_up(s: &TransportState) -> bool {
    matches!(s, TransportState::Up { .. })
}

fn address(s: TransportState) -> String {
    match s {
        TransportState::Up { address, .. } => address,
        other => panic!("not up: {other:?}"),
    }
}

#[tokio::test]
async fn address_from_metrics_or_stderr() {
    for (label, control) in [
        (
            "metrics",
            json!({"address": "from-metrics.trycloudflare.com"}),
        ),
        (
            "stderr",
            json!({"address": "from-stderr.trycloudflare.com", "no_metrics": true}),
        ),
    ] {
        let sb = Sandbox::new(label);
        let t = transport(&sb);
        sb.control("c", control.clone());
        let mut rx = t
            .start(spec(Origin::Unix(sb.root.join("l.sock"))))
            .await
            .unwrap();
        let up = wait_state(&mut rx, "up", 30, is_up).await;
        assert_eq!(
            address(up),
            format!("https://{}", control["address"].as_str().unwrap())
        );
        t.stop().await.unwrap();
    }
}

#[tokio::test]
async fn new_address_each_start() {
    let sb = Sandbox::new("fresh");
    let t = transport(&sb);
    let origin = Origin::Unix(sb.root.join("l.sock"));
    let mut rx = t.start(spec(origin.clone())).await.unwrap();
    let first = address(wait_state(&mut rx, "first up", 30, is_up).await);
    t.stop().await.unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    let mut rx = t.start(spec(origin)).await.unwrap();
    let second = address(wait_state(&mut rx, "second up", 30, is_up).await);
    assert_ne!(first, second);
    t.stop().await.unwrap();
}

#[tokio::test]
async fn crash_restarts_and_reports_the_new_address() {
    let sb = Sandbox::new("crash");
    let t = transport(&sb);
    sb.control(
        "c",
        json!({"mode": "crash", "crash_runs": 1, "crash_after_ms": 400}),
    );
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    let first = address(wait_state(&mut rx, "first up", 30, is_up).await);
    wait_state(&mut rx, "restarting", 30, |s| {
        matches!(s, TransportState::Restarting { .. })
    })
    .await;
    let second = address(wait_state(&mut rx, "up again", 30, is_up).await);
    assert_ne!(first, second);
    assert_eq!(t.info().unwrap().restarts, 1);
    t.stop().await.unwrap();
    assert!(t.info().is_none());
}

#[tokio::test]
async fn unix_socket_origin() {
    let sb = Sandbox::new("unix");
    let t = transport(&sb);
    let sock = sb.root.join("l.sock");
    let listener = std::os::unix::net::UnixListener::bind(&sock).unwrap();
    std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 256];
            let _ = s.read(&mut buf);
            let _ = s.write_all(b"HTTP/1.0 200 OK\r\n\r\nfrom the unix origin");
        }
    });
    let mut rx = t.start(spec(Origin::Unix(sock.clone()))).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    let args = argv(&sb.records("c")[0]);
    let at = args.iter().position(|a| a == "--unix-socket").unwrap();
    assert_eq!(args[at + 1], sock.to_string_lossy());
    assert!(!args.contains(&"--url".to_string()));
    let port_file = sb.bin_file("c", "fake-public-port");
    until("the public port", 20, || port_file.exists()).await;
    let port: u16 = std::fs::read_to_string(port_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut conn = TcpStream::connect(("127.0.0.1", port)).unwrap();
    conn.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n")
        .unwrap();
    let mut body = String::new();
    conn.read_to_string(&mut body).unwrap();
    assert!(body.ends_with("from the unix origin"), "{body}");
    t.stop().await.unwrap();
}

#[tokio::test]
async fn windows_key_by_environment_not_argv() {
    let sb = Sandbox::new("key");
    let t = transport(&sb);
    let origin = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = origin.local_addr().unwrap().port();
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = origin.accept() {
            let mut buf = [0u8; 1024];
            let n = s.read(&mut buf).unwrap_or(0);
            let _ = seen_tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
            let _ = s.write_all(b"HTTP/1.0 200 OK\r\n\r\nok");
        }
    });
    let key = "k3y-0123456789abcdef";
    let mut rx = t
        .start(spec(Origin::Tcp {
            port,
            key: key.into(),
        }))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    for record in sb.records("c") {
        assert!(!argv(&record).iter().any(|a| a.contains(key)), "{record}");
        assert_eq!(record["env"]["TUNNEL_HTTP_HOST_HEADER"], key);
    }
    let port_file = sb.bin_file("c", "fake-public-port");
    until("the public port", 20, || port_file.exists()).await;
    let public: u16 = std::fs::read_to_string(port_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut conn = TcpStream::connect(("127.0.0.1", public)).unwrap();
    conn.write_all(b"GET / HTTP/1.1\r\nHost: whatever.trycloudflare.com\r\n\r\n")
        .unwrap();
    let head = seen_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(head.contains(&format!("Host: {key}")), "{head}");
    t.stop().await.unwrap();
}

#[tokio::test]
async fn own_home_not_the_persons() {
    let sb = Sandbox::new("home");
    let t = transport(&sb);
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    let record = &sb.records("c")[0];
    assert_eq!(
        record["env"]["HOME"],
        sb.dirs()
            .root
            .join("cloudflared")
            .to_string_lossy()
            .as_ref()
    );
    let mut keys = env_keys(record);
    keys.sort();
    assert_eq!(keys, ["HOME", "PATH", "TMPDIR"]);
    assert!(sb.dirs().root.join("cloudflared").is_dir());
    let args = argv(record);
    for banned in [
        "--config",
        "--origincert",
        "--logfile",
        "--credentials-file",
        "--hello-world",
    ] {
        assert!(!args.iter().any(|a| a == banned), "{banned}");
    }
    assert!(args.contains(&"--no-autoupdate".to_string()));
    t.stop().await.unwrap();
}

#[tokio::test]
async fn the_binary_is_checked_before_every_start() {
    let sb = Sandbox::new("verify");
    let t = transport(&sb);
    let origin = Origin::Unix(sb.root.join("l.sock"));
    let mut rx = t.start(spec(origin.clone())).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
    let path = sb.bin_file("c", "cloudflared");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes.push(0);
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(
        t.start(spec(origin)).await.err(),
        Some(HelperError::Mismatch)
    );
    assert_eq!(sb.records("c").len(), 1, "a changed helper never runs");
}

#[tokio::test]
async fn argv_never_holds_a_secret() {
    let sb = Sandbox::new("sec");
    let t = transport(&sb);
    let key = "k3y-fedcba9876543210";
    let mut rx = t
        .start(spec(Origin::Tcp {
            port: 4100,
            key: key.into(),
        }))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("run-9f8e7d/l.sock"))))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
    let records = sb.records("c");
    assert!(records.len() >= 2);
    for record in &records[..1] {
        let line = argv(record).join(" ");
        assert!(!line.contains(key) && !line.contains("9f8e7d"), "{line}");
    }
}
