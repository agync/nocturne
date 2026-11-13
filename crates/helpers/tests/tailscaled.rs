//! The Tailscale transport against the stand-in `tailscaled` and `tailscale`.
#![cfg(feature = "fake")]

mod common;

use std::time::Duration;

use common::{Sandbox, argv, env_keys, until, wait_state};
use nocturne_helpers::{
    HelperError, HelperKind, LaunchSpec, Origin, TailscaledOptions, TailscaledTransport, Transport,
    TransportState,
};
use serde_json::json;

const DAEMON: &str = env!("CARGO_BIN_EXE_fake-tailscaled");
const CLI: &str = env!("CARGO_BIN_EXE_fake-tailscale");

fn options() -> TailscaledOptions {
    TailscaledOptions {
        poll: Duration::from_millis(100),
        ready_within: Duration::from_secs(20),
        purge_every: Duration::from_millis(100),
        backoff: Some(vec![Duration::from_millis(50)]),
        grace: Some(Duration::from_secs(2)),
    }
}

fn transport(sb: &Sandbox) -> TailscaledTransport {
    let daemon = sb.install("t", DAEMON, "tailscaled", HelperKind::Tailscaled);
    let cli = sb.install("t", CLI, "tailscale", HelperKind::Tailscaled);
    TailscaledTransport::with_options(&sb.dirs(), &daemon, Some(cli), options())
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

fn all_argv(sb: &Sandbox) -> Vec<Vec<String>> {
    sb.records("t").iter().map(argv).collect()
}

#[tokio::test]
async fn daemon_args_are_exact_and_environment_says_no_logs() {
    let sb = Sandbox::new("dm");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    let records = sb.records("t");
    let daemon = records.iter().find(|r| r["role"] == "tailscaled").unwrap();
    let state = sb.dirs().root.join("tailscale");
    assert_eq!(
        argv(daemon),
        [
            "--tun=userspace-networking".to_string(),
            format!("--statedir={}", state.display()),
            format!("--socket={}", state.join("s.sock").display()),
            "--port=0".into(),
            "--no-logs-no-support".into(),
        ]
    );
    assert_eq!(daemon["env"]["TS_NO_LOGS_NO_SUPPORT"], "true");
    let mut keys = env_keys(daemon);
    keys.sort();
    assert_eq!(keys, ["HOME", "PATH", "TMPDIR", "TS_NO_LOGS_NO_SUPPORT"]);
    assert_eq!(
        daemon["env"]["HOME"],
        state.join("home").to_string_lossy().as_ref()
    );
    t.stop().await.unwrap();
}

#[tokio::test]
async fn address_is_the_tailnet_name() {
    let sb = Sandbox::new("ad");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let up = wait_state(&mut rx, "up", 30, is_up).await;
    assert_eq!(
        up,
        TransportState::Up {
            address: "https://nocturne-ab12.tail-fake.ts.net".into(),
            route: None
        }
    );
    let up_run = sb
        .records("t")
        .into_iter()
        .find(|r| r["role"] == "tailscale" && argv(r).contains(&"up".to_string()))
        .unwrap();
    let args = argv(&up_run);
    for want in [
        "--hostname=nocturne-ab12",
        "--accept-dns=false",
        "--accept-routes=false",
        "--ssh=false",
        "--json",
    ] {
        assert!(args.contains(&want.to_string()), "{want} in {args:?}");
    }
    t.stop().await.unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
}

#[tokio::test]
async fn sign_in_url_surfaces() {
    let sb = Sandbox::new("si");
    let t = transport(&sb);
    sb.control("t", json!({"needs_login": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let waiting = wait_state(&mut rx, "sign in", 30, |s| {
        matches!(s, TransportState::NeedsSignIn { .. })
    })
    .await;
    assert_eq!(
        waiting,
        TransportState::NeedsSignIn {
            url: "https://login.example.invalid/a/fake".into(),
            allow_links: false
        }
    );
    std::fs::write(sb.dirs().root.join("tailscale/fake-signin"), "").unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
}

#[tokio::test]
async fn allow_links_url_surfaces() {
    let sb = Sandbox::new("lk");
    let t = transport(&sb);
    sb.control("t", json!({"allow_links": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let waiting = wait_state(&mut rx, "allow links", 30, |s| {
        matches!(s, TransportState::NeedsSignIn { .. })
    })
    .await;
    assert_eq!(
        waiting,
        TransportState::NeedsSignIn {
            url: "https://login.example.invalid/f/serve?node=fake".into(),
            allow_links: true
        }
    );
    std::fs::write(sb.dirs().root.join("tailscale/fake-links"), "").unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
}

#[tokio::test]
async fn serve_targets_the_socket_or_key_path() {
    let sb = Sandbox::new("sv");
    let t = transport(&sb);
    let s = spec(&sb);
    let Origin::Unix(sock) = s.origin.clone() else {
        unreachable!()
    };
    let mut rx = t.start(s).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    let serve = all_argv(&sb)
        .into_iter()
        .find(|a| a.contains(&"serve".to_string()) && a.contains(&"--bg".to_string()))
        .expect("a serve call");
    assert_eq!(serve.last().unwrap(), &format!("unix:{}", sock.display()));
    t.stop().await.unwrap();
}

#[tokio::test]
async fn windows_serve_config_not_argv() {
    let sb = Sandbox::new("ws");
    let t = transport(&sb);
    let key = "k3y-0123456789abcdef";
    let s = LaunchSpec {
        origin: Origin::Tcp {
            port: 4100,
            key: key.into(),
        },
        machine_name: "nocturne-ab12".into(),
    };
    let mut rx = t.start(s).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    for record in sb.records("t") {
        assert!(!argv(&record).iter().any(|a| a.contains(key)), "{record}");
        assert!(!record["env"].to_string().contains(key), "{record}");
    }
    let config = sb.dirs().root.join("tailscale/serve-config.json");
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(text.contains(&format!("http://127.0.0.1:4100/{key}")));
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&config).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    t.stop().await.unwrap();
}

#[tokio::test]
async fn funnel_never_requested_and_refused_if_seen() {
    let sb = Sandbox::new("pub1");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
    assert!(!all_argv(&sb).iter().flatten().any(|a| a.contains("funnel")));

    let sb = Sandbox::new("pub2");
    let t = transport(&sb);
    sb.control("t", json!({"funnel_on": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let failed = wait_state(&mut rx, "refusal", 30, |s| {
        matches!(s, TransportState::Failed { .. })
    })
    .await;
    assert!(matches!(
        failed,
        TransportState::Failed {
            error: HelperError::Exited(_)
        }
    ));
    assert!(
        all_argv(&sb)
            .iter()
            .any(|a| a.contains(&"serve".to_string()) && a.contains(&"reset".to_string())),
        "the serve settings were reset"
    );
    assert!(!all_argv(&sb).iter().flatten().any(|a| a.contains("funnel")));
    t.stop().await.unwrap();
}

#[tokio::test]
async fn taildrop_files_purged() {
    let sb = Sandbox::new("td");
    let t = transport(&sb);
    sb.control("t", json!({"taildrop_files": true}));
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    let files = sb.dirs().root.join("tailscale/files");
    until("the hourly purge", 20, || !files.exists()).await;
    std::fs::create_dir_all(&files).unwrap();
    std::fs::write(files.join("late.partial"), "x").unwrap();
    t.stop().await.unwrap();
    assert!(!files.exists(), "stop deletes received files");
}

#[tokio::test]
async fn route_direct_or_relayed() {
    let sb = Sandbox::new("rt");
    let t = transport(&sb);
    sb.control(
        "t",
        json!({"peers": {"a": {"Online": true, "CurAddr": "203.0.113.5:41641", "Relay": ""}}}),
    );
    let mut rx = t.start(spec(&sb)).await.unwrap();
    let up = wait_state(
        &mut rx,
        "direct",
        30,
        |s| matches!(s, TransportState::Up { route: Some(r), .. } if r == "direct"),
    )
    .await;
    assert!(is_up(&up));
    sb.control(
        "t",
        json!({"peers": {"a": {"Online": true, "CurAddr": "", "Relay": "fra"}}}),
    );
    wait_state(
        &mut rx,
        "relayed",
        30,
        |s| matches!(s, TransportState::Up { route: Some(r), .. } if r == "relayed"),
    )
    .await;
    t.stop().await.unwrap();
}

#[tokio::test]
async fn sign_out_runs_logout_and_forgets_state() {
    let sb = Sandbox::new("so");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.sign_out().await.unwrap();
    assert!(
        all_argv(&sb)
            .iter()
            .any(|a| a.contains(&"logout".to_string()))
    );
    assert!(!sb.dirs().root.join("tailscale").exists());
    assert!(t.info().is_none());
    assert_eq!(*rx.borrow(), TransportState::Stopped);
}

#[tokio::test]
async fn sign_out_when_stopped_still_logs_out() {
    let sb = Sandbox::new("sos");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
    t.sign_out().await.unwrap();
    let logouts = all_argv(&sb)
        .iter()
        .filter(|a| a.contains(&"logout".to_string()))
        .count();
    assert_eq!(logouts, 1);
    assert!(!sb.dirs().root.join("tailscale").exists());
}

#[tokio::test]
async fn machine_name_neutral_and_stable() {
    let sb = Sandbox::new("nm");
    let t = transport(&sb);
    let mut bad = spec(&sb);
    bad.machine_name = "my-laptop".into();
    assert!(matches!(
        t.start(bad).await.err(),
        Some(HelperError::Spawn(_))
    ));
    assert!(sb.records("t").is_empty(), "nothing ran");
    for _ in 0..2 {
        let mut rx = t.start(spec(&sb)).await.unwrap();
        wait_state(&mut rx, "up", 30, is_up).await;
        t.stop().await.unwrap();
    }
    let names: Vec<_> = sb
        .records("t")
        .iter()
        .filter(|r| r["role"] == "tailscale")
        .flat_map(argv)
        .filter(|a| a.starts_with("--hostname="))
        .collect();
    assert!(names.len() >= 2 && names.iter().all(|n| n == "--hostname=nocturne-ab12"));
}

#[tokio::test]
async fn the_binaries_are_checked_before_every_start() {
    let sb = Sandbox::new("vf");
    let t = transport(&sb);
    let mut rx = t.start(spec(&sb)).await.unwrap();
    wait_state(&mut rx, "up", 30, is_up).await;
    t.stop().await.unwrap();
    let cli = sb.bin_file("t", "tailscale");
    let mut bytes = std::fs::read(&cli).unwrap();
    bytes.push(0);
    std::fs::write(&cli, bytes).unwrap();
    let before = sb.records("t").len();
    assert_eq!(t.start(spec(&sb)).await.err(), Some(HelperError::Mismatch));
    assert_eq!(sb.records("t").len(), before);
}
