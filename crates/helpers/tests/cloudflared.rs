//! The Cloudflare transport against the stand-in `cloudflared`.
#![cfg(feature = "fake")]

mod common;

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use common::{Sandbox, argv, env_keys, until, wait_state};
use nocturne_helpers::{
    CloudflaredOptions, CloudflaredTransport, HelperError, HelperKind, LaunchSpec, Origin,
    Transport, TransportState, group_members,
};
use serde_json::json;

const FAKE: &str = env!("CARGO_BIN_EXE_fake-cloudflared");

fn options() -> CloudflaredOptions {
    CloudflaredOptions {
        poll: Duration::from_millis(50),
        ready_within: Duration::from_secs(20),
        backoff: Some(vec![Duration::from_millis(50)]),
        grace: Some(Duration::from_secs(2)),
        health_every: Duration::from_millis(50),
        renew_backoff: vec![Duration::from_millis(50)],
        steady: Duration::from_secs(600),
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
        let up = wait_state(&mut rx, "up", 180, is_up).await;
        assert_eq!(
            address(up),
            format!("https://{}", control["address"].as_str().unwrap())
        );
        t.stop().await.unwrap();
    }
}

fn up_or_failed(s: &TransportState) -> bool {
    matches!(s, TransportState::Up { .. } | TransportState::Failed { .. })
}

fn impatient(sb: &Sandbox) -> CloudflaredTransport {
    let installed = sb.install("c", FAKE, "cloudflared", HelperKind::Cloudflared);
    let opts = CloudflaredOptions {
        ready_within: Duration::from_secs(3),
        ..options()
    };
    CloudflaredTransport::with_options(&sb.dirs(), &installed, opts)
}

#[tokio::test]
async fn another_program_on_the_metrics_port_is_not_believed() {
    let sb = Sandbox::new("spoof");
    let t = impatient(&sb);
    sb.control(
        "c",
        json!({
            "address": "own-name.trycloudflare.com",
            "spoof_metrics": "someone-else.trycloudflare.com",
        }),
    );
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    let end = wait_state(&mut rx, "up or failed", 180, up_or_failed).await;
    assert!(
        matches!(end, TransportState::Failed { .. }),
        "the two disagree, so neither is used: {end:?}"
    );
    t.stop().await.unwrap();
}

#[tokio::test]
async fn an_address_that_is_not_a_quick_tunnel_is_refused() {
    for (i, host) in [
        "attacker.example.com",
        "x.evil.trycloudflare.com",
        "Upper.trycloudflare.com",
    ]
    .into_iter()
    .enumerate()
    {
        let sb = Sandbox::new(&format!("bad-host-{i}"));
        let t = impatient(&sb);
        sb.control("c", json!({ "address": host }));
        let mut rx = t
            .start(spec(Origin::Unix(sb.root.join("l.sock"))))
            .await
            .unwrap();
        let end = wait_state(&mut rx, "up or failed", 180, up_or_failed).await;
        assert!(
            matches!(end, TransportState::Failed { .. }),
            "{host}: {end:?}"
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
    let first = address(wait_state(&mut rx, "first up", 180, is_up).await);
    t.stop().await.unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    let mut rx = t.start(spec(origin)).await.unwrap();
    let second = address(wait_state(&mut rx, "second up", 180, is_up).await);
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
    let first = address(wait_state(&mut rx, "first up", 180, is_up).await);
    wait_state(&mut rx, "restarting", 180, |s| {
        matches!(s, TransportState::Restarting { .. })
    })
    .await;
    let second = address(wait_state(&mut rx, "up again", 180, is_up).await);
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
    wait_state(&mut rx, "up", 180, is_up).await;
    let args = argv(&sb.records("c")[0]);
    let cfg = args.iter().position(|a| a == "--config").unwrap();
    assert_eq!(
        args[cfg + 1],
        sb.dirs()
            .root
            .join("cloudflared/config.yml")
            .to_string_lossy()
    );
    let config = std::fs::read_to_string(sb.dirs().root.join("cloudflared/config.yml")).unwrap();
    assert!(
        config.contains(&format!("- service: \"unix:{}\"", sock.to_string_lossy())),
        "{config}"
    );
    let at = args.iter().position(|a| a == "--url").unwrap();
    assert_eq!(args[at + 1], format!("unix:{}", sock.to_string_lossy()));
    assert!(
        !args.contains(&"--unix-socket".to_string()),
        "the real cloudflared refuses a quick tunnel with only --unix-socket"
    );
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
    let seen = common::origin_for(origin, "/from-the-test", b"HTTP/1.0 200 OK\r\n\r\nok");
    let key = "k3y-0123456789abcdef";
    let mut rx = t
        .start(spec(Origin::Tcp {
            port,
            key: key.into(),
        }))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 180, is_up).await;
    let records = sb.records("c");
    assert!(!records.is_empty(), "the helper ran and was recorded");
    for record in records {
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
    conn.write_all(b"GET /from-the-test HTTP/1.1\r\nHost: whatever.trycloudflare.com\r\n\r\n")
        .unwrap();
    let head = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || seen.join().unwrap()),
    )
    .await
    .expect("the request reached the origin")
    .unwrap();
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
    wait_state(&mut rx, "up", 180, is_up).await;
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
        "--origincert",
        "--logfile",
        "--credentials-file",
        "--hello-world",
    ] {
        assert!(!args.iter().any(|a| a == banned), "{banned}");
    }
    assert!(args.contains(&"--no-autoupdate".to_string()));
    assert!(
        args.contains(&"--config".to_string()),
        "its own config file, so none from /etc is read"
    );
    t.stop().await.unwrap();
}

#[tokio::test]
async fn the_binary_is_checked_before_every_start() {
    let sb = Sandbox::new("verify");
    let t = transport(&sb);
    let origin = Origin::Unix(sb.root.join("l.sock"));
    let mut rx = t.start(spec(origin.clone())).await.unwrap();
    wait_state(&mut rx, "up", 180, is_up).await;
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
    wait_state(&mut rx, "up", 180, is_up).await;
    t.stop().await.unwrap();
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("run-9f8e7d/l.sock"))))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 180, is_up).await;
    t.stop().await.unwrap();
    let records = sb.records("c");
    assert!(records.len() >= 2);
    for record in &records {
        let line = argv(record).join(" ");
        assert!(!line.contains(key), "{line}");
        if line.contains("--url http://") {
            assert!(!line.contains("9f8e7d"), "{line}");
        }
    }
}

fn with(sb: &Sandbox, opts: CloudflaredOptions) -> CloudflaredTransport {
    let installed = sb.install("c", FAKE, "cloudflared", HelperKind::Cloudflared);
    CloudflaredTransport::with_options(&sb.dirs(), &installed, opts)
}

/// The runs of the stand-in, oldest first.
fn runs(sb: &Sandbox) -> Vec<serde_json::Value> {
    sb.records("c")
        .into_iter()
        .filter(|r| r.get("argv").is_some())
        .collect()
}

fn is_renewing(s: &TransportState) -> bool {
    matches!(s, TransportState::Renewing { .. })
}

#[tokio::test]
async fn a_deleted_tunnel_is_renewed_with_a_new_address() {
    let sb = Sandbox::new("deleted");
    let t = with(
        &sb,
        CloudflaredOptions {
            renew_backoff: vec![Duration::from_millis(300)],
            ..options()
        },
    );
    sb.control("c", json!({"drop_after_ms": 300, "drop_runs": 1}));
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    let first = address(wait_state(&mut rx, "first up", 180, is_up).await);
    let old_pid = t.info().unwrap().pid.unwrap();
    wait_state(&mut rx, "renewing", 180, is_renewing).await;
    let second = address(wait_state(&mut rx, "up on a new name", 180, is_up).await);
    assert_ne!(first, second, "a deleted name never comes back");
    assert!(
        group_members(old_pid).is_empty(),
        "the helper that kept retrying the dead name is gone"
    );
    let info = t.info().unwrap();
    assert_ne!(info.pid, Some(old_pid));
    assert_eq!(info.restarts, 1);
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(runs(&sb).len(), 2, "one new tunnel, no more");
    assert_eq!(
        *rx.borrow(),
        TransportState::Up {
            address: second,
            route: None
        }
    );
    t.stop().await.unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    assert!(t.info().is_none());
}

#[tokio::test]
async fn a_short_drop_keeps_the_address() {
    let sb = Sandbox::new("blip");
    let t = transport(&sb);
    sb.control("c", json!({"drop_after_ms": 300, "deleted": false}));
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    let first = address(wait_state(&mut rx, "up", 180, is_up).await);
    let log = sb.dirs().root.join("logs/helper-cloudflared.log");
    until("the drop", 20, || {
        std::fs::read_to_string(&log).is_ok_and(|l| l.contains("Failed to dial a quic connection"))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        *rx.borrow(),
        TransportState::Up {
            address: first,
            route: None
        }
    );
    assert_eq!(
        runs(&sb).len(),
        1,
        "the helper rode it out on the same name"
    );
    t.stop().await.unwrap();
}

#[tokio::test]
async fn a_refusal_while_still_connected_keeps_the_address() {
    let sb = Sandbox::new("refused-connected");
    let t = transport(&sb);
    sb.control(
        "c",
        json!({"drop_after_ms": 300, "deleted": true, "still_ready": true}),
    );
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    let first = address(wait_state(&mut rx, "up", 180, is_up).await);
    let log = sb.dirs().root.join("logs/helper-cloudflared.log");
    until("the refusals", 20, || {
        std::fs::read_to_string(&log).is_ok_and(|l| l.contains("Tunnel not found"))
    })
    .await;
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        *rx.borrow(),
        TransportState::Up {
            address: first,
            route: None
        }
    );
    assert_eq!(
        runs(&sb).len(),
        1,
        "a connection is still up, so the name lives"
    );
    t.stop().await.unwrap();
}

#[tokio::test]
async fn renewals_in_a_row_wait_longer() {
    let sb = Sandbox::new("renew-backoff");
    let t = with(
        &sb,
        CloudflaredOptions {
            renew_backoff: vec![Duration::from_millis(300), Duration::from_millis(3000)],
            ..options()
        },
    );
    sb.control("c", json!({"drop_after_ms": 200}));
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    // When each renewal was announced, by the same clock the stand-in records its runs with.
    let mut announced = Vec::new();
    for attempt in 1..=2 {
        wait_state(
            &mut rx,
            "a renewal",
            180,
            |s| matches!(s, TransportState::Renewing { attempt: a } if *a == attempt),
        )
        .await;
        announced.push(nocturne_helpers::fake::now_ms());
    }
    until("a third run", 30, || runs(&sb).len() >= 3).await;
    let at: Vec<u64> = runs(&sb)
        .iter()
        .map(|r| r["at_ms"].as_u64().unwrap())
        .collect();
    assert!(
        at[1] > announced[0],
        "the first renewal starts after it is announced: {at:?} {announced:?}"
    );
    assert!(
        at[2].saturating_sub(announced[1]) >= 2500,
        "the second renewal in a row waits the second step: {at:?} {announced:?}"
    );
    t.stop().await.unwrap();
}

#[tokio::test]
async fn stopping_while_waiting_to_renew() {
    let sb = Sandbox::new("renew-stop");
    let t = with(
        &sb,
        CloudflaredOptions {
            renew_backoff: vec![Duration::from_secs(600)],
            ..options()
        },
    );
    sb.control("c", json!({"drop_after_ms": 200}));
    let mut rx = t
        .start(spec(Origin::Unix(sb.root.join("l.sock"))))
        .await
        .unwrap();
    wait_state(&mut rx, "up", 180, is_up).await;
    wait_state(&mut rx, "renewing", 180, is_renewing).await;
    tokio::time::timeout(Duration::from_secs(10), t.stop())
        .await
        .expect("a stop does not wait out the backoff")
        .unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    assert_eq!(runs(&sb).len(), 1);
}
