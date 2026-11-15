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
        .filter(|k| !common::set_by_the_platform(k))
        .collect();
    keys.sort_unstable();
    let mut allowed = vec![
        "HOME",
        "LANG",
        "LEMONADE_ADMIN_API_KEY",
        "LEMONADE_API_KEY",
        "PATH",
        "XDG_RUNTIME_DIR",
    ];
    if cfg!(windows) {
        allowed.extend(["TEMP", "TMP", "USERPROFILE"]);
        allowed.sort_unstable();
    }
    assert_eq!(keys, allowed);
    assert_eq!(env["LEMONADE_API_KEY"], endpoint.key.as_str());
    let admin = env["LEMONADE_ADMIN_API_KEY"].as_str().unwrap();
    assert!(!admin.is_empty());
    assert_ne!(admin, endpoint.key.as_str());
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
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(cfg["host"], "127.0.0.1");
    assert_eq!(cfg["offline"], true);
    assert_eq!(cfg["no_fetch_executables"], true);
    assert_eq!(cfg["broadcast"], false);
    assert_eq!(
        cfg["allowed_origins"],
        nocturne_lemonade::config::ALLOWED_ORIGINS
    );
    assert_eq!(cfg["telemetry"]["enabled"], false);
    let ws = cfg["websocket_port"].as_u64().unwrap() as u16;
    assert_eq!(
        sup.client().health().await.unwrap().websocket_port,
        Some(ws)
    );
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
    #[cfg(unix)]
    assert_eq!(
        common::processes_mentioning(&dirs.cache().to_string_lossy()),
        Vec::<u32>::new()
    );
    // What the engine said on standard error is kept, so a start that fails can be explained.
    let log = dirs.logs().join("lemond.log");
    let mut said = String::new();
    for _ in 0..100 {
        said = std::fs::read_to_string(&log).unwrap_or_default();
        if said.contains("address already in use") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(said.contains("address already in use"), "{said:?}");
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
    #[cfg(unix)]
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
    let cfg = sup.admin().config().await.unwrap();
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
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(
        (
            cfg["offline"].as_bool(),
            cfg["no_fetch_executables"].as_bool()
        ),
        (Some(false), Some(false))
    );

    sup.set_online(false).await.unwrap();
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(
        (
            cfg["offline"].as_bool(),
            cfg["no_fetch_executables"].as_bool()
        ),
        (Some(true), Some(true))
    );
    sup.shutdown().await.unwrap();
}

#[cfg(unix)]
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
    #[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(unix)]
#[tokio::test]
async fn dropping_the_supervisor_stops_the_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let pid = sup.pid().unwrap();
    drop(sup);
    assert!(common::wait_dead(pid, Duration::from_secs(5)).await);
}

#[cfg(unix)]
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

/// The admin key the supervisor gave the engine, read back from the engine's own environment.
async fn admin_key_of(sup: &Supervisor) -> String {
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
    env["LEMONADE_ADMIN_API_KEY"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn the_client_never_sees_the_admin_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let admin = admin_key_of(&sup).await;
    let client = sup.client();
    let endpoint = client.endpoint().unwrap();
    assert_ne!(endpoint.key, admin);
    for shown in [
        format!("{client:?}"),
        format!("{endpoint:?}"),
        format!("{:?}", sup.admin()),
    ] {
        assert!(!shown.contains(&admin), "{shown}");
    }
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn the_admin_endpoints_refuse_the_inference_key_and_take_the_admin_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let admin = admin_key_of(&sup).await;
    let endpoint = sup.client().endpoint().unwrap();
    let http = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/internal/config", endpoint.port);
    let with_api = http
        .get(&url)
        .bearer_auth(&endpoint.key)
        .send()
        .await
        .unwrap();
    assert_eq!(with_api.status(), 401);
    let with_none = http.get(&url).send().await.unwrap();
    assert_eq!(with_none.status(), 401);
    let with_admin = http.get(&url).bearer_auth(&admin).send().await.unwrap();
    assert_eq!(with_admin.status(), 200);
    assert_eq!(sup.admin().config().await.unwrap()["host"], "127.0.0.1");
    // The inference key still opens the ordinary routes.
    sup.client().health().await.unwrap();
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn an_empty_admin_key_is_impossible() {
    use nocturne_lemonade::Admin;
    assert!(Admin::new(1, "").is_err());
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    assert!(!admin_key_of(&sup).await.is_empty());
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn each_launch_gets_new_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let first = (
        admin_key_of(&sup).await,
        sup.client().endpoint().unwrap().key,
    );
    sup.set_online(true).await.unwrap();
    let second = (
        admin_key_of(&sup).await,
        sup.client().endpoint().unwrap().key,
    );
    assert_ne!(first.0, second.0);
    assert_ne!(first.1, second.1);
    assert_ne!(second.0, second.1);
    sup.shutdown().await.unwrap();
}

async fn engine_env(sup: &Supervisor) -> Value {
    let endpoint = sup.client().endpoint().unwrap();
    reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/fake/env", endpoint.port))
        .bearer_auth(&endpoint.key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn set_cloud_none_is_a_no_op_launch() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    // A provider and a row left by an earlier run are gone from the first launch on.
    std::fs::create_dir_all(dirs.config()).unwrap();
    std::fs::write(
        dirs.config().join("user_models.json"),
        r#"{"nocturne-online": {"recipe": "cloud", "cloud_provider": "nocturne_online"}}"#,
    )
    .unwrap();
    let sup = start(&dirs, None).await;
    let rows: Value =
        serde_json::from_slice(&std::fs::read(dirs.config().join("user_models.json")).unwrap())
            .unwrap();
    assert!(rows.get(nocturne_lemonade::online::ROW).is_none());

    let before = sup.admin().config().await.unwrap();
    sup.control().set_cloud(None).await.unwrap();
    assert_eq!(common::launch_count(&dirs), 2, "the engine was relaunched");
    assert_eq!(sup.restarts(), 0, "a relaunch is not a crash");
    let after = sup.admin().config().await.unwrap();
    assert_eq!(after["cloud_providers"], serde_json::json!([]));
    assert_eq!(after["offline"], before["offline"]);
    assert_eq!(after["offline"], true);
    let env = engine_env(&sup).await;
    let keys: Vec<&String> = env
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with("LEMONADE_") && k.ends_with("_API_KEY"))
        .collect();
    assert_eq!(keys, ["LEMONADE_ADMIN_API_KEY", "LEMONADE_API_KEY"]);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn hold_online_refcounts() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    let offline = || async { sup.admin().config().await.unwrap()["offline"].as_bool() };

    let first = control.hold_online().await.unwrap();
    assert_eq!(offline().await, Some(false));
    assert_eq!(common::launch_count(&dirs), 2);
    let second = control.hold_online().await.unwrap();
    assert_eq!(common::launch_count(&dirs), 2, "already open, no relaunch");

    first.release().await;
    assert_eq!(offline().await, Some(false), "one hold left");
    assert_eq!(common::launch_count(&dirs), 2);

    second.release().await;
    assert_eq!(offline().await, Some(true));
    assert_eq!(common::launch_count(&dirs), 3);

    // A dropped hold closes the window too.
    drop(control.hold_online().await.unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let closed = sup
            .admin()
            .config()
            .await
            .ok()
            .map(|c| c["offline"] == true);
        if common::launch_count(&dirs) == 5 && closed == Some(true) {
            break;
        }
        assert!(Instant::now() < deadline, "the window did not close");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(sup.restarts(), 0);
    sup.shutdown().await.unwrap();
}

fn stand_in_route() -> nocturne_lemonade::online::CloudRoute {
    nocturne_lemonade::online::CloudRoute::new(
        "https://api.example.com/v1",
        "stand-in-model".into(),
        nocturne_lemonade::online::OnlineKey::new("sk-stand-in-0123456789".into()),
    )
    .unwrap()
}

#[tokio::test]
async fn download_window_launch_has_no_provider() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    sup.control()
        .set_cloud(Some(stand_in_route()))
        .await
        .unwrap();
    let hold = sup.control().hold_online().await.unwrap();
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(cfg["offline"], false);
    assert_eq!(cfg["cloud_providers"], serde_json::json!([]));
    let rows = std::fs::read(dirs.config().join("user_models.json")).unwrap_or_default();
    assert!(!String::from_utf8_lossy(&rows).contains(nocturne_lemonade::online::ROW));
    let env = engine_env(&sup).await;
    assert!(env.get(nocturne_lemonade::online::KEY_ENV).is_none());
    assert!(!env.to_string().contains("sk-stand-in-0123456789"));
    hold.release().await;
    sup.shutdown().await.unwrap();
}

/// The engine inherits a core file limit of 0, so no core file of its memory is written. That
/// is all the limit does: a crash handler such as systemd-coredump still records the process
/// and its environment, which is why the Online answers key is never in the engine's
/// environment (`online_route.rs` checks `/proc/<pid>/environ`).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn engine_inherits_no_core_files() {
    nocturne_lemonade::supervisor::harden_process().unwrap();
    assert!(
        !nix::sys::prctl::get_dumpable().unwrap(),
        "the app is undumpable"
    );
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let pid = sup.pid().unwrap();
    let limits = std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap();
    let core = limits
        .lines()
        .find(|l| l.starts_with("Max core file size"))
        .unwrap();
    let fields: Vec<&str> = core.split_whitespace().collect();
    assert_eq!(&fields[4..6], ["0", "0"], "{core}");
    sup.shutdown().await.unwrap();
}
