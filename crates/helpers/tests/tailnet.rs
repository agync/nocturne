//! The Most private transport against the stand-in `nocturne-tailnet`.
#![cfg(feature = "fake")]

mod common;

use std::io::{Read, Write};
use std::time::Duration;

use common::{Sandbox, argv, env_keys, until, wait_state};
use nocturne_helpers::{
    APPROVE_URL, HTTPS_URL, HelperError, HelperKind, LaunchSpec, Origin, TailnetOptions,
    TailnetTransport, Transport, TransportState,
};
use serde_json::{Value, json};

const FAKE: &str = env!("CARGO_BIN_EXE_fake-tailnet");

fn options() -> TailnetOptions {
    TailnetOptions {
        poll: Duration::from_millis(200),
        cert_grace: Duration::from_millis(400),
        stop_wait: Duration::from_secs(5),
        logout_wait: Duration::from_secs(20),
        backoff: Some(vec![Duration::from_millis(50)]),
        grace: Some(Duration::from_secs(2)),
    }
}

fn transport_with(sb: &Sandbox, opts: TailnetOptions) -> TailnetTransport {
    let helper = sb.install("t", FAKE, "nocturne-tailnet", HelperKind::Tailnet);
    TailnetTransport::with_options(&sb.dirs(), &helper, opts)
}

fn transport(sb: &Sandbox) -> TailnetTransport {
    transport_with(sb, options())
}

fn spec(sb: &Sandbox) -> LaunchSpec {
    LaunchSpec {
        origin: Origin::Unix(sb.root.join("remote/run/listener.sock")),
        machine_name: "nocturne-ab12".into(),
    }
}

fn is_up(s: &TransportState) -> bool {
    matches!(s, TransportState::Up { .. })
}

fn is_failed(s: &TransportState) -> bool {
    matches!(s, TransportState::Failed { .. })
}

fn runs(sb: &Sandbox) -> Vec<Value> {
    sb.records("t")
        .into_iter()
        .filter(|r| r["role"] == "tailnet")
        .collect()
}

/// `(role, value, at_ms)` of every input line and exit, in order.
fn notes(sb: &Sandbox) -> Vec<(String, Value, u64)> {
    sb.records("t")
        .into_iter()
        .filter(|r| r["role"] == "stdin" || r["role"] == "exit")
        .map(|r| {
            (
                r["role"].as_str().unwrap().to_string(),
                r["value"].clone(),
                r["at_ms"].as_u64().unwrap(),
            )
        })
        .collect()
}

fn commands(sb: &Sandbox) -> Vec<String> {
    notes(sb)
        .into_iter()
        .filter(|(role, _, _)| role == "stdin")
        .filter_map(|(_, v, _)| v.as_str().map(String::from))
        .filter(|l| l != r#"{"cmd":"status"}"#)
        .collect()
}

fn script(sb: &Sandbox, events: Value) {
    sb.control("t", json!({ "events": events }));
}

fn touch(sb: &Sandbox, name: &str) {
    std::fs::write(sb.bin_file("t", name), "").unwrap();
}

fn alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[tokio::test]
async fn args_exact_and_env_scrubbed() {
    let sb = Sandbox::new("tn-args");
    let t = transport(&sb);
    let s = spec(&sb);
    let Origin::Unix(sock) = s.origin.clone() else {
        unreachable!()
    };
    let mut rx = t.start(s).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    let run = &runs(&sb)[0];
    let state = sb.dirs().root.join("tailscale");
    assert_eq!(
        argv(run),
        [
            "--state-dir".to_string(),
            state.display().to_string(),
            "--hostname".into(),
            "nocturne-ab12".into(),
            "--target".into(),
            format!("unix:{}", sock.display()),
        ]
    );
    let mut keys = env_keys(run);
    keys.sort();
    assert_eq!(keys, ["HOME", "PATH", "TMPDIR"]);
    assert!(commands(&sb).is_empty(), "no key line for a socket target");
    t.stop().await.unwrap();
}

#[tokio::test]
async fn address_is_the_tailnet_name() {
    let sb = Sandbox::new("tn-addr");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let up = wait_state(&mut rx, "up", 60, is_up).await;
    assert_eq!(
        up,
        TransportState::Up {
            address: "https://nocturne-ab12.tail-fake.ts.net".into(),
            route: None
        }
    );
    let pid = t.info().unwrap().pid.unwrap();
    assert_eq!(t.info().unwrap().name, "nocturne-tailnet");
    t.stop().await.unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    assert!(!alive(pid));
    assert_eq!(commands(&sb), [r#"{"cmd":"stop"}"#]);
    assert!(notes(&sb).iter().any(|(r, v, _)| r == "exit" && v == 0));
}

#[tokio::test]
async fn status_is_polled() {
    let sb = Sandbox::new("tn-poll");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    until("two status polls", 30, || {
        notes(&sb)
            .iter()
            .filter(|(_, v, _)| v == r#"{"cmd":"status"}"#)
            .count()
            >= 2
    })
    .await;
    assert!(is_up(&rx.borrow()), "a repeated running stays up");
    t.stop().await.unwrap();
}

#[tokio::test]
async fn needs_login_is_sign_in() {
    let sb = Sandbox::new("tn-login");
    let t = transport(&sb);
    script(
        &sb,
        json!([
            {"state": "needsLogin", "authUrl": "https://login.tailscale.com/a/fake"},
            {"wait": "signed-in"},
            {"state": "running", "host": "nocturne-ab12.tail-fake.ts.net"},
        ]),
    );
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let waiting = wait_state(&mut rx, "sign in", 60, |s| {
        matches!(s, TransportState::NeedsSignIn { .. })
    })
    .await;
    assert_eq!(
        waiting,
        TransportState::NeedsSignIn {
            url: "https://login.tailscale.com/a/fake".into(),
            allow_links: false
        }
    );
    touch(&sb, "signed-in");
    wait_state(&mut rx, "up", 60, is_up).await;
    t.stop().await.unwrap();
}

#[tokio::test]
async fn needs_https_is_allow_links() {
    let sb = Sandbox::new("tn-https");
    let t = transport(&sb);
    script(
        &sb,
        json!([
            {"state": "needsHttps"},
            {"wait": "allowed"},
            {"state": "running", "host": "nocturne-ab12.tail-fake.ts.net"},
        ]),
    );
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let waiting = wait_state(&mut rx, "allow links", 60, |s| {
        matches!(s, TransportState::NeedsSignIn { .. })
    })
    .await;
    assert_eq!(
        waiting,
        TransportState::NeedsSignIn {
            url: HTTPS_URL.into(),
            allow_links: true
        }
    );
    touch(&sb, "allowed");
    wait_state(&mut rx, "up", 60, is_up).await;
    t.stop().await.unwrap();
}

#[tokio::test]
async fn needs_approval_waits_with_the_helper_alive() {
    let sb = Sandbox::new("tn-approve");
    let t = transport(&sb);
    script(
        &sb,
        json!([
            {"state": "needsApproval"},
            {"wait": "approved"},
            {"state": "running", "host": "nocturne-ab12.tail-fake.ts.net"},
        ]),
    );
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let waiting = wait_state(&mut rx, "approval", 60, |s| {
        matches!(s, TransportState::NeedsApproval { .. })
    })
    .await;
    assert_eq!(
        waiting,
        TransportState::NeedsApproval {
            url: APPROVE_URL.into()
        }
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    let info = t.info().unwrap();
    assert!(alive(info.pid.unwrap()));
    assert_eq!(info.restarts, 0);
    assert_eq!(*rx.borrow(), waiting);
    touch(&sb, "approved");
    wait_state(&mut rx, "up", 60, is_up).await;
    t.stop().await.unwrap();
}

/// A failure that ends the run: Failed, the helper stopped and never started again.
async fn fails_for_good(label: &str, events: Value) -> (Sandbox, TransportState) {
    let sb = Sandbox::new(label);
    let t = transport(&sb);
    script(&sb, events);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let failed = wait_state(&mut rx, "failure", 60, is_failed).await;
    until("the helper ends", 30, || {
        t.info().is_some_and(|i| i.pid.is_none())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs(&sb).len(), 1, "{label}: not started again");
    assert_eq!(*rx.borrow(), failed, "{label}: stays failed");
    t.stop().await.unwrap();
    (sb, failed)
}

#[tokio::test]
async fn errors_that_end_the_helper_are_failures() {
    for reason in ["start", "backend"] {
        let (_, failed) = fails_for_good(
            &format!("tn-exit-{reason}"),
            json!([{"state": "error", "reason": reason}, {"state": "stopped"}, {"exit": 1}]),
        )
        .await;
        assert!(
            matches!(
                failed,
                TransportState::Failed {
                    error: HelperError::Exited(_)
                }
            ),
            "{reason}: {failed:?}"
        );
    }
}

#[tokio::test]
async fn errors_that_leave_it_running_stop_it() {
    for reason in ["listen", "backend"] {
        let (sb, failed) = fails_for_good(
            &format!("tn-stay-{reason}"),
            json!([{"state": "error", "reason": reason}]),
        )
        .await;
        assert!(is_failed(&failed));
        assert!(commands(&sb).contains(&r#"{"cmd":"stop"}"#.to_string()));
    }
}

#[tokio::test]
async fn a_refused_key_is_a_failure() {
    let sb = Sandbox::new("tn-key");
    let t = transport(&sb);
    let s = LaunchSpec {
        origin: Origin::Tcp {
            port: 4100,
            key: "short".into(),
        },
        machine_name: "nocturne-ab12".into(),
    };
    let mut rx = t.start(s).await.unwrap();
    let failed = wait_state(&mut rx, "failure", 60, is_failed).await;
    assert!(matches!(
        failed,
        TransportState::Failed {
            error: HelperError::Exited(_)
        }
    ));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs(&sb).len(), 1);
    t.stop().await.unwrap();
}

#[tokio::test]
async fn blocked_means_the_account_needs_logging() {
    let (_, failed) = fails_for_good(
        "tn-blocked",
        json!([{"state": "starting"}, {"state": "error", "reason": "blocked"}]),
    )
    .await;
    assert_eq!(
        failed,
        TransportState::Failed {
            error: HelperError::AccountBlocked
        }
    );
}

#[tokio::test]
async fn certificate_trouble_shows_after_the_grace() {
    let sb = Sandbox::new("tn-cert");
    let t = transport(&sb);
    script(
        &sb,
        json!([
            {"state": "error", "reason": "certificate"},
            {"wait": "cert-ok"},
            {"state": "running", "host": "nocturne-ab12.tail-fake.ts.net"},
        ]),
    );
    let started = std::time::Instant::now();
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let failed = wait_state(&mut rx, "the problem", 60, is_failed).await;
    assert!(started.elapsed() >= Duration::from_millis(400));
    assert!(matches!(
        failed,
        TransportState::Failed {
            error: HelperError::Exited(_)
        }
    ));
    let pid = t.info().unwrap().pid.unwrap();
    assert!(alive(pid), "the helper keeps trying");
    until("a status poll after the problem", 30, || {
        notes(&sb)
            .iter()
            .filter(|(_, v, _)| v == r#"{"cmd":"status"}"#)
            .count()
            >= 3
    })
    .await;
    assert!(
        is_failed(&rx.borrow()),
        "a repeated starting does not hide it"
    );
    touch(&sb, "cert-ok");
    wait_state(&mut rx, "up", 60, is_up).await;
    assert_eq!(runs(&sb).len(), 1);
    t.stop().await.unwrap();
}

#[tokio::test]
async fn certificate_trouble_within_the_grace_is_not_shown() {
    let sb = Sandbox::new("tn-cert-short");
    let t = transport_with(
        &sb,
        TailnetOptions {
            cert_grace: Duration::from_secs(30),
            ..options()
        },
    );
    script(
        &sb,
        json!([
            {"state": "error", "reason": "certificate"},
            {"sleep_ms": 300},
            {"state": "running", "host": "nocturne-ab12.tail-fake.ts.net"},
        ]),
    );
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let mut seen = Vec::new();
    loop {
        let now = rx.borrow_and_update().clone();
        seen.push(now.clone());
        if is_up(&now) {
            break;
        }
        tokio::time::timeout(Duration::from_secs(60), rx.changed())
            .await
            .unwrap()
            .unwrap();
    }
    assert!(!seen.iter().any(is_failed), "{seen:?}");
    t.stop().await.unwrap();
}

#[tokio::test]
async fn stopped_then_exit_is_stopped() {
    let sb = Sandbox::new("tn-clean");
    let t = transport(&sb);
    script(&sb, json!([{"state": "stopped"}, {"exit": 0}]));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "stopped", 60, |s| *s == TransportState::Stopped).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs(&sb).len(), 1, "not started again");
    t.stop().await.unwrap();
}

#[tokio::test]
async fn an_exit_without_stopped_is_a_crash_and_restarts() {
    let sb = Sandbox::new("tn-crash");
    let t = transport(&sb);
    sb.control("t", json!({"mode": "crash", "crash_runs": 1}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "restarting", 60, |s| {
        matches!(s, TransportState::Restarting { attempt: 1 })
    })
    .await;
    wait_state(&mut rx, "up again", 60, is_up).await;
    assert_eq!(runs(&sb).len(), 2);
    assert_eq!(t.info().unwrap().restarts, 1);
    t.stop().await.unwrap();
}

#[tokio::test]
async fn a_refusal_is_not_restarted() {
    let sb = Sandbox::new("tn-refuse");
    let t = transport(&sb);
    sb.control("t", json!({"refuse": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let failed = wait_state(&mut rx, "refusal", 60, is_failed).await;
    assert!(matches!(
        failed,
        TransportState::Failed {
            error: HelperError::Spawn(_)
        }
    ));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(runs(&sb).len(), 1);
    t.stop().await.unwrap();
}

#[tokio::test]
async fn stop_asks_then_terminates() {
    let sb = Sandbox::new("tn-stop");
    let t = transport_with(
        &sb,
        TailnetOptions {
            stop_wait: Duration::from_millis(600),
            ..options()
        },
    );
    sb.control("t", json!({"ignore_stop": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    let pid = t.info().unwrap().pid.unwrap();
    let asked = std::time::Instant::now();
    t.stop().await.unwrap();
    assert!(
        asked.elapsed() >= Duration::from_millis(600),
        "waited first"
    );
    assert!(!alive(pid));
    assert_eq!(commands(&sb), [r#"{"cmd":"stop"}"#]);
    assert_eq!(*rx.borrow(), TransportState::Stopped);
}

#[tokio::test]
async fn sign_out_logs_out_waits_then_forgets() {
    let sb = Sandbox::new("tn-signout");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    let state = sb.dirs().root.join("tailscale");
    assert!(state.join("fake-node.state").exists());
    let pid = t.info().unwrap().pid.unwrap();
    t.sign_out().await.unwrap();
    assert_eq!(
        commands(&sb),
        [r#"{"cmd":"logout"}"#],
        "logout first, no stop"
    );
    let n = notes(&sb);
    let logout_at = n
        .iter()
        .find(|(_, v, _)| v == r#"{"cmd":"logout"}"#)
        .unwrap()
        .2;
    let exit_at = n.iter().find(|(r, _, _)| r == "exit").unwrap().2;
    assert!(exit_at >= logout_at);
    assert!(!alive(pid));
    assert!(
        !state.exists(),
        "the state folder goes after the helper ended"
    );
    assert!(t.info().is_none());
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    assert_eq!(runs(&sb).len(), 1);
}

#[tokio::test]
async fn a_failed_sign_out_still_forgets() {
    let sb = Sandbox::new("tn-signout-fail");
    let t = transport(&sb);
    sb.control("t", json!({"logout_fails": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    assert!(matches!(t.sign_out().await, Err(HelperError::SignIn(_))));
    assert!(!sb.dirs().root.join("tailscale").exists());
    assert!(t.info().is_none());
}

#[tokio::test]
async fn sign_out_while_off_touches_nothing() {
    let sb = Sandbox::new("tn-off");
    let t = transport(&sb);
    let state = sb.dirs().root.join("tailscale");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(state.join("tailscaled.state"), "{}").unwrap();
    t.sign_out().await.unwrap();
    assert!(sb.records("t").is_empty(), "nothing ran");
    assert!(!state.exists());
    t.sign_out().await.unwrap();
    assert_eq!(t.stop().await, Ok(()));
    assert!(t.info().is_none());
    assert!(sb.records("t").is_empty());
}

#[tokio::test]
async fn windows_key_goes_on_input_and_into_the_header() {
    let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = target.local_addr().unwrap().port();
    let seen = std::thread::spawn(move || {
        let (mut conn, _) = target.accept().unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") && conn.read(&mut byte).unwrap() == 1 {
            head.push(byte[0]);
        }
        let _ = conn.write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n");
        String::from_utf8(head).unwrap()
    });
    let sb = Sandbox::new("tn-key-header");
    let t = transport(&sb);
    let key = "k3yk3yk3y0123456789abcdefABCDEF";
    let s = LaunchSpec {
        origin: Origin::Tcp {
            port,
            key: key.into(),
        },
        machine_name: "nocturne-ab12".into(),
    };
    let mut rx = t.start(s).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    let run = &runs(&sb)[0];
    assert_eq!(argv(run)[5], format!("http://127.0.0.1:{port}"));
    assert!(!run["argv"].to_string().contains(key));
    assert!(!run["env"].to_string().contains(key));
    let first = notes(&sb)
        .into_iter()
        .find(|(r, _, _)| r == "stdin")
        .unwrap()
        .1;
    assert_eq!(first, json!({ "key": key }).to_string());

    let public: u16 = std::fs::read_to_string(sb.bin_file("t", "fake-public-port"))
        .unwrap()
        .parse()
        .unwrap();
    let mut phone = std::net::TcpStream::connect(("127.0.0.1", public)).unwrap();
    phone
        .write_all(b"GET /p HTTP/1.1\r\nHost: x\r\nnocturne_helper_key: forged\r\n\r\n")
        .unwrap();
    let head = tokio::task::spawn_blocking(move || seen.join().unwrap())
        .await
        .unwrap();
    let keys: Vec<&str> = head
        .lines()
        .filter(|l| {
            l.to_ascii_lowercase()
                .replace('_', "-")
                .starts_with("nocturne-helper-key:")
        })
        .collect();
    assert_eq!(keys, [format!("Nocturne-Helper-Key: {key}")]);
    t.stop().await.unwrap();
}

#[tokio::test]
async fn machine_name_and_binary_checked_before_every_start() {
    let sb = Sandbox::new("tn-check");
    let t = transport(&sb);
    let mut bad = spec(&sb);
    bad.machine_name = "my-laptop".into();
    assert!(matches!(
        t.start(bad).await.err(),
        Some(HelperError::Spawn(_))
    ));
    assert!(sb.records("t").is_empty(), "nothing ran");
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 60, is_up).await;
    t.stop().await.unwrap();
    let bin = sb.bin_file("t", "nocturne-tailnet");
    let mut bytes = std::fs::read(&bin).unwrap();
    bytes.push(0);
    std::fs::write(&bin, bytes).unwrap();
    let before = sb.records("t").len();
    assert_eq!(t.start(spec(&sb)).await.err(), Some(HelperError::Mismatch));
    assert_eq!(sb.records("t").len(), before);
}

#[tokio::test]
async fn off_means_off_through_the_real_lookup() {
    let sb = Sandbox::new("tn-real-off");
    let dirs = sb.dirs();
    let lock = nocturne_helpers::HelpersLock::current();
    let version = &lock.helper(HelperKind::Tailnet).unwrap().version;
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let dir = dirs.root.join("helpers/tailnet").join(version);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(FAKE, dir.join(format!("nocturne-tailnet{exe}"))).unwrap();
    let state = dirs.root.join("tailscale");
    std::fs::create_dir_all(&state).unwrap();
    let Some(found) = nocturne_helpers::installed(HelperKind::Tailnet, &dirs) else {
        assert!(nocturne_helpers::platform().is_none());
        return;
    };
    let t = nocturne_helpers::transport_for(HelperKind::Tailnet, &dirs, &found);
    t.sign_out().await.unwrap();
    t.stop().await.unwrap();
    assert!(!state.exists(), "only the local state is forgotten");
    assert!(t.info().is_none());
    assert!(
        !dir.join("fake-record.jsonl").exists(),
        "the helper never ran"
    );
    let (tx, _) = tokio::sync::watch::channel(Default::default());
    assert_eq!(
        nocturne_helpers::install(
            HelperKind::Tailnet,
            &dirs,
            tx,
            tokio_util::sync::CancellationToken::new()
        )
        .await,
        Err(HelperError::Unsupported),
        "a placeholder pin downloads nothing"
    );
}
