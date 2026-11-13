//! The supervisor against the fake engine.

mod common;

use std::time::{Duration, Instant};

use nocturne_lemonade::{Dirs, RuntimeError, RuntimeState, Supervisor, SupervisorOptions};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};

fn quick() -> SupervisorOptions {
    SupervisorOptions {
        restart_limit: 3,
        health_interval: Duration::from_millis(200),
        start_timeout: Duration::from_secs(10),
        backoff_base: Duration::from_millis(100),
    }
}

async fn start(dirs: &Dirs, mode: Option<&str>) -> Supervisor {
    if let Some(m) = mode {
        common::set_mode(dirs, m);
    }
    Supervisor::start(dirs.clone(), quick()).await.unwrap()
}

async fn wait_state(
    rx: &mut tokio::sync::watch::Receiver<RuntimeState>,
    want: impl Fn(&RuntimeState) -> bool,
    within: Duration,
) -> RuntimeState {
    tokio::time::timeout(within, async {
        loop {
            let current = rx.borrow_and_update().clone();
            if want(&current) {
                return current;
            }
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("state not reached")
}

fn ready_port(state: &RuntimeState) -> Option<u16> {
    match state {
        RuntimeState::Ready { port, .. } => Some(*port),
        _ => None,
    }
}

#[tokio::test]
async fn start_reaches_ready_and_live_answers_with_the_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let state = sup.state().borrow().clone();
    let RuntimeState::Ready { port, version } = state else {
        panic!("not ready: {state:?}");
    };
    assert_eq!(version, "fake-1.0");
    let client = sup.client();
    client.live().await.unwrap();
    let health = client.health().await.unwrap();
    assert_eq!(health.version, "fake-1.0");
    let raw = reqwest::get(format!("http://127.0.0.1:{port}/v1/health"))
        .await
        .unwrap();
    assert_eq!(raw.status(), 401);
    assert!(sup.uptime().is_some());
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn two_supervisors_get_different_ports_and_keys() {
    let a_tmp = tempfile::tempdir().unwrap();
    let b_tmp = tempfile::tempdir().unwrap();
    let a = start(&common::dirs(a_tmp.path()), None).await;
    let b = start(&common::dirs(b_tmp.path()), None).await;
    let (ea, eb) = (
        a.client().endpoint().unwrap(),
        b.client().endpoint().unwrap(),
    );
    assert_ne!(ea.port, eb.port);
    assert_ne!(ea.key, eb.key);
    assert_eq!(ea.key.len(), 64);
    a.shutdown().await.unwrap();
    b.shutdown().await.unwrap();
}

#[tokio::test]
async fn the_child_environment_holds_only_what_we_allow() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let endpoint = sup.client().endpoint().unwrap();
    let env: Value = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/fake/env", endpoint.port))
        .bearer_auth(&endpoint.key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let mut keys: Vec<&str> = env
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "HOME",
            "LANG",
            "LEMONADE_API_KEY",
            "PATH",
            "XDG_RUNTIME_DIR"
        ]
    );
    assert_eq!(env["LEMONADE_API_KEY"], endpoint.key.as_str());
    assert_eq!(env["LANG"], "C.UTF-8");
    assert_eq!(
        env["XDG_RUNTIME_DIR"],
        dirs.run().to_string_lossy().as_ref()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dirs.run()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn our_config_is_what_the_engine_sees_and_the_websocket_port_is_pinned() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let client = sup.client();
    let cfg = client.config().await.unwrap();
    assert_eq!(cfg["host"], "127.0.0.1");
    assert_eq!(cfg["offline"], true);
    assert_eq!(cfg["no_fetch_executables"], true);
    assert_eq!(cfg["broadcast"], false);
    assert_eq!(cfg["telemetry"]["enabled"], false);
    let ws = cfg["websocket_port"].as_u64().unwrap() as u16;
    assert_eq!(client.health().await.unwrap().websocket_port, Some(ws));
    assert_ne!(Some(ws), sup.client().endpoint().map(|e| e.port));
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_crash_restarts_with_a_new_port_and_the_client_follows() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some("crash-first-after-ms=300")).await;
    let mut rx = sup.state();
    let first = ready_port(&rx.borrow()).unwrap();
    wait_state(
        &mut rx,
        |s| matches!(s, RuntimeState::Restarting { attempt: 1 }),
        Duration::from_secs(5),
    )
    .await;
    let again = wait_state(&mut rx, |s| ready_port(s).is_some(), Duration::from_secs(5)).await;
    assert_ne!(ready_port(&again), Some(first));
    assert_eq!(sup.restarts(), 1);
    sup.client().health().await.unwrap();
    assert_eq!(
        sup.client().endpoint().unwrap().port,
        ready_port(&again).unwrap()
    );
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_crash_loop_backs_off_then_fails_after_three_restarts() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let begun = Instant::now();
    let sup = start(&dirs, Some("crash-after-ms=150")).await;
    let mut rx = sup.state();
    let failed = match wait_state(
        &mut rx,
        |s| matches!(s, RuntimeState::Failed { .. }),
        Duration::from_secs(60),
    )
    .await
    {
        RuntimeState::Failed { reason } => reason,
        other => panic!("{other:?}"),
    };
    assert!(failed.contains("4 times"), "{failed}");
    assert_eq!(sup.restarts(), 3);
    // How long each restart waited is read from the engine's own record of when it died and when
    // the next one began: states seen from here can be missed or late on a busy machine.
    let events = common::timeline(&dirs);
    let kinds: Vec<&str> = events.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "launch", "crash", "launch", "crash", "launch", "crash", "launch", "crash"
        ],
        "{events:?}"
    );
    let at = |kind: &str| -> Vec<u128> {
        events
            .iter()
            .filter(|(k, _)| k == kind)
            .map(|(_, ms)| *ms)
            .collect()
    };
    let (launches, crashes) = (at("launch"), at("crash"));
    for i in 0..3 {
        let died = crashes[i];
        let relaunched = launches[i + 1];
        let floor = 100 * 2u128.pow(i as u32);
        assert!(
            relaunched >= died + floor,
            "restart {} came {} ms after the crash, wanted {floor}",
            i + 1,
            relaunched.saturating_sub(died)
        );
    }
    assert!(begun.elapsed() >= Duration::from_millis(700));
    assert!(matches!(
        sup.client().live().await,
        Err(RuntimeError::Unavailable)
    ));
    let launches = common::settled_launch_count(&dirs);
    assert_eq!(launches, 4, "no busy loop: one launch per crash");
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_busy_answer_to_the_key_check_is_waited_out_not_taken_for_a_port_clash() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some("busy-on-first-refusal")).await;
    assert!(ready_port(&sup.state().borrow()).is_some());
    assert_eq!(common::launch_count(&dirs), 1, "no new port was tried");
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn a_taken_port_is_retried_with_a_new_one() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some("fail-bind-first=2")).await;
    assert!(ready_port(&sup.state().borrow()).is_some());
    let launches = common::launch_count(&dirs);
    assert_eq!(launches, 3);
    assert_eq!(sup.restarts(), 0, "port retries are not crashes");
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn giving_up_on_ports_after_five_tries() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "fail-bind");
    let err = Supervisor::start(dirs.clone(), quick())
        .await
        .err()
        .unwrap();
    assert!(matches!(err, RuntimeError::Start(_)), "{err:?}");
    let launches = common::launch_count(&dirs);
    assert_eq!(launches, 5);
    #[cfg(target_os = "linux")]
    assert_eq!(
        common::processes_mentioning(&dirs.cache().to_string_lossy()),
        Vec::<u32>::new()
    );
}

#[tokio::test]
async fn a_slow_start_past_the_timeout_is_an_error_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "slow-start-ms=5000");
    let opts = SupervisorOptions {
        start_timeout: Duration::from_millis(400),
        ..quick()
    };
    let err = Supervisor::start(dirs.clone(), opts).await.err().unwrap();
    assert!(matches!(err, RuntimeError::Start(_)), "{err:?}");
    #[cfg(target_os = "linux")]
    assert_eq!(
        common::processes_mentioning(&dirs.cache().to_string_lossy()),
        Vec::<u32>::new()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn online_relaunches_the_engine_and_survives_a_crash() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let first = sup.client().endpoint().unwrap().port;
    sup.set_online(true).await.unwrap();
    assert_ne!(
        sup.client().endpoint().unwrap().port,
        first,
        "the engine was relaunched"
    );
    assert_eq!(sup.restarts(), 0, "a relaunch is not a crash");
    let cfg = sup.client().config().await.unwrap();
    assert_eq!(
        (
            cfg["offline"].as_bool(),
            cfg["no_fetch_executables"].as_bool()
        ),
        (Some(false), Some(false))
    );
    let launches = |d: &Dirs| common::launch_count(d);
    assert_eq!(launches(&dirs), 2);
    sup.set_online(true).await.unwrap();
    assert_eq!(launches(&dirs), 2, "no change, no relaunch");

    let pid = sup.pid().unwrap();
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    let mut rx = sup.state();
    wait_state(
        &mut rx,
        |s| matches!(s, RuntimeState::Restarting { .. }),
        Duration::from_secs(5),
    )
    .await;
    wait_state(&mut rx, |s| ready_port(s).is_some(), Duration::from_secs(5)).await;
    let cfg = sup.client().config().await.unwrap();
    assert_eq!(
        (
            cfg["offline"].as_bool(),
            cfg["no_fetch_executables"].as_bool()
        ),
        (Some(false), Some(false))
    );

    sup.set_online(false).await.unwrap();
    let cfg = sup.client().config().await.unwrap();
    assert_eq!(
        (
            cfg["offline"].as_bool(),
            cfg["no_fetch_executables"].as_bool()
        ),
        (Some(true), Some(true))
    );
    sup.shutdown().await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_stuck_engine_is_stopped_within_the_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some("hang-shutdown,no-watchdog,leak-sleeper")).await;
    let pgid = sup.pid().unwrap();
    let begun = Instant::now();
    sup.shutdown().await.unwrap();
    let took = begun.elapsed();
    assert!(
        took >= Duration::from_secs(2),
        "the request got its two seconds: {took:?}"
    );
    assert!(took < Duration::from_secs(12), "{took:?}");
    assert_eq!(common::group_members(pgid), Vec::<u32>::new());
}

/// Quitting cannot wait the full polite stop: past its budget the engine's whole group is
/// sent SIGTERM, then SIGKILL, and nothing of it is left a moment later.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn quitting_stops_a_stuck_engine_soon_after_the_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some("hang-shutdown,no-watchdog,leak-sleeper")).await;
    let pgid = sup.pid().unwrap();
    let begun = Instant::now();
    sup.shutdown_within(Duration::from_secs(1)).await.unwrap();
    let took = begun.elapsed();
    assert!(
        took >= Duration::from_secs(1),
        "the request got its budget: {took:?}"
    );
    assert!(took < Duration::from_millis(2_000), "{took:?}");
    assert_eq!(common::group_members(pgid), Vec::<u32>::new());
}

#[tokio::test]
async fn an_engine_that_ignores_its_key_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "ignore-key");
    let err = Supervisor::start(dirs.clone(), quick())
        .await
        .err()
        .unwrap();
    assert!(
        matches!(err, RuntimeError::Start(ref m) if m.contains("key")),
        "{err:?}"
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        common::processes_mentioning(&dirs.cache().to_string_lossy()),
        Vec::<u32>::new()
    );
}

#[tokio::test]
async fn an_engine_on_the_wrong_websocket_port_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "bad-ws-port");
    let err = Supervisor::start(dirs, quick()).await.err().unwrap();
    assert!(
        matches!(err, RuntimeError::Start(ref m) if m.contains("websocket")),
        "{err:?}"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn shutdown_leaves_no_process_in_the_group() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some("leak-sleeper")).await;
    let pgid = sup.pid().unwrap();
    assert!(
        common::group_members(pgid).len() >= 2,
        "the engine and its leaked helper"
    );
    sup.shutdown().await.unwrap();
    assert_eq!(common::group_members(pgid), Vec::<u32>::new());
    assert!(!common::alive(pgid));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn dropping_the_supervisor_stops_the_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let pid = sup.pid().unwrap();
    drop(sup);
    assert!(common::wait_dead(pid, Duration::from_secs(5)).await);
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn killing_the_owner_makes_the_engine_exit_by_itself() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "spawn-sleeper");
    let mut host = tokio::process::Command::new(env!("CARGO_BIN_EXE_supervisor-host"))
        .arg(&dirs.bundle)
        .arg(&dirs.root)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(host.stdout.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let pid: u32 = line.split_whitespace().nth(2).unwrap().parse().unwrap();
    assert!(common::alive(pid));
    host.kill().await.unwrap();
    assert!(
        common::wait_dead(pid, Duration::from_secs(5)).await,
        "the engine outlived its owner"
    );
    assert_eq!(common::group_members(pid), Vec::<u32>::new());
}
