//! The engine route for Online answers against the fake engine: what each launch writes and
//! hands the engine, the address rules, the relaunches, and the redacting log pipe.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use nocturne_engine::OnlineFailure;
use nocturne_lemonade::client::{ChatRequest, Message};
use nocturne_lemonade::logpipe::{self, Redactor};
use nocturne_lemonade::online::{
    self, CloudRoute, KEY_ENV, MODEL_ID, OnlineKey, PROVIDER, ROW, UrlProblem, classify,
    validate_url,
};
use nocturne_lemonade::{Dirs, Supervisor, SupervisorOptions};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

const KEY: &str = "sk-route-test-0123456789abcdefghij";

fn quick() -> SupervisorOptions {
    SupervisorOptions {
        restart_limit: 3,
        health_interval: Duration::from_millis(200),
        start_timeout: Duration::from_secs(15),
        backoff_base: Duration::from_millis(50),
    }
}

async fn start(dirs: &Dirs, route: Option<CloudRoute>) -> Supervisor {
    Supervisor::start_with(dirs.clone(), quick(), route)
        .await
        .unwrap()
}

fn route() -> CloudRoute {
    CloudRoute::new(
        "https://API.Example.com./v1",
        "provider/model-name".into(),
        OnlineKey::new(KEY.into()),
    )
    .unwrap()
}

fn config(dirs: &Dirs) -> Value {
    serde_json::from_slice(&std::fs::read(dirs.config().join("config.json")).unwrap()).unwrap()
}

fn rows(dirs: &Dirs) -> Value {
    std::fs::read(dirs.config().join("user_models.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(json!({}))
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

/// Whether the running engine holds a provider key in memory, and whether the online model is
/// loaded.
async fn engine_holds(sup: &Supervisor) -> (bool, bool) {
    let admin = sup.admin();
    let health = admin.health().await.unwrap();
    let loaded = health
        .all_models_loaded
        .iter()
        .any(|m| m.model_name == MODEL_ID);
    let endpoint = sup.client().endpoint().unwrap();
    let info: Value = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{}/v1/system-info", endpoint.port))
        .bearer_auth(&endpoint.key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let key = info["cloud"]["providers"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| p["runtime_key_set"] == true);
    (key, loaded)
}

/// The key is in neither the engine's environment nor its process record.
async fn assert_key_not_in_environment(sup: &Supervisor) {
    let env = engine_env(sup).await;
    assert!(env.get(KEY_ENV).is_none());
    assert!(!env.to_string().contains(KEY));
    #[cfg(target_os = "linux")]
    {
        let environ = std::fs::read(format!("/proc/{}/environ", sup.pid().unwrap())).unwrap();
        assert!(!String::from_utf8_lossy(&environ).contains(KEY));
    }
}

/// Off: no provider, no row, no key, and nothing in the environment that names a proxy.
async fn assert_off(sup: &Supervisor, dirs: &Dirs) {
    assert_eq!(config(dirs)["cloud_providers"], json!([]));
    assert!(rows(dirs).get(ROW).is_none());
    assert_key_not_in_environment(sup).await;
    assert_eq!(engine_holds(sup).await, (false, false));
    assert!(!sup.control().route_taken());
}

#[tokio::test]
async fn off_launch_has_no_provider_row_or_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    assert_off(&sup, &dirs).await;
    assert_eq!(config(&dirs)["offline"], true);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn on_launch_writes_one_entry_and_row_and_hands_the_key_over() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    std::fs::create_dir_all(dirs.config()).unwrap();
    std::fs::write(
        dirs.config().join("user_models.json"),
        r#"{"mine": {"recipe": "llamacpp", "checkpoint": "a/b"}}"#,
    )
    .unwrap();
    let sup = start(&dirs, Some(route())).await;
    assert_eq!(
        config(&dirs)["cloud_providers"],
        json!([{
            "name": PROVIDER,
            "base_url": "https://api.example.com/v1",
            "allow_insecure_http": false,
            "wire_format": "openai",
        }]),
        "only the normalised address is written"
    );
    let rows = rows(&dirs);
    assert_eq!(
        rows[ROW],
        json!({"recipe": "cloud", "cloud_provider": PROVIDER, "checkpoint": "provider/model-name",
               "labels": ["chat"], "suggested": false})
    );
    assert_eq!(rows["mine"]["recipe"], "llamacpp", "other rows are kept");
    assert_key_not_in_environment(&sup).await;
    assert_eq!(engine_holds(&sup).await, (true, true));
    assert!(sup.control().route_taken());
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn key_never_in_argv_or_config_or_row() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    for file in ["config.json", "user_models.json", "recipe_options.json"] {
        let text = std::fs::read_to_string(dirs.config().join(file)).unwrap_or_default();
        assert!(!text.contains(KEY) && !text.contains(&KEY[..20]), "{file}");
    }
    #[cfg(target_os = "linux")]
    {
        let pid = sup.pid().unwrap();
        let argv = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap();
        assert!(!String::from_utf8_lossy(&argv).contains(&KEY[..20]));
        assert!(!String::from_utf8_lossy(&argv).contains("--log-file"));
    }
    let log = std::fs::read_to_string(dirs.logs().join("lemond.log")).unwrap_or_default();
    assert!(!log.contains(KEY));
    sup.shutdown().await.unwrap();
}

/// With a route, the engine's environment names no proxy and holds only the launch's two keys.
#[tokio::test]
async fn env_has_no_proxy_variables_or_provider_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    let env = engine_env(&sup).await;
    let names: Vec<String> = env.as_object().unwrap().keys().cloned().collect();
    assert!(
        names
            .iter()
            .all(|n| !n.to_ascii_lowercase().contains("proxy")),
        "{names:?}"
    );
    let mut lemonade: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| n.starts_with("LEMONADE_"))
        .collect();
    lemonade.sort_unstable();
    assert_eq!(lemonade, ["LEMONADE_ADMIN_API_KEY", "LEMONADE_API_KEY"]);
    assert!(sup.control().route_taken());
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn download_window_launch_has_no_provider_row_or_key() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    let control = sup.control();
    let hold = control.hold_online().await.unwrap();
    assert!(control.download_window_open());
    assert_eq!(config(&dirs)["offline"], false);
    assert_off(&sup, &dirs).await;
    hold.release().await;
    assert!(!control.download_window_open());
    assert_eq!(config(&dirs)["offline"], true);
    assert_eq!(config(&dirs)["cloud_providers"][0]["name"], PROVIDER);
    assert!(
        rows(&dirs).get(ROW).is_some(),
        "the closing launch restores it"
    );
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn offline_stays_true_with_route() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    sup.control().set_cloud(Some(route())).await.unwrap();
    let cfg = sup.admin().config().await.unwrap();
    assert_eq!(cfg["offline"], true);
    assert_eq!(config(&dirs)["no_fetch_executables"], true);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn turn_off_relaunches_without_route() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    control.set_cloud(Some(route())).await.unwrap();
    assert!(control.has_cloud());
    assert_eq!(engine_holds(&sup).await, (true, true));
    control.set_cloud(None).await.unwrap();
    assert!(!control.has_cloud());
    assert_off(&sup, &dirs).await;
    assert_eq!(common::launch_count(&dirs), 3);
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn relaunch_for_online_not_counted_as_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    for _ in 0..4 {
        sup.control().set_cloud(Some(route())).await.unwrap();
        sup.control().set_cloud(None).await.unwrap();
    }
    assert_eq!(
        sup.restarts(),
        0,
        "more relaunches than the restart limit, none a crash"
    );
    assert!(matches!(
        *sup.state().borrow(),
        nocturne_lemonade::RuntimeState::Ready { .. }
    ));
    sup.shutdown().await.unwrap();
}

#[tokio::test]
async fn relaunch_waits_for_idle_with_a_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    let client = sup.client();
    let endpoint = client.endpoint().unwrap();
    let script = |n: usize| {
        json!(
            (0..n)
                .map(|_| json!({"deltas": ["a ", "b ", "c [m1]."], "delay_ms": 50}))
                .collect::<Vec<_>>()
        )
    };
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{}/fake/script", endpoint.port))
        .bearer_auth(&endpoint.key)
        .json(&script(2))
        .send()
        .await
        .unwrap();
    let req = ChatRequest::new("some-model", vec![Message::user("q")]);

    // A request in flight is waited for, here until it is read to its end.
    control.set_idle_waits(Duration::from_secs(10), Duration::from_secs(10));
    let stream = client.chat(&req).await.unwrap();
    assert_eq!(client.busy_total(), 1);
    let reader = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        stream.collect::<Vec<_>>().await
    });
    let began = Instant::now();
    control.set_cloud(Some(route())).await.unwrap();
    let waited = began.elapsed();
    assert!(waited >= Duration::from_millis(350), "{waited:?}");
    assert!(waited < Duration::from_secs(8), "{waited:?}");
    let _ = reader.await;

    // One that does not end is waited for only up to the cap.
    let stream = client.chat(&req).await.unwrap();
    control.set_idle_waits(Duration::from_millis(300), Duration::from_millis(300));
    let began = Instant::now();
    control.set_cloud(None).await.unwrap();
    let waited = began.elapsed();
    assert!(waited >= Duration::from_millis(300), "{waited:?}");
    drop(stream);
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// Waits until the engine's state passes `ok`.
async fn until(sup: &Supervisor, ok: impl Fn(&nocturne_lemonade::RuntimeState) -> bool) {
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if ok(&state.borrow_and_update()) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine reached the state");
}

fn starting(s: &nocturne_lemonade::RuntimeState) -> bool {
    matches!(s, nocturne_lemonade::RuntimeState::Starting)
}

fn ready(s: &nocturne_lemonade::RuntimeState) -> bool {
    matches!(s, nocturne_lemonade::RuntimeState::Ready { .. })
}

/// A relaunch asked for while another one starts the engine waits its turn: the engine ends
/// up running with the last route asked for, and nothing stops it.
#[tokio::test]
async fn relaunches_during_a_launch_wait_and_the_last_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    common::set_mode(&dirs, "slow-start-ms=600");
    for last in [None, Some(route())] {
        let first = {
            let c = control.clone();
            let route = if last.is_some() { None } else { Some(route()) };
            tokio::spawn(async move { c.set_cloud(route).await })
        };
        until(&sup, starting).await;
        let with_route = last.is_some();
        control.set_cloud(last).await.unwrap();
        first.await.unwrap().unwrap();
        assert!(ready(&sup.state().borrow()), "the engine runs");
        assert_eq!(control.has_cloud(), with_route);
        if with_route {
            assert_eq!(engine_holds(&sup).await, (true, true));
            assert_eq!(config(&dirs)["cloud_providers"][0]["name"], PROVIDER);
        } else {
            assert_off(&sup, &dirs).await;
        }
    }
    assert_eq!(sup.restarts(), 0);
    sup.shutdown().await.unwrap();
}

/// The download window and the route changing at once are both kept: the engine runs in the
/// window without the route, and gets the route back when the window closes.
#[tokio::test]
async fn the_window_and_the_route_changing_at_once_both_hold() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    common::set_mode(&dirs, "slow-start-ms=600");
    let routed = {
        let c = control.clone();
        tokio::spawn(async move { c.set_cloud(Some(route())).await })
    };
    until(&sup, starting).await;
    let hold = control.hold_online().await.unwrap();
    routed.await.unwrap().unwrap();
    assert!(ready(&sup.state().borrow()));
    assert!(control.download_window_open());
    assert!(control.has_cloud());
    assert_eq!(config(&dirs)["offline"], false);
    assert_off(&sup, &dirs).await;

    // The other way round: the window closes while the route is taken away.
    let released = tokio::spawn(hold.release());
    until(&sup, starting).await;
    control.set_cloud(None).await.unwrap();
    released.await.unwrap();
    until(&sup, ready).await;
    assert!(!control.download_window_open());
    assert!(!control.has_cloud());
    assert_eq!(config(&dirs)["offline"], true);
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// The `config.json` entry and the static row are written in one place, the launch, from the
/// same route: no other code outside the unit tests calls either writer.
#[test]
fn one_writer_for_entry_and_row() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut calls = Vec::new();
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let code = text.split("#[cfg(test)]").next().unwrap_or_default();
            for (n, l) in code.lines().enumerate() {
                let l = l.trim_start();
                if l.starts_with("//") || l.starts_with("pub fn") {
                    continue;
                }
                if l.contains("config::write(") || l.contains("write_row(") {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    calls.push(format!("{name}:{}", n + 1));
                }
            }
        }
    }
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls.iter().all(|c| c.starts_with("supervisor.rs:")),
        "{calls:?}"
    );
}

#[test]
fn url_refusals() {
    for raw in [
        "https://api.example.com/v1?key=1",
        "https://127.0.0.1/v1",
        "https://localhost/v1",
        "https://169.254.169.254/latest/meta-data",
        "https://[fe80::1]/v1",
        "https://0.0.0.0/v1",
        "https://[::]/v1",
        "https://10.0.0.1/v1",
        "https://172.16.0.1/v1",
        "https://172.31.255.255/v1",
        "https://192.168.0.10/v1",
        "https://[fc00::1]/v1",
        "https://[fd12:3456::1]/v1",
        "https://[::ffff:10.0.0.1]/v1",
        "https://printer.local/v1",
    ] {
        assert!(validate_url(raw, false).is_err(), "{raw}");
    }
    assert_eq!(
        validate_url("https://api.example.com/v1?key=1", false),
        Err(UrlProblem::Extra)
    );
    assert_eq!(
        validate_url("https://[::ffff:10.0.0.1]/v1", false),
        Err(UrlProblem::NotAllowed)
    );
    assert!(validate_url("https://172.32.0.1/v1", false).is_ok());
}

/// A public name is never resolved here: one that points at this computer or the home network
/// is accepted. The residual risk is written down in ADR 0011.
#[test]
fn private_resolving_name_is_a_residual() {
    for raw in [
        "https://localtest.me/v1",
        "https://127.0.0.1.nip.io/v1",
        "https://10.0.0.1.sslip.io/v1",
    ] {
        assert!(validate_url(raw, false).is_ok(), "{raw}");
    }
}

#[test]
fn release_refuses_http_and_userinfo() {
    for raw in [
        "http://api.example.com/v1",
        "http://127.0.0.1:8080/v1",
        "https://user:secret@api.example.com/v1",
        "https://user@api.example.com/v1",
        "https://api.example.com/v1#x",
    ] {
        assert!(validate_url(raw, false).is_err(), "{raw}");
        assert!(CloudRoute::new(raw, "m".into(), OnlineKey::new("k".into())).is_err());
    }
    let route = route();
    assert!(!route.allow_insecure_http());
    assert!(!online::entry(&route).allow_insecure_http);
}

#[cfg(feature = "online-stand-in")]
#[test]
fn stand_in_http_only_for_loopback_with_feature() {
    let key = || OnlineKey::new("k".into());
    let r = CloudRoute::stand_in("http://127.0.0.1:9/v1", "m".into(), key()).unwrap();
    assert!(r.allow_insecure_http());
    assert!(online::entry(&r).allow_insecure_http);
    for raw in [
        "http://10.0.0.1:9/v1",
        "http://localhost:9/v1",
        "http://127.0.0.1/v1",
        "http://[::1]:9/v1",
    ] {
        assert!(
            CloudRoute::stand_in(raw, "m".into(), key()).is_err(),
            "{raw}"
        );
    }
    let https = CloudRoute::stand_in("https://api.example.com/v1", "m".into(), key()).unwrap();
    assert!(!https.allow_insecure_http());
}

#[cfg(not(feature = "online-stand-in"))]
#[test]
fn stand_in_http_only_for_loopback_with_feature() {
    assert_eq!(
        validate_url("http://127.0.0.1:9/v1", true),
        Err(UrlProblem::NotHttps),
        "without the feature even the stand-in address is refused"
    );
}

#[test]
fn classify_maps_statuses_and_curl_text() {
    use OnlineFailure::*;
    let cases = [
        (None, "", Unreachable),
        (
            Some(401),
            "cloud (nocturne_online) request failed",
            KeyRefused,
        ),
        (Some(403), "", KeyRefused),
        (Some(402), "", NoCredit),
        (Some(404), "", ModelMissing),
        (Some(429), "", Busy),
        (
            Some(500),
            "cloud (nocturne_online) request failed",
            Other(500),
        ),
        (Some(503), "", Other(503)),
        (
            Some(500),
            "Network error: CURL error: Couldn't resolve host name",
            NoInternet,
        ),
        (
            Some(500),
            "Network error: CURL error: Could not resolve host: api.example.com",
            NoInternet,
        ),
        (
            Some(500),
            "Network error: CURL error: Network is unreachable",
            NoInternet,
        ),
        (
            Some(500),
            "Network error: CURL error: Couldn't connect to server",
            Unreachable,
        ),
        (
            Some(500),
            "Network error: CURL error: Timeout was reached",
            Unreachable,
        ),
        (
            Some(500),
            "Network error: CURL error: SSL peer certificate or SSH remote key was not OK",
            Unreachable,
        ),
        (Some(400), "model 'x' is not available", ModelMissing),
        (Some(400), "bad request", Refused),
        (Some(307), "", Refused),
    ];
    for (status, text, want) in cases {
        assert_eq!(classify(status, text), want, "{status:?} {text}");
    }
}

fn line(severity: &str, tag: &str, message: &str) -> String {
    format!("2026-11-13 22:06:22.734 [{severity}] ({tag}) {message}")
}

/// Feeds `lines` through the pipe as the engine's output and returns the log written.
async fn piped(lines: &[String], key: Option<&str>) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("logs").join("lemond.log");
    let redact = Arc::new(Redactor::new());
    let key = key.map(|k| OnlineKey::new(k.into()));
    redact.set_key(key.as_ref());
    let (mut engine, stdout) = tokio::io::duplex(4096);
    let task = logpipe::spawn(stdout, file.clone(), redact);
    let end = line("Info", "Server", "end-of-feed");
    for l in lines.iter().chain([&end]) {
        engine.write_all(l.as_bytes()).await.unwrap();
        engine.write_all(b"\n").await.unwrap();
    }
    drop(engine);
    task.await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        if text.contains("end-of-feed") {
            return text;
        }
        assert!(Instant::now() < deadline, "the log was written: {text}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The error bodies the stand-in sends, as the engine logs them: the non-streaming dump cut at
/// about 200 bytes, so a partial key, and the streaming one.
fn engine_error_lines(key: &str, marker: &str) -> Vec<String> {
    let body = |status: u16| {
        json!({"details": {"response": {"error": {"code": status,
            "message": format!("{marker} authorization was Bearer {key}"),
            "type": "stand_in_error"}}, "status_code": status},
            "message": "cloud (nocturne_online) request failed", "type": "backend_error"})
        .to_string()
    };
    let mut out = Vec::new();
    for status in [401, 500] {
        let full = body(status);
        let cut: String = full.chars().take(200).collect();
        out.push(line(
            "Error",
            "Server",
            &format!("Backend returned error response: {cut}"),
        ));
        out.push(line(
            "Error",
            "Cloud",
            &format!(
                "Provider returned status {status}, body: {}",
                json!({"error": {"message": format!("{marker} authorization was Bearer {key}")}})
            ),
        ));
    }
    out.push(line(
        "Warning",
        "Cloud",
        &format!("GET https://api.example.com/v1/models returned HTTP 401. Body: {marker} {key}"),
    ));
    out.push(line("Info", "Server", "kept-after the bodies"));
    out
}

#[tokio::test]
async fn log_pipe_withholds_bodies() {
    let marker = "LOG-MARKER-7f3a";
    let text = piped(&engine_error_lines(KEY, marker), Some(KEY)).await;
    assert!(!text.contains(marker), "{text}");
    assert!(!text.contains(KEY));
    assert!(!text.contains(&KEY[..20]), "a partial key is withheld too");
    assert!(!text.contains(&KEY[KEY.len() - 8..]));
    assert!(text.contains("Backend returned error response: [withheld]"));
    assert!(text.contains("Provider returned status 500, body: [withheld]"));
    assert!(text.contains("kept-after the bodies"));
    // A key in an ordinary line is replaced, ends and all.
    let text = piped(
        &[line(
            "Info",
            "Server",
            &format!("odd {} and {}", &KEY[..8], &KEY[KEY.len() - 8..]),
        )],
        Some(KEY),
    )
    .await;
    assert!(
        !text.contains(&KEY[..8]) && !text.contains(&KEY[KEY.len() - 8..]),
        "{text}"
    );
}

#[tokio::test]
async fn log_pipe_withholds_multi_line_bodies() {
    let marker = "MULTI-MARKER-91c2";
    let lines = vec![
        line("Info", "Server", "before"),
        line(
            "Error",
            "Cloud",
            &format!("Provider returned status 401, body: {{\"error\": \"{marker} first line"),
        ),
        format!("{marker} second line"),
        format!("authorization was Bearer {}", &KEY[..20]),
        "2026-11-13 not a prefix".to_string(),
        format!("{marker} last line\"}}"),
        line("Info", "Server", "kept-after the body"),
        "plain line after a prefixed one is kept".to_string(),
    ];
    let text = piped(&lines, Some(KEY)).await;
    assert!(!text.contains(marker), "{text}");
    assert!(!text.contains(&KEY[..20]));
    assert!(text.contains("before"));
    assert!(text.contains("body: [withheld]"));
    assert!(
        text.contains("kept-after the body"),
        "the next prefixed line is kept"
    );
    assert!(text.contains("plain line after a prefixed one is kept"));
}

#[tokio::test]
async fn log_pipe_never_blocks_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("lemond.log");
    let (mut engine, stdout) = tokio::io::duplex(1024);
    let task = logpipe::spawn(stdout, file.clone(), Arc::new(Redactor::new()));
    let began = Instant::now();
    let filler = line("Info", "Server", &"x".repeat(900));
    for _ in 0..20_000 {
        engine.write_all(filler.as_bytes()).await.unwrap();
        engine.write_all(b"\n").await.unwrap();
    }
    drop(engine);
    task.await.unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(30),
        "the engine's output was read at its own pace: {:?}",
        began.elapsed()
    );
}

#[tokio::test]
async fn log_file_owner_only_and_rotated() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("lemond.log");
    let (mut engine, stdout) = tokio::io::duplex(4096);
    let task = logpipe::spawn_rotating_at(stdout, file.clone(), Arc::new(Redactor::new()), 2048);
    for i in 0..200 {
        let l = line("Info", "Server", &format!("line {i} {}", "y".repeat(60)));
        engine.write_all(l.as_bytes()).await.unwrap();
        engine.write_all(b"\n").await.unwrap();
        if i % 10 == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    drop(engine);
    task.await.unwrap();
    let rotated = |n: usize| tmp.path().join(format!("lemond.log.{n}"));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !rotated(2).exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(file.exists() && rotated(1).exists() && rotated(2).exists());
    assert!(!rotated(3).exists(), "three files in all");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for p in [file, rotated(1), rotated(2)] {
            let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{}", p.display());
        }
    }
}

/// The route B probe, kept: the real pinned engine runs without the key in its environment,
/// takes it at run time, reads the provider's list exactly once, and serves the static row with
/// `offline` true; the stand-in sees exactly the body Nocturne sent. Run with
/// `NOCTURNE_LEMOND` naming a bundle folder.
#[cfg(feature = "online-stand-in")]
#[tokio::test]
#[ignore = "needs the real engine (NOCTURNE_LEMOND)"]
async fn real_engine_takes_the_key_at_run_time() {
    let bundle = std::path::PathBuf::from(std::env::var("NOCTURNE_LEMOND").unwrap());
    let provider = Provider::start();
    let port = provider.port;
    let tmp = tempfile::tempdir().unwrap();
    let dirs = Dirs::new(bundle, tmp.path().join("root"));
    let sup = Supervisor::start_with(
        dirs.clone(),
        SupervisorOptions::default(),
        Some(stand_in_route(port)),
    )
    .await
    .unwrap();
    assert!(sup.control().route_taken());
    assert_eq!(provider.listings().await, 1, "one list at launch");
    let environ = std::fs::read(format!("/proc/{}/environ", sup.pid().unwrap())).unwrap();
    assert!(!String::from_utf8_lossy(&environ).contains(KEY));
    assert!(!String::from_utf8_lossy(&environ).contains(KEY_ENV));
    let mind = nocturne_lemonade::mind::online::OnlineMind::new(
        sup.client(),
        Arc::new(|ids: &[String]| vec![true; ids.len()]),
    );
    let fact = nocturne_engine::Fact::from(
        serde_json::from_value::<nocturne_contract::MemoryView>(json!({
            "id": "mem_1", "sentence": "The bins go out on Tuesday.", "subject": "Bins",
            "entityId": null, "predicate": "scheduled", "object": "Tuesday", "certainty": "sure",
            "status": "active", "validFrom": null, "validTo": null, "due": null,
            "learnedAt": "2026-11-10T09:30:00+00:00", "lastUsed": null, "letGoAt": null,
            "keepForever": false, "private": false, "suggestPrivate": false, "was": null,
            "receipts": [],
        }))
        .unwrap(),
    );
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    let meta = mind
        .answer("When do the bins go out?", std::slice::from_ref(&fact), tx)
        .await
        .unwrap();
    assert!(!meta.not_sure);
    assert_eq!(config(&dirs)["offline"], true);
    // A redirect from the provider is not followed by the engine's POST.
    reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/fake/script"))
        .json(&json!({"redirect": "/v1/elsewhere"}))
        .send()
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(4);
    assert!(mind.answer("again?", &[fact], tx).await.is_err());
    let seen = provider.requests().await;
    assert!(seen.iter().all(|r| r["path"] != "/v1/elsewhere"));
    assert!(
        seen.iter()
            .all(|r| r["headers"]["authorization"] == format!("Bearer {KEY}"))
    );
    // Turning off: the engine forgets the key, and the engine after it has none.
    sup.control().set_cloud(None).await.unwrap();
    assert!(!sup.control().route_taken());
    assert_eq!(provider.listings().await, 1, "no list without the route");
    let log = std::fs::read_to_string(dirs.logs().join("lemond.log")).unwrap_or_default();
    assert!(!log.contains(KEY));
    sup.shutdown().await.unwrap();
}

/// A stand-in provider on loopback for these tests, stopped when dropped.
#[cfg(feature = "online-stand-in")]
struct Provider {
    child: std::process::Child,
    port: u16,
}

#[cfg(feature = "online-stand-in")]
impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(feature = "online-stand-in")]
impl Provider {
    fn start() -> Provider {
        use std::io::BufRead;
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_fake-provider"))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line.trim().strip_prefix("ready ").unwrap().parse().unwrap();
        Provider { child, port }
    }

    async fn requests(&self) -> Vec<Value> {
        reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/fake/requests", self.port))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    /// Requests for the provider's list of models.
    async fn listings(&self) -> usize {
        self.requests()
            .await
            .iter()
            .filter(|r| r["path"] == "/v1/models")
            .count()
    }
}

#[cfg(feature = "online-stand-in")]
fn stand_in_route(port: u16) -> CloudRoute {
    CloudRoute::stand_in(
        &format!("http://127.0.0.1:{port}/v1"),
        "stand-in-model".into(),
        OnlineKey::new(KEY.into()),
    )
    .unwrap()
}

/// The engine reads the provider's list once per launch with the route, with the key it holds
/// in memory, and never in a launch without the route: off, or the download window.
#[cfg(feature = "online-stand-in")]
#[tokio::test]
async fn one_listing_per_launch_with_a_route_and_none_otherwise() {
    let provider = Provider::start();
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    assert_eq!(provider.listings().await, 0, "none while off");
    control
        .set_cloud(Some(stand_in_route(provider.port)))
        .await
        .unwrap();
    assert!(control.route_taken());
    assert_eq!(provider.listings().await, 1);
    let ask = ChatRequest::new(MODEL_ID, vec![Message::user("When?")]);
    assert!(!sup.client().chat_complete(&ask).await.unwrap().is_empty());
    let hold = control.hold_online().await.unwrap();
    assert!(!control.route_taken());
    assert_eq!(provider.listings().await, 1, "none in the download window");
    hold.release().await;
    assert!(control.route_taken());
    assert_eq!(
        provider.listings().await,
        2,
        "the closing relaunch gives it back"
    );
    control.set_cloud(None).await.unwrap();
    assert_eq!(provider.listings().await, 2, "none after turning off");
    let seen = provider.requests().await;
    assert_eq!(seen.len(), 3, "two lists and the one question");
    assert!(
        seen.iter()
            .all(|r| r["headers"]["authorization"] == format!("Bearer {KEY}"))
    );
    assert_eq!(sup.restarts(), 0);
    sup.shutdown().await.unwrap();
}

/// A key request the engine refuses leaves the route closed: no online model, no key held,
/// nothing can be sent, and the change says so. The engine itself keeps running.
#[tokio::test]
async fn a_refused_key_request_leaves_the_route_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    common::set_mode(&dirs, "cloud-auth-fail");
    let sup = start(&dirs, Some(route())).await;
    let control = sup.control();
    assert!(ready(&sup.state().borrow()));
    assert!(control.has_cloud());
    assert!(!control.route_taken());
    assert_eq!(engine_holds(&sup).await, (false, false));
    let ask = ChatRequest::new(MODEL_ID, vec![Message::user("When?")]);
    assert!(sup.client().chat_complete(&ask).await.is_err());
    assert!(matches!(
        control.set_cloud(Some(route())).await,
        Err(nocturne_lemonade::RuntimeError::RouteRefused)
    ));
    assert!(!control.route_taken());
    assert_key_not_in_environment(&sup).await;
    assert_eq!(sup.restarts(), 0);
    sup.shutdown().await.unwrap();
}

/// Turning off makes the running engine forget the key and drop the online model at once,
/// while the relaunch still waits for work in flight.
#[tokio::test]
async fn turning_off_clears_the_key_in_the_engine_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    let control = sup.control();
    assert_eq!(engine_holds(&sup).await, (true, true));
    let client = sup.client();
    let old = client.endpoint().unwrap();
    let busy = client
        .chat(&ChatRequest::new("some-model", vec![Message::user("q")]))
        .await
        .unwrap();
    control.set_idle_waits(Duration::from_secs(5), Duration::from_secs(5));
    let off = {
        let c = control.clone();
        tokio::spawn(async move { c.set_cloud(None).await })
    };
    let held = |port: u16, key: String| async move {
        let info: Value = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{port}/v1/system-info"))
            .bearer_auth(key)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        info["cloud"]["providers"][0]["runtime_key_set"] == true
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while held(old.port, old.key.clone()).await {
        assert!(Instant::now() < deadline, "the key was not cleared");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        common::launch_count(&dirs),
        1,
        "cleared before the relaunch"
    );
    assert!(!control.route_taken());
    drop(busy);
    off.await.unwrap().unwrap();
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// A download window whose relaunch failed is closed again: no later restart opens it without
/// the person's yes.
#[tokio::test]
async fn a_window_that_failed_to_open_stays_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let opts = SupervisorOptions {
        restart_limit: 50,
        backoff_base: Duration::from_millis(300),
        ..quick()
    };
    let sup = Supervisor::start_with(dirs.clone(), opts, None)
        .await
        .unwrap();
    let control = sup.control();
    common::set_mode(&dirs, "fail-bind");
    assert!(control.hold_online().await.is_err());
    assert!(!control.download_window_open(), "rolled back at once");
    common::set_mode(&dirs, "");
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                *state.borrow_and_update(),
                nocturne_lemonade::RuntimeState::Ready { .. }
            ) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine came back");
    assert_eq!(config(&dirs)["offline"], true, "the restart stayed offline");
    assert_eq!(config(&dirs)["no_fetch_executables"], true);
    assert!(!control.download_window_open());
    // A later yes still opens it.
    let hold = control.hold_online().await.unwrap();
    assert_eq!(config(&dirs)["offline"], false);
    hold.release().await;
    sup.shutdown().await.unwrap();
}

/// Closing the download window whose relaunch fails keeps it closed: the restart after it
/// launches the engine offline.
#[tokio::test]
async fn a_window_that_failed_to_close_stays_closed() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let opts = SupervisorOptions {
        restart_limit: 50,
        backoff_base: Duration::from_millis(300),
        ..quick()
    };
    let sup = Supervisor::start_with(dirs.clone(), opts, None)
        .await
        .unwrap();
    let control = sup.control();
    let hold = control.hold_online().await.unwrap();
    assert_eq!(config(&dirs)["offline"], false);
    common::set_mode(&dirs, "fail-bind");
    hold.release().await;
    assert!(
        !control.download_window_open(),
        "a failed close is not undone"
    );
    common::set_mode(&dirs, "");
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                *state.borrow_and_update(),
                nocturne_lemonade::RuntimeState::Ready { .. }
            ) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine came back");
    assert_eq!(
        config(&dirs)["offline"],
        true,
        "the restart launched offline"
    );
    assert_eq!(config(&dirs)["no_fetch_executables"], true);
    sup.shutdown().await.unwrap();
}

/// A relaunch with a route that fails takes the route away: the restart after it has no key.
#[tokio::test]
async fn a_route_whose_relaunch_failed_is_not_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let opts = SupervisorOptions {
        restart_limit: 50,
        backoff_base: Duration::from_millis(300),
        ..quick()
    };
    let sup = Supervisor::start_with(dirs.clone(), opts, None)
        .await
        .unwrap();
    let control = sup.control();
    common::set_mode(&dirs, "fail-bind");
    assert!(control.set_cloud(Some(route())).await.is_err());
    assert!(!control.has_cloud(), "dropped at once");
    common::set_mode(&dirs, "");
    let mut state = sup.state();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if matches!(
                *state.borrow_and_update(),
                nocturne_lemonade::RuntimeState::Ready { .. }
            ) {
                break;
            }
            state.changed().await.unwrap();
        }
    })
    .await
    .expect("the engine came back");
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// A line longer than the pipe holds is cut: the rest of it, key and all, is withheld, and the
/// next line is read as usual.
#[tokio::test]
async fn a_very_long_line_is_cut_and_the_rest_withheld() {
    let long = format!(
        "{} {} tail",
        line("Info", "Server", &"z".repeat(200 * 1024)),
        KEY
    );
    let text = piped(
        &[long, line("Info", "Server", "kept-after the long one")],
        None,
    )
    .await;
    assert!(
        !text.contains(&KEY[..20]),
        "nothing past the cut is written"
    );
    assert!(text.contains("kept-after the long one"));
    assert!(text.lines().all(|l| l.len() <= logpipe::MAX_LINE));
}

/// Turned off while the engine starts with the route: the launch read the route before the
/// change, yet it hands nothing over, so the provider never sees the list request the key
/// would cause. The same holds for Delete everything, which turns off the same way.
#[cfg(feature = "online-stand-in")]
#[tokio::test]
async fn turning_off_while_the_engine_starts_hands_nothing_over() {
    let provider = Provider::start();
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, None).await;
    let control = sup.control();
    common::set_mode(&dirs, "slow-start-ms=800");
    let on = {
        let c = control.clone();
        let route = stand_in_route(provider.port);
        tokio::spawn(async move { c.set_cloud(Some(route)).await })
    };
    // The new engine process exists: the launch has read the route and waits for it.
    let deadline = Instant::now() + Duration::from_secs(10);
    while common::launch_count(&dirs) < 2 {
        assert!(Instant::now() < deadline, "the relaunch never began");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(starting(&sup.state().borrow()));
    common::set_mode(&dirs, "");
    control.set_cloud(None).await.unwrap();
    on.await.unwrap().unwrap();
    assert!(!control.route_taken());
    assert_eq!(provider.listings().await, 0, "no key, so no list");
    assert!(provider.requests().await.is_empty());
    assert_off(&sup, &dirs).await;
    sup.shutdown().await.unwrap();
}

/// A local model that fails to load makes the engine unload every model, the online one too,
/// and it cannot load that one again until it restarts. The supervisor notices, and giving the
/// route again (a check, "Try again") brings it back.
#[tokio::test]
async fn an_online_model_unloaded_by_the_engine_is_noticed() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    let sup = start(&dirs, Some(route())).await;
    let control = sup.control();
    assert!(control.route_held().await);
    let endpoint = sup.client().endpoint().unwrap();
    let http = reqwest::Client::new();
    let url = |p: &str| format!("http://127.0.0.1:{}{p}", endpoint.port);
    http.post(url("/fake/mode"))
        .bearer_auth(&endpoint.key)
        .json(&json!({"mode": "load-oom"}))
        .send()
        .await
        .unwrap();
    let load = http
        .post(url("/v1/load"))
        .bearer_auth(&endpoint.key)
        .json(&json!({"model_name": "Qwen3.5-2B-GGUF"}))
        .send()
        .await
        .unwrap();
    assert_eq!(load.status(), 500);
    assert!(control.route_taken(), "the key is still held");
    assert!(!control.route_held().await, "but the online model is gone");
    assert!(
        sup.admin().load(MODEL_ID).await.is_err(),
        "and cannot come back without a restart"
    );
    let ask = ChatRequest::new(MODEL_ID, vec![Message::user("When?")]);
    assert!(matches!(
        sup.client().chat_complete(&ask).await,
        Err(nocturne_lemonade::RuntimeError::NotFound(_))
    ));
    http.post(url("/fake/mode"))
        .bearer_auth(&endpoint.key)
        .json(&json!({"mode": ""}))
        .send()
        .await
        .unwrap();
    control.set_cloud(Some(route())).await.unwrap();
    assert!(control.route_held().await);
    sup.shutdown().await.unwrap();
}

/// Like the real engine, the fake refuses a handed-over key while one is in its environment,
/// and says the environment's key is set.
#[tokio::test]
async fn a_key_in_the_environment_refuses_a_handed_over_one() {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = common::dirs(tmp.path());
    std::fs::write(
        dirs.config().join("config.json"),
        json!({"cloud_providers": [{"name": PROVIDER, "base_url": "https://api.example.com/v1",
                                     "allow_insecure_http": false, "wire_format": "openai"}]})
        .to_string(),
    )
    .unwrap();
    let port = common::free_port();
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_fake-lemond"))
        .arg(dirs.cache())
        .arg(dirs.config())
        .args(["--port", &port.to_string(), "--host", "127.0.0.1"])
        .env_clear()
        .env("LEMONADE_API_KEY", "k3y")
        .env("LEMONADE_ADMIN_API_KEY", "adm1n")
        .env(KEY_ENV, "from-the-environment")
        .stdin(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let admin = nocturne_lemonade::Admin::new(port, "adm1n").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while admin.health().await.is_err() {
        assert!(Instant::now() < deadline, "the fake did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let refused = admin
        .cloud_auth(PROVIDER, &OnlineKey::new(KEY.into()))
        .await;
    assert!(
        matches!(
            refused,
            Err(nocturne_lemonade::RuntimeError::Rejected { status: 409, .. })
        ),
        "{refused:?}"
    );
    let _ = child.kill();
    let _ = child.wait();
}
